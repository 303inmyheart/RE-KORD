//! External metadata providers (Discogs, MusicBrainz, iTunes, Deezer,
//! TheAudioDB, LRCLIB).
//!
//! Search hits are only used when they pass the similarity checks in
//! [`super::matching`] (artist AND title close to ours); a provider's first
//! row is never taken blindly. Nothing here writes to disk.

use super::discogs::{DiscogsTrackEntry, DISCOGS_APPLY_MIN_SCORE};
use super::error::{MetaError, SourceError};
use super::genres::normalize_genre_opt;
use super::http::{client, get_json};
use super::matching::{
    audiodb_track_candidates, deezer_candidates, itunes_track_candidates, match_tracklist,
    musicbrainz_artist_credit, musicbrainz_genres, musicbrainz_release_rank, musicbrainz_tracklist,
    pick_best_track, sanitize_expected_track_count, score_album_candidate, AlbumQuery, ScoredTrack,
    TrackQuery, TracklistMatch,
};
use super::text::{
    clean_album_name_for_search, core_title, normalize_name, prepare_track_title_for_meta,
};
use crate::config::AppConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// iTunes storefronts (`ITUNES_STORE_COUNTRIES`, default `it,us,gb`).
fn itunes_countries() -> Vec<String> {
    let raw = std::env::var("ITUNES_STORE_COUNTRIES").unwrap_or_default();
    let list: Vec<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|c| c.trim().to_lowercase())
        .filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_lowercase()))
        .collect();
    if list.is_empty() {
        vec!["it".into(), "us".into(), "gb".into()]
    } else {
        list
    }
}

pub(crate) fn theaudiodb_key() -> String {
    std::env::var("THEAUDIODB_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "2".into())
}

/// Extra Discogs fields stored in `albums.discogs_extra_json` / sidecar (camelCase, legacy parity).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscogsAlbumExtra {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discogs_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_no: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discogs_artist_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// `{ have, want, rating: { average, count } }`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community: Option<Value>,
    /// `{ lowestPrice, currency, numForSale, blockedFromSale }`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FetchedAlbumMeta {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub musicbrainz_release_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs_release_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs_extra: Option<DiscogsAlbumExtra>,
    /// Raw JSON blob for DB (`discogs_extra_json`); preferred over re-serializing `discogs_extra`.
    #[serde(skip)]
    pub discogs_extra_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_track_count: Option<i64>,
}

impl FetchedAlbumMeta {
    /// JSON to persist in `albums.discogs_extra_json` (raw import blob, else typed extra).
    pub fn discogs_extra_json_for_db(&self) -> Option<String> {
        if let Some(raw) = self
            .discogs_extra_json
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return Some(raw.to_string());
        }
        self.discogs_extra
            .as_ref()
            .and_then(|e| serde_json::to_string(e).ok())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FetchedTrackMeta {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscogsReleaseCandidate {
    pub release_id: i64,
    pub title: String,
    pub year: Option<String>,
    pub thumb: Option<String>,
    /// Full-size cover (artwork search).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    pub uri: Option<String>,
    pub score: i64,
    pub country: Option<String>,
    pub label: Option<String>,
}

pub(crate) fn discogs_headers(cfg: &AppConfig) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.discogs.v2.discogs+json"),
    );
    if let Some(tok) = cfg
        .discogs_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        if let Ok(v) = format!("Discogs token={tok}").parse() {
            h.insert(reqwest::header::AUTHORIZATION, v);
        }
    }
    h
}

/// True when a Discogs token is configured (search needs one).
pub fn discogs_configured(cfg: &AppConfig) -> bool {
    cfg.discogs_token
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty())
}

/// Discogs source errors → coded errors (rate limit 429, token 401).
pub(crate) fn discogs_err(e: SourceError) -> anyhow::Error {
    match e.code.as_str() {
        "discogs_rate_limited" => MetaError::discogs_rate_limited().err(),
        "discogs_unauthorized" => MetaError::discogs_unauthorized().err(),
        _ => anyhow::Error::new(e),
    }
}

fn discogs_candidates_from(
    data: &Value,
    artist: &str,
    album: &str,
) -> Vec<DiscogsReleaseCandidate> {
    let mut out = Vec::new();
    for r in data
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let id = r.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        if id <= 0 {
            continue;
        }
        let title = r
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let score = super::discogs::score_discogs_candidate(&r, artist, album).round() as i64;
        out.push(DiscogsReleaseCandidate {
            release_id: id,
            title,
            year: r.get("year").map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            }),
            thumb: r
                .get("thumb")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && !s.contains("spacer.gif"))
                .map(|s| s.to_string()),
            cover_image: r
                .get("cover_image")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && !s.contains("spacer.gif"))
                .map(|s| s.to_string()),
            uri: r.get("uri").and_then(|v| v.as_str()).map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https://www.discogs.com{s}")
                }
            }),
            score,
            country: r
                .get("country")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            label: r
                .get("label")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        });
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.score));
    out
}

