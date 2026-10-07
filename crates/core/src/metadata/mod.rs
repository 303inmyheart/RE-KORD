//! External metadata providers + album/track/artwork apply.
//!
//! Rules shared by every fetch:
//! - a provider hit is used only when it really is our track / album
//!   ([`matching`]); when nothing passes the result is `no_match` /
//!   `no_metadata_found` and nothing is written;
//! - fields a person edited are never replaced (DB `user_fields` via
//!   [`Db::apply_track_meta`] / [`Db::apply_album_meta`], sidecar
//!   `userEdited`), and a track title is only replaced while it is still the
//!   file-name title;
//! - lyrics only come from lyric sources (LRCLIB) and are saved explicitly;
//! - sidecar writes are atomic and serialized per file ([`sidecar`]).

pub mod artwork;
pub mod discogs;
pub mod entity_search;
pub mod error;
pub mod genres;
pub mod http;
pub mod jsre;
pub mod matching;
pub mod providers;
pub mod sidecar;
pub mod text;

use crate::config::AppConfig;
use crate::db::{CuratedTrackMeta, CuratedWrite, Db};
use crate::path_util::{join_under_root, safe_rel_path, under_root};
use anyhow::Result;
use error::{MetaError, SourceError};
use matching::{album_name_is_folder_name, sanitize_expected_track_count, AlbumQuery, TrackQuery};
use providers::{
    check_release_matches_folder, discogs_fetch_release, discogs_search_releases,
    fetch_album_meta_for, fetch_track_lyrics_lrclib, match_track_meta, resolve_album_tracklist,
    AlbumTracklist, DiscogsReleaseCandidate, FetchedAlbumMeta, FetchedTrackMeta, KnownRelease,
    ALBUM_TITLE_CONFIDENT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sidecar::{mark_user_edited, row_user_edited, FILE_TRACK};
use std::fs;
use std::path::{Path, PathBuf};

pub use artwork::{
    apply_artwork_url, search_artwork, search_artwork_detailed, upload_artwork, ArtworkHit,
    ArtworkSearchResult,
};
pub use text::{prepare_track_title_for_meta, sanitize_local_track_title_display};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlbumMetaPatch {
    pub title: Option<String>,
    pub release_date: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub country: Option<String>,
    pub musicbrainz_release_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetaPatch {
    pub title: Option<String>,
    pub release_date: Option<String>,
    pub genre: Option<String>,
    pub lyrics: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub source: Option<String>,
    pub url: Option<String>,
}

/// Options of an album fetch.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFetchOptions {
    /// Replace the album name with the fetched title even when it is not
    /// just the folder name (still never over a name a person typed).
    #[serde(default)]
    pub overwrite_title: bool,
}

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "wma", "webm",
];

fn is_audio_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

fn audio_files(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| is_audio_file(n))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| crate::db::nat_cmp(a, b));
    out
}

/// Number of audio tracks that belong to an album folder, including CD1/CD2
/// and bonus-disc subdirectories. Metadata matching uses this count to reject
/// singles/editions with the wrong track count, so counting only the top level
/// creates a cascade of false "no metadata" errors for multi-disc albums.
fn audio_file_count_recursive(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry
                    .file_name()
                    .to_str()
                    .is_none_or(|name| !crate::layout::is_excluded_dir(name))
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(is_audio_file)
        })
        .count()
}

fn album_dir(music_root: &Path, album_path: &str) -> Result<PathBuf> {
    let rel = safe_rel_path(album_path).map_err(|_| MetaError::invalid_path().err())?;
    if rel.is_empty() {
        return Err(MetaError::album_path_required().err());
    }
    let abs = join_under_root(music_root, &rel).map_err(|_| MetaError::invalid_path().err())?;
    if !abs.is_dir() || !under_root(&abs, music_root) {
        return Err(MetaError::album_not_found().err());
    }
    Ok(abs)
}

fn track_file(music_root: &Path, rel_path: &str) -> Result<(String, PathBuf)> {
    let rel = safe_rel_path(rel_path).map_err(|_| MetaError::invalid_path().err())?;
    let abs = join_under_root(music_root, &rel).map_err(|_| MetaError::invalid_path().err())?;
    if rel.is_empty() || !abs.is_file() || !under_root(&abs, music_root) {
        return Err(MetaError::track_not_found().err());
    }
    Ok((rel, abs))
}

fn file_stem(file_name: &str) -> &str {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if AUDIO_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) => stem,
        _ => file_name,
    }
}

