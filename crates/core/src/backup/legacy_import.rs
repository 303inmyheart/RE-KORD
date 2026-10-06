//! Import from a legacy library: studio metadata (sidecars, `.kord/rekord.db`)
//! and per-account personal data (`.kord/{account}_info`).

use super::*;
use std::collections::BTreeSet;

pub(super) fn playlists_from_legacy_user_state(
    raw: &str,
) -> Result<(Vec<String>, Vec<PlaylistBackup>)> {
    let v: serde_json::Value = serde_json::from_str(raw)?;
    let favorites = v
        .get("favorites")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut playlists = Vec::new();
    if let Some(arr) = v.get("playlists").and_then(|x| x.as_array()) {
        for pl in arr {
            let id = pl
                .get("id")
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let name = pl
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("Playlist")
                .to_string();
            let mut tracks = Vec::new();
            if let Some(ts) = pl.get("tracks").and_then(|x| x.as_array()) {
                for t in ts {
                    let rel = t
                        .get("relPath")
                        .or_else(|| t.get("rel_path"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    if rel.is_empty() {
                        continue;
                    }
                    tracks.push(PlaylistBackupTrack {
                        rel_path: rel,
                        title: t
                            .get("title")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        artist_name: t
                            .get("artist")
                            .or_else(|| t.get("artist_name"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        album_name: t
                            .get("album")
                            .or_else(|| t.get("album_name"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    });
                }
            }
            playlists.push(PlaylistBackup { id, name, tracks });
        }
    }
    Ok((favorites, playlists))
}

pub(super) fn account_id_from_info_dir_name(name: &str) -> Option<String> {
    let id = name.strip_suffix("_info")?;
    if id.is_empty() || id == "global" {
        return None;
    }
    Some(id.to_string())
}

pub(super) fn legacy_album_key_to_folder(key: &str) -> String {
    if key.contains("::") {
        key.replacen("::", "/", 1).replace('\\', "/")
    } else {
        key.replace('\\', "/")
    }
}

pub(super) fn remap_legacy_excluded_albums(
    state: &mut UserStateV1,
    album_folder_to_id: &BTreeMap<String, i64>,
) {
    let Some(keys_val) = state.settings.remove("legacyExcludedAlbumKeys") else {
        return;
    };
    let Some(arr) = keys_val.as_array() else {
        return;
    };
    let mut seen: std::collections::HashSet<i64> =
        state.excluded_album_ids.iter().copied().collect();
    for item in arr {
        let Some(key) = item.as_str() else { continue };
        let folder = legacy_album_key_to_folder(key);
        if let Some(id) = album_folder_to_id.get(&folder) {
            if seen.insert(*id) {
                state.excluded_album_ids.push(*id);
            }
        }
    }
}

pub(super) fn json_str_field(obj: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = obj.get(*k).and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

pub(super) fn json_i64_field(obj: &Value, keys: &[&str]) -> Option<i64> {
    for k in keys {
        let Some(v) = obj.get(*k) else { continue };
        if let Some(n) = v.as_i64() {
            return Some(n);
        }
        if let Some(n) = v.as_u64() {
            return Some(n as i64);
        }
        if let Some(s) = v.as_str() {
            if let Ok(n) = s.trim().parse::<i64>() {
                return Some(n);
            }
        }
    }
    None
}

/// Drop 1-char / short-numeric stubs that are not real studio metadata.
pub(super) fn scrub_studio_str(v: Option<String>) -> Option<String> {
    let s = v?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if s.len() == 1 {
        return None;
    }
    if s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(s)
}

/// Like [`scrub_studio_str`], also drops generic ID3 genre stubs (`Music`, `Unknown`, …).
pub(super) fn scrub_studio_genre(v: Option<String>) -> Option<String> {
    let s = scrub_studio_str(v)?;
    if crate::db::is_weak_genre(Some(&s)) {
        None
    } else {
        Some(s)
    }
}

pub(super) fn album_meta_from_sidecar_json(json: &Value) -> Option<FetchedAlbumMeta> {
    if !json.is_object() {
        return None;
    }
    let title = json_str_field(json, &["title"]);
    let release_date = scrub_studio_str(json_str_field(
        json,
        &["releaseDate", "release_date", "date"],
    ));
    let genre = scrub_studio_genre(json_str_field(json, &["genre"]));
    let label = scrub_studio_str(json_str_field(json, &["label"]));
    let country = json_str_field(json, &["country"]).and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let source = json_str_field(json, &["source"]);
    let musicbrainz_release_id =
        json_str_field(json, &["musicbrainzReleaseId", "musicbrainz_release_id"]);
    let discogs_release_id = json_i64_field(json, &["discogsReleaseId", "discogs_release_id"])
        .map(|n| n.to_string())
        .or_else(|| json_str_field(json, &["discogsReleaseId", "discogs_release_id"]));
    let discogs_extra_value = json
        .get("discogsExtra")
        .or_else(|| json.get("discogs_extra"))
        .filter(|v| v.is_object())
        .cloned();
    let discogs_extra = discogs_extra_value
        .as_ref()
        .and_then(|v| serde_json::from_value::<DiscogsAlbumExtra>(v.clone()).ok())
        .filter(|e| {
            e.master_id.is_some()
                || e.discogs_uri.as_ref().is_some_and(|s| !s.trim().is_empty())
                || e.format_summary
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty())
                || e.catalog_no.as_ref().is_some_and(|s| !s.trim().is_empty())
        });
    let discogs_extra_json = discogs_extra_value.and_then(|v| serde_json::to_string(&v).ok());
    let discogs_uri = json_str_field(json, &["discogsUri", "discogs_uri"]).or_else(|| {
        discogs_extra
            .as_ref()
            .and_then(|e| e.discogs_uri.clone())
            .filter(|s| !s.trim().is_empty())
    });
    if title.is_none()
        && release_date.is_none()
        && genre.is_none()
        && label.is_none()
        && country.is_none()
        && musicbrainz_release_id.is_none()
        && discogs_release_id.is_none()
        && discogs_uri.is_none()
        && discogs_extra.is_none()
    {
        return None;
    }
    Some(FetchedAlbumMeta {
        ok: true,
        title,
        release_date,
        genre,
        label,
        country,
        source,
        musicbrainz_release_id,
        discogs_release_id,
        discogs_uri,
        discogs_extra,
        discogs_extra_json,
        expected_track_count: None,
    })
}

pub(super) fn track_meta_from_sidecar_json(json: &Value) -> Option<FetchedTrackMeta> {
    if !json.is_object() {
        return None;
    }
    let title = json_str_field(json, &["title"]);
    let release_date = scrub_studio_str(json_str_field(
        json,
        &["releaseDate", "release_date", "date"],
    ));
    let genre = scrub_studio_genre(json_str_field(json, &["genre"]));
    let lyrics = json_str_field(json, &["lyrics"]);
    let source = json_str_field(json, &["source"]);
    let url = json_str_field(json, &["url"]);
    let duration_ms = json_i64_field(json, &["durationMs", "duration_ms"]);
    if title.is_none()
        && release_date.is_none()
        && genre.is_none()
        && lyrics.is_none()
        && source.is_none()
        && url.is_none()
    {
        return None;
    }
    Some(FetchedTrackMeta {
        ok: true,
        title,
        release_date,
        genre,
        lyrics,
        track_number: None,
        disc_number: None,
        source,
        url,
        duration_ms,
    })
}

fn file_stem_of(rel_or_name: &str) -> String {
    let name = rel_or_name.rsplit('/').next().unwrap_or(rel_or_name);
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .trim()
        .to_string()
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Legacy epoch milliseconds → RFC3339 (the hub's timestamp format).
fn millis_to_rfc3339(ms: Option<f64>) -> Option<String> {
    let ms = ms.filter(|m| m.is_finite() && *m > 0.0)? as i64;
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// Curated album values of a sidecar (`kord-albuminfo.json`). A title that
/// only repeats the folder name is not a curated title.
pub(super) fn curated_album_from_sidecar(
    json: &Value,
    folder_key: &str,
) -> Option<CuratedAlbumMeta> {
    let meta = album_meta_from_sidecar_json(json)?;
    let folder_name = folder_key.rsplit('/').next().unwrap_or(folder_key);
    Some(CuratedAlbumMeta {
        title: meta.title.filter(|t| t.trim() != folder_name.trim()),
        release_date: meta.release_date,
        genre: meta.genre,
        label: meta.label,
        country: meta.country,
        musicbrainz_release_id: meta.musicbrainz_release_id,
        discogs_release_id: meta.discogs_release_id,
        discogs_extra_json: meta.discogs_extra_json,
        expected_track_count: json_i64_field(json, &["expectedTrackCount", "expected_track_count"])
            .or_else(|| {
                json.get("expectedTracks")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len() as i64)
                    .filter(|n| *n > 0)
            }),
        user_mask: sidecar_user_mask(json),
        ..Default::default()
    })
}

/// `db::field` bits of the fields a sidecar row marks as typed by a person
/// (`userEdited`, or a manual save).
fn sidecar_user_mask(row: &Value) -> i64 {
    use crate::db::field;
    use crate::metadata::sidecar::row_user_edited;
    [
        ("title", field::TITLE),
        ("releaseDate", field::RELEASE_DATE),
        ("genre", field::GENRE),
        ("trackNumber", field::TRACK_NUMBER),
        ("discNumber", field::DISC_NUMBER),
    ]
    .iter()
    .filter(|(key, _)| row_user_edited(row, key))
    .fold(0, |mask, (_, bit)| mask | bit)
}

/// Curated track values of one `kord-trackinfo.json` entry.
pub(super) fn curated_track_from_sidecar(
    json: &Value,
    file_name: &str,
) -> Option<CuratedTrackMeta> {
    let numbers = (
        json_i64_field(json, &["trackNumber", "track_number"]),
        json_i64_field(json, &["discNumber", "disc_number"]),
    );
    let meta = track_meta_from_sidecar_json(json);
    if meta.is_none() && numbers == (None, None) {
        return None;
    }
    let stem = file_stem_of(file_name);
    let meta = meta.unwrap_or_default();
    Some(CuratedTrackMeta {
        title: meta.title.filter(|t| t.trim() != stem),
        release_date: meta.release_date,
        genre: meta.genre,
        track_number: numbers.0,
        disc_number: numbers.1,
        lyrics: meta.lyrics,
        source: meta.source,
        url: meta.url,
        duration_ms: meta.duration_ms,
        user_mask: sidecar_user_mask(json),
        ..Default::default()
    })
}

/// Import album/track studio metadata from library sidecar JSON files
/// (parity legacy `importLegacyAlbumMetaToDb` / `importLegacyTrackMetaMapToDb`).
/// Only fields nobody curated yet are written, so this is cheap and safe
/// after every scan, and never undoes a Studio edit or the legacy import.
pub fn import_sidecar_metadata(db: &Db, music_root: &Path) -> Result<(u32, u32)> {
    if !music_root.is_dir() {
        return Ok((0, 0));
    }
    let mut albums: Vec<(String, CuratedAlbumMeta)> = Vec::new();
    let mut tracks: Vec<(String, CuratedTrackMeta)> = Vec::new();
    for entry in WalkDir::new(music_root)
        .into_iter()
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some(".kord" | ".rekord" | ".wpp" | "node_modules" | ".git")
            )
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let name = entry.file_name().to_string_lossy();
        let is_album = matches!(name.as_ref(), "kord-albuminfo.json" | "wpp-albuminfo.json");
        let is_track = matches!(name.as_ref(), "kord-trackinfo.json" | "wpp-trackinfo.json");
        if !is_album && !is_track {
            continue;
        }
        let abs = entry.path();
        let Some(parent) = abs.parent() else {
            continue;
        };
        let Ok(folder_rel) = parent.strip_prefix(music_root) else {
            continue;
        };
        let folder_key = folder_rel.to_string_lossy().replace('\\', "/");
        let Ok(raw) = fs::read_to_string(abs) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if is_album {
            if let Some(meta) = curated_album_from_sidecar(&json, &folder_key) {
                albums.push((folder_key, meta));
            }
        } else if let Some(map) = json.as_object() {
            for (file_name, meta_v) in map {
                let Some(meta) = curated_track_from_sidecar(meta_v, file_name) else {
                    continue;
                };
                let Ok(Some(rel_path)) = db.resolve_track_rel_in_album(&folder_key, file_name)
                else {
                    continue;
                };
                tracks.push((rel_path, meta));
            }
        }
    }
    let (a, t) = db.apply_curated_batch(&albums, &tracks, CuratedWrite::FillUncurated)?;
    if a > 0 || t > 0 {
        db.rebuild_genres()?;
        db.rebuild_fts()?;
    }
    Ok((a, t))
}

fn table_has_column(conn: &Connection, table: &str, col: &str) -> bool {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .ok()
        .map(|mut s| {
            s.query_map([], |r| r.get::<_, String>(1))
                .map(|rows| rows.flatten().any(|name| name == col))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Curated metadata from a legacy `.kord/rekord.db`: album titles that differ
/// from the folder, full release dates, genres, labels, track titles that
/// differ from the file name, track / disc numbers that are not the file
/// name's, lyrics, sources, `user_edited` flags and added / updated times.
/// Legacy values replace tag-derived ones (they are what legacy showed), but
/// never what a person typed in next.
pub fn import_legacy_library_db_metadata(db: &Db, legacy_db_path: &Path) -> Result<(u32, u32)> {
    if !legacy_db_path.is_file() {
        return Ok((0, 0));
    }
    let legacy = Connection::open_with_flags(legacy_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open legacy db {}", legacy_db_path.display()))?;
    let col = |table: &str, name: &str, fallback: &str| -> String {
        if table_has_column(&legacy, table, name) {
            name.to_string()
        } else {
            format!("{fallback} AS {name}")
        }
    };

    let mut albums: Vec<(String, CuratedAlbumMeta)> = Vec::new();
    {
        let sql = format!(
            "SELECT folder_rel_path, {}, {}, release_date, genre, label, country, \
                    musicbrainz_release_id, {}, {}, {}, {}, {}, {} FROM albums",
            col("albums", "name", "NULL"),
            col("albums", "title", "NULL"),
            col("albums", "discogs_release_id", "NULL"),
            col("albums", "expected_track_count", "NULL"),
            col("albums", "discogs_extra_json", "NULL"),
            col("albums", "user_edited", "0"),
            col("albums", "added_at", "NULL"),
            col("albums", "updated_at", "NULL"),
        );
        let mut stmt = legacy.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<i64>>(11)?,
                r.get::<_, Option<f64>>(12)?,
                r.get::<_, Option<f64>>(13)?,
            ))
        })?;
        for row in rows.flatten() {
            let (
                folder,
                name,
                title,
                release_date,
                genre,
                label,
                country,
                mb_id,
                discogs_id,
                expected,
                extra,
                user_edited,
                added_at,
                updated_at,
            ) = row;
            let folder_key = folder.replace('\\', "/");
            let folder_name = folder_key
                .rsplit('/')
                .next()
                .unwrap_or(&folder_key)
                .to_string();
            let shown = non_empty(title).or(non_empty(name));
            albums.push((
                folder_key,
                CuratedAlbumMeta {
                    title: shown.filter(|t| t.trim() != folder_name.trim()),
                    release_date: scrub_studio_str(release_date),
                    genre: scrub_studio_genre(genre),
                    label: scrub_studio_str(label),
                    country: non_empty(country),
                    musicbrainz_release_id: non_empty(mb_id),
                    discogs_release_id: discogs_id.map(|n| n.to_string()),
                    discogs_extra_json: non_empty(extra),
                    expected_track_count: expected,
                    added_at: millis_to_rfc3339(added_at),
                    updated_at: millis_to_rfc3339(updated_at),
                    user_edited: user_edited.unwrap_or(0) != 0,
                    user_mask: 0,
                },
            ));
        }
    }

    let mut tracks: Vec<(String, CuratedTrackMeta)> = Vec::new();
    {
        let sql = format!(
            "SELECT rel_path, title, genre, release_date, lyrics, source, url, {}, {}, {}, {} FROM tracks",
            col("tracks", "track_number", "NULL"),
            col("tracks", "disc_number", "NULL"),
            col("tracks", "user_edited", "0"),
            col("tracks", "added_at", "NULL"),
        );
        let mut stmt = legacy.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<f64>>(10)?,
            ))
        })?;
        for row in rows.flatten() {
            let (
                rel_path,
                title,
                genre,
                release_date,
                lyrics,
                source,
                url,
                track_number,
                disc_number,
                user_edited,
                added_at,
            ) = row;
            let rel = normalize_import_rel_path(&rel_path);
            let stem = file_stem_of(&rel);
            let user_edited = user_edited.unwrap_or(0) != 0;
            // Legacy guessed numbers from the file name too: only differing
            // (or hand-edited) numbers are curated.
            let (guess_disc, guess_track) = crate::db::text::guess_track_numbers(&stem);
            let track_number =
                track_number.filter(|n| *n > 0 && (user_edited || Some(*n) != guess_track));
            let disc_number =
                disc_number.filter(|n| *n > 0 && (user_edited || Some(*n) != guess_disc));
            tracks.push((
                rel,
                CuratedTrackMeta {
                    title: non_empty(title).filter(|t| *t != stem),
                    release_date: scrub_studio_str(release_date),
                    genre: scrub_studio_genre(genre),
                    track_number,
                    disc_number,
                    lyrics: non_empty(lyrics),
                    source: non_empty(source),
                    url: non_empty(url),
                    duration_ms: None,
                    added_at: millis_to_rfc3339(added_at),
                    user_edited,
                    user_mask: 0,
                },
            ));
        }
    }
    let out = db.apply_curated_batch(&albums, &tracks, CuratedWrite::Override)?;
    db.rebuild_genres()?;
    db.rebuild_fts()?;
    Ok(out)
}

/// Legacy library metadata (`.kord/rekord.db`, wins over tags) then sidecars
/// (fill what is still uncurated). Used by restore and the manual sync.
pub fn sync_restored_library_metadata(db: &Db, music_root: &Path) -> Result<(u32, u32)> {
    let legacy_db = music_root.join(".kord").join("rekord.db");
    let (mut albums, mut tracks) = import_legacy_library_db_metadata(db, &legacy_db)?;
    let (a2, t2) = import_sidecar_metadata(db, music_root)?;
    albums += a2;
    tracks += t2;
    Ok((albums, tracks))
}

// ------------------------------------------------------------ personal data
//
// Legacy layout (`server/userState.mjs`, `server/librarySelection.mjs`):
//
// - `.kord/global_info/accounts.json`: `{ accounts: [{ id, name }] }`
// - `.kord/{account}_info/user-state.json`: `favorites` (rel paths),
//   `playlists` (`[{ id, name, tracks: [{ relPath, … }] }]`), `trackMoods`
//   (`{ relPath: [mood, …] }`), `shuffleExcludedTrackRelPaths` (blocked
//   tracks), `shuffleExcludedAlbumIds` (`"Artist::Album"` keys),
//   `trackPlayCounts`, `recent`, `queue`, `settings`, `plectrBests`
// - `.kord/{account}_info/library-selection.json`, `theme-bg.*`
//
// Every track is keyed by its path relative to the music root, the same key
// RE-KORD 5 uses, so the import is a merge by rel_path.

pub(super) fn normalize_import_rel_path(p: &str) -> String {
    let mut s = p.trim().replace('\\', "/");
    while s.starts_with('/') {
        s = s[1..].to_string();
    }
    s = s.replace("/Tracce/", "/Tracks/");
    if s.starts_with("Tracce/") {
        s = format!("Tracks/{}", &s["Tracce/".len()..]);
    }
    s
}

pub(super) fn load_legacy_accounts_registry(music_root: &Path) -> Vec<Account> {
    let global_accounts = music_root
        .join(".kord")
        .join("global_info")
        .join("accounts.json");
    let Ok(raw) = fs::read_to_string(&global_accounts) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(arr) = v.get("accounts").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for a in arr {
        let id = a.get("id").and_then(|x| x.as_str()).unwrap_or("").trim();
        let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("").trim();
        if id.is_empty() {
            continue;
        }
        list.push(Account {
            id: id.to_string(),
            name: if name.is_empty() {
                if id == DEFAULT_ACCOUNT_ID {
                    accounts::DEFAULT_ACCOUNT_NAME.to_string()
                } else {
                    "Account".to_string()
                }
            } else {
                name.to_string()
            },
        });
    }
    list
}

/// Key that ignores case, punctuation and how accented letters are encoded
/// (precomposed, or a letter plus a combining mark as NFD file systems store
/// them): non-ASCII characters are dropped, and a combining mark also drops
/// the letter it decorates. Only used when it designates one indexed path.
fn fold_path_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            out.pop();
        } else if c.is_ascii_alphanumeric() || c == '/' {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

fn insert_unique<K: std::hash::Hash + Eq, V: PartialEq>(
    map: &mut std::collections::HashMap<K, Option<V>>,
    key: K,
    value: V,
) {
    match map.get_mut(&key) {
        None => {
            map.insert(key, Some(value));
        }
        Some(slot) => {
            if slot.as_ref() != Some(&value) {
                *slot = None;
            }
        }
    }
}

/// Maps legacy rel_paths onto indexed ones: exact, the `Tracce` → `Tracks`
/// rename, then case-insensitive, then [`fold_path_key`], the last two only
/// when they point at a single track.
pub(super) struct TrackPathResolver {
    exact: std::collections::HashSet<String>,
    lower: std::collections::HashMap<String, Option<String>>,
    folded: std::collections::HashMap<String, Option<String>>,
}

impl TrackPathResolver {
    pub(super) fn new(paths: impl IntoIterator<Item = String>) -> Self {
        let mut r = Self {
            exact: Default::default(),
            lower: Default::default(),
            folded: Default::default(),
        };
        for p in paths {
            insert_unique(&mut r.lower, p.to_lowercase(), p.clone());
            let folded = fold_path_key(&p);
            if !folded.is_empty() {
                insert_unique(&mut r.folded, folded, p.clone());
            }
            r.exact.insert(p);
        }
        r
    }

    fn from_db(db: &Db) -> Self {
        Self::new(db.all_track_rel_paths().unwrap_or_default())
    }

    /// The indexed rel_path a legacy one designates, if any.
    pub(super) fn resolve(&self, legacy: &str) -> Option<String> {
        let norm = normalize_import_rel_path(legacy);
        if norm.is_empty() {
            return None;
        }
        if self.exact.contains(&norm) {
            return Some(norm);
        }
        let raw = legacy.trim().replace('\\', "/");
        let raw = raw.trim_start_matches('/');
        if self.exact.contains(raw) {
            return Some(raw.to_string());
        }
        if let Some(Some(p)) = self.lower.get(&norm.to_lowercase()) {
            return Some(p.clone());
        }
        let folded = fold_path_key(&norm);
        if folded.is_empty() {
            return None;
        }
        self.folded.get(&folded).cloned().flatten()
    }

    /// Indexed path when there is one; otherwise the normalised legacy path
    /// (recorded in `unmatched`), which links up if the file is indexed later.
    fn map(&self, legacy: &str, unmatched: &mut BTreeSet<String>) -> String {
        match self.resolve(legacy) {
            Some(p) => p,
            None => {
                let n = normalize_import_rel_path(legacy);
                if !n.is_empty() {
                    unmatched.insert(n.clone());
                }
                n
            }
        }
    }

    fn is_indexed(&self, rel: &str) -> bool {
        self.exact.contains(rel)
    }
}

/// Legacy blocked-album keys (`"Artist::Album"`, or a folder) → hub album ids.
struct AlbumKeyResolver {
    by_folder: std::collections::HashMap<String, i64>,
    by_folder_lower: std::collections::HashMap<String, Option<i64>>,
    by_artist_album: std::collections::HashMap<(String, String), Option<i64>>,
}

impl AlbumKeyResolver {
    fn from_db(db: &Db) -> Self {
        let mut r = Self {
            by_folder: Default::default(),
            by_folder_lower: Default::default(),
            by_artist_album: Default::default(),
        };
        for a in db.list_albums().unwrap_or_default() {
            let folder = a.folder_key.replace('\\', "/");
            insert_unique(&mut r.by_folder_lower, folder.to_lowercase(), a.id);
            insert_unique(
                &mut r.by_artist_album,
                (
                    a.artist_name.trim().to_lowercase(),
                    a.name.trim().to_lowercase(),
                ),
                a.id,
            );
            r.by_folder.insert(folder, a.id);
        }
        r
    }

    fn resolve(&self, key: &str) -> Option<i64> {
        let folder = legacy_album_key_to_folder(key.trim());
        if let Some(id) = self.by_folder.get(&folder) {
            return Some(*id);
        }
        if let Some(Some(id)) = self.by_folder_lower.get(&folder.to_lowercase()) {
            return Some(*id);
        }
        let (artist, album) = key.split_once("::")?;
        self.by_artist_album
            .get(&(artist.trim().to_lowercase(), album.trim().to_lowercase()))
            .copied()
            .flatten()
    }
}

/// Merge legacy `plectrBests` into the client's `settings.plectr` store
/// (`apps/client-ui/src/lib/plectr/records.ts`): the better record per track
/// wins (score, then accuracy). Returns how many records were added or improved.
pub(super) fn merge_plectr_bests(
    settings: &mut serde_json::Map<String, Value>,
    bests: &Value,
) -> u32 {
    let Some(bests) = bests.as_object().filter(|b| !b.is_empty()) else {
        return 0;
    };
    let mut store = settings
        .get("plectr")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "version": 1,
                "difficulty": "easy",
                "bests": {},
                "runs": 0,
                "notesHit": 0,
                "lastRunAt": null,
                "lowEnd": null,
                "resetAt": null,
            })
        });
    let reset_at = store
        .get("resetAt")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let score = |v: &Value| v.get("score").and_then(|x| x.as_f64());
    let accuracy = |v: &Value| v.get("accuracy").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let mut changed = 0u32;
    let mut latest: Option<String> = store
        .get("lastRunAt")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    {
        let Some(map) = store
            .as_object_mut()
            .map(|o| o.entry("bests").or_insert_with(|| json!({})))
            .and_then(|b| b.as_object_mut())
        else {
            return 0;
        };
        for (rel, best) in bests {
            let rel = normalize_import_rel_path(rel);
            let Some(sc) = score(best) else { continue };
            let hits = best.get("hits").and_then(|x| x.as_f64()).unwrap_or(0.0);
            if rel.is_empty() || (sc <= 0.0 && hits <= 0.0) {
                continue;
            }
            let updated = best.get("updatedAt").and_then(|v| v.as_str());
            // Records older than a reset stay reset.
            if let (Some(reset), Some(at)) = (reset_at.as_deref(), updated) {
                if at <= reset {
                    continue;
                }
            }
            let better = match map.get(&rel) {
                None => true,
                Some(cur) => {
                    let cs = score(cur).unwrap_or(0.0);
                    sc > cs || (sc == cs && accuracy(best) > accuracy(cur))
                }
            };
            if better {
                map.insert(rel, best.clone());
                changed += 1;
                if let Some(at) = updated {
                    if latest.as_deref().is_none_or(|l| at > l) {
                        latest = Some(at.to_string());
                    }
                }
            }
        }
    }
    if changed == 0 {
        return 0;
    }
    if let (Some(obj), Some(at)) = (store.as_object_mut(), latest) {
        obj.insert("lastRunAt".into(), Value::String(at));
    }
    settings.insert("plectr".into(), store);
    changed
}

