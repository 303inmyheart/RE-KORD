//! Import from a legacy library: studio metadata (sidecars, `.kord/rekord.db`)
//! and per-account personal data (`.kord/{account}_info`).

use super::*;

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

/// Import per-account moods (and fill play counts / recent gaps) from
/// `music_root/.kord/{account}_info/user-state.json` into the hub data_dir.
pub fn import_legacy_account_user_state(
    data_dir: &Path,
    music_root: &Path,
    mode: MoodImportMode,
) -> Result<(u32, u32)> {
    let kord = music_root.join(".kord");
    if !kord.is_dir() {
        return Ok((0, 0));
    }
    let mut accounts = 0u32;
    let mut moods = 0u32;
    for entry in fs::read_dir(&kord).with_context(|| format!("read {}", kord.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(account_id) = name.strip_suffix("_info") else {
            continue;
        };
        if account_id.is_empty() || account_id == "global" {
            continue;
        }
        let legacy_path = entry.path().join("user-state.json");
        if !legacy_path.is_file() {
            continue;
        }
        let raw = match fs::read_to_string(&legacy_path) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, path = %legacy_path.display(), "skip legacy user-state");
                continue;
            }
        };
        let Ok(legacy_val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let has_legacy_shape = legacy_val.get("trackPlayCounts").is_some()
            || legacy_val.get("favorites").is_some()
            || legacy_val.get("trackMoods").is_some()
            || legacy_val.get("recent").is_some();
        let converted = if has_legacy_shape {
            match user_state::user_state_from_legacy_json(&raw) {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, path = %legacy_path.display(), "legacy user-state convert failed");
                    continue;
                }
            }
        } else if let Ok(s) = serde_json::from_str::<UserStateV1>(&raw) {
            s
        } else {
            continue;
        };

        // Dry run on a snapshot first: routine scans call this, and a write
        // bumps the revision every client syncs on.
        let mut probe = user_state::load_user_state(data_dir, account_id);
        let (changed, n_moods) = apply_legacy_user_state(&mut probe, &converted, &legacy_val, mode);
        if changed {
            user_state::update_user_state(data_dir, account_id, None, |hub| {
                apply_legacy_user_state(hub, &converted, &legacy_val, mode);
            })?;
            moods += n_moods;
            accounts += 1;
        }
    }
    Ok((accounts, moods))
}

/// Apply one legacy `user-state.json` onto `hub` per `mode`.
/// Returns (changed, moods imported).
pub(super) fn apply_legacy_user_state(
    hub: &mut UserStateV1,
    converted: &UserStateV1,
    legacy_val: &Value,
    mode: MoodImportMode,
) -> (bool, u32) {
    let mut changed = false;
    let mut n_moods = 0u32;
    match mode {
        MoodImportMode::ReplaceFromLegacy => {
            if (legacy_val.get("trackMoods").is_some() || !converted.track_moods.is_empty())
                && hub.track_moods != converted.track_moods
            {
                n_moods += converted.track_moods.len() as u32;
                hub.track_moods = converted.track_moods.clone();
                changed = true;
            }
        }
        MoodImportMode::FillEmpty => {
            for (k, v) in &converted.track_moods {
                if !hub.track_moods.contains_key(k) {
                    hub.track_moods.insert(k.clone(), v.clone());
                    n_moods += 1;
                    changed = true;
                }
            }
        }
    }

    for (k, v) in &converted.play_counts {
        let cur = hub.play_counts.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let n = v.as_u64().unwrap_or(0);
        if n > cur {
            hub.play_counts.insert(k.clone(), Value::from(n));
            changed = true;
        }
    }
    if hub.recent_rel_paths.is_empty() && !converted.recent_rel_paths.is_empty() {
        hub.recent_rel_paths = converted.recent_rel_paths.clone();
        changed = true;
    }
    match mode {
        MoodImportMode::ReplaceFromLegacy => {
            if hub.excluded_rel_paths != converted.excluded_rel_paths {
                hub.excluded_rel_paths = converted.excluded_rel_paths.clone();
                changed = true;
            }
            // Full replace (including clearing hub-only album blocks).
            // String keys remapped by `import_legacy_accounts_personal_data`.
            if hub.excluded_album_ids != converted.excluded_album_ids {
                hub.excluded_album_ids = converted.excluded_album_ids.clone();
                changed = true;
            }
            if let Some(keys) = converted.settings.get("legacyExcludedAlbumKeys") {
                if hub.settings.get("legacyExcludedAlbumKeys") != Some(keys) {
                    hub.settings
                        .insert("legacyExcludedAlbumKeys".into(), keys.clone());
                    changed = true;
                }
            }
            if !converted.settings.is_empty() {
                for (k, v) in &converted.settings {
                    if k == "legacyExcludedAlbumKeys" {
                        continue;
                    }
                    if hub.settings.get(k) != Some(v) {
                        hub.settings.insert(k.clone(), v.clone());
                        changed = true;
                    }
                }
            }
        }
        MoodImportMode::FillEmpty => {
            if hub.excluded_rel_paths.is_empty() && !converted.excluded_rel_paths.is_empty() {
                hub.excluded_rel_paths = converted.excluded_rel_paths.clone();
                changed = true;
            }
        }
    }
    (changed, n_moods)
}

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

