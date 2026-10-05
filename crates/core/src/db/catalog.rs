//! Filesystem catalog writes: scan upserts (batched in one transaction per
//! album), pruning of vanished files and the parking of user links.
//!
//! Favorites and playlist entries reference `tracks.id` with `ON DELETE
//! CASCADE`. Whenever the *catalog* (not the user) drops a track, its user
//! links are first parked by rel_path; the `tracks_reattach_user_links`
//! trigger puts them back as soon as a track with that rel_path is indexed
//! again (remounted drive, interrupted full rescan, restored backup).

use super::{Db, UserLinkSnapshot};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// One track row as written by the scanner.
#[derive(Debug, Clone)]
pub struct TrackRow<'a> {
    pub rel_path: &'a str,
    pub file_path: &'a Path,
    pub title: &'a str,
    pub artist_name: &'a str,
    pub album_name: &'a str,
    pub duration_ms: i64,
    pub track_number: Option<i64>,
    pub album_id: Option<i64>,
    pub artist_id: Option<i64>,
    pub size: u64,
    pub mtime: i64,
    pub genre: Option<&'a str>,
    pub release_date: Option<&'a str>,
    pub lyrics: Option<&'a str>,
    pub bpm: Option<f64>,
    pub disc_number: Option<i64>,
    /// Album title from the file's tags (the album row picks the most common).
    pub tag_album: Option<&'a str>,
}

/// Catalog writes sharing one transaction; see [`Db::write_batch`].
pub struct CatalogBatch<'a> {
    conn: &'a Connection,
}

impl CatalogBatch<'_> {
    /// Store (or, with `None`, drop) the synthetic Xing frame `/media`
    /// splices into an MP3 at `insert_at`; valid while size and mtime match.
    pub fn set_mp3_seek_header(
        &self,
        rel_path: &str,
        size: u64,
        mtime: i64,
        header: Option<(u64, &[u8])>,
    ) -> Result<()> {
        match header {
            Some((insert_at, frame)) => self.conn.execute(
                "INSERT INTO mp3_seek_headers(rel_path, size, mtime, insert_at, frame)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(rel_path) DO UPDATE SET size = excluded.size,
                   mtime = excluded.mtime, insert_at = excluded.insert_at,
                   frame = excluded.frame",
                params![rel_path, size as i64, mtime, insert_at as i64, frame],
            )?,
            None => self.conn.execute(
                "DELETE FROM mp3_seek_headers WHERE rel_path = ?1",
                params![rel_path],
            )?,
        };
        Ok(())
    }

    pub fn upsert_artist(&self, name: &str) -> Result<i64> {
        upsert_artist(self.conn, name)
    }

    pub fn upsert_album(
        &self,
        name: &str,
        artist_name: &str,
        artist_id: Option<i64>,
        folder_key: &str,
        cover_path: Option<&Path>,
        loose: bool,
    ) -> Result<i64> {
        upsert_album(
            self.conn,
            name,
            artist_name,
            artist_id,
            folder_key,
            cover_path,
            loose,
        )
    }

    /// Current display title of an album row.
    pub fn album_name(&self, album_id: i64) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT name FROM albums WHERE id = ?1",
            params![album_id],
            |r| r.get(0),
        )?)
    }

    /// See [`sync_album_display`].
    pub fn sync_album_display(
        &self,
        album_id: i64,
        folder_title: &str,
        use_tags: bool,
    ) -> Result<()> {
        sync_album_display(self.conn, album_id, folder_title, use_tags)
    }

    /// Mark an album as updated (new or changed files).
    pub fn touch_album(&self, album_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE albums SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
            params![album_id],
        )?;
        Ok(())
    }

    pub fn upsert_track(&self, row: &TrackRow<'_>) -> Result<i64> {
        upsert_track(self.conn, row)
    }

    pub fn relink_track(
        &self,
        rel_path: &str,
        album_id: i64,
        artist_id: i64,
        artist_name: &str,
        album_name: &str,
    ) -> Result<()> {
        relink_track(
            self.conn,
            rel_path,
            album_id,
            artist_id,
            artist_name,
            album_name,
        )
    }

    pub fn backfill_album_meta_from_tracks(&self, album_id: i64) -> Result<()> {
        backfill_album_meta_from_tracks(self.conn, album_id)
    }
}