/// Recent tracks kept per account (same cap as the client).
const RECENT_CAP: usize = 100;
/// Unmatched paths listed per account in a report (the count is complete).
const UNMATCHED_LIST_CAP: usize = 50;
/// Settings key holding blocked-album keys that match no indexed album yet.
const LEGACY_ALBUM_KEYS: &str = "legacyExcludedAlbumKeys";

/// What one import added, per category.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct LegacyImportCounts {
    /// Favorites added (linked + parked).
    pub favorites: u32,
    /// Of `favorites`, those whose file is not indexed: they link on a scan.
    pub favorites_parked: u32,
    pub playlists: u32,
    pub playlist_tracks: u32,
    pub playlist_tracks_parked: u32,
    /// Tracks that received moods.
    pub moods: u32,
    /// Tracks blocked from shuffle.
    pub excluded_tracks: u32,
    /// Albums blocked from shuffle.
    pub excluded_albums: u32,
    /// Tracks whose play count went up.
    pub play_counts: u32,
    pub recent: u32,
    /// Settings keys added.
    pub settings: u32,
    pub plectr_bests: u32,
    pub selections: u32,
    pub theme_backgrounds: u32,
}

impl LegacyImportCounts {
    fn add(&mut self, o: &Self) {
        self.favorites += o.favorites;
        self.favorites_parked += o.favorites_parked;
        self.playlists += o.playlists;
        self.playlist_tracks += o.playlist_tracks;
        self.playlist_tracks_parked += o.playlist_tracks_parked;
        self.moods += o.moods;
        self.excluded_tracks += o.excluded_tracks;
        self.excluded_albums += o.excluded_albums;
        self.play_counts += o.play_counts;
        self.recent += o.recent;
        self.settings += o.settings;
        self.plectr_bests += o.plectr_bests;
        self.selections += o.selections;
        self.theme_backgrounds += o.theme_backgrounds;
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LegacyAccountStatus {
    /// Merged by this run (the counts may be zero: nothing was missing).
    #[default]
    Imported,
    /// Legacy files unchanged since an earlier import: left alone, so what
    /// was removed in RE-KORD 5 since then stays removed.
    Unchanged,
    /// Not imported; see `reason`.
    Skipped,
}

/// One legacy account (`.kord/{id}_info`) in an import report.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct LegacyAccountReport {
    pub legacy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_name: Option<String>,
    /// Hub account the data went (or would go) to.
    pub hub_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hub_name: Option<String>,
    pub status: LegacyAccountStatus,
    /// `not_registered` (deleted in legacy), `no_data`, `already_imported`, `read_error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The hub account was created by this import.
    pub created: bool,
    pub counts: LegacyImportCounts,
    /// Legacy paths that match no indexed track (first ones only).
    pub unmatched_paths: Vec<String>,
    pub unmatched_count: u32,
    /// Blocked-album keys that match no indexed album.
    pub unmatched_album_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LegacyImportTrigger {
    /// First start (or first scan) on a legacy library.
    #[default]
    Auto,
    /// Admin panel / API.
    Manual,
    /// `rekord-server --legacy-import`.
    Cli,
}

/// Outcome of a legacy import (or of a dry run).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct LegacyImportReport {
    pub trigger: LegacyImportTrigger,
    /// Nothing was written.
    pub dry_run: bool,
    pub ran_at: String,
    pub music_root: String,
    /// `<music root>/.kord` exists.
    pub kord_found: bool,
    pub album_meta_merged: u32,
    pub track_meta_merged: u32,
    /// The legacy library database could not be read (personal data still imported).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_error: Option<String>,
    /// Hub accounts created from the legacy registry.
    pub accounts_added: u32,
    pub totals: LegacyImportCounts,
    pub unmatched_count: u32,
    pub accounts: Vec<LegacyAccountReport>,
}

impl LegacyImportReport {
    /// Accounts merged by this run.
    pub fn imported_accounts(&self) -> usize {
        self.accounts
            .iter()
            .filter(|a| a.status == LegacyAccountStatus::Imported)
            .count()
    }
}

/// How an import runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyImportOptions {
    pub mode: LegacyImportMode,
    pub trigger: LegacyImportTrigger,
    /// Compute the report without writing anything.
    pub dry_run: bool,
    /// Merge again accounts whose legacy files did not change since the last
    /// import (brings back what was removed in RE-KORD 5 meanwhile).
    pub force: bool,
}