/// Discogs release search by artist + album (either may be empty).
pub async fn discogs_search_releases(
    cfg: &AppConfig,
    artist: &str,
    album: &str,
) -> Result<Vec<DiscogsReleaseCandidate>> {
    let (artist, album) = (artist.trim(), album.trim());
    if artist.is_empty() && album.is_empty() {
        return Err(MetaError::query_too_short().err());
    }
    let mut q: Vec<(&str, &str)> = vec![("type", "release"), ("per_page", "15")];
    if !artist.is_empty() {
        q.push(("artist", artist));
    }
    if !album.is_empty() {
        q.push(("release_title", album));
    }
    let data = get_json(
        "discogs",
        "https://api.discogs.com/database/search",
        &q,
        Some(discogs_headers(cfg)),
    )
    .await
    .map_err(discogs_err)?;
    Ok(discogs_candidates_from(&data, artist, album))
}

/// Discogs free-text release search (artwork search).
pub async fn discogs_search_free_text(
    cfg: &AppConfig,
    query: &str,
) -> Result<Vec<DiscogsReleaseCandidate>> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Err(MetaError::query_too_short().err());
    }
    let data = get_json(
        "discogs",
        "https://api.discogs.com/database/search",
        &[("q", query), ("type", "release"), ("per_page", "10")],
        Some(discogs_headers(cfg)),
    )
    .await
    .map_err(discogs_err)?;
    Ok(discogs_candidates_from(&data, "", query))
}

/// A fetched Discogs release: album-level meta plus its tracklist.
#[derive(Debug, Clone)]
pub struct DiscogsRelease {
    pub meta: FetchedAlbumMeta,
    pub tracklist: Vec<super::discogs::DiscogsTrackEntry>,
    /// The raw `/releases/{id}` payload.
    pub raw: Value,
}

pub(crate) async fn discogs_get(cfg: &AppConfig, path: &str) -> Result<Value> {
    let v = get_json(
        "discogs",
        &format!("https://api.discogs.com{path}"),
        &[],
        Some(discogs_headers(cfg)),
    )
    .await
    .map_err(discogs_err)?;
    if v.is_null() {
        return Err(MetaError::no_metadata_found().err());
    }
    Ok(v)
}

fn str_field(v: &Value, key: &str, max: usize) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(max).collect())
}

/// Normalize a `/releases/{id}` payload (legacy `normalizeDiscogsRelease`):
/// every genre and style is kept, the tracklist is returned alongside.
pub fn discogs_release_from_json(
    data: Value,
    stats: Option<&Value>,
    release_id: i64,
    fallback_title: &str,
) -> DiscogsRelease {
    use super::discogs::{discogs_format_summary, genres_from_release, tracklist_from_release};
    let tracklist = tracklist_from_release(&data);
    let title = str_field(&data, "title", 500).unwrap_or_else(|| fallback_title.to_string());
    let discogs_uri = str_field(&data, "uri", 500)
        .or_else(|| str_field(&data, "resource_url", 500))
        .or_else(|| Some(format!("https://www.discogs.com/release/{release_id}")));
    let barcode = data
        .get("identifiers")
        .and_then(|v| v.as_array())
        .and_then(|ids| {
            ids.iter().find(|i| {
                i.get("type")
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| t.eq_ignore_ascii_case("barcode"))
            })
        })
        .and_then(|i| str_field(i, "value", 64));
    let community = data.get("community").map(|c| {
        json!({
            "have": c.get("have").cloned().unwrap_or(Value::Null),
            "want": c.get("want").cloned().unwrap_or(Value::Null),
            "rating": {
                "average": c.pointer("/rating/average").cloned().unwrap_or(Value::Null),
                "count": c.pointer("/rating/count").cloned().unwrap_or(Value::Null),
            },
        })
    });
    let marketplace = match stats {
        Some(st) => json!({
            "lowestPrice": st.pointer("/lowest_price/value").or_else(|| st.get("lowest_price")).cloned().unwrap_or(Value::Null),
            "currency": st.pointer("/lowest_price/currency").cloned().unwrap_or(Value::Null),
            "numForSale": st.get("num_for_sale").cloned().unwrap_or(Value::Null),
            "blockedFromSale": st.get("blocked_from_sale").cloned().unwrap_or(Value::Bool(false)),
        }),
        None => json!({
            "lowestPrice": data.get("lowest_price").cloned().unwrap_or(Value::Null),
            "currency": Value::Null,
            "numForSale": data.get("num_for_sale").cloned().unwrap_or(Value::Null),
            "blockedFromSale": false,
        }),
    };
    let discogs_extra = DiscogsAlbumExtra {
        master_id: data.get("master_id").and_then(|v| v.as_i64()),
        discogs_uri: discogs_uri.clone(),
        format_summary: discogs_format_summary(data.get("formats")),
        catalog_no: data
            .pointer("/labels/0/catno")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(120).collect()),
        discogs_artist_id: data.pointer("/artists/0/id").and_then(|v| v.as_i64()),
        styles: data
            .get("styles")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        barcode,
        notes: str_field(&data, "notes", 4000),
        community,
        marketplace: Some(marketplace),
    };
    // Prefer the full release date, fall back to the year.
    let release_date = str_field(&data, "released", 64).or_else(|| {
        data.get("year")
            .filter(|y| y.as_i64().is_some_and(|n| n > 0) || y.is_string())
            .map(|y| {
                y.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| y.to_string())
            })
            .filter(|s| !s.trim().is_empty() && s != "0")
    });
    let meta = FetchedAlbumMeta {
        ok: true,
        title: Some(title),
        release_date,
        genre: genres_from_release(&data),
        label: data
            .pointer("/labels/0/name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().chars().take(300).collect()),
        country: str_field(&data, "country", 64),
        source: Some("discogs".into()),
        musicbrainz_release_id: None,
        discogs_release_id: Some(release_id.to_string()),
        discogs_uri,
        discogs_extra: Some(discogs_extra),
        discogs_extra_json: None,
        expected_track_count: (!tracklist.is_empty()).then_some(tracklist.len() as i64),
    };
    DiscogsRelease {
        meta,
        tracklist,
        raw: data,
    }
}