/// Union `legacy` into `hub` without overwriting anything set in the hub.
/// Returns true when `hub` changed.
pub(super) fn merge_user_state(hub: &mut UserStateV1, legacy: UserStateV1) -> bool {
    let mut changed = false;
    for (k, v) in legacy.track_moods {
        if !hub.track_moods.contains_key(&k) {
            hub.track_moods.insert(k, v);
            changed = true;
        }
    }
    for (k, v) in legacy.play_counts {
        let cur = hub
            .play_counts
            .get(&k)
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let n = v.as_u64().unwrap_or(0);
        if n > cur {
            hub.play_counts.insert(k, Value::from(n));
            changed = true;
        }
    }
    if hub.recent_rel_paths.is_empty() && !legacy.recent_rel_paths.is_empty() {
        hub.recent_rel_paths = legacy.recent_rel_paths;
        changed = true;
    }
    for p in legacy.excluded_rel_paths {
        if !hub.excluded_rel_paths.contains(&p) {
            hub.excluded_rel_paths.push(p);
            changed = true;
        }
    }
    for id in legacy.excluded_album_ids {
        if !hub.excluded_album_ids.contains(&id) {
            hub.excluded_album_ids.push(id);
            changed = true;
        }
    }
    for (k, v) in legacy.settings {
        if !hub.settings.contains_key(&k) {
            hub.settings.insert(k, v);
            changed = true;
        }
    }
    changed
}

/// Merge legacy accounts into the hub registry: same id or same display name
/// maps onto the hub account, everything else is added; hub accounts are
/// never dropped. Returns (legacy id → hub id, accounts added).
pub(super) fn merge_legacy_registry(
    data_dir: &Path,
    legacy: &[Account],
) -> Result<(BTreeMap<String, String>, u32)> {
    let hub = accounts::ensure_accounts(data_dir)?;
    let (targets, id_map) = resolve_restore_account_targets(legacy, &hub);
    let mut merged = hub.clone();
    let mut added = 0u32;
    for acc in targets {
        if !merged.iter().any(|h| h.id == acc.id) {
            merged.push(acc);
            added += 1;
        }
    }
    if added > 0 {
        accounts::replace_accounts_registry(data_dir, &merged)?;
    }
    Ok((id_map, added))
}