/// What happened to one legacy account, as remembered in the marker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAccountOutcome {
    pub hub_id: String,
    pub imported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Digest of the legacy files that were imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
}

/// The files of one legacy account.
struct LegacyAccountFiles {
    legacy_id: String,
    dir: PathBuf,
    user_state: Option<String>,
    selection: Option<String>,
    theme_bg: Option<PathBuf>,
    read_error: Option<String>,
}

impl LegacyAccountFiles {
    fn load(legacy_id: String, dir: PathBuf) -> Self {
        let mut read_error = None;
        let mut read = |name: &str| -> Option<String> {
            let p = dir.join(name);
            if !p.is_file() {
                return None;
            }
            match fs::read_to_string(&p) {
                Ok(s) => Some(s),
                Err(e) => {
                    warn!(error = %e, path = %p.display(), "cannot read legacy file");
                    read_error = Some(format!("{name}: {e}"));
                    None
                }
            }
        };
        let user_state = read("user-state.json");
        let selection = read("library-selection.json");
        let theme_bg = [
            "theme-bg.jpg",
            "theme-bg.jpeg",
            "theme-bg.png",
            "theme-bg.webp",
            "theme-bg.gif",
        ]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file());
        Self {
            legacy_id,
            dir,
            user_state,
            selection,
            theme_bg,
            read_error,
        }
    }

    fn has_data(&self) -> bool {
        self.user_state.is_some() || self.selection.is_some() || self.theme_bg.is_some()
    }

    /// Digest of what an import reads, to recognise an unchanged account.
    fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(self.user_state.as_deref().unwrap_or("").as_bytes());
        h.update([0u8]);
        h.update(self.selection.as_deref().unwrap_or("").as_bytes());
        h.update([0u8]);
        if let Some(meta) = self.theme_bg.as_ref().and_then(|p| fs::metadata(p).ok()) {
            h.update(meta.len().to_le_bytes());
        }
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }
}