/// Fetch and normalize one release (marketplace stats are optional).
pub async fn discogs_fetch_release(cfg: &AppConfig, release_id: i64) -> Result<DiscogsRelease> {
    if release_id < 1 {
        return Err(MetaError::invalid_release_id().err());
    }
    let data = discogs_get(cfg, &format!("/releases/{release_id}")).await?;
    let stats = discogs_get(cfg, &format!("/marketplace/stats/{release_id}"))
        .await
        .ok();
    Ok(discogs_release_from_json(
        data,
        stats.as_ref(),
        release_id,
        "",
    ))
}

/// Fetch a release and check it plausibly belongs to `artist` / `album`
/// (skipped when both are empty). Album-level meta only; see
/// [`discogs_fetch_release`] for the tracklist.
pub async fn discogs_apply_release(
    cfg: &AppConfig,
    release_id: i64,
    artist: &str,
    album: &str,
) -> Result<FetchedAlbumMeta> {
    let release = discogs_fetch_release(cfg, release_id).await?;
    check_release_matches_folder(&release, artist, album)?;
    Ok(release.meta)
}

/// Legacy folder guard (`DISCOGS_APPLY_MIN_SCORE`).
pub fn check_release_matches_folder(
    release: &DiscogsRelease,
    artist: &str,
    album: &str,
) -> Result<()> {
    if artist.is_empty() && album.is_empty() {
        return Ok(());
    }
    let score = super::discogs::score_discogs_release_for_folder(&release.raw, artist, album);
    if score < DISCOGS_APPLY_MIN_SCORE {
        return Err(MetaError::discogs_release_mismatch(score).err());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Release picking (Discogs / MusicBrainz) with similarity checks
// ---------------------------------------------------------------------------

/// "Artist - Album" (Discogs search titles) → (artist, album).
fn split_discogs_title(title: &str) -> (String, String) {
    match title.split_once(" - ") {
        Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
        None => (String::new(), title.trim().to_string()),
    }
}

/// Best Discogs release for the album (search + similarity), fetched.
async fn discogs_pick_release(
    cfg: &AppConfig,
    q: &AlbumQuery,
) -> Result<Option<(DiscogsRelease, f64)>, SourceError> {
    let cands = match discogs_search_releases(cfg, &q.artist, &q.album).await {
        Ok(c) => c,
        Err(e) => {
            let (_, code, msg) = super::error::classify(&e);
            return Err(SourceError::new("discogs", code, msg));
        }
    };
    let mut scored: Vec<(f64, &DiscogsReleaseCandidate)> = cands
        .iter()
        .take(8)
        .filter_map(|c| {
            let (a, t) = split_discogs_title(&c.title);
            let artist = if a.is_empty() { q.artist.clone() } else { a };
            score_album_candidate(q, &artist, &t, None).map(|s| (s, c))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.score.cmp(&a.1.score)));
    for (score, c) in scored.into_iter().take(2) {
        let release = match discogs_fetch_release(cfg, c.release_id).await {
            Ok(r) => r,
            Err(e) => {
                let (_, code, msg) = super::error::classify(&e);
                return Err(SourceError::new("discogs", code, msg));
            }
        };
        // Re-check with the real track count (singles vs full albums).
        let (a, t) = split_discogs_title(&c.title);
        let artist = if a.is_empty() { q.artist.clone() } else { a };
        let n = (!release.tracklist.is_empty()).then_some(release.tracklist.len() as i64);
        if score_album_candidate(q, &artist, &t, n).is_none() {
            continue;
        }
        return Ok(Some((release, score)));
    }
    Ok(None)
}

/// A MusicBrainz release with its tracklist.
#[derive(Debug, Clone)]
struct MbRelease {
    meta: FetchedAlbumMeta,
    tracklist: Vec<DiscogsTrackEntry>,
}

fn mb_release_meta(rel: &Value, info: Option<&Value>) -> MbRelease {
    let src = info.unwrap_or(rel);
    let tracklist = info.map(musicbrainz_tracklist).unwrap_or_default();
    let media_total: i64 = src
        .get("media")
        .and_then(Value::as_array)
        .map(|m| {
            m.iter()
                .filter_map(|x| x.get("track-count").and_then(Value::as_i64))
                .sum()
        })
        .unwrap_or(0);
    let count = if !tracklist.is_empty() {
        Some(tracklist.len() as i64)
    } else if media_total > 0 {
        Some(media_total)
    } else {
        rel.get("track-count").and_then(Value::as_i64)
    };
    let id = rel
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let label = src
        .pointer("/label-info/0/label/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    // The original release date (release group) over this edition's.
    let release_date = src
        .pointer("/release-group/first-release-date")
        .and_then(Value::as_str)
        .or_else(|| src.get("date").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    MbRelease {
        meta: FetchedAlbumMeta {
            ok: true,
            title: src.get("title").and_then(Value::as_str).map(str::to_string),
            release_date,
            genre: normalize_genre_opt(musicbrainz_genres(src).as_deref()),
            label,
            country: src
                .get("country")
                .and_then(Value::as_str)
                .map(str::to_string),
            source: Some("musicbrainz".into()),
            musicbrainz_release_id: (!id.is_empty()).then_some(id),
            expected_track_count: count,
            ..Default::default()
        },
        tracklist,
    }
}

async fn musicbrainz_release_by_id(id: &str) -> Result<Option<Value>, SourceError> {
    let v = get_json(
        "musicbrainz",
        &format!("https://musicbrainz.org/ws/2/release/{id}"),
        &[
            (
                "inc",
                "labels+artist-credits+recordings+genres+release-groups",
            ),
            ("fmt", "json"),
        ],
        None,
    )
    .await?;
    Ok((!v.is_null()).then_some(v))
}

/// Best MusicBrainz release for the album (search + similarity), with its
/// tracklist.
async fn musicbrainz_pick_release(q: &AlbumQuery) -> Result<Option<(MbRelease, f64)>, SourceError> {
    let artist = q.artist.replace('"', " ");
    let album = clean_album_name_for_search(&q.album).replace('"', " ");
    if album.trim().is_empty() {
        return Ok(None);
    }
    let query = if artist.trim().is_empty() {
        format!("release:\"{album}\"")
    } else {
        format!("release:\"{album}\" AND artist:\"{artist}\"")
    };
    let data = get_json(
        "musicbrainz",
        "https://musicbrainz.org/ws/2/release/",
        &[("query", query.as_str()), ("fmt", "json"), ("limit", "10")],
        None,
    )
    .await?;
    let mut scored: Vec<(f64, &Value)> = data
        .get("releases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let title = r.get("title").and_then(Value::as_str)?;
            let credit = musicbrainz_artist_credit(r);
            let n = r.get("track-count").and_then(Value::as_i64);
            score_album_candidate(q, &credit, title, n).map(|s| (s, r))
        })
        .collect();
    // Best match first; among equally good ones the official worldwide /
    // original edition, not whichever regional pressing the search lists first.
    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| musicbrainz_release_rank(a.1).cmp(&musicbrainz_release_rank(b.1)))
    });
    let Some((score, rel)) = scored.first().copied() else {
        return Ok(None);
    };
    let id = rel.get("id").and_then(Value::as_str).unwrap_or("");
    let info = if id.is_empty() {
        None
    } else {
        musicbrainz_release_by_id(id).await.ok().flatten()
    };
    Ok(Some((mb_release_meta(rel, info.as_ref()), score)))
}