/// Import registry + per-account favorites, playlists, selection, theme-bg and
/// user-state from `music_root/.kord`, merging with the hub (see
/// [`LegacyImportMode::Merge`]).
/// Returns `(accounts, moods, favorites, playlists, playlist_tracks, selections, registry)`.
pub fn import_legacy_accounts_personal_data(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
) -> Result<(u32, u32, u32, u32, u32, u32, u32)> {
    import_legacy_accounts_personal_data_with(db, data_dir, music_root, LegacyImportMode::Merge)
}

pub fn import_legacy_accounts_personal_data_with(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    mode: LegacyImportMode,
) -> Result<(u32, u32, u32, u32, u32, u32, u32)> {
    let (counts, _) =
        import_legacy_accounts_personal_data_opts(db, data_dir, music_root, mode, false)?;
    Ok(counts)
}

/// Has this hub account been used in next yet? Favorites, playlists or any
/// setting count; play counts / moods alone do not (old builds copied those
/// from the legacy library after every scan).
fn hub_account_is_fresh(db: &Db, data_dir: &Path, account_id: &str) -> bool {
    let favorites = db
        .export_favorite_rel_paths(account_id)
        .map(|v| v.is_empty())
        .unwrap_or(false);
    let playlists = db
        .list_playlists(account_id)
        .map(|v| v.is_empty())
        .unwrap_or(false);
    favorites
        && playlists
        && user_state::load_user_state(data_dir, account_id)
            .settings
            .is_empty()
}

/// Merge legacy `plectrBests` into the client's `settings.plectr` store
/// (`apps/client-ui/src/lib/plectr/records.ts`): the better record per track
/// wins (score, then accuracy). Returns true when the store changed.
pub(super) fn merge_plectr_bests(
    settings: &mut serde_json::Map<String, Value>,
    bests: &Value,
) -> bool {
    let Some(bests) = bests.as_object().filter(|b| !b.is_empty()) else {
        return false;
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
    let mut changed = false;
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
            return false;
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
                changed = true;
                if let Some(at) = updated {
                    if latest.as_deref().is_none_or(|l| at > l) {
                        latest = Some(at.to_string());
                    }
                }
            }
        }
    }
    if !changed {
        return false;
    }
    if let (Some(obj), Some(at)) = (store.as_object_mut(), latest) {
        obj.insert("lastRunAt".into(), Value::String(at));
    }
    settings.insert("plectr".into(), store);
    true
}

/// What happened to one legacy account during an import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAccountOutcome {
    pub hub_id: String,
    pub imported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `(accounts, moods, favorites, playlists, playlist_tracks, selections, registry)`.
pub type PersonalImportCounts = (u32, u32, u32, u32, u32, u32, u32);