/// Every `.kord/{id}_info` folder, sorted by id.
fn legacy_account_dirs(kord: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(kord).with_context(|| format!("read {}", kord.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = account_id_from_info_dir_name(&name) {
            out.push((id, entry.path()));
        }
    }
    out.sort();
    Ok(out)
}

/// Legacy user-state with every path mapped onto the catalog.
struct LegacyPersonal {
    favorites: Vec<String>,
    playlists: Vec<PlaylistBackup>,
    state: UserStateV1,
    plectr: Value,
    unmatched: BTreeSet<String>,
    unmatched_album_keys: Vec<String>,
}

fn convert_legacy_personal(
    raw: &str,
    tracks: &TrackPathResolver,
    albums: &AlbumKeyResolver,
) -> Result<LegacyPersonal> {
    let mut unmatched = BTreeSet::new();
    let (favorites, mut playlists) = playlists_from_legacy_user_state(raw)?;
    let favorites: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        favorites
            .iter()
            .map(|p| tracks.map(p, &mut unmatched))
            .filter(|p| !p.is_empty() && seen.insert(p.clone()))
            .collect()
    };
    for pl in &mut playlists {
        for t in &mut pl.tracks {
            t.rel_path = tracks.map(&t.rel_path, &mut unmatched);
        }
        pl.tracks.retain(|t| !t.rel_path.is_empty());
    }

    let legacy = user_state::user_state_from_legacy_json(raw)?;
    let mut state = UserStateV1 {
        settings: legacy.settings,
        ..Default::default()
    };
    let moods = crate::track_moods::normalize_track_moods_map(legacy.track_moods);
    for (rel, moods) in moods {
        let rel = tracks.map(&rel, &mut unmatched);
        state.track_moods.entry(rel).or_insert(moods);
    }
    for (rel, n) in legacy.play_counts {
        let rel = tracks.map(&rel, &mut unmatched);
        let n = n.as_u64().unwrap_or(0);
        let cur = state
            .play_counts
            .get(&rel)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if n > cur {
            state.play_counts.insert(rel, Value::from(n));
        }
    }
    for rel in legacy.recent_rel_paths {
        let rel = tracks.map(&rel, &mut unmatched);
        if !state.recent_rel_paths.contains(&rel) {
            state.recent_rel_paths.push(rel);
        }
    }
    for rel in legacy.excluded_rel_paths {
        let rel = tracks.map(&rel, &mut unmatched);
        if !state.excluded_rel_paths.contains(&rel) {
            state.excluded_rel_paths.push(rel);
        }
    }
    state.excluded_album_ids = legacy.excluded_album_ids;
    let mut unmatched_album_keys = Vec::new();
    if let Some(keys) = state.settings.remove(LEGACY_ALBUM_KEYS) {
        for key in keys
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str())
        {
            match albums.resolve(key) {
                Some(id) => {
                    if !state.excluded_album_ids.contains(&id) {
                        state.excluded_album_ids.push(id);
                    }
                }
                None => unmatched_album_keys.push(key.to_string()),
            }
        }
    }
    if let Some(Value::Object(q)) = state.settings.get_mut("legacyQueue") {
        if let Some(Value::Array(paths)) = q.get_mut("relPaths") {
            for p in paths.iter_mut() {
                if let Some(s) = p.as_str() {
                    *p = Value::String(tracks.resolve(s).unwrap_or_else(|| s.to_string()));
                }
            }
        }
    }

    let plectr = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.get("plectrBests").cloned())
        .and_then(|v| match v {
            Value::Object(map) => Some(Value::Object(
                map.into_iter()
                    .map(|(rel, best)| (tracks.map(&rel, &mut unmatched), best))
                    .collect(),
            )),
            _ => None,
        })
        .unwrap_or(Value::Null);

    Ok(LegacyPersonal {
        favorites,
        playlists,
        state,
        plectr,
        unmatched,
        unmatched_album_keys,
    })
}

