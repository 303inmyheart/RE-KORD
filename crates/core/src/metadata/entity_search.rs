//! Curiosità / entity info search (port of legacy `server/entityInfo.mjs`
//! `searchEntityInfoSources`).
//!
//! Works in ONE language: finds the right Wikipedia page by checking the
//! intro has a music context (no "Book of Psalms" when looking for the
//! rapper Salmo), skipping disambiguation pages and falling back to English
//! when the language has no page; then adds the interesting sections
//! (`SECTION_PRIORITY`), Wikiquote quotes, the Last.fm bio (with
//! `LASTFM_API_KEY`), TheAudioDB and the Discogs profile / release notes
//! (with a Discogs token). A failing source is reported in `errors` next to
//! the partial results instead of failing the whole search.

use super::error::{MetaError, SourceError};
use super::http::get_json;
use super::jsre::JsRegex;
use super::text::{
    artist_similarity, cap_at_sentence, content_hash_id, normalize_name, take_chars,
    title_similarity,
};
use crate::js_re;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::sync::LazyLock;

/// Intro / bio limit (legacy `ITEM_TEXT_MAX`).
pub const ITEM_TEXT_MAX: usize = 6000;
/// Section limit (legacy 1600; more generous here, still cut at a sentence).
pub const SECTION_TEXT_MAX: usize = 3000;
/// Default number of candidates returned.
pub const DEFAULT_MAX_CANDIDATES: usize = 10;

static MUSIC_RE_IT: LazyLock<JsRegex> = js_re!(
    r"(musicist|rapper|cantant|cantautor|gruppo musical|band|discografi|album|singol|produttore discografic|\bdj\b|chitarrist|polistrumentist)",
    "i"
);
static MUSIC_RE_EN: LazyLock<JsRegex> = js_re!(
    r"(musician|rapper|singer|songwriter|band|discograph|album|single|record producer|\bdj\b|guitarist|hip.hop)",
    "i"
);

/// Discography / list pages: about a body of work, never the artist's bio nor
/// one album's description ("Discografia di Salmo" for Salmo).
static LIST_PAGE_RE: LazyLock<JsRegex> = js_re!(
    r"^(discografia|videografia|elenco|lista|list of|discography|videography)\b|\b(discography|videography)$",
    "i"
);

pub fn is_list_page(title: &str) -> bool {
    LIST_PAGE_RE.is_match(title.trim())
}

fn music_re(lang: &str) -> &'static JsRegex {
    if lang == "en" {
        &MUSIC_RE_EN
    } else {
        &MUSIC_RE_IT
    }
}

fn music_hint(lang: &str) -> &'static str {
    if lang == "en" {
        "music"
    } else {
        "musica"
    }
}

static SECTION_PRIORITY_IT: LazyLock<Vec<JsRegex>> = LazyLock::new(|| {
    [
        r"curiosit|aneddot",
        r"vita privata",
        r"controversi|polemich",
        r"origine del nome|nome d'arte|pseudonimo",
        r"stile|influenz",
        r"produzione|registrazione",
        r"accoglienza|critica",
        r"tematich|concept",
        r"riconoscim|premi",
        r"eredità|impatto",
    ]
    .iter()
    .map(|p| JsRegex::new(p, "i"))
    .collect()
});

static SECTION_PRIORITY_EN: LazyLock<Vec<JsRegex>> = LazyLock::new(|| {
    [
        r"trivia|did you know",
        r"personal life",
        r"controvers",
        r"name|etymology",
        r"style|influence",
        r"production|recording",
        r"reception|critical",
        r"themes|composition|concept",
        r"accolade|award",
        r"legacy|impact",
    ]
    .iter()
    .map(|p| JsRegex::new(p, "i"))
    .collect()
});

fn section_priority(lang: &str) -> &'static [JsRegex] {
    if lang == "en" {
        &SECTION_PRIORITY_EN
    } else {
        &SECTION_PRIORITY_IT
    }
}

static TRIVIA_RE: LazyLock<JsRegex> = js_re!(r"curiosit|aneddot|trivia|did you know", "i");

/// One selectable curiosità candidate.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityCandidate {
    /// Stable content id (language + normalized text); equals the id the
    /// item gets when saved, so the client can tick "already saved".
    pub id: String,
    /// `wikipedia`, `wikiquote`, `lastfm`, `theaudiodb`, `discogs`.
    pub source: String,
    /// `bio` (artist), `desc` (album), `section`, `trivia`.
    pub kind: String,
    /// Language of the text (may be `en` when the requested language had no page).
    pub lang: String,
    pub title: Option<String>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Same as `image_url` (legacy field name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// An item with the same text is already saved for this artist / album.
    #[serde(default)]
    pub already_saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_id: Option<String>,
}

