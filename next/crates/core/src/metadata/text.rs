//! Title cleaning and fuzzy comparison shared by the metadata providers,
//! sanitize-titles and entity info (ports of legacy `server/albumInfo.mjs`
//! `cleanTrackTitleForSearch`, `prepareTrackTitleForMeta`,
//! `sanitizeLocalTrackTitleDisplay` and `entityInfo.mjs` `normalizeName`).

use super::jsre::JsRegex;
use crate::js_re;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Lowercase ASCII fold of one (already lowercased) char; `None` for chars
/// that are not letters/digits after folding.
pub fn fold_char(c: char) -> Option<char> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'æ' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ő' | 'ø' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' | 'ß' => 's',
        'ť' | 'ţ' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c if c.is_ascii_alphanumeric() => c,
        // Fullwidth digits / letters (YouTube titles): fold to ASCII.
        c @ '０'..='９' | c @ 'ａ'..='ｚ' => char::from_u32(c as u32 - 0xFEE0)?,
        _ => return None,
    })
}

/// Legacy `normalizeName`: lowercase, accents dropped, every run of other
/// characters becomes one space, trimmed.
pub fn normalize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars().flat_map(char::to_lowercase) {
        match fold_char(c) {
            Some(f) => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(f);
            }
            None => pending_space = true,
        }
    }
    out
}

/// First `max` chars of `s` (never splits a char).
pub fn take_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Legacy `capAtSentence`: cut at the last sentence end within `max` chars
/// when that keeps at least 40 % of the budget, else hard cut.
pub fn cap_at_sentence(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let slice = &chars[..max];
    let mut cut: Option<usize> = None;
    for i in 0..slice.len().saturating_sub(1) {
        if matches!(slice[i], '.' | '!' | '?') && matches!(slice[i + 1], ' ' | '\n') {
            // `.\n` and `. ` count for `.`; `!`/`?` only before a space (legacy).
            if slice[i] == '.' || slice[i + 1] == ' ' {
                cut = Some(i);
            }
        }
    }
    match cut {
        Some(c) if (c as f64) > (max as f64) * 0.4 => slice[..=c].iter().collect(),
        _ => slice.iter().collect(),
    }
}