/// Union the legacy state into `hub`. Nothing the hub has is replaced: moods
/// and settings only fill gaps, play counts keep the higher value, recent
/// tracks are appended after the hub's own, blocked tracks / albums are a
/// union. With `hub_in_use` (the account already has settings in RE-KORD 5)
/// the language and the legacy queue are left out: they would change what
/// the person sees on the next start.
fn merge_legacy_state(
    hub: &mut UserStateV1,
    legacy: &LegacyPersonal,
    unmatched_album_keys: &[String],
    hub_in_use: bool,
) -> LegacyImportCounts {
    let mut c = LegacyImportCounts::default();
    let l = &legacy.state;
    for (k, v) in &l.track_moods {
        if !hub.track_moods.contains_key(k) {
            hub.track_moods.insert(k.clone(), v.clone());
            c.moods += 1;
        }
    }
    for (k, v) in &l.play_counts {
        let cur = hub.play_counts.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let n = v.as_u64().unwrap_or(0);
        if n > cur {
            hub.play_counts.insert(k.clone(), Value::from(n));
            c.play_counts += 1;
        }
    }
    for p in &l.recent_rel_paths {
        if hub.recent_rel_paths.len() >= RECENT_CAP {
            break;
        }
        if !hub.recent_rel_paths.contains(p) {
            hub.recent_rel_paths.push(p.clone());
            c.recent += 1;
        }
    }
    for p in &l.excluded_rel_paths {
        if !hub.excluded_rel_paths.contains(p) {
            hub.excluded_rel_paths.push(p.clone());
            c.excluded_tracks += 1;
        }
    }
    for id in &l.excluded_album_ids {
        if !hub.excluded_album_ids.contains(id) {
            hub.excluded_album_ids.push(*id);
            c.excluded_albums += 1;
        }
    }
    let has_queue = hub.settings.contains_key("queue");
    for (k, v) in &l.settings {
        let skip = hub_in_use && k == "locale" || k == "legacyQueue" && has_queue;
        if !skip && !hub.settings.contains_key(k) {
            hub.settings.insert(k.clone(), v.clone());
            c.settings += 1;
        }
    }
    if !unmatched_album_keys.is_empty() {
        let mut keys: Vec<Value> = hub
            .settings
            .get(LEGACY_ALBUM_KEYS)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for k in unmatched_album_keys {
            let v = Value::String(k.clone());
            if !keys.contains(&v) {
                keys.push(v);
            }
        }
        hub.settings
            .insert(LEGACY_ALBUM_KEYS.into(), Value::Array(keys));
    }
    c.plectr_bests = merge_plectr_bests(&mut hub.settings, &legacy.plectr);
    c
}