/// The Wikipedia page the candidates come from.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityPage {
    pub title: String,
    pub lang: String,
    pub url: String,
    /// True when the requested language had no music page and English was used.
    pub fallback: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntitySearchResult {
    pub candidates: Vec<EntityCandidate>,
    /// Sources that failed (results are partial when not empty).
    pub errors: Vec<SourceError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<EntityPage>,
    pub lang: String,
}

#[derive(Debug, Clone, Default)]
pub struct EntitySearchOptions {
    pub artist: String,
    pub album: Option<String>,
    /// `it` (default) or `en`.
    pub lang: String,
    pub discogs_token: Option<String>,
    /// Last.fm API key; `None` reads `LASTFM_API_KEY` (legacy).
    pub lastfm_key: Option<String>,
    pub max_candidates: Option<usize>,
}

/// Dedupe key of an item (legacy `itemDedupeKey`): language + first 140
/// chars of the normalized text.
pub fn dedupe_key(lang: &str, text: &str) -> String {
    format!("{lang}\u{0}{}", take_chars(&normalize_name(text), 140))
}

/// Stable id for an item's content.
pub fn item_content_id(lang: &str, text: &str) -> String {
    content_hash_id(&[lang, &take_chars(&normalize_name(text), 140)])
}

// ---------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------

static WT_REF_SELF: LazyLock<JsRegex> = js_re!(r"<ref[^>]*\/>", "gi");
static WT_REF: LazyLock<JsRegex> = js_re!(r"<ref[^>]*>[\s\S]*?<\/ref>", "gi");
static WT_COMMENT: LazyLock<JsRegex> = js_re!(r"<!--[\s\S]*?-->", "g");
static WT_TEMPLATE: LazyLock<JsRegex> = js_re!(r"\{\{[^{}]*\}\}", "g");
static WT_FILE: LazyLock<JsRegex> = js_re!(r"\[\[(?:File|Image|Immagine):[^\]]*\]\]", "gi");
static WT_PIPED: LazyLock<JsRegex> = js_re!(r"\[\[[^\]|]*\|([^\]]*)\]\]", "g");
static WT_LINK: LazyLock<JsRegex> = js_re!(r"\[\[([^\]]*)\]\]", "g");
static WT_EXT: LazyLock<JsRegex> = js_re!(r"\[https?:\/\/\S+\s+([^\]]*)\]", "g");
static WT_QUOTES: LazyLock<JsRegex> = js_re!(r"'{2,}", "g");
static WT_HEADING: LazyLock<JsRegex> = js_re!(r"^=+.*=+\s*$", "gm");
static WT_LIST: LazyLock<JsRegex> = js_re!(r"^\s*[*#:;]+\s*", "gm");
static WT_TAG: LazyLock<JsRegex> = js_re!(r"<[^>]+>", "g");
static WT_BLANKS: LazyLock<JsRegex> = js_re!(r"\n{3,}", "g");

/// Legacy `stripWikitext`: wikitext → readable text (templates, refs,
/// links, headings and markup removed).
pub fn strip_wikitext(raw: &str) -> String {
    let mut s = WT_REF_SELF.replace_all(raw, "");
    s = WT_REF.replace_all(&s, "");
    s = WT_COMMENT.replace_all(&s, "");
    let mut i = 0;
    while i < 6 && s.contains("{{") {
        s = WT_TEMPLATE.replace_all(&s, "");
        i += 1;
    }
    s = WT_FILE.replace_all(&s, "");
    s = WT_PIPED.replace_all(&s, "$1");
    s = WT_LINK.replace_all(&s, "$1");
    s = WT_EXT.replace_all(&s, "$1");
    s = WT_QUOTES.replace_all(&s, "");
    s = WT_HEADING.replace_all(&s, "");
    s = WT_LIST.replace_all(&s, "• ");
    s = WT_TAG.replace_all(&s, "");
    WT_BLANKS.replace_all(&s, "\n\n").trim().to_string()
}

static LFM_LINK: LazyLock<JsRegex> = js_re!(r"<a href[^>]*>.*?<\/a>\.?", "gis");
static LFM_READ_MORE: LazyLock<JsRegex> = js_re!(r"Read more on Last\.fm.*$", "is");

/// Last.fm bio / wiki content → plain text (legacy cleanup); `None` when
/// shorter than 80 chars.
pub fn clean_lastfm_text(content: &str) -> Option<String> {
    let s = LFM_LINK.replace_all(content, "");
    let s = WT_TAG.replace_all(&s, "");
    let s = LFM_READ_MORE.replace(&s, "");
    let s = s.trim();
    (s.chars().count() >= 80).then(|| cap_at_sentence(s, ITEM_TEXT_MAX))
}