/// Personal data import. `only_fresh`: accounts already used in next are
/// left alone (automatic first-start import); the explicit sync merges.
pub fn import_legacy_accounts_personal_data_opts(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    mode: LegacyImportMode,
    only_fresh: bool,
) -> Result<(PersonalImportCounts, BTreeMap<String, LegacyAccountOutcome>)> {
    let mut outcomes: BTreeMap<String, LegacyAccountOutcome> = BTreeMap::new();
    let kord = music_root.join(".kord");
    if !kord.is_dir() {
        return Ok(((0, 0, 0, 0, 0, 0, 0), outcomes));
    }

    let mut registry_n = 0u32;
    let list = load_legacy_accounts_registry(music_root);
    let mut id_map: BTreeMap<String, String> = BTreeMap::new();
    match mode {
        LegacyImportMode::Replace => {
            if !list.is_empty() {
                registry_n = accounts::replace_accounts_registry(data_dir, &list)?.len() as u32;
            }
        }
        LegacyImportMode::Merge => {
            if !list.is_empty() {
                let (map, added) = merge_legacy_registry(data_dir, &list)?;
                id_map = map;
                registry_n = added;
            }
        }
    }
    // The legacy name of the default account ("Default") replaces the stock
    // one a fresh hub starts with.
    if let Some(legacy_default) = list.iter().find(|a| a.id == DEFAULT_ACCOUNT_ID) {
        let hub = accounts::ensure_accounts(data_dir)?;
        // "Locale" was the stock name of earlier next builds.
        let rename = hub.iter().any(|a| {
            a.id == DEFAULT_ACCOUNT_ID
                && (a.name == accounts::DEFAULT_ACCOUNT_NAME || a.name == "Locale")
                && a.name != legacy_default.name
        });
        if rename {
            accounts::update_account(data_dir, DEFAULT_ACCOUNT_ID, Some(&legacy_default.name))?;
        }
    }
    let registered: std::collections::HashSet<String> = accounts::ensure_accounts(data_dir)?
        .into_iter()
        .map(|a| a.id)
        .collect();

    let album_folder_to_id: BTreeMap<String, i64> = db
        .list_albums()
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.folder_key.replace('\\', "/"), a.id))
        .collect();

    let mut accounts_synced = 0u32;
    let mut moods_imported = 0u32;
    let mut favorites_linked = 0u32;
    let mut playlists_imported = 0u32;
    let mut playlist_tracks_linked = 0u32;
    let mut selections_imported = 0u32;

    for entry in fs::read_dir(&kord).with_context(|| format!("read {}", kord.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(legacy_id) = account_id_from_info_dir_name(&name) else {
            continue;
        };
        let account_id = id_map.get(&legacy_id).cloned().unwrap_or(legacy_id.clone());
        // Folders of accounts deleted in legacy (or never registered) must not
        // turn into hub state files.
        if !registered.contains(&account_id) {
            outcomes.insert(
                legacy_id,
                LegacyAccountOutcome {
                    hub_id: account_id,
                    imported: false,
                    reason: Some("not_registered".into()),
                },
            );
            continue;
        }
        if only_fresh && !hub_account_is_fresh(db, data_dir, &account_id) {
            outcomes.insert(
                legacy_id,
                LegacyAccountOutcome {
                    hub_id: account_id,
                    imported: false,
                    reason: Some("hub_account_in_use".into()),
                },
            );
            continue;
        }
        outcomes.insert(
            legacy_id,
            LegacyAccountOutcome {
                hub_id: account_id.clone(),
                imported: true,
                reason: None,
            },
        );
        let info_dir = entry.path();
        let mut touched = false;

        let legacy_path = info_dir.join("user-state.json");
        if legacy_path.is_file() {
            let raw = match fs::read_to_string(&legacy_path) {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, path = %legacy_path.display(), "skip legacy user-state");
                    String::new()
                }
            };
            if !raw.is_empty() {
                if let Ok((fav, pls)) = playlists_from_legacy_user_state(&raw) {
                    let fav: Vec<String> = fav
                        .into_iter()
                        .map(|p| normalize_import_rel_path(&p))
                        .filter(|p| !p.is_empty())
                        .collect();
                    let mut pls_norm = pls;
                    for pl in &mut pls_norm {
                        for t in &mut pl.tracks {
                            t.rel_path = normalize_import_rel_path(&t.rel_path);
                        }
                    }
                    let (f, (p, t)) = match mode {
                        LegacyImportMode::Replace => (
                            db.replace_favorites_by_rel_paths(&account_id, &fav)?,
                            db.replace_playlists_backup(&account_id, &pls_norm)?,
                        ),
                        LegacyImportMode::Merge => (
                            db.merge_favorites_by_rel_paths(&account_id, &fav)?,
                            db.merge_playlists_backup(&account_id, &pls_norm)?,
                        ),
                    };
                    favorites_linked += f;
                    playlists_imported += p;
                    playlist_tracks_linked += t;
                    touched |= f > 0 || p > 0 || t > 0 || mode == LegacyImportMode::Replace;
                }
                let plectr = serde_json::from_str::<Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("plectrBests").cloned())
                    .unwrap_or(Value::Null);
                match user_state::user_state_from_legacy_json(&raw) {
                    Ok(mut ustate) => {
                        remap_legacy_excluded_albums(&mut ustate, &album_folder_to_id);
                        match mode {
                            LegacyImportMode::Replace => {
                                moods_imported += ustate.track_moods.len() as u32;
                                user_state::update_user_state(data_dir, &account_id, None, |s| {
                                    *s = ustate;
                                    merge_plectr_bests(&mut s.settings, &plectr);
                                })?;
                                touched = true;
                            }
                            LegacyImportMode::Merge => {
                                // Dry run first: an unchanged state is not rewritten.
                                let mut probe = user_state::load_user_state(data_dir, &account_id);
                                let before = probe.track_moods.len();
                                let merged = merge_user_state(&mut probe, ustate.clone());
                                let plectr_changed =
                                    merge_plectr_bests(&mut probe.settings, &plectr);
                                if merged || plectr_changed {
                                    moods_imported += (probe.track_moods.len() - before) as u32;
                                    user_state::update_user_state(
                                        data_dir,
                                        &account_id,
                                        None,
                                        |s| {
                                            merge_user_state(s, ustate);
                                            merge_plectr_bests(&mut s.settings, &plectr);
                                        },
                                    )?;
                                    touched = true;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            path = %legacy_path.display(),
                            "legacy user-state convert failed"
                        );
                    }
                }
            }
        }

        // Selection may live under *_info (legacy) — next uses accounts/{id}/.
        let sel_src = info_dir.join("library-selection.json");
        // New accounts get an empty selection file: that is not a choice.
        let hub_has_selection = account_id == DEFAULT_ACCOUNT_ID
            || selection::read_library_selection(data_dir, &account_id)
                .map(|s| {
                    selection::get_selection_filter_mode(&s)
                        != selection::SelectionFilterMode::Empty
                })
                .unwrap_or(false);
        if sel_src.is_file() && (mode == LegacyImportMode::Replace || !hub_has_selection) {
            if let Ok(raw) = fs::read_to_string(&sel_src) {
                if let Ok(sel) = serde_json::from_str::<selection::LibrarySelection>(&raw) {
                    if selection::write_library_selection(data_dir, &account_id, &sel).is_ok() {
                        selections_imported += 1;
                        touched = true;
                    }
                }
            }
        }

        let hub_has_theme = user_state::find_theme_bg_path(data_dir, &account_id).is_some();
        if mode == LegacyImportMode::Replace || !hub_has_theme {
            for bg in [
                "theme-bg.jpg",
                "theme-bg.jpeg",
                "theme-bg.png",
                "theme-bg.webp",
                "theme-bg.gif",
            ] {
                let src = info_dir.join(bg);
                if !src.is_file() {
                    continue;
                }
                let ext = Path::new(bg)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("jpg");
                let dest = user_state::theme_bg_path_for_ext(data_dir, &account_id, ext);
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = user_state::delete_theme_bg(data_dir, &account_id);
                if fs::copy(&src, &dest).is_ok() {
                    touched = true;
                }
                break;
            }
        }

        if touched {
            accounts_synced += 1;
        }
    }

    Ok((
        (
            accounts_synced,
            moods_imported,
            favorites_linked,
            playlists_imported,
            playlist_tracks_linked,
            selections_imported,
            registry_n,
        ),
        outcomes,
    ))
}