async fn theaudiodb_album(
    q: &AlbumQuery,
) -> Result<Option<(FetchedAlbumMeta, Vec<DiscogsTrackEntry>, f64)>, SourceError> {
    let key = theaudiodb_key();
    let url = format!("https://www.theaudiodb.com/api/v1/json/{key}/searchalbum.php");
    let data = get_json(
        "theaudiodb",
        &url,
        &[("s", q.artist.as_str()), ("a", q.album.as_str())],
        None,
    )
    .await?;
    let rows: Vec<&Value> = match data.get("album") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::Object(_)) => vec![v],
        _ => vec![],
    };
    let best = rows
        .into_iter()
        .filter_map(|r| {
            let artist = r.get("strArtist").and_then(Value::as_str).unwrap_or("");
            let title = r.get("strAlbum").and_then(Value::as_str)?;
            score_album_candidate(q, artist, title, None).map(|s| (s, r))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0));
    let Some((score, r)) = best else {
        return Ok(None);
    };
    let year = r
        .get("intYearReleased")
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
        })
        .filter(|y| y.len() >= 4 && y[..4].chars().all(|c| c.is_ascii_digit()) && &y[..4] != "0000")
        .map(|y| y[..4].to_string());
    let meta = FetchedAlbumMeta {
        ok: true,
        title: r
            .get("strAlbum")
            .and_then(Value::as_str)
            .map(str::to_string),
        release_date: year,
        genre: normalize_genre_opt(r.get("strGenre").and_then(Value::as_str)),
        label: r
            .get("strLabel")
            .and_then(Value::as_str)
            .map(str::to_string),
        source: Some("theaudiodb".into()),
        ..Default::default()
    };
    // Tracklist (legacy attachExpectedTracksTheAudioDb).
    let mut tracklist = Vec::new();
    if let Some(id) = r.get("idAlbum").and_then(|v| {
        v.as_str()
            .map(str::to_string)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    }) {
        let turl = format!("https://www.theaudiodb.com/api/v1/json/{key}/track.php");
        if let Ok(t) = get_json("theaudiodb", &turl, &[("m", id.as_str())], None).await {
            let mut rows = audiodb_track_candidates(&t);
            rows.sort_by_key(|c| c.track_number.unwrap_or(0));
            for (i, c) in rows.into_iter().enumerate() {
                tracklist.push(DiscogsTrackEntry {
                    disc: c.disc_number.unwrap_or(1),
                    position: c.track_number.or(Some(i as i64 + 1)),
                    title: c.title,
                    duration_ms: c.duration_ms,
                });
            }
        }
    }
    Ok(Some((meta, tracklist, score)))
}