/// Result of [`Db::prune_tracks_outside_guarded`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneOutcome {
    /// Tracks indexed before the prune.
    pub indexed: u64,
    /// Indexed tracks whose rel_path was not seen on disk.
    pub missing: u64,
    /// Tracks actually deleted (0 when skipped).
    pub removed: u64,
    /// Why the guard refused to prune, if it did.
    pub skipped: Option<String>,
}

pub(super) fn upsert_artist(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO artists(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        params![name],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM artists WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub(super) fn upsert_album(
    conn: &Connection,
    name: &str,
    artist_name: &str,
    artist_id: Option<i64>,
    folder_key: &str,
    cover_path: Option<&Path>,
    loose: bool,
) -> Result<i64> {
    let cover = cover_path.map(|p| p.to_string_lossy().into_owned());
    let has_cover = cover.is_some();
    let cover_version = cover_path.and_then(cover_version_of);
    // The display title of an existing album is kept: it comes from the tags
    // or a curated value (see `sync_album_display`), not from the folder.
    conn.execute(
        r#"
        INSERT INTO albums(name, artist_name, artist_id, folder_key, cover_path, has_cover, loose, cover_version)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(folder_key) DO UPDATE SET
          artist_name=excluded.artist_name,
          artist_id=COALESCE(excluded.artist_id, albums.artist_id),
          cover_path=excluded.cover_path,
          has_cover=excluded.has_cover,
          loose=excluded.loose,
          cover_version=excluded.cover_version
        "#,
        params![
            name,
            artist_name,
            artist_id,
            folder_key,
            cover,
            has_cover as i64,
            loose as i64,
            cover_version
        ],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM albums WHERE folder_key = ?1",
        params![folder_key],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub(super) fn upsert_track(conn: &Connection, row: &TrackRow<'_>) -> Result<i64> {
    // Curated fields (`edited_fields`, see `db::field`) keep their value: a
    // re-read of the tags never replaces a sidecar, legacy or Studio value.
    conn.execute(
        r#"
        INSERT INTO tracks(
          rel_path, file_path, album_id, artist_id, title, artist_name, album_name,
          duration_ms, track_number, size, mtime, genre, release_date, lyrics, bpm,
          disc_number, tag_album
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
        ON CONFLICT(rel_path) DO UPDATE SET
          file_path=excluded.file_path,
          album_id=excluded.album_id,
          artist_id=excluded.artist_id,
          title=CASE WHEN tracks.edited_fields & 1 THEN tracks.title ELSE excluded.title END,
          artist_name=excluded.artist_name,
          album_name=excluded.album_name,
          duration_ms=excluded.duration_ms,
          track_number=CASE WHEN tracks.edited_fields & 8 THEN tracks.track_number
            ELSE COALESCE(excluded.track_number, tracks.track_number) END,
          disc_number=CASE WHEN tracks.edited_fields & 16 THEN tracks.disc_number
            ELSE COALESCE(excluded.disc_number, tracks.disc_number) END,
          size=excluded.size,
          mtime=excluded.mtime,
          genre=CASE WHEN tracks.edited_fields & 4 THEN tracks.genre
            ELSE COALESCE(excluded.genre, tracks.genre) END,
          release_date=CASE WHEN tracks.edited_fields & 2 THEN tracks.release_date
            ELSE COALESCE(excluded.release_date, tracks.release_date) END,
          lyrics=COALESCE(excluded.lyrics, tracks.lyrics),
          bpm=COALESCE(excluded.bpm, tracks.bpm),
          tag_album=excluded.tag_album
        "#,
        params![
            row.rel_path,
            row.file_path.to_string_lossy().as_ref(),
            row.album_id,
            row.artist_id,
            row.title,
            row.artist_name,
            row.album_name,
            row.duration_ms,
            row.track_number,
            row.size as i64,
            row.mtime,
            row.genre,
            row.release_date,
            row.lyrics,
            row.bpm,
            row.disc_number,
            row.tag_album
        ],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM tracks WHERE rel_path = ?1",
        params![row.rel_path],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO files(rel_path, size, mtime) VALUES (?1,?2,?3)
         ON CONFLICT(rel_path) DO UPDATE SET size=excluded.size, mtime=excluded.mtime",
        params![row.rel_path, row.size as i64, row.mtime],
    )?;
    Ok(id)
}

/// Re-point a track at its album/artist rows. `album_name` is the album's
/// current display title; it is only written when it actually differs, so a
/// routine scan does not bump `updated_at` (the delta cursor).
pub(super) fn relink_track(
    conn: &Connection,
    rel_path: &str,
    album_id: i64,
    artist_id: i64,
    artist_name: &str,
    album_name: &str,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE tracks SET
          album_id = ?2,
          artist_id = ?3,
          artist_name = ?4,
          album_name = ?5
        WHERE rel_path = ?1
          AND (album_id IS NOT ?2 OR artist_id IS NOT ?3
               OR artist_name <> ?4 OR album_name <> ?5)
        "#,
        params![rel_path, album_id, artist_id, artist_name, album_name],
    )?;
    Ok(())
}

/// Album display title: unless curated, the most common album tag of its
/// tracks, else the folder name (both through `album_display_title`). The
/// tracks' `album_name` follows.
pub(super) fn sync_album_display(
    conn: &Connection,
    album_id: i64,
    folder_title: &str,
    use_tags: bool,
) -> Result<()> {
    let tag: Option<String> = if use_tags {
        conn.query_row(
            r#"
            SELECT trim(tag_album) FROM tracks
            WHERE album_id = ?1 AND tag_album IS NOT NULL AND trim(tag_album) != ''
            GROUP BY trim(tag_album)
            ORDER BY COUNT(*) DESC, trim(tag_album)
            LIMIT 1
            "#,
            params![album_id],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    let name = if use_tags {
        super::text::album_display_title(tag.as_deref().unwrap_or(folder_title))
    } else {
        folder_title.to_string()
    };
    conn.execute(
        &format!(
            "UPDATE albums SET name = ?2 WHERE id = ?1 AND (edited_fields & {}) = 0 AND name IS NOT ?2",
            super::field::TITLE
        ),
        params![album_id, name],
    )?;
    conn.execute(
        r#"
        UPDATE tracks SET album_name = (SELECT name FROM albums WHERE id = ?1)
        WHERE album_id = ?1
          AND album_name IS NOT (SELECT name FROM albums WHERE id = ?1)
        "#,
        params![album_id],
    )?;
    Ok(())
}

/// Most common value among the most precise dates (full date over year-month
/// over year).
fn best_track_date(dates: &[String]) -> Option<String> {
    let best = dates
        .iter()
        .map(|d| super::text::date_precision(d))
        .max()
        .filter(|p| *p > 0)?;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for d in dates
        .iter()
        .filter(|d| super::text::date_precision(d) == best)
    {
        *counts.entry(d.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(d, _)| d.to_string())
}

/// Album genre / release date from its tracks when the album has none of its
/// own (curated values are never touched). A more precise track date of the
/// same year replaces a bare year.
pub(super) fn backfill_album_meta_from_tracks(conn: &Connection, album_id: i64) -> Result<()> {
    use super::field;
    let (edited, cur_date, cur_genre): (i64, Option<String>, Option<String>) = conn.query_row(
        "SELECT edited_fields, release_date, genre FROM albums WHERE id = ?1",
        params![album_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut stmt = conn.prepare("SELECT release_date, genre FROM tracks WHERE album_id = ?1")?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map(params![album_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .flatten()
        .collect();
    if edited & field::RELEASE_DATE == 0 {
        let dates: Vec<String> = rows
            .iter()
            .filter_map(|(d, _)| d.as_deref().map(str::trim))
            .filter(|d| !d.is_empty())
            .map(str::to_string)
            .collect();
        if let Some(best) = best_track_date(&dates) {
            let cur = cur_date.as_deref().map(str::trim).unwrap_or("");
            let replace = cur.is_empty()
                || (super::text::date_precision(cur) < super::text::date_precision(&best)
                    && best.starts_with(cur));
            if replace && cur != best {
                conn.execute(
                    "UPDATE albums SET release_date = ?2 WHERE id = ?1",
                    params![album_id, best],
                )?;
            }
        }
    }
    if edited & field::GENRE == 0 && cur_genre.as_deref().map(str::trim).unwrap_or("").is_empty() {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for g in rows
            .iter()
            .filter_map(|(_, g)| g.as_deref().map(str::trim))
            .filter(|g| !g.is_empty())
        {
            *counts.entry(g).or_default() += 1;
        }
        if let Some((g, _)) = counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        {
            conn.execute(
                "UPDATE albums SET genre = ?2 WHERE id = ?1",
                params![album_id, g],
            )?;
        }
    }
    Ok(())
}

/// Cache-busting token for a cover file: changes with its size or mtime.
pub(super) fn cover_version_of(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Some(format!("{:x}-{:x}", mtime, meta.len()))
}

fn now_stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Park the user links of every track matched by `track_filter` (a SQL
/// condition over alias `t`), so they survive the track rows being deleted.
fn park_user_links(conn: &Connection, track_filter: &str) -> Result<()> {
    let now = now_stamp();
    conn.execute(
        &format!(
            r#"
            INSERT OR REPLACE INTO parked_favorites(account_id, rel_path, created_at, parked_at)
            SELECT f.account_id, t.rel_path, f.created_at, ?1
            FROM favorites f JOIN tracks t ON t.id = f.track_id
            WHERE {track_filter}
            "#
        ),
        params![now],
    )?;
    conn.execute(
        &format!(
            r#"
            INSERT OR REPLACE INTO parked_playlist_tracks(playlist_id, rel_path, position, parked_at)
            SELECT pt.playlist_id, t.rel_path, pt.position, ?1
            FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id
            WHERE {track_filter}
            "#
        ),
        params![now],
    )?;
    Ok(())
}

/// Park one user link for a rel_path that is not (yet) in the catalog.
pub(super) fn park_favorite(
    conn: &Connection,
    account_id: &str,
    rel_path: &str,
    created_at: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO parked_favorites(account_id, rel_path, created_at, parked_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        params![account_id, rel_path, created_at, now_stamp()],
    )?;
    Ok(())
}

pub(super) fn park_playlist_track(
    conn: &Connection,
    playlist_id: &str,
    rel_path: &str,
    position: i64,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO parked_playlist_tracks(playlist_id, rel_path, position, parked_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        params![playlist_id, rel_path, position, now_stamp()],
    )?;
    Ok(())
}

pub(super) fn track_id_by_rel(conn: &Connection, rel: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM tracks WHERE rel_path = ?1",
            params![rel],
            |r| r.get(0),
        )
        .optional()?)
}

fn fill_scan_seen(conn: &Connection, seen: &HashSet<String>) -> Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS scan_seen (rel_path TEXT PRIMARY KEY); DELETE FROM scan_seen;",
    )?;
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO scan_seen(rel_path) VALUES (?1)")?;
    for rel in seen {
        stmt.execute(params![rel])?;
    }
    Ok(())
}

/// Synthetic Xing frame of an indexed MP3, see [`CatalogBatch::set_mp3_seek_header`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3SeekHeaderRow {
    pub size: u64,
    /// Seconds since the epoch, as the scan stored it.
    pub mtime: i64,
    pub insert_at: u64,
    pub frame: Vec<u8>,
}

impl Db {
    /// The synthetic Xing frame stored for `rel_path`, if any.
    pub fn mp3_seek_header(&self, rel_path: &str) -> Result<Option<Mp3SeekHeaderRow>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT size, mtime, insert_at, frame FROM mp3_seek_headers WHERE rel_path = ?1",
                params![rel_path],
                |r| {
                    Ok(Mp3SeekHeaderRow {
                        size: r.get::<_, i64>(0)? as u64,
                        mtime: r.get(1)?,
                        insert_at: r.get::<_, i64>(2)? as u64,
                        frame: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// Run catalog writes in one IMMEDIATE transaction (one commit, one fsync).
    pub fn write_batch<T>(&self, f: impl FnOnce(&CatalogBatch<'_>) -> Result<T>) -> Result<T> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&CatalogBatch { conn: &tx })?;
        tx.commit()?;
        Ok(out)
    }

    /// Snapshot favorites/playlist membership by rel_path.
    ///
    /// Kept for callers that want an in-memory copy; catalog rebuilds no longer
    /// need it because user links are parked in the database.
    pub fn snapshot_user_links(&self) -> Result<UserLinkSnapshot> {
        let conn = self.lock();
        let mut favs = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT f.account_id, t.rel_path FROM favorites f JOIN tracks t ON t.id = f.track_id",
            )?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows.flatten() {
                favs.push(row);
            }
        }
        let mut playlist_tracks = Vec::new();
        {
            let mut stmt = conn.prepare(
                r#"
                SELECT pt.playlist_id, t.rel_path, pt.position
                FROM playlist_tracks pt
                JOIN tracks t ON t.id = pt.track_id
                "#,
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows.flatten() {
                playlist_tracks.push(row);
            }
        }
        Ok(UserLinkSnapshot {
            favorite_rel_paths: favs,
            playlist_tracks,
        })
    }

    /// Re-attach a snapshot; rel_paths not in the catalog are parked so they
    /// come back when their file is indexed.
    pub fn restore_user_links(&self, snap: &UserLinkSnapshot) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for (account_id, rel) in &snap.favorite_rel_paths {
            match track_id_by_rel(&tx, rel)? {
                Some(id) => {
                    tx.execute(
                        "INSERT OR IGNORE INTO favorites(account_id, track_id, created_at) VALUES (?1, ?2, ?3)",
                        params![account_id, id, now],
                    )?;
                }
                None => park_favorite(&tx, account_id, rel, &now)?,
            }
        }
        for (playlist_id, rel, position) in &snap.playlist_tracks {
            match track_id_by_rel(&tx, rel)? {
                Some(id) => {
                    tx.execute(
                        "INSERT OR IGNORE INTO playlist_tracks(playlist_id, track_id, position) VALUES (?1,?2,?3)",
                        params![playlist_id, id, position],
                    )?;
                }
                None => park_playlist_track(&tx, playlist_id, rel, *position)?,
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn track_id_by_rel(&self, rel: &str) -> Result<Option<i64>> {
        track_id_by_rel(&self.lock(), rel)
    }

    /// `rel_path -> (size, mtime)` for every indexed file, to skip unchanged files.
    pub fn file_states(&self) -> Result<HashMap<String, (i64, i64)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT f.rel_path, f.size, f.mtime
            FROM files f
            JOIN tracks t ON t.rel_path = f.rel_path
            "#,
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
            ))
        })?;
        let mut map = HashMap::new();
        for row in rows.flatten() {
            map.insert(row.0, row.1);
        }
        Ok(map)
    }

    /// Every indexed rel_path.
    pub fn all_track_rel_paths(&self) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT rel_path FROM tracks")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    }

    /// Indexed rel_paths under `prefix/` (used to protect unreadable folders
    /// from being pruned).
    pub fn track_rel_paths_under(&self, prefix: &str) -> Result<Vec<String>> {
        let conn = self.lock();
        let prefix = prefix.trim_end_matches('/');
        let mut stmt =
            conn.prepare("SELECT rel_path FROM tracks WHERE substr(rel_path, 1, ?2) = ?1")?;
        let like = format!("{prefix}/");
        let rows = stmt.query_map(params![like, like.chars().count() as i64], |r| {
            r.get::<_, String>(0)
        })?;
        Ok(rows.flatten().collect())
    }

    /// Re-point an unchanged track at the current album/artist rows without
    /// touching any metadata edited from Studio.
    pub fn relink_track(
        &self,
        rel_path: &str,
        album_id: i64,
        artist_id: i64,
        artist_name: &str,
        album_name: &str,
    ) -> Result<()> {
        relink_track(
            &self.lock(),
            rel_path,
            album_id,
            artist_id,
            artist_name,
            album_name,
        )
    }

    /// Drop tracks whose files are no longer on disk, unconditionally. Their
    /// favorites / playlist entries are parked, not lost.
    pub fn prune_tracks_outside(&self, seen: &HashSet<String>) -> Result<u64> {
        Ok(self
            .prune_tracks_outside_guarded(seen, |_, _| Ok(()))?
            .removed)
    }

    /// Like [`Db::prune_tracks_outside`], but `allow(indexed, missing)` decides
    /// (inside the same transaction) whether the deletion may proceed; an `Err`
    /// reason leaves the catalog untouched and is reported back.
    pub fn prune_tracks_outside_guarded(
        &self,
        seen: &HashSet<String>,
        allow: impl FnOnce(u64, u64) -> std::result::Result<(), String>,
    ) -> Result<PruneOutcome> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        fill_scan_seen(&tx, seen)?;
        let indexed: i64 = tx.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))?;
        let missing: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tracks WHERE rel_path NOT IN (SELECT rel_path FROM scan_seen)",
            [],
            |r| r.get(0),
        )?;
        let mut outcome = PruneOutcome {
            indexed: indexed as u64,
            missing: missing as u64,
            ..Default::default()
        };
        if missing > 0 {
            if let Err(reason) = allow(outcome.indexed, outcome.missing) {
                tx.execute_batch("DELETE FROM scan_seen;")?;
                tx.commit()?;
                outcome.skipped = Some(reason);
                return Ok(outcome);
            }
        }
        park_user_links(&tx, "t.rel_path NOT IN (SELECT rel_path FROM scan_seen)")?;
        let removed = tx.execute(
            "DELETE FROM tracks WHERE rel_path NOT IN (SELECT rel_path FROM scan_seen)",
            [],
        )?;
        tx.execute(
            "DELETE FROM files WHERE rel_path NOT IN (SELECT rel_path FROM scan_seen)",
            [],
        )?;
        tx.execute(
            "DELETE FROM mp3_seek_headers WHERE rel_path NOT IN (SELECT rel_path FROM scan_seen)",
            [],
        )?;
        tx.execute_batch("DELETE FROM scan_seen;")?;
        tx.commit()?;
        outcome.removed = removed as u64;
        Ok(outcome)
    }

    pub fn prune_empty_albums(&self) -> Result<u64> {
        let conn = self.lock();
        let removed = conn.execute(
            "DELETE FROM albums WHERE NOT EXISTS (SELECT 1 FROM tracks t WHERE t.album_id = albums.id)",
            [],
        )?;
        Ok(removed as u64)
    }

    pub fn prune_empty_artists(&self) -> Result<u64> {
        let conn = self.lock();
        let removed = conn.execute(
            r#"
            DELETE FROM artists
            WHERE NOT EXISTS (SELECT 1 FROM tracks t WHERE t.artist_id = artists.id)
              AND NOT EXISTS (SELECT 1 FROM albums a WHERE a.artist_id = artists.id)
            "#,
            [],
        )?;
        Ok(removed as u64)
    }

    /// Wipe the FS catalog in one transaction. Favorites / playlist membership
    /// are parked first and re-attach as the tracks are indexed again, so a
    /// crash halfway through a rebuild loses nothing.
    pub fn clear_catalog(&self) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        park_user_links(&tx, "1")?;
        tx.execute_batch(
            r#"
            DELETE FROM playlist_tracks;
            DELETE FROM favorites;
            DELETE FROM tracks_fts;
            DELETE FROM tracks;
            DELETE FROM albums;
            DELETE FROM artists;
            DELETE FROM files;
            "#,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Forget parked user links older than `days` (files that never came back).
    pub fn purge_parked_user_links(&self, days: i64) -> Result<u64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let conn = self.lock();
        let a = conn.execute(
            "DELETE FROM parked_favorites WHERE parked_at < ?1",
            params![cutoff],
        )?;
        let b = conn.execute(
            "DELETE FROM parked_playlist_tracks WHERE parked_at < ?1",
            params![cutoff],
        )?;
        Ok((a + b) as u64)
    }

    /// `(parked favorites, parked playlist entries)` waiting for their file.
    pub fn parked_user_link_counts(&self) -> Result<(u64, u64)> {
        let conn = self.lock();
        let a: i64 = conn.query_row("SELECT COUNT(*) FROM parked_favorites", [], |r| r.get(0))?;
        let b: i64 = conn.query_row("SELECT COUNT(*) FROM parked_playlist_tracks", [], |r| {
            r.get(0)
        })?;
        Ok((a as u64, b as u64))
    }

    pub fn upsert_artist(&self, name: &str) -> Result<i64> {
        upsert_artist(&self.lock(), name)
    }

    pub fn upsert_album(
        &self,
        name: &str,
        artist_name: &str,
        artist_id: Option<i64>,
        folder_key: &str,
        cover_path: Option<&Path>,
        loose: bool,
    ) -> Result<i64> {
        upsert_album(
            &self.lock(),
            name,
            artist_name,
            artist_id,
            folder_key,
            cover_path,
            loose,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_track(
        &self,
        rel_path: &str,
        file_path: &Path,
        title: &str,
        artist_name: &str,
        album_name: &str,
        duration_ms: i64,
        track_number: Option<i64>,
        album_id: Option<i64>,
        artist_id: Option<i64>,
        size: u64,
        mtime: i64,
        genre: Option<&str>,
        release_date: Option<&str>,
        lyrics: Option<&str>,
    ) -> Result<i64> {
        upsert_track(
            &self.lock(),
            &TrackRow {
                rel_path,
                file_path,
                title,
                artist_name,
                album_name,
                duration_ms,
                track_number,
                album_id,
                artist_id,
                size,
                mtime,
                genre,
                release_date,
                lyrics,
                bpm: None,
                disc_number: None,
                tag_album: None,
            },
        )
    }

    pub fn rebuild_fts(&self) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute_batch(
            r#"
            DELETE FROM tracks_fts;
            INSERT INTO tracks_fts(rowid, title, artist_name, album_name, genre)
              SELECT id, title, artist_name, album_name, COALESCE(genre, '') FROM tracks;
            "#,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn refresh_counts(&self) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute_batch(
            r#"
            UPDATE albums SET track_count = (
              SELECT COUNT(*) FROM tracks t WHERE t.album_id = albums.id
            );
            UPDATE artists SET
              track_count = (SELECT COUNT(*) FROM tracks t WHERE t.artist_id = artists.id),
              album_count = (SELECT COUNT(*) FROM albums a WHERE a.artist_id = artists.id);
            "#,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Fill album genre/release_date from track tags when album meta is empty.
    pub fn backfill_album_meta_from_tracks(&self, album_id: i64) -> Result<()> {
        backfill_album_meta_from_tracks(&self.lock(), album_id)
    }
}