/// One-shot: merge studio metadata from sidecars + `.kord/rekord.db`, and
/// merge personal data (moods, excludes, settings, favorites, playlists,
/// selection) from `.kord/{account}_info/user-state.json` into the hub.
/// Never drops next-only accounts, favorites or playlists.
pub fn sync_legacy_library_data(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
) -> Result<LegacySyncReport> {
    sync_legacy_library_data_with(db, data_dir, music_root, LegacyImportMode::Merge)
}

pub fn sync_legacy_library_data_with(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
    mode: LegacyImportMode,
) -> Result<LegacySyncReport> {
    let (album_meta_merged, track_meta_merged) = sync_restored_library_metadata(db, music_root)?;
    // After import: drop stubs reintroduced by bad sidecars (e.g. genre "e").
    let _ = db.clear_weak_studio_placeholders();

    let (
        accounts_moods_synced,
        moods_imported,
        favorites_linked,
        playlists_imported,
        playlist_tracks_linked,
        selections_imported,
        accounts_registry,
    ) = import_legacy_accounts_personal_data_with(db, data_dir, music_root, mode)?;

    info!(
        album_meta_merged,
        track_meta_merged,
        accounts_moods_synced,
        moods_imported,
        favorites_linked,
        playlists_imported,
        playlist_tracks_linked,
        selections_imported,
        accounts_registry,
        ?mode,
        "legacy library sync finished"
    );
    Ok(LegacySyncReport {
        album_meta_merged,
        track_meta_merged,
        accounts_moods_synced,
        moods_imported,
        favorites_linked,
        playlists_imported,
        playlist_tracks_linked,
        selections_imported,
        accounts_registry,
    })
}