/// Leading track number of a file name ("03 - x", "3. x", "103 x" → 3 on disc 1 is not guessed).
fn leading_number(stem: &str) -> Option<i64> {
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    let rest = &stem[digits.len()..];
    if !rest.is_empty() && !rest.starts_with([' ', '-', '.', '_', ')', '–', '—']) {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

/// `user_fields` bit mask of a track / album row (0 when unknown).
fn db_user_fields(db: &Db, table: &str, key_col: &str, key: &str) -> i64 {
    let sql = format!("SELECT COALESCE(user_fields, 0) FROM {table} WHERE {key_col} = ?1");
    db.with_conn(|c| {
        Ok(c.query_row(&sql, [key], |r| r.get::<_, i64>(0))
            .unwrap_or(0))
    })
    .unwrap_or(0)
}

fn skip(field: &str, reason: &str) -> Value {
    json!({ "field": field, "reason": reason })
}

// ---------------------------------------------------------------------------
// Sidecars
// ---------------------------------------------------------------------------

/// Album sidecar after a fetch: fields a person edited are kept, the title
/// only when `apply_title`, the fetched title is kept as `fetchedTitle`.
fn merge_album_sidecar(
    dir: &Path,
    meta: &FetchedAlbumMeta,
    apply_title: bool,
    tracklist: &[discogs::DiscogsTrackEntry],
    local_tracks: usize,
) -> Result<Value> {
    sidecar::mutate_album(dir, |obj| {
        let row = Value::Object(obj.clone());
        let mut set = |key: &str, v: Option<Value>| {
            if let Some(v) = v.filter(|v| !v.is_null()) {
                if !row_user_edited(&row, key) {
                    obj.insert(key.into(), v);
                }
            }
        };
        if apply_title {
            set("title", meta.title.as_ref().map(|t| json!(t)));
        }
        set("fetchedTitle", meta.title.as_ref().map(|t| json!(t)));
        set("releaseDate", meta.release_date.as_ref().map(|v| json!(v)));
        set("genre", meta.genre.as_ref().map(|v| json!(v)));
        set("label", meta.label.as_ref().map(|v| json!(v)));
        set("country", meta.country.as_ref().map(|v| json!(v)));
        set(
            "musicbrainzReleaseId",
            meta.musicbrainz_release_id.as_ref().map(|v| json!(v)),
        );
        set(
            "discogsReleaseId",
            meta.discogs_release_id.as_ref().map(|v| json!(v)),
        );
        set("discogsUri", meta.discogs_uri.as_ref().map(|v| json!(v)));
        set(
            "discogsExtra",
            meta.discogs_extra
                .as_ref()
                .and_then(|e| serde_json::to_value(e).ok()),
        );
        set("source", meta.source.as_ref().map(|v| json!(v)));
        match meta.expected_track_count {
            Some(n) => {
                obj.insert("expectedTrackCount".into(), json!(n));
            }
            None => {
                // Drop a stored count that contradicts the folder (old bad match).
                let stored = obj.get("expectedTrackCount").and_then(Value::as_i64);
                if stored.is_some() && sanitize_expected_track_count(stored, local_tracks).is_none()
                {
                    obj.remove("expectedTrackCount");
                    obj.remove("expectedTracks");
                }
            }
        }
        if !tracklist.is_empty() && meta.expected_track_count.is_some() {
            obj.insert(
                "expectedTracks".into(),
                json!(tracklist
                    .iter()
                    .map(|t| json!({"disc": t.disc, "position": t.position, "title": t.title}))
                    .collect::<Vec<_>>()),
            );
        }
        obj.insert("fetchedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
        Ok((true, Value::Object(obj.clone())))
    })
}

/// Merge one file's entry in `kord-trackinfo.json`. `user`: a person typed
/// these values (they are marked `userEdited`); otherwise fields a person
/// edited are kept.
fn merge_track_sidecar(
    album_dir: &Path,
    filename: &str,
    meta: &FetchedTrackMeta,
    user: bool,
) -> Result<()> {
    sidecar::mutate_tracks(album_dir, |obj| {
        let entry = obj.entry(filename.to_string()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        let row_snapshot = entry.clone();
        let e = entry.as_object_mut().expect("object ensured above");
        let mut written: Vec<&str> = Vec::new();
        let fields: [(&str, Value); 6] = [
            ("title", json!(meta.title)),
            ("releaseDate", json!(meta.release_date)),
            ("genre", json!(meta.genre)),
            ("lyrics", json!(meta.lyrics)),
            ("trackNumber", json!(meta.track_number)),
            ("discNumber", json!(meta.disc_number)),
        ];
        for (key, v) in fields {
            if v.is_null() || (!user && row_user_edited(&row_snapshot, key)) {
                continue;
            }
            e.insert(key.to_string(), v);
            written.push(key);
        }
        if let Some(s) = &meta.source {
            if user || !written.is_empty() {
                e.insert("source".into(), json!(s));
            }
        }
        if let Some(u) = &meta.url {
            e.insert("url".into(), json!(u));
        }
        if user && !written.is_empty() {
            mark_user_edited(e, &written);
        }
        e.insert("fetchedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
        Ok((true, ()))
    })
}

fn track_sidecar_row(album_dir: &Path, filename: &str) -> Value {
    sidecar::read_object(&sidecar::track_read_path(album_dir))
        .get(filename)
        .cloned()
        .unwrap_or_else(|| json!({}))
}

// ---------------------------------------------------------------------------
// Album
// ---------------------------------------------------------------------------

fn album_names(db: &Db, rel: &str) -> (Option<crate::db::Album>, String) {
    let folder = rel.rsplit('/').next().unwrap_or(rel).to_string();
    let album = db
        .list_albums()
        .ok()
        .and_then(|list| list.into_iter().find(|a| a.folder_key == rel));
    (album, folder)
}

/// Album fetch with the default options (title kept unless it is the folder
/// name and the match is confident).
pub async fn album_info_fetch(
    cfg: &AppConfig,
    music_root: &Path,
    db: &Db,
    album_path: &str,
    artist: Option<&str>,
    album: Option<&str>,
) -> Result<Value> {
    album_info_fetch_with(
        cfg,
        music_root,
        db,
        album_path,
        artist,
        album,
        &AlbumFetchOptions::default(),
    )
    .await
}

/// Fetch album metadata and apply it (see the module rules). The response
/// carries `meta` (what was fetched), `titleApplied`, `confidence`,
/// `expectedTracks`, `skipped` and per-source `errors`.
pub async fn album_info_fetch_with(
    cfg: &AppConfig,
    music_root: &Path,
    db: &Db,
    album_path: &str,
    artist: Option<&str>,
    album: Option<&str>,
    opts: &AlbumFetchOptions,
) -> Result<Value> {
    let dir = album_dir(music_root, album_path)?;
    let rel = safe_rel_path(album_path)?;
    let (db_album, folder) = album_names(db, &rel);
    let artist_name = artist
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            db_album
                .as_ref()
                .map(|a| a.artist_name.clone())
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| rel.split('/').next().map(str::to_string))
        .unwrap_or_default();
    let current_name = db_album
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| folder.clone());
    let album_name = album
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| current_name.clone());
    let local_tracks = audio_file_count_recursive(&dir);
    let q = AlbumQuery {
        artist: artist_name.clone(),
        album: album_name.clone(),
        local_track_count: local_tracks,
    };
    let outcome = fetch_album_meta_for(cfg, &q).await?;
    let fetched = outcome.meta.clone();

    let mut skipped = Vec::new();
    let side = sidecar::read_object(&sidecar::album_read_path(&dir));
    let user_bits = db_user_fields(db, "albums", "folder_key", &rel);
    let title_user = row_user_edited(&side, "title") || user_bits & crate::db::field::TITLE != 0;
    let fetched_title = fetched.title.clone().filter(|t| !t.trim().is_empty());
    let title_changes = fetched_title
        .as_deref()
        .is_some_and(|t| t.trim() != current_name.trim());
    let apply_title = if !title_changes {
        false
    } else if title_user {
        skipped.push(skip("title", "user_edited"));
        false
    } else if opts.overwrite_title {
        true
    } else if !album_name_is_folder_name(&current_name, &folder) {
        skipped.push(skip("title", "curated_title"));
        false
    } else if outcome.confidence < ALBUM_TITLE_CONFIDENT {
        skipped.push(skip("title", "low_confidence"));
        false
    } else {
        true
    };

    let mut to_db = fetched.clone();
    if !apply_title {
        to_db.title = None;
    }
    if let (Some(cur), Some(new)) = (
        db_album.as_ref().and_then(|a| a.genre.as_deref()),
        to_db.genre.as_deref(),
    ) {
        if !crate::db::should_replace_genre(Some(cur), Some(new)) {
            skipped.push(skip("genre", "kept_current"));
            to_db.genre = None;
        }
    }
    if fetched.expected_track_count.is_none() && local_tracks > 0 {
        skipped.push(skip("expectedTrackCount", "contradicts_local_tracks"));
    }
    let sidecar_json =
        merge_album_sidecar(&dir, &to_db, apply_title, &outcome.tracklist, local_tracks)?;
    db.apply_album_meta(&rel, &to_db)?;
    clear_contradicting_expected_count(db, &rel, local_tracks);
    Ok(json!({
        "ok": true,
        "albumPath": rel,
        "meta": fetched,
        "titleApplied": apply_title,
        "confidence": outcome.confidence,
        "expectedTracks": outcome.tracklist,
        "localTrackCount": local_tracks,
        "skipped": skipped,
        "errors": outcome.errors,
        "album": sidecar_json,
    }))
}

/// Clear `albums.expected_track_count` when it contradicts the folder
/// (e.g. 1 from a single matched to an 8-track album).
fn clear_contradicting_expected_count(db: &Db, folder_key: &str, local: usize) {
    let _ = db.with_conn(|c| {
        let cur: Option<i64> = c
            .query_row(
                "SELECT expected_track_count FROM albums WHERE folder_key = ?1",
                [folder_key],
                |r| r.get(0),
            )
            .unwrap_or(None);
        if cur.is_some() && sanitize_expected_track_count(cur, local).is_none() {
            c.execute(
                "UPDATE albums SET expected_track_count = NULL WHERE folder_key = ?1",
                [folder_key],
            )?;
        }
        Ok(())
    });
}

/// Manual album save (a person typed it: marked `userEdited`, wins in DB).
pub fn album_info_save(
    music_root: &Path,
    db: &Db,
    album_path: &str,
    patch: AlbumMetaPatch,
) -> Result<Value> {
    let dir = album_dir(music_root, album_path)?;
    let rel = safe_rel_path(album_path)?;
    let mut patch = patch;
    if let Some(g) = patch.genre.take() {
        // An explicit empty genre clears it; otherwise a cleaned token list.
        patch.genre = Some(genres::normalize_genre_string(&g).unwrap_or_default());
    }
    let j = sidecar::mutate_album(&dir, |obj| {
        let mut written = Vec::new();
        let mut set = |key: &'static str, v: &Option<String>| {
            if let Some(v) = v {
                obj.insert(key.into(), json!(v.trim()));
                written.push(key);
            }
        };
        set("title", &patch.title);
        set("releaseDate", &patch.release_date);
        set("genre", &patch.genre);
        set("label", &patch.label);
        set("country", &patch.country);
        set("musicbrainzReleaseId", &patch.musicbrainz_release_id);
        let user_fields: Vec<&str> = written
            .iter()
            .copied()
            .filter(|k| *k != "musicbrainzReleaseId")
            .collect();
        if !user_fields.is_empty() {
            mark_user_edited(obj, &user_fields);
        }
        obj.insert("editedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
        Ok((true, Value::Object(obj.clone())))
    })?;
    let meta = FetchedAlbumMeta {
        ok: true,
        title: patch.title.clone(),
        release_date: patch.release_date.clone(),
        genre: patch.genre.clone().filter(|g| !g.is_empty()),
        label: patch.label.clone(),
        country: patch.country.clone(),
        source: Some("manual".into()),
        musicbrainz_release_id: patch.musicbrainz_release_id.clone(),
        ..Default::default()
    };
    db.apply_album_meta(&rel, &meta)?;
    if let Some(g) = patch.genre.as_deref().filter(|g| !g.is_empty()) {
        db.set_album_tracks_genre(&rel, g)?;
    }
    Ok(json!({
        "albumPath": rel,
        "meta": meta,
        "album": j,
    }))
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

/// Album context shared by the tracks of one album fetch.
struct AlbumCtx {
    artist: String,
    album: String,
    tracklist: Option<AlbumTracklist>,
    errors: Vec<SourceError>,
}

async fn album_ctx(cfg: &AppConfig, music_root: &Path, db: &Db, album_rel: &str) -> AlbumCtx {
    let (db_album, folder) = album_names(db, album_rel);
    let artist = db_album
        .as_ref()
        .map(|a| a.artist_name.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| album_rel.split('/').next().unwrap_or("").to_string());
    let album = db_album
        .as_ref()
        .map(|a| a.name.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(folder);
    let mut ctx = AlbumCtx {
        artist,
        album,
        tracklist: None,
        errors: Vec::new(),
    };
    if !crate::path_util::rel_path_looks_like_album_folder(album_rel) {
        return ctx;
    }
    let Ok(dir) = album_dir(music_root, album_rel) else {
        return ctx;
    };
    let side = sidecar::read_object(&sidecar::album_read_path(&dir));
    // A stored tracklist from an earlier album fetch.
    if let Some(rows) = side.get("expectedTracks").and_then(Value::as_array) {
        let entries: Vec<discogs::DiscogsTrackEntry> = rows
            .iter()
            .filter_map(|r| {
                let title = r.get("title").and_then(Value::as_str)?.trim().to_string();
                (!title.is_empty()).then(|| discogs::DiscogsTrackEntry {
                    disc: r.get("disc").and_then(Value::as_i64).unwrap_or(1),
                    position: r.get("position").and_then(Value::as_i64),
                    title,
                    duration_ms: None,
                })
            })
            .collect();
        if !entries.is_empty() {
            ctx.tracklist = Some(AlbumTracklist {
                source: side
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("album")
                    .to_string(),
                entries,
                release_date: side
                    .get("releaseDate")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                genre: None,
                url: side
                    .get("discogsUri")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
            return ctx;
        }
    }
    let known = KnownRelease {
        discogs_release_id: side
            .get("discogsReleaseId")
            .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok())),
        musicbrainz_release_id: side
            .get("musicbrainzReleaseId")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let q = AlbumQuery {
        artist: ctx.artist.clone(),
        album: ctx.album.clone(),
        local_track_count: audio_file_count_recursive(&dir),
    };
    let (tl, errors) = resolve_album_tracklist(cfg, &q, &known).await;
    ctx.tracklist = tl;
    ctx.errors = errors;
    ctx
}

/// Is `current` still the title derived from the file name?
fn title_is_from_filename(current: &str, stem: &str, artist: &str) -> bool {
    let c = current.trim();
    if c.is_empty() || c == stem.trim() {
        return true;
    }
    let n = text::normalize_name(c);
    n == text::normalize_name(stem)
        || n == text::normalize_name(&sanitize_local_track_title_display(
            stem,
            Some(artist),
            None,
        ))
        || n == text::normalize_name(&prepare_track_title_for_meta(artist, stem))
}

async fn fetch_one_track(
    music_root: &Path,
    db: &Db,
    rel_path: &str,
    ctx: &AlbumCtx,
) -> Result<Value> {
    let (rel, abs) = track_file(music_root, rel_path)?;
    let fname = abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let stem = file_stem(&fname).to_string();
    let parent = abs.parent().map(Path::to_path_buf).unwrap_or_default();
    let track = db.track_by_rel(&rel)?;
    let row = track_sidecar_row(&parent, &fname);
    let artist = track
        .as_ref()
        .map(|t| t.artist_name.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ctx.artist.clone());
    let current_title = track
        .as_ref()
        .map(|t| t.title.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| stem.clone());
    let q = TrackQuery {
        artist: artist.clone(),
        album: ctx.album.clone(),
        title: prepare_track_title_for_meta(&artist, &current_title),
        title_from_file: prepare_track_title_for_meta(
            &artist,
            &sanitize_local_track_title_display(&stem, Some(&ctx.artist), None),
        ),
        duration_ms: track.as_ref().map(|t| t.duration_ms).filter(|d| *d > 0),
        track_number: leading_number(&stem).or(track.as_ref().and_then(|t| t.track_number)),
        disc_number: None,
    };
    let outcome = match_track_meta(&q, ctx.tracklist.as_ref()).await?;
    let fetched = outcome.meta.clone();

    let user_bits = db_user_fields(db, "tracks", "rel_path", &rel);
    let edited = |field: &str, bit: i64| row_user_edited(&row, field) || user_bits & bit != 0;
    let mut skipped = Vec::new();
    let mut applied = FetchedTrackMeta {
        ok: true,
        source: fetched.source.clone(),
        url: fetched.url.clone(),
        ..Default::default()
    };
    use crate::db::field;
    // Title: only over a file-name title, never over an edited one.
    if let Some(t) = fetched
        .title
        .clone()
        .filter(|t| t.trim() != current_title.trim())
    {
        if edited("title", field::TITLE) {
            skipped.push(skip("title", "user_edited"));
        } else if !title_is_from_filename(&current_title, &stem, &ctx.artist) {
            skipped.push(skip("title", "curated_title"));
        } else {
            applied.title = Some(t);
        }
    }
    // Track / disc numbers: only from the same release.
    if fetched.track_number.is_some() || fetched.disc_number.is_some() {
        if !outcome.album_matches {
            skipped.push(skip("trackNumber", "other_release"));
        } else if edited("trackNumber", field::TRACK_NUMBER) {
            skipped.push(skip("trackNumber", "user_edited"));
        } else {
            applied.track_number = fetched.track_number;
            if !edited("discNumber", field::DISC_NUMBER) {
                applied.disc_number = fetched.disc_number;
            }
        }
    }
    if let Some(g) = fetched.genre.clone() {
        let cur = track.as_ref().and_then(|t| t.genre.as_deref());
        if edited("genre", field::GENRE) {
            skipped.push(skip("genre", "user_edited"));
        } else if crate::db::should_replace_genre(cur, Some(&g)) {
            applied.genre = Some(g);
        } else {
            skipped.push(skip("genre", "kept_current"));
        }
    }
    if let Some(d) = fetched.release_date.clone() {
        let cur = track
            .as_ref()
            .and_then(|t| t.release_date.clone())
            .unwrap_or_default();
        if edited("releaseDate", field::RELEASE_DATE) {
            skipped.push(skip("releaseDate", "user_edited"));
        } else if cur.trim().is_empty() || (cur.len() < d.len() && d.starts_with(cur.trim())) {
            applied.release_date = Some(d);
        } else {
            skipped.push(skip("releaseDate", "kept_current"));
        }
    }
    let mut applied_fields = Vec::new();
    for (name, set) in [
        ("title", applied.title.is_some()),
        ("trackNumber", applied.track_number.is_some()),
        ("discNumber", applied.disc_number.is_some()),
        ("genre", applied.genre.is_some()),
        ("releaseDate", applied.release_date.is_some()),
    ] {
        if set {
            applied_fields.push(name);
        }
    }
    if !applied_fields.is_empty() {
        merge_track_sidecar(&parent, &fname, &applied, false)?;
        db.apply_track_meta(&rel, &applied)?;
    }
    Ok(json!({
        "ok": true,
        "matched": true,
        "relPath": rel,
        "meta": applied,
        "fetched": fetched,
        "strategy": outcome.strategy,
        "albumMatches": outcome.album_matches,
        "match": outcome.search_match.as_ref().map(|m| json!({
            "source": m.candidate.source,
            "title": m.candidate.title,
            "artist": m.candidate.artist,
            "album": m.candidate.album,
            "titleScore": m.title_score,
            "artistScore": m.artist_score,
            "albumScore": m.album_score,
            "score": m.score,
        })).or_else(|| outcome.tracklist_match.as_ref().map(|m| json!({
            "source": fetched.source,
            "title": m.entry.title,
            "position": m.entry.position,
            "disc": m.entry.disc,
            "titleScore": m.title_score,
            "positionMatches": m.position_matches,
        }))),
        "applied": applied_fields,
        "skipped": skipped,
        "errors": outcome.errors,
    }))
}

/// Fetch one track's metadata (album tracklist first, then checked
/// searches) and apply what is allowed. `Err` with code `no_match` when no
/// provider result is close enough (nothing written).
pub async fn track_info_fetch(
    cfg: &AppConfig,
    music_root: &Path,
    db: &Db,
    rel_path: &str,
) -> Result<Value> {
    let (rel, _) = track_file(music_root, rel_path)?;
    let album_rel = rel.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
    let ctx = album_ctx(cfg, music_root, db, album_rel).await;
    let mut v = fetch_one_track(music_root, db, &rel, &ctx).await?;
    if let Some(o) = v.as_object_mut() {
        if !ctx.errors.is_empty() {
            o.insert("tracklistErrors".into(), json!(ctx.errors));
        }
    }
    Ok(v)
}

/// Fetch every track of an album with one shared album context. Per-track
/// outcomes: `tracks` (matched), `noMatch` and `errors` (`{relPath, code,
/// error}`); nothing is written for unmatched tracks.
pub async fn track_info_fetch_album(
    cfg: &AppConfig,
    music_root: &Path,
    db: &Db,
    album_path: &str,
) -> Result<Value> {
    let dir = album_dir(music_root, album_path)?;
    let rel_album = safe_rel_path(album_path)?;
    let mut rels: Vec<String> = db
        .tracks_by_album_folder(&rel_album)?
        .into_iter()
        .map(|t| t.rel_path)
        .collect();
    if rels.is_empty() {
        rels = audio_files(&dir)
            .into_iter()
            .map(|f| format!("{rel_album}/{f}"))
            .collect();
    }
    let ctx = album_ctx(cfg, music_root, db, &rel_album).await;
    let (mut fetched, mut no_match, mut failed) = (0u32, 0u32, 0u32);
    let mut out_tracks = Vec::new();
    let mut errors = Vec::new();
    for rel in rels {
        match fetch_one_track(music_root, db, &rel, &ctx).await {
            Ok(v) => {
                fetched += 1;
                out_tracks.push(v);
            }
            Err(e) => {
                let (_, code, msg) = error::classify(&e);
                if code == "no_match" {
                    no_match += 1;
                } else {
                    failed += 1;
                }
                errors.push(json!({ "relPath": rel, "code": code, "error": msg }));
            }
        }
    }
    Ok(json!({
        "albumPath": rel_album,
        "fetched": fetched,
        "noMatch": no_match,
        "failed": failed,
        "tracks": out_tracks,
        "errors": errors,
        "tracklist": ctx.tracklist.as_ref().map(|t| json!({
            "source": t.source,
            "count": t.entries.len(),
        })),
        "sourceErrors": ctx.errors,
    }))
}

/// Fetch synced/plain lyrics from LRCLIB (does not auto-save; saving goes
/// through [`track_info_save`], an explicit user action).
pub async fn track_lyrics_fetch(music_root: &Path, db: &Db, rel_path: &str) -> Result<Value> {
    let (rel, abs) = track_file(music_root, rel_path)?;
    let parts: Vec<&str> = rel.split('/').collect();
    let artist_folder = parts.first().copied().unwrap_or("");
    let album_folder = if parts.len() >= 3 { parts[1] } else { "" };
    let track = db.track_by_rel(&rel)?;
    let artist = track
        .as_ref()
        .map(|t| t.artist_name.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(artist_folder);
    let title = track
        .as_ref()
        .map(|t| t.title.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            abs.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("track")
                .to_string()
        });
    let title = prepare_track_title_for_meta(artist, &title);
    let duration_ms = track.as_ref().map(|t| t.duration_ms).filter(|d| *d > 0);
    let (synced, plain) =
        fetch_track_lyrics_lrclib(artist, &title, album_folder, duration_ms).await?;
    let has_existing = track
        .as_ref()
        .and_then(|t| t.lyrics.as_deref())
        .is_some_and(|l| !l.trim().is_empty());
    Ok(json!({
        "relPath": rel,
        "syncedLyrics": synced,
        "plainLyrics": plain,
        "found": synced.is_some() || plain.is_some(),
        "hasExistingLyrics": has_existing,
    }))
}

/// Manual track save: a person typed it (DB user mode, sidecar
/// `userEdited`). Genres are cleaned; lyrics replace existing ones only here.
pub fn track_info_save(
    music_root: &Path,
    db: &Db,
    rel_path: &str,
    patch: TrackMetaPatch,
) -> Result<Value> {
    let (rel, abs) = track_file(music_root, rel_path)?;
    let genre = patch
        .genre
        .as_deref()
        .map(|g| genres::normalize_genre_string(g).unwrap_or_default());
    let meta = FetchedTrackMeta {
        ok: true,
        title: patch.title.clone(),
        release_date: patch.release_date.clone(),
        genre: genre.clone(),
        lyrics: patch.lyrics.clone(),
        track_number: patch.track_number,
        disc_number: patch.disc_number,
        source: patch.source.clone().or(Some("manual".into())),
        url: patch.url.clone(),
        duration_ms: None,
    };
    if let (Some(parent), Some(fname)) = (abs.parent(), abs.file_name().and_then(|s| s.to_str())) {
        merge_track_sidecar(parent, fname, &meta, true)?;
    }
    db.apply_curated_track(
        &rel,
        &CuratedTrackMeta {
            title: meta.title.clone(),
            release_date: meta.release_date.clone(),
            genre: genre.filter(|g| !g.is_empty()),
            track_number: meta.track_number,
            disc_number: meta.disc_number,
            lyrics: meta.lyrics.clone(),
            source: meta.source.clone(),
            url: meta.url.clone(),
            ..Default::default()
        },
        CuratedWrite::User,
    )?;
    Ok(json!({
        "ok": true,
        "relPath": rel,
        "meta": meta,
    }))
}

pub async fn discogs_search(
    cfg: &AppConfig,
    artist: &str,
    album: &str,
) -> Result<Vec<DiscogsReleaseCandidate>> {
    discogs_search_releases(cfg, artist, album).await
}

/// Apply a Discogs release to an album folder (legacy
/// `applyDiscogsReleaseToAlbum`): album meta, then every file matched to a
/// tracklist row gets track/disc number, duration (when unknown) and, for
/// files without a real title, the Discogs title.
pub async fn discogs_apply(
    cfg: &AppConfig,
    music_root: &Path,
    db: &Db,
    album_path: &str,
    release_id: i64,
    artist: Option<&str>,
    album: Option<&str>,
) -> Result<Value> {
    let dir = album_dir(music_root, album_path)?;
    let rel_album = safe_rel_path(album_path)?;
    // Like legacy: without explicit names the folder names are checked.
    let artist_name = artist
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| rel_album.split('/').next().map(str::to_string))
        .unwrap_or_default();
    let album_name = album
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| rel_album.rsplit('/').next().map(str::to_string))
        .unwrap_or_default();
    let release = discogs_fetch_release(cfg, release_id).await?;
    check_release_matches_folder(&release, &artist_name, &album_name)?;
    let local_tracks = audio_file_count_recursive(&dir);
    let mut meta = release.meta.clone();
    meta.genre = genres::normalize_genre_opt(meta.genre.as_deref());
    meta.expected_track_count =
        sanitize_expected_track_count(meta.expected_track_count, local_tracks);
    // Choosing a release explicitly is a request to use its title, unless a
    // person typed the album name.
    let side = sidecar::read_object(&sidecar::album_read_path(&dir));
    let title_user = row_user_edited(&side, "title")
        || db_user_fields(db, "albums", "folder_key", &rel_album) & crate::db::field::TITLE != 0;
    if title_user {
        meta.title = None;
    }
    let sidecar_json =
        merge_album_sidecar(&dir, &meta, !title_user, &release.tracklist, local_tracks)?;
    db.apply_album_meta(&rel_album, &meta)?;
    clear_contradicting_expected_count(db, &rel_album, local_tracks);

    let tracks = db.tracks_by_album_folder(&rel_album)?;
    let deltas = discogs::plan_discogs_track_deltas(
        &tracks,
        &artist_name,
        &release.tracklist,
        meta.discogs_uri.as_deref(),
    );
    let mut out_tracks = Vec::with_capacity(deltas.len());
    for d in &deltas {
        let mut track_meta = FetchedTrackMeta {
            ok: true,
            title: d.title.clone(),
            track_number: Some(d.track_number),
            disc_number: Some(d.disc_number),
            duration_ms: d.duration_ms,
            source: Some(d.source.clone()),
            url: d.url.clone(),
            ..Default::default()
        };
        if let Ok(abs) = join_under_root(music_root, &d.rel_path) {
            if let (Some(parent), Some(fname)) =
                (abs.parent(), abs.file_name().and_then(|n| n.to_str()))
            {
                let row = track_sidecar_row(parent, fname);
                if row_user_edited(&row, "title") {
                    track_meta.title = None;
                }
                if let Err(e) = merge_track_sidecar(parent, fname, &track_meta, false) {
                    tracing::warn!(error = %e, rel = %d.rel_path, "discogs track sidecar not written");
                }
            }
        }
        db.apply_discogs_track_meta(&d.rel_path, &track_meta)?;
        out_tracks.push(json!({
            "relPath": d.rel_path,
            "meta": {
                "trackNumber": d.track_number,
                "discNumber": d.disc_number,
                "title": track_meta.title,
                "source": d.source,
                "url": d.url,
                "durationMs": d.duration_ms,
            },
        }));
    }
    Ok(json!({
        "ok": true,
        "albumPath": rel_album,
        "meta": meta,
        "album": sidecar_json,
        "expectedTracks": release.tracklist,
        "tracks": out_tracks,
        "trackDeltas": deltas,
    }))
}

// ---------------------------------------------------------------------------
// Sanitize titles / prune
// ---------------------------------------------------------------------------

/// Preview/apply sanitized titles for one album or the whole library
/// (legacy `sanitizeTrackTitlesInAlbumDir`). Titles a person edited are
/// left alone (`skipped`). Changes are sorted by album, then track order.
/// Blocking (filesystem + DB): run it on the blocking pool.
pub fn sanitize_track_titles(
    music_root: &Path,
    db: &Db,
    scope: &str,
    album_path: Option<&str>,
    dry_run: bool,
) -> Result<Value> {
    let mut albums: Vec<String> = if scope == "all" {
        db.list_albums()?
            .into_iter()
            .filter(|a| !a.loose)
            .map(|a| a.folder_key)
            .filter(|k| !k.is_empty())
            .collect()
    } else {
        let p = album_path.unwrap_or("").trim();
        if p.is_empty() {
            return Err(MetaError::album_path_required().err());
        }
        vec![safe_rel_path(p).map_err(|_| MetaError::invalid_path().err())?]
    };
    albums.sort_by(|a, b| crate::db::nat_cmp(&a.to_lowercase(), &b.to_lowercase()));
    albums.dedup();

    let mut changes = Vec::new();
    let mut skipped = Vec::new();
    for album_rel in &albums {
        let dir = match album_dir(music_root, album_rel) {
            Ok(d) => d,
            Err(e) if scope != "all" => return Err(e),
            Err(_) => continue,
        };
        let artist_folder = album_rel.split('/').next().unwrap_or("");
        let side = sidecar::read_object(&sidecar::track_read_path(&dir));
        // Track order: DB order (disc, track number, name), then file name.
        let db_tracks = db.tracks_by_album_folder(album_rel).unwrap_or_default();
        let order: std::collections::HashMap<String, usize> = db_tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.rel_path.clone(), i))
            .collect();
        let mut files = audio_files(&dir);
        files.sort_by(|a, b| {
            let ra = order
                .get(&format!("{album_rel}/{a}"))
                .copied()
                .unwrap_or(usize::MAX);
            let rb = order
                .get(&format!("{album_rel}/{b}"))
                .copied()
                .unwrap_or(usize::MAX);
            ra.cmp(&rb).then_with(|| crate::db::nat_cmp(a, b))
        });
        for fname in files {
            let base = file_stem(&fname).trim().to_string();
            let base = if base.is_empty() { fname.clone() } else { base };
            let row = side.get(&fname).cloned().unwrap_or_else(|| json!({}));
            let track_artist = row
                .get("artist")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let to = sanitize_local_track_title_display(
                &base,
                Some(artist_folder),
                track_artist.as_deref(),
            );
            if to == base || to.is_empty() {
                continue;
            }
            let rel = format!("{album_rel}/{fname}");
            let current = db_tracks
                .iter()
                .find(|t| t.rel_path == rel)
                .map(|t| t.title.clone());
            let user_title = row_user_edited(&row, "title")
                || db_user_fields(db, "tracks", "rel_path", &rel) & crate::db::field::TITLE != 0;
            if user_title {
                skipped.push(json!({
                    "albumRel": album_rel,
                    "fileName": fname,
                    "current": current,
                    "reason": "user_edited",
                }));
                continue;
            }
            changes.push(json!({
                "albumRel": album_rel,
                "albumPath": album_rel,
                "fileName": fname,
                "from": base,
                "to": to,
                "current": current,
            }));
            if !dry_run {
                let meta = FetchedTrackMeta {
                    ok: true,
                    title: Some(to.clone()),
                    ..Default::default()
                };
                merge_track_sidecar(&dir, &fname, &meta, false)?;
                db.apply_curated_track(
                    &rel,
                    &CuratedTrackMeta {
                        title: Some(to),
                        ..Default::default()
                    },
                    CuratedWrite::Override,
                )?;
            }
        }
    }

    Ok(json!({
        "changes": changes,
        "skipped": skipped,
        "albumsScanned": albums.len(),
        "dryRun": dry_run,
        "written": !dry_run && !changes.is_empty(),
        "albumPath": album_path.unwrap_or(""),
    }))
}

/// Sources whose ordering fields (track / disc numbers, expected tracks)
/// are real metadata, never stale leftovers.
fn is_fetched_source(row: &Value) -> bool {
    let src = row.get("source").and_then(Value::as_str).unwrap_or("");
    !src.is_empty() || row.get("fetchedAt").is_some() || row.get("userEdited").is_some()
}

/// Remove orphan keys from `kord-trackinfo.json` (legacy prune). Ordering
/// fields are cleared only on rows that are legacy leftovers: numbers that
/// came from a fetch, a Discogs apply or a person are kept.
pub fn prune_album_library_metadata(music_root: &Path, album_path: &str) -> Result<Value> {
    let dir = album_dir(music_root, album_path)?;
    let audio_names: std::collections::HashSet<String> = audio_files(&dir).into_iter().collect();

    let track_path = dir.join(FILE_TRACK);
    let (orphan_removed, track_ordering_cleared, trimmed) =
        if sidecar::track_read_path(&dir).is_file() {
            sidecar::mutate_tracks(&dir, |obj| {
                let mut removed = Vec::new();
                let keys: Vec<String> = obj.keys().cloned().collect();
                for k in keys {
                    if !audio_names.contains(&k) {
                        obj.remove(&k);
                        removed.push(k);
                    }
                }
                let mut cleared = 0u32;
                for (_k, row) in obj.iter_mut() {
                    if is_fetched_source(row) {
                        continue;
                    }
                    if let Some(r) = row.as_object_mut() {
                        let mut touched = false;
                        for key in ["trackNumber", "discNumber"] {
                            if r.get(key).is_some_and(|v| !v.is_null()) {
                                r.remove(key);
                                touched = true;
                            }
                        }
                        if touched {
                            cleared += 1;
                        }
                    }
                }
                let write = !removed.is_empty() || cleared > 0;
                Ok((write, (removed, cleared, u32::from(write))))
            })?
        } else {
            (Vec::new(), 0, 0)
        };
    let _ = track_path;

    let mut expected_tracks_cleared = false;
    if sidecar::album_read_path(&dir).is_file() {
        expected_tracks_cleared = sidecar::mutate_album(&dir, |obj| {
            let row = Value::Object(obj.clone());
            if is_fetched_source(&row) {
                return Ok((false, false));
            }
            let mut touched = false;
            for k in ["expectedTrackCount", "expectedTracks"] {
                if obj.remove(k).is_some() {
                    touched = true;
                }
            }
            Ok((touched, touched))
        })?;
    }

    let written =
        !orphan_removed.is_empty() || expected_tracks_cleared || track_ordering_cleared > 0;
    Ok(json!({
        "albumPath": safe_rel_path(album_path)?,
        "removed": orphan_removed,
        "orphanTrackKeysRemoved": orphan_removed,
        "written": written,
        "expectedTracksCleared": expected_tracks_cleared,
        "trackOrderingFieldsCleared": track_ordering_cleared,
        "albumFieldsMerged": 0,
        "tracksMerged": 0,
        "jsonFilesRemoved": 0,
        "jsonFilesTrimmed": trimmed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_numbers() {
        assert_eq!(leading_number("03 - Twenty-One"), Some(3));
        assert_eq!(leading_number("3. Song"), Some(3));
        assert_eq!(leading_number("1999"), None);
        assert_eq!(leading_number("21st Century"), None);
        assert_eq!(leading_number("Song"), None);
    }

    #[test]
    fn recursive_album_track_count_includes_disc_subfolders() {
        let root = std::env::temp_dir().join(format!(
            "rekord-meta-count-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("CD1")).unwrap();
        fs::create_dir_all(root.join("CD2/Bonus")).unwrap();
        fs::write(root.join("01.flac"), b"x").unwrap();
        fs::write(root.join("CD1/02.flac"), b"x").unwrap();
        fs::write(root.join("CD2/Bonus/03.mp3"), b"x").unwrap();
        fs::write(root.join("CD2/cover.jpg"), b"x").unwrap();
        assert_eq!(audio_file_count_recursive(&root), 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn filename_titles() {
        assert!(title_is_from_filename(
            "03 - Twenty-One",
            "03 - Twenty-One",
            "Eagles"
        ));
        assert!(title_is_from_filename(
            "Twenty-One",
            "03 - Twenty-One",
            "Eagles"
        ));
        assert!(title_is_from_filename("", "x", "Eagles"));
        assert!(!title_is_from_filename(
            "Heart of Gold",
            "03 - Twenty-One",
            "Eagles"
        ));
    }
}