/// Stable short content id (FNV-1a 64, two seeds → 16 hex chars).
pub fn content_hash_id(parts: &[&str]) -> String {
    fn fnv(seed: u64, parts: &[&str]) -> u64 {
        let mut h = seed;
        for p in parts {
            for b in p.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
    let a = fnv(0xcbf2_9ce4_8422_2325, parts);
    format!("{a:016x}")
}

// ---------------------------------------------------------------------------
// cleanTrackTitleForSearch / prepareTrackTitleForMeta
// ---------------------------------------------------------------------------

fn re_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if ".*+?^${}()|[]\\/-".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

static RE_SLASH_SPACES: LazyLock<JsRegex> = js_re!(r"\s*\/\s*", "g");

/// Legacy `stripRedundantArtistPrefix`.
fn strip_redundant_artist_prefix(artist: &str, title: &str) -> String {
    let ar = RE_SLASH_SPACES.replace_all(artist.trim(), " / ");
    let t = title.trim();
    if ar.chars().count() < 2 {
        return t.to_string();
    }
    let re = JsRegex::new(&format!(r"^\s*{}\s*[-–—:|]\s*", re_escape(&ar)), "i");
    re.replace(t, "").trim().to_string()
}

static RE_Q: LazyLock<JsRegex> = js_re!(r"[？?…]", "g");
static RE_SLASHES: LazyLock<JsRegex> = js_re!(r"[⧸／﹨]", "g");
static RE_AMP: LazyLock<JsRegex> = js_re!(r"[＆&]", "g");
static RE_PIPE_SPLIT: LazyLock<JsRegex> = js_re!(r"\s*[|｜]\s*", "");
static RE_DSLASH_SPLIT: LazyLock<JsRegex> = js_re!(r"\s*\/\/\s*", "");
static RE_LEAD_NUM_SEARCH: LazyLock<JsRegex> = js_re!(r"^\d{1,2}\s*[-–—.]\s*", "i");
static RE_BRACKETS: LazyLock<JsRegex> = js_re!(r"\s*[\[【]([\s\S]*?)[\]】]", "gi");
static RE_SKIT: LazyLock<JsRegex> = js_re!(r"^skit$", "i");
static RE_SEARCH_PARENS: LazyLock<JsRegex> = js_re!(
    r"\s*[\(（](?:official|lyric|hd|4k|video|audio|anime|hidden|original|re-?master|remaster|music\s*video|lyric(?:s)?\s*video|audio\s*only|visuali[sz]er|amazon(?:\s*music)?|apple(?:\s*music)?|youtub(?:e|e\s*music|e\s*topic)?|spotify|deezer|tidal|vevo|soundcloud|pandora|iheartradio|shazam|napster|full\s*album)[^)\]]*[\)）]",
    "gi"
);
static RE_DASH_VIDEO: LazyLock<JsRegex> = js_re!(
    r"\s*-\s*(?:Official\s+)?(?:Music\s+Video|Video|Audio|Lyric\s+Video|Lyrics?|Remaster)\b",
    "gi"
);
static RE_TAIL_BRACKET: LazyLock<JsRegex> = js_re!(r"\s+\[[a-z0-9\s]+\]\s*$", "i");
static RE_TAIL_BRACKET_STRIP: LazyLock<JsRegex> = js_re!(r"^\s*\[|\]\s*$", "g");
static RE_WS: LazyLock<JsRegex> = js_re!(r"\s+", "g");

/// Collapse whitespace runs (JS `\s+` → `" "`) and trim.
pub fn squash_ws(s: &str) -> String {
    RE_WS.replace_all(s, " ").trim().to_string()
}

/// Legacy `cleanTrackTitleForSearch`: makes YouTube-ish titles searchable.
pub fn clean_track_title_for_search(raw: &str, artist: &str) -> String {
    let mut s = raw.to_string();
    let ar = artist.trim();
    if ar.chars().count() >= 2 {
        s = strip_redundant_artist_prefix(ar, &s);
    }
    s = RE_Q.replace_all(&s, " ");
    s = RE_SLASHES.replace_all(&s, " ");
    s = RE_AMP.replace_all(&s, " ");
    s = RE_PIPE_SPLIT.split_first(&s).to_string();
    s = RE_DSLASH_SPLIT.split_first(&s).to_string();
    s = RE_LEAD_NUM_SEARCH.replace(&s, "");
    s = RE_BRACKETS.replace_all_with(&s, |m| {
        if RE_SKIT.is_match(m.group(1).unwrap_or_default().trim()) {
            m.text()
        } else {
            " ".into()
        }
    });
    s = RE_SEARCH_PARENS.replace_all(&s, " ");
    s = RE_DASH_VIDEO.replace_all(&s, " ");
    s = RE_TAIL_BRACKET.replace_with(&s, |m| {
        let inner = RE_TAIL_BRACKET_STRIP.replace_all(&m.text(), "");
        if RE_SKIT.is_match(inner.trim()) {
            m.text()
        } else {
            " ".into()
        }
    });
    s = squash_ws(&s);
    if s.chars().count() > 200 {
        s = take_chars(&s, 200);
    }
    s
}

/// Legacy `prepareTrackTitleForMeta`.
pub fn prepare_track_title_for_meta(artist: &str, title_from_file: &str) -> String {
    let base = title_from_file.trim();
    let cleaned = clean_track_title_for_search(base, artist.trim());
    if cleaned.is_empty() {
        base.to_string()
    } else {
        cleaned
    }
}

static RE_ALBUM_SLASH: LazyLock<JsRegex> = js_re!(r"[⧸／]", "g");

/// Legacy `cleanAlbumNameForSearch`.
pub fn clean_album_name_for_search(raw: &str) -> String {
    let s = RE_Q.replace_all(raw, " ");
    let s = RE_ALBUM_SLASH.replace_all(&s, "/");
    squash_ws(&s)
}

// ---------------------------------------------------------------------------
// sanitizeLocalTrackTitleDisplay (exact port of the legacy patterns)
// ---------------------------------------------------------------------------

static COLLAB_PATTERNS: LazyLock<Vec<JsRegex>> = LazyLock::new(|| {
    vec![
        JsRegex::new(
            r"^feat\.?|^ft[.\s]|^with\s+|\bfeatur(?:ing|e)\b|^\s*con\s+\w",
            "i",
        ),
        JsRegex::new(r#"\b(?:feat|ft)\.?\s+[A-Za-zÀ-ÿ"'‘’“”]"#, "i"),
        JsRegex::new(r#"[,&]\s*[\w"'‘’ -]+\s+(?:&|feat|and)\b"#, "i"),
    ]
});

fn is_collab_parens_content(inner: &str) -> bool {
    let t = inner.trim();
    if t.is_empty() {
        return false;
    }
    COLLAB_PATTERNS.iter().any(|re| re.is_match(t))
}

static JUNK_PAREN_PATTERNS: LazyLock<Vec<JsRegex>> = LazyLock::new(|| {
    [
        (r"\bofficial(\s*audio|\s*video|\s*music)?\b", "i"),
        (r"\boriginal(\s*mix)?\b", "i"),
        (r"\boriginal\s*version\b", "i"),
        (r"\borig\.?\b", "i"),
        (r"\bremaster(ed|ing)?\b", "i"),
        (r"\bre-?master", "i"),
        (r"\bradio\s*edit", "i"),
        (r"\bextended(\s*mix|\s*version)?\b", "i"),
        (r"\b(?:club|dub|extended|radio)\s*mix\b", "i"),
        (r"\bvisuali[sz]er\b", "i"),
        (r"\b(?:lyric|lyrics?)\s*video", "i"),
        (r"^\s*lyrics?\s*$", "i"),
        (r"^\s*music\s*video\s*$", "i"),
        (r"\b(?:4k|uhd|h\.?265|h\.?264|2160p|1080p|720p)\b", "i"),
        (r"(?:^|[^\d])\bhd\b|^\s*hd\s*$", "i"),
        (r"\bmusic\s*video\b", "i"),
        (r"\b(?:video|audio|clip)\b(?!\s*feat)", "i"),
        (r"\baudio\s*only", "i"),
        (r"\bfrom\s+the\b", "i"),
        (r"\bsoundtrack|^\s*ost\s*$", "i"),
        (r"\btrailer|teaser|preview\b", "i"),
        (r"\b(?:deluxe|explicit)\b", "i"),
        (
            r"\b(?:amazon|apple\s*music|youtub(?:e|e\s*music|e\s*topic)?|spotify|deezer|tidal|vevo|soundcloud|pandora|iheartradio|shazam|napster|bandcamp)(?:\s*music)?\b",
            "i",
        ),
        (r"^\s*mv\s*$", "i"),
        (r"\bclip\b|^\s*clip\s*$", "i"),
        (r"\bbts\b|behind\s+the\s+scenes", "i"),
        (r"\blive(\s*at|\s*acoustic|\s*in\b)", "i"),
        (r"^\s*live\s*$", "i"),
        (r"studio\s*session|piano|orchestra|unplugged|acoustic(?!a)", "i"),
        (
            r"instrumental(?!e)|karaoke|mono|stereo|lossless|high[-\s]*quality|^\s*hq\s*$",
            "i",
        ),
        (r"^\s*edit\s*$", "i"),
        // Case-sensitive in legacy: "(Robin Schulz Remix)" is kept.
        (r"\bremix\b", ""),
        (r"^\s*mix\s*$", "i"),
        (
            r"\bwork\s*print|rough\s*mix|outtake|acapella|a[\s*]cappella",
            "i",
        ),
        (r"^\s*version\s*$", "i"),
        (r"\b\d{4}\s*remaster", "i"),
        (r"cover(?:\s*ver|version)?", "i"),
        (r"re-?(?:issue|press|press(?:ing|ed)|cut)", "i"),
        (r"demo|sketch|bootleg(?!a)", "i"),
        (r"m\/v\b", "i"),
    ]
    .iter()
    .map(|(p, f)| JsRegex::new(p, f))
    .collect()
});

fn is_junk_parens_content(inner: &str) -> bool {
    if is_collab_parens_content(inner) {
        return false;
    }
    if inner.trim().is_empty() {
        return false;
    }
    JUNK_PAREN_PATTERNS.iter().any(|re| re.is_match(inner))
}

static RE_ROUND_PARENS: LazyLock<JsRegex> = js_re!(r"\s*([\(（])([^)）]+)([\)）])", "g");

fn strip_junk_round_parens(s: &str) -> String {
    let mut t = s.to_string();
    for _ in 0..15 {
        let before = t.clone();
        t = RE_ROUND_PARENS.replace_all_with(&t, |m| {
            if is_junk_parens_content(&m.group(2).unwrap_or_default()) {
                " ".into()
            } else {
                m.text()
            }
        });
        t = squash_ws(&t);
        if t == before {
            break;
        }
    }
    t
}

static RE_TOPIC: LazyLock<JsRegex> = js_re!(r"\s*-\s*topic\s*$", "i");
static RE_PIPE_TOPIC: LazyLock<JsRegex> = js_re!(r"\s*\|\s*[^|]+\s*-\s*Topic\s*$", "i");

fn strip_tail_youtube_cruft(s: &str) -> String {
    let mut t = squash_ws(s);
    t = strip_junk_round_parens(&t);
    for _ in 0..4 {
        let before = t.clone();
        t = RE_TOPIC.replace(&t, "").trim().to_string();
        t = RE_PIPE_TOPIC.replace(&t, "").trim().to_string();
        t = squash_ws(&t);
        if t == before {
            break;
        }
    }
    t
}

static RE_LEADING_THE: LazyLock<JsRegex> = js_re!(r"^The\s+", "i");

fn artist_folder_name_variants(artist_folder: &str) -> Vec<String> {
    let a = artist_folder.trim();
    if a.chars().count() < 2 {
        return vec![];
    }
    let mut out = vec![a.to_string()];
    if RE_LEADING_THE.is_match(a) {
        out.push(RE_LEADING_THE.replace(a, "").trim().to_string());
    } else {
        out.push(format!("The {a}"));
    }
    out.into_iter().filter(|x| x.chars().count() >= 2).collect()
}

fn strip_if_artist_leads_name(s: &str, artist_folder: &str) -> String {
    for v in artist_folder_name_variants(artist_folder) {
        let re = JsRegex::new(&format!(r"^\s*{}\s*[-–—|]\s*", re_escape(&v)), "i");
        if re.is_match(s) {
            return re.replace(s, "").trim().to_string();
        }
    }
    s.to_string()
}

fn strip_if_artist_trails_with_dash(s: &str, artist_folder: &str) -> String {
    for v in artist_folder_name_variants(artist_folder) {
        let re = JsRegex::new(
            &format!(r"^(.*)\s*[-–—|]\s*{}(?:\s+(\([^)]+\)))?\s*$", re_escape(&v)),
            "i",
        );
        if let Some(m) = re.find(s) {
            let left = m.group(1).unwrap_or_default().trim().to_string();
            if left.is_empty() {
                return s.to_string();
            }
            if let Some(tail) = m.group(2) {
                return squash_ws(&format!("{left} {}", tail.trim()));
            }
            return left;
        }
    }
    s.to_string()
}

static RE_SQUARE: LazyLock<JsRegex> = js_re!(r"\[[^\]]*\]", "g");
static RE_LEAD_NUM_DASH: LazyLock<JsRegex> = js_re!(r"^\d+\s*[-–—]\s*", "i");
static RE_LEAD_NUM_DOT: LazyLock<JsRegex> = js_re!(r"^\d+\s*\.\s+", "");

/// Legacy `sanitizeLocalTrackTitleDisplay`: drops `[…]`, the leading track
/// number, junk round parens (never `feat.`), `- Topic` and a redundant
/// artist prefix/suffix (`track_artist` from the sidecar wins over the
/// folder name).
pub fn sanitize_local_track_title_display(
    raw: &str,
    artist_folder: Option<&str>,
    track_artist: Option<&str>,
) -> String {
    let mut s = squash_ws(&RE_SQUARE.replace_all(raw, " "));
    s = RE_LEAD_NUM_DASH.replace(&s, "").trim().to_string();
    s = RE_LEAD_NUM_DOT.replace(&s, "").trim().to_string();
    s = squash_ws(&s);
    s = strip_tail_youtube_cruft(&s);
    let ar = track_artist
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .or(artist_folder)
        .unwrap_or("");
    s = strip_if_artist_leads_name(&s, ar);
    s = strip_if_artist_trails_with_dash(&s, ar);
    s = squash_ws(&s);
    if s.chars().count() > 200 {
        s = take_chars(&s, 200);
    }
    s
}

// ---------------------------------------------------------------------------
// Fuzzy similarity (0..=1)
// ---------------------------------------------------------------------------

fn tokens(s: &str) -> Vec<String> {
    normalize_name(s)
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Levenshtein distance on chars (small strings only).
fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Similarity of two names: 1.0 when equal after normalization, otherwise
/// the best of token overlap (Dice) and a whole-string edit ratio. Order and
/// punctuation do not matter, accents are folded.
pub fn similarity(a: &str, b: &str) -> f64 {
    let na = normalize_name(a);
    let nb = normalize_name(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let compact_a: Vec<char> = na.chars().filter(|c| *c != ' ').collect();
    let compact_b: Vec<char> = nb.chars().filter(|c| *c != ' ').collect();
    if compact_a == compact_b {
        return 0.98;
    }
    let ta: HashSet<String> = tokens(a).into_iter().collect();
    let tb: HashSet<String> = tokens(b).into_iter().collect();
    let inter = ta.intersection(&tb).count() as f64;
    let dice = if ta.is_empty() || tb.is_empty() {
        0.0
    } else {
        2.0 * inter / (ta.len() + tb.len()) as f64
    };
    let ratio = if compact_a.len() <= 80 && compact_b.len() <= 80 {
        let d = levenshtein(&compact_a, &compact_b) as f64;
        1.0 - d / compact_a.len().max(compact_b.len()) as f64
    } else {
        0.0
    };
    dice.max(ratio)
}

/// True when one normalized name contains the other as whole words
/// ("Eagles" in "The Eagles", "Jay-Z" in "Jay-Z & Kanye West").
pub fn contains_words(haystack: &str, needle: &str) -> bool {
    let h = format!(" {} ", normalize_name(haystack));
    let n = normalize_name(needle);
    !n.is_empty() && h.contains(&format!(" {n} "))
}

/// Artist similarity tolerant of credits: a leading "The", collaborations
/// and "feat." lists count as a match when the wanted artist is one of them.
pub fn artist_similarity(wanted: &str, found: &str) -> f64 {
    let strip_the = |s: &str| {
        let n = normalize_name(s);
        n.strip_prefix("the ").map(str::to_string).unwrap_or(n)
    };
    let w = strip_the(wanted);
    let f = strip_the(found);
    if w.is_empty() || f.is_empty() {
        return 0.0;
    }
    if w == f {
        return 1.0;
    }
    if contains_words(&f, &w) || contains_words(&w, &f) {
        return 0.9;
    }
    similarity(&w, &f)
}

/// Strip edition noise ("(Remastered 2013)", "- Single", "[Deluxe]") before
/// comparing titles; keeps the original when everything would go.
pub fn core_title(s: &str) -> String {
    static RE_EDITION: LazyLock<JsRegex> = js_re!(
        r"\s*[\(\[][^)\]]*(?:remaster|deluxe|edition|version|mono|stereo|explicit|clean|bonus|anniversary|expanded|single|live|mix|edit|feat|ft\.)[^)\]]*[\)\]]",
        "gi"
    );
    static RE_DASH_EDITION: LazyLock<JsRegex> = js_re!(
        r"\s+[-–—]\s+(?:single|ep|remaster(?:ed)?(?:\s+\d{4})?|\d{4}\s+remaster(?:ed)?|radio edit|mono|stereo|deluxe(?:\s+edition)?)\s*$",
        "i"
    );
    let a = RE_EDITION.replace_all(s, " ");
    let a = RE_DASH_EDITION.replace(&a, "");
    let a = squash_ws(&a);
    if normalize_name(&a).is_empty() {
        s.trim().to_string()
    } else {
        a
    }
}

/// Title similarity: the better of the raw and the edition-stripped forms.
pub fn title_similarity(wanted: &str, found: &str) -> f64 {
    let raw = similarity(wanted, found);
    let core = similarity(&core_title(wanted), &core_title(found));
    raw.max(core)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_basics() {
        assert_eq!(similarity("Twenty-One", "twenty one"), 1.0);
        assert!(similarity("Twenty-One", "Heart of Gold") < 0.3);
        assert!(artist_similarity("Eagles", "The Eagles") >= 0.9);
        assert!(artist_similarity("Eagles", "Neil Young") < 0.4);
        assert!(title_similarity("Desperado", "Desperado (2013 Remaster)") >= 0.95);
        assert!(contains_words("Jay-Z & Kanye West", "Kanye West"));
    }

    #[test]
    fn cap_at_sentence_cuts_after_full_stop() {
        let t = "One two three. Four five six. Seven eight nine ten eleven.";
        assert_eq!(cap_at_sentence(t, 40), "One two three. Four five six.");
        assert_eq!(cap_at_sentence(t, 500), t);
        // No sentence end late enough: hard cut.
        assert_eq!(cap_at_sentence("abcdefghij", 4), "abcd");
    }

    #[test]
    fn content_hash_is_stable() {
        assert_eq!(content_hash_id(&["it", "x"]), content_hash_id(&["it", "x"]));
        assert_ne!(content_hash_id(&["it", "x"]), content_hash_id(&["en", "x"]));
        assert_eq!(content_hash_id(&["a"]).len(), 16);
    }
}