/// Everything an import run shares between accounts.
struct ImportCtx<'a> {
    db: &'a Db,
    data_dir: &'a Path,
    opts: LegacyImportOptions,
    tracks: TrackPathResolver,
    albums: AlbumKeyResolver,
    previous: BTreeMap<String, LegacyAccountOutcome>,
}

impl ImportCtx<'_> {
    /// Already imported from the same files into the same hub account.
    fn unchanged(&self, files: &LegacyAccountFiles, hub_id: &str, fingerprint: &str) -> bool {
        if self.opts.force || self.opts.mode == LegacyImportMode::Replace {
            return false;
        }
        match self.previous.get(&files.legacy_id) {
            Some(prev) if prev.imported && prev.hub_id == hub_id => match &prev.fingerprint {
                Some(fp) => fp == fingerprint,
                // Imported by an earlier version, which recorded no digest.
                None => true,
            },
            _ => false,
        }
    }

    fn import_account(
        &self,
        files: &LegacyAccountFiles,
        hub_id: &str,
        report: &mut LegacyAccountReport,
    ) -> Result<()> {
        let dry = self.opts.dry_run;
        let replace = self.opts.mode == LegacyImportMode::Replace;
        let c = &mut report.counts;

        if let Some(raw) = files.user_state.as_deref() {
            match convert_legacy_personal(raw, &self.tracks, &self.albums) {
                Ok(legacy) => {
                    report.unmatched_count = legacy.unmatched.len() as u32;
                    report.unmatched_paths = legacy
                        .unmatched
                        .iter()
                        .take(UNMATCHED_LIST_CAP)
                        .cloned()
                        .collect();
                    report.unmatched_album_keys = legacy.unmatched_album_keys.clone();
                    self.import_favorites_playlists(hub_id, &legacy, c)?;
                    self.import_state(hub_id, &legacy, c)?;
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        path = %files.dir.join("user-state.json").display(),
                        "legacy user-state unreadable"
                    );
                    report.reason = Some("read_error".into());
                }
            }
        }

        // Selection: new accounts get an empty one, which is not a choice.
        let hub_has_selection = hub_id == DEFAULT_ACCOUNT_ID
            || selection::read_library_selection(self.data_dir, hub_id)
                .map(|s| {
                    selection::get_selection_filter_mode(&s)
                        != selection::SelectionFilterMode::Empty
                })
                .unwrap_or(false);
        if let Some(raw) = files.selection.as_deref() {
            if replace || !hub_has_selection {
                if let Ok(sel) = serde_json::from_str::<selection::LibrarySelection>(raw) {
                    if dry
                        || selection::write_library_selection(self.data_dir, hub_id, &sel).is_ok()
                    {
                        c.selections += 1;
                    }
                }
            }
        }

        if let Some(src) = files.theme_bg.as_ref() {
            let hub_has_theme = user_state::find_theme_bg_path(self.data_dir, hub_id).is_some();
            if replace || !hub_has_theme {
                let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                if dry {
                    c.theme_backgrounds += 1;
                } else {
                    let dest = user_state::theme_bg_path_for_ext(self.data_dir, hub_id, ext);
                    if let Some(parent) = dest.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = user_state::delete_theme_bg(self.data_dir, hub_id);
                    if fs::copy(src, &dest).is_ok() {
                        c.theme_backgrounds += 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn import_favorites_playlists(
        &self,
        hub_id: &str,
        legacy: &LegacyPersonal,
        c: &mut LegacyImportCounts,
    ) -> Result<()> {
        let db = self.db;
        if self.opts.mode == LegacyImportMode::Replace {
            let linked = db.replace_favorites_by_rel_paths(hub_id, &legacy.favorites)?;
            c.favorites = legacy.favorites.len() as u32;
            c.favorites_parked = c.favorites.saturating_sub(linked);
            let (p, t) = db.replace_playlists_backup(hub_id, &legacy.playlists)?;
            c.playlists = p;
            c.playlist_tracks = legacy.playlists.iter().map(|p| p.tracks.len() as u32).sum();
            c.playlist_tracks_parked = c.playlist_tracks.saturating_sub(t);
            return Ok(());
        }
        let present: std::collections::HashSet<String> =
            db.export_favorite_rel_paths(hub_id)?.into_iter().collect();
        let missing: Vec<String> = legacy
            .favorites
            .iter()
            .filter(|p| !present.contains(*p))
            .cloned()
            .collect();
        c.favorites = missing.len() as u32;
        c.favorites_parked = missing
            .iter()
            .filter(|p| !self.tracks.is_indexed(p))
            .count() as u32;
        if !self.opts.dry_run && !missing.is_empty() {
            db.merge_favorites_by_rel_paths(hub_id, &missing)?;
        }
        let (p, t, parked) =
            db.merge_playlists_backup_opts(hub_id, &legacy.playlists, self.opts.dry_run)?;
        c.playlists = p;
        c.playlist_tracks = t;
        c.playlist_tracks_parked = parked;
        Ok(())
    }

    fn import_state(
        &self,
        hub_id: &str,
        legacy: &LegacyPersonal,
        c: &mut LegacyImportCounts,
    ) -> Result<()> {
        if self.opts.mode == LegacyImportMode::Replace {
            let mut fresh = UserStateV1::default();
            let counts =
                merge_legacy_state(&mut fresh, legacy, &legacy.unmatched_album_keys, false);
            user_state::update_user_state(self.data_dir, hub_id, None, |s| *s = fresh)?;
            add_state_counts(c, &counts);
            return Ok(());
        }
        // Work on a copy first: an unchanged state is not rewritten (a write
        // bumps the revision every client syncs on).
        let mut probe = user_state::load_user_state(self.data_dir, hub_id);
        let in_use = !probe.settings.is_empty();
        let counts = merge_legacy_state(&mut probe, legacy, &legacy.unmatched_album_keys, in_use);
        let album_keys_changed = !legacy.unmatched_album_keys.is_empty()
            && user_state::load_user_state(self.data_dir, hub_id)
                .settings
                .get(LEGACY_ALBUM_KEYS)
                != probe.settings.get(LEGACY_ALBUM_KEYS);
        if !self.opts.dry_run && (!counts.is_empty() || album_keys_changed) {
            user_state::update_user_state(self.data_dir, hub_id, None, |s| {
                merge_legacy_state(s, legacy, &legacy.unmatched_album_keys, in_use);
            })?;
        }
        add_state_counts(c, &counts);
        Ok(())
    }
}

fn add_state_counts(c: &mut LegacyImportCounts, s: &LegacyImportCounts) {
    c.moods += s.moods;
    c.play_counts += s.play_counts;
    c.recent += s.recent;
    c.excluded_tracks += s.excluded_tracks;
    c.excluded_albums += s.excluded_albums;
    c.settings += s.settings;
    c.plectr_bests += s.plectr_bests;
}

/// Import (or preview) the personal data of every legacy account into the
/// hub. Legacy accounts map onto hub accounts with the same id, then the same
/// display name (`default` always onto `default`); the others are created
/// with their legacy id and name. Hub accounts are never removed.
fn import_personal_data(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    opts: LegacyImportOptions,
    previous: BTreeMap<String, LegacyAccountOutcome>,
    report: &mut LegacyImportReport,
) -> Result<BTreeMap<String, LegacyAccountOutcome>> {
    let mut outcomes = previous.clone();
    let kord = music_root.join(".kord");
    if !kord.is_dir() {
        return Ok(outcomes);
    }
    if opts.dry_run && opts.mode == LegacyImportMode::Replace {
        bail!("a dry run only previews the merge");
    }

    let legacy_registry = load_legacy_accounts_registry(music_root);
    let legacy_names: BTreeMap<String, String> = legacy_registry
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();
    let hub_before = accounts::ensure_accounts(data_dir)?;
    let mut id_map: BTreeMap<String, String> = BTreeMap::new();
    let mut created: std::collections::HashSet<String> = Default::default();
    let mut registered: BTreeMap<String, String> = hub_before
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();
    if !legacy_registry.is_empty() {
        match opts.mode {
            LegacyImportMode::Replace => {
                let list = accounts::replace_accounts_registry(data_dir, &legacy_registry)?;
                report.accounts_added = list.len() as u32;
                registered = list.into_iter().map(|a| (a.id, a.name)).collect();
            }
            LegacyImportMode::Merge => {
                let (targets, map) = resolve_restore_account_targets(&legacy_registry, &hub_before);
                id_map = map;
                let mut merged = hub_before.clone();
                for acc in targets {
                    if !merged.iter().any(|h| h.id == acc.id) {
                        created.insert(acc.id.clone());
                        registered.insert(acc.id.clone(), acc.name.clone());
                        merged.push(acc);
                    }
                }
                report.accounts_added = created.len() as u32;
                if !opts.dry_run && !created.is_empty() {
                    accounts::replace_accounts_registry(data_dir, &merged)?;
                }
            }
        }
    }
    // The legacy name of the default account replaces the stock one a fresh
    // hub starts with ("Locale" was the stock name of earlier builds).
    if let Some(legacy_default) = legacy_registry.iter().find(|a| a.id == DEFAULT_ACCOUNT_ID) {
        let stock = registered
            .get(DEFAULT_ACCOUNT_ID)
            .is_some_and(|n| n == accounts::DEFAULT_ACCOUNT_NAME || n == "Locale");
        if stock && registered.get(DEFAULT_ACCOUNT_ID) != Some(&legacy_default.name) {
            if !opts.dry_run {
                accounts::update_account(data_dir, DEFAULT_ACCOUNT_ID, Some(&legacy_default.name))?;
            }
            registered.insert(DEFAULT_ACCOUNT_ID.into(), legacy_default.name.clone());
        }
    }

    let ctx = ImportCtx {
        db,
        data_dir,
        opts,
        tracks: TrackPathResolver::from_db(db),
        albums: AlbumKeyResolver::from_db(db),
        previous,
    };
    let now = chrono::Utc::now().to_rfc3339();

    for (legacy_id, dir) in legacy_account_dirs(&kord)? {
        let files = LegacyAccountFiles::load(legacy_id.clone(), dir);
        let hub_id = id_map
            .get(&legacy_id)
            .cloned()
            .unwrap_or_else(|| legacy_id.clone());
        let mut acc = LegacyAccountReport {
            legacy_id: legacy_id.clone(),
            legacy_name: legacy_names.get(&legacy_id).cloned(),
            hub_id: hub_id.clone(),
            hub_name: registered.get(&hub_id).cloned(),
            created: created.contains(&hub_id),
            ..Default::default()
        };
        let skip = |acc: &mut LegacyAccountReport, status, reason: &str| {
            acc.status = status;
            acc.reason = Some(reason.to_string());
        };
        // Folders of accounts deleted in legacy (or never registered) must
        // not turn into hub accounts.
        if !registered.contains_key(&hub_id) {
            skip(&mut acc, LegacyAccountStatus::Skipped, "not_registered");
        } else if !files.has_data() {
            let reason = if files.read_error.is_some() {
                "read_error"
            } else {
                "no_data"
            };
            skip(&mut acc, LegacyAccountStatus::Skipped, reason);
        } else {
            let fingerprint = files.fingerprint();
            if ctx.unchanged(&files, &hub_id, &fingerprint) {
                skip(&mut acc, LegacyAccountStatus::Unchanged, "already_imported");
            } else {
                ctx.import_account(&files, &hub_id, &mut acc)?;
                if acc.reason.is_none() {
                    acc.status = LegacyAccountStatus::Imported;
                    outcomes.insert(
                        legacy_id.clone(),
                        LegacyAccountOutcome {
                            hub_id: hub_id.clone(),
                            imported: true,
                            reason: None,
                            fingerprint: Some(fingerprint),
                            imported_at: Some(now.clone()),
                        },
                    );
                } else {
                    acc.status = LegacyAccountStatus::Skipped;
                }
            }
        }
        if acc.status == LegacyAccountStatus::Skipped {
            // A later run tries again.
            let keep = outcomes.get(&legacy_id).is_some_and(|o| o.imported);
            if !keep {
                outcomes.insert(
                    legacy_id.clone(),
                    LegacyAccountOutcome {
                        hub_id: hub_id.clone(),
                        imported: false,
                        reason: acc.reason.clone(),
                        fingerprint: None,
                        imported_at: None,
                    },
                );
            }
        }
        report.totals.add(&acc.counts);
        report.unmatched_count += acc.unmatched_count;
        report.accounts.push(acc);
    }
    Ok(outcomes)
}

/// Marker of the legacy import (`<data_dir>/legacy-import.json`). Version 2
/// records, per legacy account, a digest of the files imported, so a later
/// run (automatic or manual) leaves unchanged accounts alone: what is removed
/// in RE-KORD 5 stays removed. A marker written by an earlier version (no
/// version) makes the automatic import run once more, for the accounts that
/// version skipped.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportMarker {
    #[serde(default)]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_root: Option<String>,
    #[serde(default)]
    pub album_meta_merged: u32,
    #[serde(default)]
    pub track_meta_merged: u32,
    /// Legacy account id → what happened to it.
    #[serde(default)]
    pub accounts: BTreeMap<String, LegacyAccountOutcome>,
    /// Report of the latest run that wrote something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_report: Option<LegacyImportReport>,
}

pub const LEGACY_IMPORT_MARKER: &str = "legacy-import.json";
pub const LEGACY_IMPORT_MARKER_VERSION: u32 = 2;

pub fn legacy_import_marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LEGACY_IMPORT_MARKER)
}