static DG_ENTITY: LazyLock<JsRegex> = js_re!(r"\[(?:a|l|m|r)=([^\]]+)\]", "g");
static DG_ID: LazyLock<JsRegex> = js_re!(r"\[(?:a|l|m|r)\d+\]", "g");
static DG_URL: LazyLock<JsRegex> = js_re!(r"\[url=[^\]]*\]([\s\S]*?)\[\/url\]", "gi");
static DG_FMT: LazyLock<JsRegex> = js_re!(r"\[\/?(?:b|i|u)\]", "gi");

/// Discogs profile markup (`[a=Artist]`, `[url=…]x[/url]`, `[b]`) → text.
pub fn clean_discogs_markup(raw: &str) -> String {
    let s = DG_URL.replace_all(raw, "$1");
    let s = DG_ENTITY.replace_all(&s, "$1");
    let s = DG_ID.replace_all(&s, "");
    let s = DG_FMT.replace_all(&s, "");
    s.replace("\r\n", "\n").trim().to_string()
}

static WQ_SKIP: LazyLock<JsRegex> = js_re!(
    r"^==|^Altri progetti|^Note|^Bibliografia|^Voci correlate|^External|^See also|^References",
    "i"
);

/// Quotes from a Wikiquote page extract (legacy `wikiquoteArtistQuotes`):
/// the head must describe a musician; 2..=8 lines of 40..=320 chars.
pub fn wikiquote_quotes(lang: &str, extract: &str) -> Option<String> {
    if extract.is_empty() || !music_re(lang).is_match(&take_chars(extract, 600)) {
        return None;
    }
    let quotes: Vec<String> = extract
        .split('\n')
        .map(str::trim)
        .filter(|l| {
            let n = l.chars().count();
            (40..=320).contains(&n) && !WQ_SKIP.is_match(l)
        })
        .take(8)
        .map(|q| format!("• {q}"))
        .collect();
    (quotes.len() >= 2).then(|| quotes.join("\n"))
}

/// One Wikipedia page intro.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiIntro {
    pub title: String,
    pub extract: String,
    pub thumbnail: Option<String>,
    pub url: String,
    pub disambiguation: bool,
}

/// Parse an `action=query&prop=extracts|pageimages|pageprops|info` reply.
pub fn parse_wiki_intro(lang: &str, j: &Value, fallback_title: &str) -> Option<WikiIntro> {
    let page = j.pointer("/query/pages")?.as_object()?.values().next()?;
    if page.get("missing").is_some() {
        return None;
    }
    let extract = page
        .get("extract")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if extract.is_empty() {
        return None;
    }
    let title = page
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(fallback_title)
        .to_string();
    let url = page
        .get("fullurl")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| wiki_url(lang, "wikipedia", &title));
    Some(WikiIntro {
        extract: cap_at_sentence(extract, ITEM_TEXT_MAX),
        thumbnail: page
            .pointer("/thumbnail/source")
            .and_then(Value::as_str)
            .map(str::to_string),
        disambiguation: page.pointer("/pageprops/disambiguation").is_some(),
        title,
        url,
    })
}