async fn itunes_album(
    q: &AlbumQuery,
) -> Result<Option<(FetchedAlbumMeta, Vec<DiscogsTrackEntry>, f64)>, SourceError> {
    let album = clean_album_name_for_search(&q.album);
    if album.is_empty() {
        return Ok(None);
    }
    let term = [album.as_str(), q.artist.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut last_err = None;
    for cc in itunes_countries().into_iter().take(3) {
        let data = match get_json(
            "itunes",
            "https://itunes.apple.com/search",
            &[
                ("term", term.as_str()),
                ("entity", "album"),
                ("limit", "15"),
                ("country", cc.as_str()),
            ],
            None,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let rows = data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if rows.is_empty() {
            continue;
        }
        let best = rows
            .iter()
            .filter_map(|r| {
                let artist = r.get("artistName").and_then(Value::as_str).unwrap_or("");
                let title = r.get("collectionName").and_then(Value::as_str)?;
                let n = r.get("trackCount").and_then(Value::as_i64);
                score_album_candidate(q, artist, title, n).map(|s| (s, r))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let Some((score, r)) = best else {
            // Same catalogue in the other storefronts: do not retry.
            return Ok(None);
        };
        let meta = FetchedAlbumMeta {
            ok: true,
            title: r
                .get("collectionName")
                .and_then(Value::as_str)
                .map(str::to_string),
            release_date: r
                .get("releaseDate")
                .and_then(Value::as_str)
                .filter(|s| s.len() >= 10)
                .map(|s| s[..10].to_string()),
            // "Music" and other stubs are dropped here, so they never stand
            // in for a real genre from another source.
            genre: normalize_genre_opt(r.get("primaryGenreName").and_then(Value::as_str)),
            source: Some("itunes".into()),
            expected_track_count: r.get("trackCount").and_then(Value::as_i64),
            ..Default::default()
        };
        let mut tracklist = Vec::new();
        if let Some(cid) = r.get("collectionId").and_then(Value::as_i64) {
            let cid = cid.to_string();
            if let Ok(l) = get_json(
                "itunes",
                "https://itunes.apple.com/lookup",
                &[
                    ("id", cid.as_str()),
                    ("entity", "song"),
                    ("limit", "200"),
                    ("country", cc.as_str()),
                ],
                None,
            )
            .await
            {
                for (i, c) in itunes_track_candidates(&l).into_iter().enumerate() {
                    tracklist.push(DiscogsTrackEntry {
                        disc: c.disc_number.unwrap_or(1),
                        position: c.track_number.or(Some(i as i64 + 1)),
                        title: c.title,
                        duration_ms: c.duration_ms,
                    });
                }
            }
        }
        return Ok(Some((meta, tracklist, score)));
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

fn merge_album(base: &mut FetchedAlbumMeta, other: FetchedAlbumMeta) {
    if base.title.is_none() {
        base.title = other.title;
    }
    if base.release_date.is_none() {
        base.release_date = other.release_date;
    }
    if base.genre.is_none() {
        base.genre = normalize_genre_opt(other.genre.as_deref());
    }
    if base.label.is_none() {
        base.label = other.label;
    }
    if base.country.is_none() {
        base.country = other.country;
    }
    if base.musicbrainz_release_id.is_none() {
        base.musicbrainz_release_id = other.musicbrainz_release_id;
    }
    if base.discogs_release_id.is_none() {
        base.discogs_release_id = other.discogs_release_id;
    }
    if base.discogs_uri.is_none() {
        base.discogs_uri = other.discogs_uri;
    }
    if base.discogs_extra.is_none() {
        base.discogs_extra = other.discogs_extra;
    }
    if base.discogs_extra_json.is_none() {
        base.discogs_extra_json = other.discogs_extra_json;
    }
    if base.expected_track_count.is_none() {
        base.expected_track_count = other.expected_track_count;
    }
    if base.source.is_none() {
        base.source = other.source;
    }
    base.ok = true;
}

/// Album titles at least this close to the folder name count as a
/// confident match (the album may then be renamed to the fetched title).
pub const ALBUM_TITLE_CONFIDENT: f64 = 0.85;

/// Album lookup result.
#[derive(Debug, Clone, Default)]
pub struct AlbumFetchOutcome {
    pub meta: FetchedAlbumMeta,
    /// Match score (0..≈1.05) of the primary source.
    pub confidence: f64,
    /// Release tracklist (for matching the files), may be empty.
    pub tracklist: Vec<DiscogsTrackEntry>,
    /// Sources that failed (the result may still be usable).
    pub errors: Vec<SourceError>,
}

/// Discogs (when a token is set), then MusicBrainz, TheAudioDB and iTunes,
/// each hit checked for artist + album similarity; missing fields are
/// merged from the later sources. The fetched track count is dropped when
/// it contradicts the folder by a lot, genres are normalized.
pub async fn fetch_album_meta_for(cfg: &AppConfig, q: &AlbumQuery) -> Result<AlbumFetchOutcome> {
    let mut out = AlbumFetchOutcome::default();
    let mut attempted = 0usize;
    if discogs_configured(cfg) {
        attempted += 1;
        match discogs_pick_release(cfg, q).await {
            Ok(Some((rel, score))) => {
                out.meta = rel.meta;
                out.meta.genre = normalize_genre_opt(out.meta.genre.as_deref());
                out.tracklist = rel.tracklist;
                out.confidence = score;
            }
            Ok(None) => {}
            Err(e) => out.errors.push(e),
        }
    }
    attempted += 1;
    match musicbrainz_pick_release(q).await {
        Ok(Some((mb, score))) => {
            if out.meta.ok {
                merge_album(&mut out.meta, mb.meta);
            } else {
                out.meta = mb.meta;
                out.confidence = score;
            }
            if out.tracklist.is_empty() {
                out.tracklist = mb.tracklist;
            }
        }
        Ok(None) => {}
        Err(e) => out.errors.push(e),
    }
    if !out.meta.ok || out.meta.genre.is_none() || out.tracklist.is_empty() {
        attempted += 1;
        match theaudiodb_album(q).await {
            Ok(Some((m, tl, score))) => {
                if out.meta.ok {
                    merge_album(&mut out.meta, m);
                } else {
                    out.meta = m;
                    out.confidence = score;
                }
                if out.tracklist.is_empty() {
                    out.tracklist = tl;
                }
            }
            Ok(None) => {}
            Err(e) => out.errors.push(e),
        }
    }
    if !out.meta.ok || out.meta.genre.is_none() || out.tracklist.is_empty() {
        attempted += 1;
        match itunes_album(q).await {
            Ok(Some((m, tl, score))) => {
                if out.meta.ok {
                    merge_album(&mut out.meta, m);
                } else {
                    out.meta = m;
                    out.confidence = score;
                }
                if out.tracklist.is_empty() {
                    out.tracklist = tl;
                }
            }
            Ok(None) => {}
            Err(e) => out.errors.push(e),
        }
    }
    if !out.meta.ok {
        if out.errors.len() >= attempted {
            if let Some(e) = out.errors.first() {
                return Err(anyhow::Error::new(e.clone()));
            }
        }
        return Err(MetaError::no_metadata_found().err());
    }
    if out.meta.expected_track_count.is_none() && !out.tracklist.is_empty() {
        out.meta.expected_track_count = Some(out.tracklist.len() as i64);
    }
    out.meta.expected_track_count =
        sanitize_expected_track_count(out.meta.expected_track_count, q.local_track_count);
    out.meta.genre = normalize_genre_opt(out.meta.genre.as_deref());
    Ok(out)
}

/// Compatibility wrapper (no local track count known).
pub async fn fetch_album_meta(
    cfg: &AppConfig,
    artist: &str,
    album: &str,
) -> Result<FetchedAlbumMeta> {
    let q = AlbumQuery {
        artist: artist.to_string(),
        album: album.to_string(),
        local_track_count: 0,
    };
    Ok(fetch_album_meta_for(cfg, &q).await?.meta)
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

/// A release tracklist used to match an album's files.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumTracklist {
    pub source: String,
    pub entries: Vec<DiscogsTrackEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Release ids already stored for the album (sidecar / DB).
#[derive(Debug, Clone, Default)]
pub struct KnownRelease {
    pub discogs_release_id: Option<i64>,
    pub musicbrainz_release_id: Option<String>,
}

/// Album tracklist for matching files: the stored Discogs / MusicBrainz
/// release first, else a checked Discogs then MusicBrainz search.
pub async fn resolve_album_tracklist(
    cfg: &AppConfig,
    q: &AlbumQuery,
    known: &KnownRelease,
) -> (Option<AlbumTracklist>, Vec<SourceError>) {
    let mut errors = Vec::new();
    let from_discogs = |r: DiscogsRelease| AlbumTracklist {
        source: "discogs".into(),
        entries: r.tracklist,
        release_date: r.meta.release_date,
        genre: normalize_genre_opt(r.meta.genre.as_deref()),
        url: r.meta.discogs_uri,
    };
    let from_mb = |r: MbRelease| AlbumTracklist {
        source: "musicbrainz".into(),
        entries: r.tracklist,
        release_date: r.meta.release_date,
        genre: None,
        url: r
            .meta
            .musicbrainz_release_id
            .map(|id| format!("https://musicbrainz.org/release/{id}")),
    };
    if q.album.trim().is_empty() {
        return (None, errors);
    }
    if discogs_configured(cfg) {
        if let Some(id) = known.discogs_release_id.filter(|n| *n > 0) {
            match discogs_fetch_release(cfg, id).await {
                Ok(r) if !r.tracklist.is_empty() => return (Some(from_discogs(r)), errors),
                Ok(_) => {}
                Err(e) => {
                    let (_, code, msg) = super::error::classify(&e);
                    errors.push(SourceError::new("discogs", code, msg));
                }
            }
        }
    }
    if let Some(id) = known
        .musicbrainz_release_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match musicbrainz_release_by_id(id).await {
            Ok(Some(info)) => {
                let r = mb_release_meta(&info, Some(&info));
                if !r.tracklist.is_empty() {
                    return (Some(from_mb(r)), errors);
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if discogs_configured(cfg) {
        match discogs_pick_release(cfg, q).await {
            Ok(Some((r, _))) if !r.tracklist.is_empty() => return (Some(from_discogs(r)), errors),
            Ok(_) => {}
            Err(e) => errors.push(e),
        }
    }
    match musicbrainz_pick_release(q).await {
        Ok(Some((r, _))) if !r.tracklist.is_empty() => return (Some(from_mb(r)), errors),
        Ok(_) => {}
        Err(e) => errors.push(e),
    }
    (None, errors)
}

/// How a track was matched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMatchOutcome {
    pub meta: FetchedTrackMeta,
    /// `"tracklist"` (album release) or `"search"` (Deezer / TheAudioDB / iTunes).
    pub strategy: &'static str,
    /// The hit belongs to the same release as the local album.
    pub album_matches: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracklist_match: Option<TracklistMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match: Option<ScoredTrack>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SourceError>,
}

/// Title to store from a hit: edition noise ("(2013 Remaster)") is dropped
/// when what is left is the title we searched for.
fn display_title(hit: &str, wanted: &str) -> String {
    let core = core_title(hit);
    if core != hit.trim() && normalize_name(&core) == normalize_name(wanted) {
        core
    } else {
        hit.trim().to_string()
    }
}

async fn deezer_search(q: &TrackQuery) -> Result<Option<ScoredTrack>, SourceError> {
    let ar = q.artist.trim();
    let tt = q.title.trim();
    let al = clean_album_name_for_search(&q.album);
    let mut queries: Vec<String> = Vec::new();
    let mut add = |s: String| {
        if s.chars().count() >= 2 && !queries.contains(&s) {
            queries.push(s);
        }
    };
    if !ar.is_empty() && !tt.is_empty() {
        add(format!("artist:\"{ar}\" track:\"{tt}\""));
    }
    let tf = q.title_from_file.trim();
    if !ar.is_empty() && !tf.is_empty() && tf != tt {
        add(format!("artist:\"{ar}\" track:\"{tf}\""));
    }
    if !ar.is_empty() && !tt.is_empty() {
        add(format!("{ar} {tt}"));
    }
    if !ar.is_empty() && !al.is_empty() && !tt.is_empty() {
        add(format!("{ar} {al} {tt}"));
    }
    let mut last_err = None;
    let mut any_ok = false;
    for query in queries.iter().take(4) {
        let data = match get_json(
            "deezer",
            "https://api.deezer.com/search/track",
            &[("q", query.as_str()), ("limit", "25")],
            None,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        any_ok = true;
        let rows = deezer_candidates(&data);
        let Some(mut best) = pick_best_track(q, &rows) else {
            continue;
        };
        // Details (track number, release date) only matter for the same release.
        if best.album_matches {
            if let Some(id) = best.candidate.id.clone() {
                if let Ok(d) = get_json(
                    "deezer",
                    &format!("https://api.deezer.com/track/{id}"),
                    &[],
                    None,
                )
                .await
                {
                    if let Some(full) = deezer_candidates(&d).into_iter().next() {
                        best.candidate.track_number = full.track_number;
                        best.candidate.disc_number = full.disc_number;
                        best.candidate.release_date =
                            full.release_date.or(best.candidate.release_date);
                    }
                }
            }
        }
        return Ok(Some(best));
    }
    match (any_ok, last_err) {
        (false, Some(e)) => Err(e),
        _ => Ok(None),
    }
}

async fn audiodb_search(q: &TrackQuery) -> Result<Option<ScoredTrack>, SourceError> {
    if q.artist.trim().is_empty() || q.title.trim().is_empty() {
        return Ok(None);
    }
    let key = theaudiodb_key();
    let url = format!("https://www.theaudiodb.com/api/v1/json/{key}/searchtrack.php");
    let data = get_json(
        "theaudiodb",
        &url,
        &[("s", q.artist.trim()), ("t", q.title.trim())],
        None,
    )
    .await?;
    Ok(pick_best_track(q, &audiodb_track_candidates(&data)))
}

async fn itunes_search(q: &TrackQuery) -> Result<Option<ScoredTrack>, SourceError> {
    let ar = q.artist.trim();
    let tt = q.title.trim();
    if tt.is_empty() {
        return Ok(None);
    }
    let al = clean_album_name_for_search(&q.album);
    let terms: Vec<String> = [
        [tt, ar, al.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        [tt, ar]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    ]
    .into_iter()
    .filter(|s| s.chars().count() >= 2)
    .collect();
    let mut last_err = None;
    let mut any_ok = false;
    'store: for cc in itunes_countries().into_iter().take(2) {
        let mut seen = Vec::new();
        for term in &terms {
            if seen.contains(term) {
                continue;
            }
            seen.push(term.clone());
            let data = match get_json(
                "itunes",
                "https://itunes.apple.com/search",
                &[
                    ("term", term.as_str()),
                    ("entity", "song"),
                    ("limit", "15"),
                    ("country", cc.as_str()),
                ],
                None,
            )
            .await
            {
                Ok(d) => d,
                Err(e) => {
                    last_err = Some(e);
                    continue 'store;
                }
            };
            any_ok = true;
            if let Some(best) = pick_best_track(q, &itunes_track_candidates(&data)) {
                return Ok(Some(best));
            }
        }
    }
    match (any_ok, last_err) {
        (false, Some(e)) => Err(e),
        _ => Ok(None),
    }
}

/// Match one track: the album tracklist first (by title, position as
/// support), then Deezer, TheAudioDB and iTunes searches whose hits must pass
/// the artist + title thresholds. Track / disc numbers and the release date
/// are only taken from a hit on the same release. `Err(no_match)` when
/// nothing passes (nothing must be written then).
pub async fn match_track_meta(
    q: &TrackQuery,
    tracklist: Option<&AlbumTracklist>,
) -> Result<TrackMatchOutcome> {
    if q.title.trim().is_empty() && q.title_from_file.trim().is_empty() {
        return Err(MetaError::missing_artist_or_title().err());
    }
    if let Some(tl) = tracklist {
        if let Some(m) = match_tracklist(q, &tl.entries) {
            let meta = FetchedTrackMeta {
                ok: true,
                title: Some(m.entry.title.clone()),
                release_date: tl.release_date.clone(),
                genre: tl.genre.clone(),
                lyrics: None,
                track_number: Some(m.entry.position.unwrap_or(m.index as i64 + 1)),
                disc_number: Some(m.entry.disc),
                source: Some(tl.source.clone()),
                url: tl.url.clone(),
                duration_ms: m.entry.duration_ms,
            };
            return Ok(TrackMatchOutcome {
                meta,
                strategy: "tracklist",
                album_matches: true,
                tracklist_match: Some(m),
                search_match: None,
                errors: Vec::new(),
            });
        }
    }
    let mut errors = Vec::new();
    let mut found = None;
    match deezer_search(q).await {
        Ok(Some(b)) => found = Some(b),
        Ok(None) => {}
        Err(e) => errors.push(e),
    }
    if found.is_none() {
        match audiodb_search(q).await {
            Ok(Some(b)) => found = Some(b),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if found.is_none() {
        match itunes_search(q).await {
            Ok(Some(b)) => found = Some(b),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    let Some(best) = found else {
        if errors.len() >= 3 {
            return Err(anyhow::Error::new(errors.remove(0)));
        }
        return Err(MetaError::no_match().err());
    };
    let c = &best.candidate;
    let same_release = best.album_matches;
    let meta = FetchedTrackMeta {
        ok: true,
        title: Some(display_title(&c.title, &q.title)),
        release_date: if same_release {
            c.release_date.clone()
        } else {
            None
        },
        genre: normalize_genre_opt(c.genre.as_deref()),
        lyrics: None,
        track_number: if same_release { c.track_number } else { None },
        disc_number: if same_release { c.disc_number } else { None },
        source: Some(c.source.clone()),
        url: c.url.clone(),
        duration_ms: c.duration_ms,
    };
    Ok(TrackMatchOutcome {
        meta,
        strategy: "search",
        album_matches: same_release,
        tracklist_match: None,
        search_match: Some(best),
        errors,
    })
}

/// Compatibility wrapper: match by artist / album / title (cleaned here).
pub async fn fetch_track_meta(
    _cfg: &AppConfig,
    artist: &str,
    album: &str,
    title: &str,
) -> Result<FetchedTrackMeta> {
    let prepared = prepare_track_title_for_meta(artist, title);
    let q = TrackQuery {
        artist: artist.to_string(),
        album: album.to_string(),
        title: prepared.clone(),
        title_from_file: prepared,
        ..Default::default()
    };
    Ok(match_track_meta(&q, None).await?.meta)
}

/// LRCLIB lyrics fetch — parity with legacy `/api/track-lyrics/fetch`.
pub async fn fetch_track_lyrics_lrclib(
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: Option<i64>,
) -> Result<(Option<String>, Option<String>)> {
    let ar = artist.trim();
    let tt = title.trim();
    let al = album.trim();
    if ar.is_empty() || tt.is_empty() {
        return Err(MetaError::missing_artist_or_title().err());
    }
    let mut url = url::Url::parse("https://lrclib.net/api/get").context("lrclib url")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("artist_name", ar);
        qp.append_pair("track_name", tt);
        if !al.is_empty() {
            qp.append_pair("album_name", al);
        }
        if let Some(ms) = duration_ms.filter(|v| *v > 0) {
            let dur_sec = ((ms as f64) / 1000.0).round().max(1.0) as i64;
            qp.append_pair("duration", &dur_sec.to_string());
        }
    }
    let client = client();
    let send = |u: url::Url| async move {
        client
            .get(u)
            .send()
            .await
            .map_err(|e| super::error::from_reqwest("lrclib", &e).err())
    };
    let mut res = send(url.clone()).await?;
    for _ in 0..2 {
        if res.status().as_u16() != 503 && res.status().as_u16() != 429 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1100)).await;
        res = send(url.clone()).await?;
    }
    if res.status().as_u16() == 404 {
        return Ok((None, None));
    }
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        let short = detail.trim().chars().take(220).collect::<String>();
        return Err(MetaError::upstream_unavailable(format!(
            "LRCLIB {}{}",
            status.as_u16(),
            if short.is_empty() {
                String::new()
            } else {
                format!(": {short}")
            }
        ))
        .err());
    }
    let j: Value = res.json().await?;
    let synced = j
        .get("syncedLyrics")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let plain = j
        .get("plainLyrics")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok((synced, plain))
}

/// Curiosità candidates as JSON (compatibility wrapper around
/// [`super::entity_search::search_entity_sources`], without Discogs).
pub async fn wikipedia_search(artist: &str, album: Option<&str>, lang: &str) -> Result<Vec<Value>> {
    let res =
        super::entity_search::search_entity_sources(&super::entity_search::EntitySearchOptions {
            artist: artist.to_string(),
            album: album.map(str::to_string),
            lang: lang.to_string(),
            discogs_token: None,
            lastfm_key: None,
            max_candidates: None,
        })
        .await?;
    Ok(res
        .candidates
        .into_iter()
        .filter_map(|c| serde_json::to_value(c).ok())
        .collect())
}