pub fn read_legacy_import_marker(data_dir: &Path) -> Option<LegacyImportMarker> {
    let raw = fs::read_to_string(legacy_import_marker_path(data_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_legacy_import_marker(data_dir: &Path, marker: &LegacyImportMarker) -> Result<()> {
    let path = legacy_import_marker_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(marker)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn legacy_import_opted_out() -> bool {
    std::env::var("REKORD_SKIP_LEGACY_IMPORT")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false)
}

/// A legacy library (`.kord`) is next to the music and the automatic import
/// has not run yet (or ran with a version that skipped accounts in use).
pub fn legacy_import_pending(data_dir: &Path, music_root: &Path) -> bool {
    !legacy_import_opted_out()
        && music_root.join(".kord").is_dir()
        && read_legacy_import_marker(data_dir)
            .is_none_or(|m| m.imported_at.is_none() || m.version < LEGACY_IMPORT_MARKER_VERSION)
}

/// Import legacy data into the hub: library metadata (`.kord/rekord.db`, then
/// sidecars), the accounts registry, and per account favorites, playlists,
/// moods, blocked tracks and albums, play counts, recent tracks, settings,
/// Plectr records, library selection and theme background. A merge by
/// default (see [`LegacyImportMode::Merge`]), idempotent, and a preview with
/// `dry_run`. Needs an indexed catalog to link tracks; paths not indexed are
/// kept (favorites and playlist entries are parked until a scan finds them)
/// and listed in the report.
pub fn run_legacy_import(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    opts: LegacyImportOptions,
) -> Result<LegacyImportReport> {
    let mut report = LegacyImportReport {
        trigger: opts.trigger,
        dry_run: opts.dry_run,
        ran_at: chrono::Utc::now().to_rfc3339(),
        music_root: music_root.to_string_lossy().into_owned(),
        kord_found: music_root.join(".kord").is_dir(),
        ..Default::default()
    };
    if !opts.dry_run {
        // A broken legacy database must not block the personal data.
        match sync_restored_library_metadata(db, music_root) {
            Ok((a, t)) => {
                report.album_meta_merged = a;
                report.track_meta_merged = t;
            }
            Err(e) => {
                warn!(error = %e, "legacy library metadata import failed");
                report.metadata_error = Some(e.to_string());
            }
        }
        // Drop stubs reintroduced by bad sidecars (e.g. genre "e").
        let _ = db.clear_weak_studio_placeholders();
    }
    let marker = read_legacy_import_marker(data_dir);
    let previous = marker
        .as_ref()
        .map(|m| m.accounts.clone())
        .unwrap_or_default();
    let outcomes = import_personal_data(db, data_dir, music_root, opts, previous, &mut report)?;

    log_legacy_import_report(&report);
    if !opts.dry_run && report.kord_found {
        let prev = marker.unwrap_or_default();
        write_legacy_import_marker(
            data_dir,
            &LegacyImportMarker {
                version: LEGACY_IMPORT_MARKER_VERSION,
                imported_at: prev.imported_at.or_else(|| Some(report.ran_at.clone())),
                music_root: Some(report.music_root.clone()),
                album_meta_merged: prev.album_meta_merged + report.album_meta_merged,
                track_meta_merged: prev.track_meta_merged + report.track_meta_merged,
                accounts: outcomes,
                last_report: Some(report.clone()),
            },
        )?;
        let t = &report.totals;
        crate::diagnostics::log_activity(
            data_dir,
            crate::diagnostics::ActivityEvent::new(
                "system",
                "legacyImport",
                format!(
                    "import dalla versione precedente: {} account, {} preferiti, {} playlist, {} mood, {} brani bloccati",
                    report.imported_accounts(),
                    t.favorites,
                    t.playlists,
                    t.moods,
                    t.excluded_tracks
                ),
            )
            .params(json!({
                "accounts": report.imported_accounts(),
                "favorites": t.favorites,
                "playlists": t.playlists,
                "moods": t.moods,
                "blocked": t.excluded_tracks + t.excluded_albums,
            })),
        );
    }
    Ok(report)
}

fn log_legacy_import_report(report: &LegacyImportReport) {
    let t = &report.totals;
    info!(
        trigger = ?report.trigger,
        dry_run = report.dry_run,
        music_root = %report.music_root,
        album_meta_merged = report.album_meta_merged,
        track_meta_merged = report.track_meta_merged,
        accounts_imported = report.imported_accounts(),
        accounts_added = report.accounts_added,
        favorites = t.favorites,
        playlists = t.playlists,
        playlist_tracks = t.playlist_tracks,
        moods = t.moods,
        blocked_tracks = t.excluded_tracks,
        blocked_albums = t.excluded_albums,
        play_counts = t.play_counts,
        recent = t.recent,
        settings = t.settings,
        plectr_bests = t.plectr_bests,
        selections = t.selections,
        theme_backgrounds = t.theme_backgrounds,
        unmatched_paths = report.unmatched_count,
        "legacy import finished"
    );
    for a in &report.accounts {
        info!(
            legacy_account = %a.legacy_id,
            hub_account = %a.hub_id,
            status = ?a.status,
            reason = a.reason.as_deref().unwrap_or(""),
            created = a.created,
            favorites = a.counts.favorites,
            playlists = a.counts.playlists,
            moods = a.counts.moods,
            blocked_tracks = a.counts.excluded_tracks,
            blocked_albums = a.counts.excluded_albums,
            play_counts = a.counts.play_counts,
            settings = a.counts.settings,
            unmatched_paths = a.unmatched_count,
            "legacy import account"
        );
        if a.unmatched_count > 0 {
            warn!(
                legacy_account = %a.legacy_id,
                count = a.unmatched_count,
                first = ?a.unmatched_paths.iter().take(5).collect::<Vec<_>>(),
                "legacy paths not found in the library (kept; they link when the files are indexed)"
            );
        }
        if !a.unmatched_album_keys.is_empty() {
            warn!(
                legacy_account = %a.legacy_id,
                albums = ?a.unmatched_album_keys,
                "legacy blocked albums not found in the library"
            );
        }
    }
}

/// Merge legacy `.kord` data into the hub (admin panel and CLI). Accounts
/// imported earlier from the same files are left alone.
pub fn sync_legacy_library_data(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
) -> Result<LegacyImportReport> {
    sync_legacy_library_data_with(db, data_dir, music_root, LegacyImportMode::Merge)
}

pub fn sync_legacy_library_data_with(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    mode: LegacyImportMode,
) -> Result<LegacyImportReport> {
    run_legacy_import(
        db,
        data_dir,
        music_root,
        LegacyImportOptions {
            mode,
            trigger: LegacyImportTrigger::Manual,
            ..Default::default()
        },
    )
}

/// First start on a legacy library: import everything (see
/// [`run_legacy_import`]) and write the marker. Accounts already used in
/// RE-KORD 5 are merged too, without replacing anything they have. Needs an
/// indexed catalog (album ids, track links): run it after a scan.
pub fn auto_import_legacy_once(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
) -> Result<Option<LegacyImportReport>> {
    if !legacy_import_pending(data_dir, music_root) {
        return Ok(None);
    }
    run_legacy_import(
        db,
        data_dir,
        music_root,
        LegacyImportOptions {
            trigger: LegacyImportTrigger::Auto,
            ..Default::default()
        },
    )
    .map(Some)
}

/// What the admin panel shows about the legacy import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportStatus {
    pub music_root: Option<String>,
    pub kord_found: bool,
    pub pending: bool,
    pub opted_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_report: Option<LegacyImportReport>,
}

pub fn legacy_import_status(data_dir: &Path, music_root: Option<&Path>) -> LegacyImportStatus {
    let marker = read_legacy_import_marker(data_dir);
    LegacyImportStatus {
        music_root: music_root.map(|p| p.to_string_lossy().into_owned()),
        kord_found: music_root.is_some_and(|r| r.join(".kord").is_dir()),
        pending: music_root.is_some_and(|r| legacy_import_pending(data_dir, r)),
        opted_out: legacy_import_opted_out(),
        imported_at: marker.as_ref().and_then(|m| m.imported_at.clone()),
        last_report: marker.and_then(|m| m.last_report),
    }
}