fn wiki_url(lang: &str, site: &str, title: &str) -> String {
    let path: String = url::form_urlencoded::byte_serialize(title.replace(' ', "_").as_bytes())
        .collect::<String>()
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%28", "(")
        .replace("%29", ")");
    format!("https://{lang}.{site}.org/wiki/{path}")
}

/// Legacy music-page rule: the intro has music context, or (albums) cites
/// the artist. Disambiguation pages never pass.
pub fn intro_is_music_page(
    lang: &str,
    intro: &WikiIntro,
    artist: &str,
    album: Option<&str>,
) -> bool {
    if intro.disambiguation {
        return false;
    }
    let musical = music_re(lang).is_match(&intro.extract);
    let artist_norm = normalize_name(artist);
    let mentions_artist =
        !artist_norm.is_empty() && normalize_name(&intro.extract).contains(&artist_norm);
    musical || (album.is_some() && mentions_artist)
}

/// Page titles that name the entity itself ("Salmo (rapper)" for Salmo,
/// "Exuvia (album)" for Exuvia) are tried first.
fn title_names_entity(page_title: &str, wanted: &str) -> bool {
    let base = page_title.split(" (").next().unwrap_or(page_title);
    title_similarity(base, wanted) >= 0.8 || artist_similarity(wanted, base) >= 0.9
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

struct Ctx {
    errors: Vec<SourceError>,
    /// Every failed request, repeated ones included (`errors` is deduped).
    failures: usize,
}

impl Ctx {
    fn fail(&mut self, e: SourceError) {
        self.failures += 1;
        if !self
            .errors
            .iter()
            .any(|x| x.source == e.source && x.code == e.code)
        {
            self.errors.push(e);
        }
    }

    async fn json(&mut self, source: &str, url: &str, q: &[(&str, &str)]) -> Option<Value> {
        self.json_h(source, url, q, None).await
    }

    async fn json_h(
        &mut self,
        source: &str,
        url: &str,
        q: &[(&str, &str)],
        h: Option<reqwest::header::HeaderMap>,
    ) -> Option<Value> {
        match get_json(source, url, q, h).await {
            Ok(v) if v.is_null() => None,
            Ok(v) => Some(v),
            Err(e) => {
                self.fail(e);
                None
            }
        }
    }
}

async fn wiki_search_titles(ctx: &mut Ctx, lang: &str, query: &str, limit: usize) -> Vec<String> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let limit = limit.to_string();
    let Some(j) = ctx
        .json(
            "wikipedia",
            &url,
            &[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", query),
                ("srlimit", &limit),
                ("srprop", ""),
                ("format", "json"),
            ],
        )
        .await
    else {
        return vec![];
    };
    j.pointer("/query/search")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("title").and_then(Value::as_str))
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

async fn wiki_intro(ctx: &mut Ctx, lang: &str, title: &str) -> Option<WikiIntro> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let j = ctx
        .json(
            "wikipedia",
            &url,
            &[
                ("action", "query"),
                ("prop", "extracts|pageimages|pageprops|info"),
                ("exintro", "1"),
                ("explaintext", "1"),
                ("redirects", "1"),
                ("piprop", "thumbnail"),
                ("pithumbsize", "600"),
                ("ppprop", "disambiguation"),
                ("inprop", "url"),
                ("format", "json"),
                ("titles", title),
            ],
        )
        .await?;
    parse_wiki_intro(lang, &j, title)
}

/// Legacy `wikiFindMusicPage` (+ entity-named titles first, disambiguation
/// skipped).
async fn wiki_find_music_page(
    ctx: &mut Ctx,
    lang: &str,
    artist: &str,
    album: Option<&str>,
) -> Option<WikiIntro> {
    let queries: Vec<String> = match album {
        Some(al) => vec![format!("{al} {artist}"), format!("{al} album {artist}")],
        None => vec![format!("{artist} {}", music_hint(lang)), artist.to_string()],
    };
    let mut titles: Vec<String> = Vec::new();
    for q in &queries {
        for t in wiki_search_titles(ctx, lang, q, 4).await {
            if !titles.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                titles.push(t);
            }
        }
        if titles.len() >= 6 {
            break;
        }
    }
    titles.retain(|t| !is_list_page(t));
    titles.truncate(6);
    let wanted = album.unwrap_or(artist);
    // Entity-named pages first, keeping the search order otherwise.
    titles.sort_by_key(|t| !title_names_entity(t, wanted));
    for t in &titles {
        let failures = ctx.failures;
        let mut intro = wiki_intro(ctx, lang, t).await;
        if intro.is_none() && ctx.failures > failures && title_names_entity(t, wanted) {
            // The page naming the entity failed to load (timeout, busy API):
            // one more try before settling for a weaker match.
            intro = wiki_intro(ctx, lang, t).await;
        }
        let Some(intro) = intro else {
            continue;
        };
        if intro_is_music_page(lang, &intro, artist, album) {
            return Some(intro);
        }
    }
    None
}

#[derive(Debug, Clone)]
struct WikiSection {
    index: String,
    line: String,
    anchor: String,
}