/// Marker of the automatic legacy import (`<data_dir>/legacy-import.json`):
/// once written, scans and restarts never import legacy data again, so
/// whatever is cleared in next stays cleared. The explicit "sync legacy meta"
/// remains available as a merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportMarker {
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
}

pub const LEGACY_IMPORT_MARKER: &str = "legacy-import.json";

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

/// A legacy library (`.kord`) is next to the music and was never imported.
pub fn legacy_import_pending(data_dir: &Path, music_root: &Path) -> bool {
    let opted_out = std::env::var("REKORD_SKIP_LEGACY_IMPORT")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false);
    !opted_out
        && music_root.join(".kord").is_dir()
        && read_legacy_import_marker(data_dir)
            .and_then(|m| m.imported_at)
            .is_none()
}

/// First start on a legacy library: import everything once (library
/// metadata, accounts registry, settings, favorites, playlists, selections,
/// play counts, moods, exclusions, theme backgrounds, Plectr records) and
/// write the marker. Accounts already used in next are not touched. Needs an
/// indexed catalog (album ids, track links): run it after a scan.
pub fn auto_import_legacy_once(
    db: &Db,
    data_dir: &Path,
    music_root: &Path,
) -> Result<Option<LegacySyncReport>> {
    if !legacy_import_pending(data_dir, music_root) {
        return Ok(None);
    }
    // A broken legacy database must not block the personal data (nor be
    // retried on every scan): log it and go on.
    let (album_meta_merged, track_meta_merged) = sync_restored_library_metadata(db, music_root)
        .unwrap_or_else(|e| {
            warn!(error = %e, "legacy library metadata import failed");
            (0, 0)
        });
    let _ = db.clear_weak_studio_placeholders();
    let (counts, outcomes) = import_legacy_accounts_personal_data_opts(
        db,
        data_dir,
        music_root,
        LegacyImportMode::Merge,
        true,
    )?;
    let (
        accounts_moods_synced,
        moods_imported,
        favorites_linked,
        playlists_imported,
        playlist_tracks_linked,
        selections_imported,
        accounts_registry,
    ) = counts;
    write_legacy_import_marker(
        data_dir,
        &LegacyImportMarker {
            imported_at: Some(chrono::Utc::now().to_rfc3339()),
            music_root: Some(music_root.to_string_lossy().into_owned()),
            album_meta_merged,
            track_meta_merged,
            accounts: outcomes,
        },
    )?;
    info!(
        album_meta_merged,
        track_meta_merged,
        accounts_moods_synced,
        favorites_linked,
        playlists_imported,
        selections_imported,
        accounts_registry,
        "legacy library imported (first start)"
    );
    Ok(Some(LegacySyncReport {
        album_meta_merged,
        track_meta_merged,
        accounts_moods_synced,
        moods_imported,
        favorites_linked,
        playlists_imported,
        playlist_tracks_linked,
        selections_imported,
        accounts_registry,
    }))
}