async fn wiki_sections(ctx: &mut Ctx, lang: &str, title: &str) -> Vec<WikiSection> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let Some(j) = ctx
        .json(
            "wikipedia",
            &url,
            &[
                ("action", "parse"),
                ("prop", "sections"),
                ("redirects", "1"),
                ("format", "json"),
                ("page", title),
            ],
        )
        .await
    else {
        return vec![];
    };
    j.pointer("/parse/sections")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|s| {
                    let index = match s.get("index") {
                        Some(Value::String(x)) => x.clone(),
                        Some(Value::Number(n)) => n.to_string(),
                        _ => return None,
                    };
                    if index.is_empty() {
                        return None;
                    }
                    let line = s.get("line").and_then(Value::as_str).unwrap_or("");
                    // Section titles may carry HTML (<i>…</i>).
                    let line = WT_TAG.replace_all(line, "").trim().to_string();
                    Some(WikiSection {
                        index,
                        anchor: s
                            .get("anchor")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        line,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn wiki_section_text(ctx: &mut Ctx, lang: &str, title: &str, index: &str) -> Option<String> {
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let j = ctx
        .json(
            "wikipedia",
            &url,
            &[
                ("action", "parse"),
                ("prop", "wikitext"),
                ("redirects", "1"),
                ("format", "json"),
                ("page", title),
                ("section", index),
            ],
        )
        .await?;
    let raw = j.pointer("/parse/wikitext/*").and_then(Value::as_str)?;
    let text = strip_wikitext(raw);
    if text.chars().count() < 60 {
        return None;
    }
    Some(cap_at_sentence(&text, SECTION_TEXT_MAX))
}

async fn wikiquote(ctx: &mut Ctx, lang: &str, artist: &str) -> Option<(String, String)> {
    let base = format!("https://{lang}.wikiquote.org/w/api.php");
    let js = ctx
        .json(
            "wikiquote",
            &base,
            &[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", artist),
                ("srlimit", "3"),
                ("srprop", ""),
                ("format", "json"),
            ],
        )
        .await?;
    let artist_norm = normalize_name(artist);
    let hit = js
        .pointer("/query/search")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|s| s.get("title").and_then(Value::as_str))
        .find(|t| {
            let n = normalize_name(t);
            !artist_norm.is_empty() && (n == artist_norm || n.contains(&artist_norm))
        })?
        .to_string();
    let jx = ctx
        .json(
            "wikiquote",
            &base,
            &[
                ("action", "query"),
                ("prop", "extracts"),
                ("explaintext", "1"),
                ("redirects", "1"),
                ("format", "json"),
                ("titles", &hit),
            ],
        )
        .await?;
    let extract = jx
        .pointer("/query/pages")
        .and_then(Value::as_object)
        .and_then(|p| p.values().next())
        .and_then(|p| p.get("extract"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let quotes = wikiquote_quotes(lang, extract)?;
    Some((quotes, wiki_url(lang, "wikiquote", &hit)))
}

async fn lastfm(
    ctx: &mut Ctx,
    key: &str,
    lang: &str,
    artist: &str,
    album: Option<&str>,
) -> Option<(String, Option<String>)> {
    let mut q: Vec<(&str, &str)> = vec![
        ("api_key", key),
        ("format", "json"),
        ("lang", lang),
        ("artist", artist),
    ];
    match album {
        Some(al) => {
            q.push(("method", "album.getinfo"));
            q.push(("album", al));
        }
        None => q.push(("method", "artist.getinfo")),
    }
    let j = ctx
        .json("lastfm", "https://ws.audioscrobbler.com/2.0/", &q)
        .await?;
    let (content, url) = match album {
        Some(_) => (j.pointer("/album/wiki/content"), j.pointer("/album/url")),
        None => (j.pointer("/artist/bio/content"), j.pointer("/artist/url")),
    };
    let text = clean_lastfm_text(content.and_then(Value::as_str).unwrap_or(""))?;
    Some((text, url.and_then(Value::as_str).map(str::to_string)))
}

fn audiodb_field(row: &Value, base: &str, lang: &str) -> Option<String> {
    let suffix = if lang == "it" { "IT" } else { "EN" };
    row.get(format!("{base}{suffix}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// TheAudioDB artist bio / album description in `lang`, with thumb and url.
async fn audiodb(
    ctx: &mut Ctx,
    lang: &str,
    artist: &str,
    album: Option<&str>,
) -> Option<(Option<String>, Option<String>, Option<String>)> {
    let key = super::providers::theaudiodb_key();
    match album {
        None => {
            let url = format!("https://www.theaudiodb.com/api/v1/json/{key}/search.php");
            let j = ctx.json("theaudiodb", &url, &[("s", artist)]).await?;
            let row = j
                .get("artists")
                .and_then(Value::as_array)?
                .iter()
                .find(|r| {
                    artist_similarity(
                        artist,
                        r.get("strArtist").and_then(Value::as_str).unwrap_or(""),
                    ) >= 0.75
                })?;
            let text = audiodb_field(row, "strBiography", lang);
            let thumb = row
                .get("strArtistThumb")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let url = row
                .get("idArtist")
                .and_then(Value::as_str)
                .map(|id| format!("https://www.theaudiodb.com/artist/{id}"));
            (text.is_some() || thumb.is_some()).then_some((text, thumb, url))
        }
        Some(al) => {
            let url = format!("https://www.theaudiodb.com/api/v1/json/{key}/searchalbum.php");
            let j = ctx
                .json("theaudiodb", &url, &[("s", artist), ("a", al)])
                .await?;
            let row = j.get("album").and_then(Value::as_array)?.iter().find(|r| {
                title_similarity(al, r.get("strAlbum").and_then(Value::as_str).unwrap_or("")) >= 0.7
            })?;
            let text = audiodb_field(row, "strDescription", lang)?;
            let url = row
                .get("idAlbum")
                .and_then(Value::as_str)
                .map(|id| format!("https://www.theaudiodb.com/album/{id}"));
            Some((Some(text), None, url))
        }
    }
}

fn discogs_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.discogs.v2.discogs+json"),
    );
    if let Ok(v) = format!("Discogs token={token}").parse() {
        h.insert(reqwest::header::AUTHORIZATION, v);
    }
    h
}

/// Discogs artist profile / release notes: (text, thumb, url).
async fn discogs_info(
    ctx: &mut Ctx,
    token: &str,
    artist: &str,
    album: Option<&str>,
) -> Option<(String, Option<String>, Option<String>)> {
    let h = || Some(discogs_headers(token));
    let search = "https://api.discogs.com/database/search";
    match album {
        Some(al) => {
            let s = ctx
                .json_h(
                    "discogs",
                    search,
                    &[
                        ("artist", artist),
                        ("release_title", al),
                        ("type", "release"),
                        ("per_page", "5"),
                    ],
                    h(),
                )
                .await?;
            let top = s
                .get("results")
                .and_then(Value::as_array)?
                .iter()
                .find(|r| {
                    let t = r.get("title").and_then(Value::as_str).unwrap_or("");
                    let album_part = t.split_once(" - ").map(|(_, b)| b).unwrap_or(t);
                    title_similarity(al, album_part) >= 0.7
                })?;
            let id = top.get("id").and_then(Value::as_i64)?;
            let rel = ctx
                .json_h(
                    "discogs",
                    &format!("https://api.discogs.com/releases/{id}"),
                    &[],
                    h(),
                )
                .await?;
            let notes =
                clean_discogs_markup(rel.get("notes").and_then(Value::as_str).unwrap_or(""));
            if notes.chars().count() < 40 {
                return None;
            }
            Some((
                cap_at_sentence(&notes, ITEM_TEXT_MAX),
                top.get("thumb")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                rel.get("uri").and_then(Value::as_str).map(str::to_string),
            ))
        }
        None => {
            let s = ctx
                .json_h(
                    "discogs",
                    search,
                    &[("q", artist), ("type", "artist"), ("per_page", "3")],
                    h(),
                )
                .await?;
            let top = s
                .get("results")
                .and_then(Value::as_array)?
                .iter()
                .find(|r| {
                    artist_similarity(artist, r.get("title").and_then(Value::as_str).unwrap_or(""))
                        >= 0.75
                })?;
            let id = top.get("id").and_then(Value::as_i64)?;
            let ar = ctx
                .json_h(
                    "discogs",
                    &format!("https://api.discogs.com/artists/{id}"),
                    &[],
                    h(),
                )
                .await?;
            let profile =
                clean_discogs_markup(ar.get("profile").and_then(Value::as_str).unwrap_or(""));
            if profile.chars().count() < 40 {
                return None;
            }
            let img = ar.pointer("/images/0").and_then(|i| {
                i.get("uri150")
                    .or_else(|| i.get("uri"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            });
            Some((
                cap_at_sentence(&profile, ITEM_TEXT_MAX),
                img,
                ar.get("uri").and_then(Value::as_str).map(str::to_string),
            ))
        }
    }
}

struct Out {
    max: usize,
    list: Vec<EntityCandidate>,
    seen: std::collections::HashSet<String>,
}

impl Out {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        source: &str,
        kind: &str,
        lang: &str,
        title: Option<String>,
        text: String,
        url: Option<String>,
        image: Option<String>,
    ) {
        let key = dedupe_key(lang, &text);
        if normalize_name(&text).is_empty() || !self.seen.insert(key) {
            return;
        }
        self.list.push(EntityCandidate {
            id: item_content_id(lang, &text),
            source: source.into(),
            kind: kind.into(),
            lang: lang.into(),
            title,
            text,
            url,
            thumbnail: image.clone(),
            image_url: image,
            already_saved: false,
            saved_id: None,
        });
    }
}

/// Search every source for one artist (or one album when `album` is set).
pub async fn search_entity_sources(opts: &EntitySearchOptions) -> Result<EntitySearchResult> {
    let art = opts.artist.trim();
    if art.is_empty() {
        return Err(MetaError::artist_required().err());
    }
    let alb = opts
        .album
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let lang = if opts.lang.trim().eq_ignore_ascii_case("en") {
        "en"
    } else {
        "it"
    };
    let max = opts
        .max_candidates
        .unwrap_or(DEFAULT_MAX_CANDIDATES)
        .clamp(1, 40);
    let mut ctx = Ctx {
        errors: Vec::new(),
        failures: 0,
    };
    let mut out = Out {
        max,
        list: Vec::new(),
        seen: Default::default(),
    };
    let kind_main = if alb.is_some() { "desc" } else { "bio" };

    // Wikipedia: requested language, else English.
    let mut page_lang = lang;
    let mut page = wiki_find_music_page(&mut ctx, lang, art, alb).await;
    if page.is_none() && lang != "en" {
        page = wiki_find_music_page(&mut ctx, "en", art, alb).await;
        if page.is_some() {
            page_lang = "en";
        }
    }
    let mut page_thumb = None;
    let mut page_info = None;
    if let Some(p) = &page {
        page_thumb = p.thumbnail.clone();
        page_info = Some(EntityPage {
            title: p.title.clone(),
            lang: page_lang.into(),
            url: p.url.clone(),
            fallback: page_lang != lang,
        });
        out.push(
            "wikipedia",
            kind_main,
            page_lang,
            Some(p.title.clone()),
            p.extract.clone(),
            Some(p.url.clone()),
            p.thumbnail.clone(),
        );
        let sections = wiki_sections(&mut ctx, page_lang, &p.title).await;
        let mut used = std::collections::HashSet::new();
        // Leave room for the other sources.
        let section_budget = out.max.saturating_sub(4).max(2);
        for re in section_priority(page_lang) {
            if out.list.len() > section_budget {
                break;
            }
            let Some(hit) = sections
                .iter()
                .find(|s| !used.contains(&s.index) && re.is_match(&s.line))
            else {
                continue;
            };
            used.insert(hit.index.clone());
            let Some(text) = wiki_section_text(&mut ctx, page_lang, &p.title, &hit.index).await
            else {
                continue;
            };
            let kind = if TRIVIA_RE.is_match(&hit.line) {
                "trivia"
            } else {
                "section"
            };
            let url = if hit.anchor.is_empty() {
                p.url.clone()
            } else {
                format!("{}#{}", p.url, hit.anchor)
            };
            out.push(
                "wikipedia",
                kind,
                page_lang,
                Some(hit.line.clone()),
                text,
                Some(url),
                page_thumb.clone(),
            );
        }
    }

    if alb.is_none() {
        if let Some((quotes, url)) = wikiquote(&mut ctx, lang, art).await {
            let title = if lang == "it" { "Citazioni" } else { "Quotes" };
            out.push(
                "wikiquote",
                "trivia",
                lang,
                Some(title.into()),
                quotes,
                Some(url),
                page_thumb.clone(),
            );
        }
    }

    let lfm_key = opts
        .lastfm_key
        .clone()
        .or_else(|| std::env::var("LASTFM_API_KEY").ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if let Some(key) = lfm_key {
        if let Some((text, url)) = lastfm(&mut ctx, &key, lang, art, alb).await {
            out.push(
                "lastfm",
                kind_main,
                lang,
                None,
                text,
                url,
                page_thumb.clone(),
            );
        }
    }

    let adb = audiodb(&mut ctx, lang, art, alb).await;
    let adb_thumb = adb.as_ref().and_then(|(_, t, _)| t.clone());
    if let Some((Some(text), thumb, url)) = adb.clone() {
        out.push(
            "theaudiodb",
            kind_main,
            lang,
            None,
            cap_at_sentence(&text, ITEM_TEXT_MAX),
            url,
            thumb.or_else(|| page_thumb.clone()),
        );
    }
    if alb.is_none() {
        if let Some(best) = adb_thumb.or_else(|| page_thumb.clone()) {
            for c in out.list.iter_mut().filter(|c| c.image_url.is_none()) {
                c.image_url = Some(best.clone());
                c.thumbnail = Some(best.clone());
            }
        }
    }

    if let Some(token) = opts
        .discogs_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        if out.list.len() < out.max {
            if let Some((text, thumb, url)) = discogs_info(&mut ctx, token, art, alb).await {
                out.push(
                    "discogs",
                    kind_main,
                    lang,
                    Some("Discogs".into()),
                    text,
                    url,
                    thumb,
                );
            }
        }
    }

    let mut candidates = out.list;
    candidates.truncate(max);
    Ok(EntitySearchResult {
        candidates,
        errors: ctx.errors,
        page: page_info,
        lang: lang.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_wikitext_like_legacy() {
        let raw = "== Curiosità ==\n'''Salmo''' è nato a [[Olbia]].<ref>x</ref> Ha collaborato con [[Noyz Narcos|Noyz]]{{citazione necessaria}}.\n* Primo punto\n<!-- nota -->[[File:Foto.jpg|thumb]]Vedi [https://example.com il sito].";
        let s = strip_wikitext(raw);
        assert_eq!(
            s,
            "Salmo è nato a Olbia. Ha collaborato con Noyz.\n• Primo punto\nVedi il sito."
        );
    }

    #[test]
    fn music_page_detection_and_disambiguation() {
        let psalm = WikiIntro {
            title: "Salmo".into(),
            extract: "Un salmo è un componimento poetico religioso della Bibbia.".into(),
            thumbnail: None,
            url: String::new(),
            disambiguation: false,
        };
        assert!(!intro_is_music_page("it", &psalm, "Salmo", None));
        let rapper = WikiIntro {
            title: "Salmo (rapper)".into(),
            extract: "Salmo, pseudonimo di Maurizio Pisciottu, è un rapper e produttore discografico italiano.".into(),
            ..psalm.clone()
        };
        assert!(intro_is_music_page("it", &rapper, "Salmo", None));
        let disamb = WikiIntro {
            disambiguation: true,
            ..rapper.clone()
        };
        assert!(!intro_is_music_page("it", &disamb, "Salmo", None));
        assert!(title_names_entity("Salmo (rapper)", "Salmo"));
        assert!(!title_names_entity("Libro dei Salmi", "Salmo"));
        assert!(is_list_page("Discografia di Salmo"));
        assert!(is_list_page("Kid Yugi discography"));
        assert!(is_list_page("List of songs recorded by Salmo"));
        assert!(!is_list_page("Salmo (rapper)"));
        assert!(!is_list_page("Blocco 181 - Original Soundtrack"));
    }

    #[test]
    fn parses_intro_reply() {
        let j = json!({"query": {"pages": {"123": {
            "title": "Exuvia (album)", "extract": "Exuvia è l'ottavo album in studio del rapper italiano Caparezza.",
            "thumbnail": {"source": "https://upload.wikimedia.org/x.jpg"},
            "fullurl": "https://it.wikipedia.org/wiki/Exuvia_(album)"
        }}}});
        let intro = parse_wiki_intro("it", &j, "Exuvia").unwrap();
        assert_eq!(intro.title, "Exuvia (album)");
        assert_eq!(
            intro.thumbnail.as_deref(),
            Some("https://upload.wikimedia.org/x.jpg")
        );
        assert!(!intro.disambiguation);
        assert!(intro_is_music_page(
            "it",
            &intro,
            "Caparezza",
            Some("Exuvia")
        ));
        let dis = json!({"query": {"pages": {"1": {"title": "Salmo (disambigua)", "extract": "Salmo può riferirsi a: un album", "pageprops": {"disambiguation": ""}}}}});
        assert!(
            parse_wiki_intro("it", &dis, "Salmo")
                .unwrap()
                .disambiguation
        );
    }

    #[test]
    fn wikiquote_needs_a_musician_and_two_quotes() {
        let ok = "Salmo è un rapper italiano.\n== Citazioni ==\nLa musica è l'unica cosa che mi ha salvato davvero da tutto.\nNon ho mai voluto essere un modello per nessuno, solo me stesso.\nCorto";
        let q = wikiquote_quotes("it", ok).unwrap();
        assert_eq!(q.lines().count(), 2);
        assert!(q.starts_with("• La musica"));
        let psalm = "Il Libro dei Salmi è una raccolta di preghiere.\nBeato l'uomo che non segue il consiglio degli empi, dice il salmo.\nIl Signore è il mio pastore, non manco di nulla, in pascoli erbosi.";
        assert!(wikiquote_quotes("it", psalm).is_none());
    }

    #[test]
    fn lastfm_and_discogs_cleanup() {
        let raw = "Caparezza is an Italian rapper from Molfetta who released many albums over twenty years of career. <a href=\"https://www.last.fm/music/Caparezza\">Read more on Last.fm</a>. User-contributed text";
        let t = clean_lastfm_text(raw).unwrap();
        assert!(t.starts_with("Caparezza is"));
        assert!(!t.contains("Read more"));
        assert_eq!(
            clean_discogs_markup(
                "Member of [a=Club Dogo]. See [url=https://x]site[/url] [b]bold[/b] [a123]"
            ),
            "Member of Club Dogo. See site bold"
        );
    }

    #[test]
    fn ids_are_content_hashes() {
        assert_eq!(
            item_content_id("it", "Ciao, mondo!"),
            item_content_id("it", "ciao mondo")
        );
        assert_ne!(
            item_content_id("it", "a b c"),
            item_content_id("en", "a b c")
        );
        assert_eq!(dedupe_key("it", "Ciao, Mondo"), "it\u{0}ciao mondo");
    }
}
