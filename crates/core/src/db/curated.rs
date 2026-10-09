//! Curated metadata: values that come from a person or a curated source
//! (sidecars, the legacy library, Studio, metadata fetches) rather than from
//! the file tags. Writing one sets its bit in `edited_fields` (see
//! `db::field`), so scans keep it; values a person typed also set
//! `user_fields`, which imports and fetches never replace.

use super::text::{date_precision, normalize_date};
use super::{field, genres, Db};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// How a curated write meets what the hub already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuratedWrite {
    /// Only fields nobody curated yet (sidecars after every scan).
    FillUncurated,
    /// Replace everything a person did not type in the hub (legacy library
    /// import, metadata fetch).
    Override,
    /// A person typed it (Studio save): always wins.
    User,
}

#[derive(Debug, Clone, Default)]
pub struct CuratedAlbumMeta {
    pub title: Option<String>,
    pub release_date: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub country: Option<String>,
    pub musicbrainz_release_id: Option<String>,
    pub discogs_release_id: Option<String>,
    pub discogs_extra_json: Option<String>,
    pub expected_track_count: Option<i64>,
    /// RFC3339; only ever moves `added_at` earlier.
    pub added_at: Option<String>,
    /// RFC3339; replaces `updated_at` on [`CuratedWrite::Override`].
    pub updated_at: Option<String>,
    /// The source flags the row as edited by a person (legacy `user_edited`).
    pub user_edited: bool,
    /// Fields the source marks as typed by a person (`db::field` bits, e.g.
    /// sidecar `userEdited: ["title"]`): they win over values that are only
    /// curated, even when filling.
    pub user_mask: i64,
}

#[derive(Debug, Clone, Default)]
pub struct CuratedTrackMeta {
    pub title: Option<String>,
    pub release_date: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub lyrics: Option<String>,
    pub source: Option<String>,
    pub url: Option<String>,
    pub duration_ms: Option<i64>,
    pub added_at: Option<String>,
    pub user_edited: bool,
    /// See [`CuratedAlbumMeta::user_mask`].
    pub user_mask: i64,
}

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn clean_date(v: &Option<String>) -> Option<String> {
    clean(v).map(|d| normalize_date(&d).unwrap_or(d))
}

fn clean_genre(v: &Option<String>) -> Option<String> {
    clean(v).filter(|g| !super::is_weak_genre(Some(g)))
}

/// Can a write in `mode` replace the field `bit`?
fn may_write(mode: CuratedWrite, bit: i64, edited: i64, user: i64) -> bool {
    match mode {
        CuratedWrite::FillUncurated => edited & bit == 0,
        CuratedWrite::Override => user & bit == 0,
        CuratedWrite::User => true,
    }
}

/// A less precise curated date of the same year never replaces a full date
/// (e.g. legacy "1994" over a tag "1994-08-01").
fn keep_more_precise(current: Option<&str>, incoming: String) -> String {
    match current.map(str::trim).filter(|c| !c.is_empty()) {
        Some(cur)
            if date_precision(cur) > date_precision(&incoming) && cur.starts_with(&incoming) =>
        {
            cur.to_string()
        }
        _ => incoming,
    }
}

/// Accumulates the column writes of one row.
struct RowWrite {
    sets: Vec<(&'static str, rusqlite::types::Value)>,
    edited: i64,
    user: i64,
    /// `embedded_fields` of the row.
    embedded: i64,
    /// Embedded priority: fills leave values that came from the tags.
    protect_embedded: bool,
}

impl RowWrite {
    /// A value a person typed also replaces one that is only curated.
    fn allowed(&self, mode: CuratedWrite, user_flag: bool, bit: i64) -> bool {
        if mode == CuratedWrite::FillUncurated
            && self.protect_embedded
            && self.embedded & bit != 0
            && !user_flag
        {
            return false;
        }
        may_write(mode, bit, self.edited, self.user) || (user_flag && self.user & bit == 0)
    }

    /// The field now holds a curated value (no longer the tag's).
    fn mark_curated(&mut self, bit: i64, user_flag: bool) {
        self.edited |= bit;
        self.embedded &= !bit;
        if user_flag {
            self.user |= bit;
        }
    }

    fn set(&mut self, col: &'static str, v: impl Into<rusqlite::types::Value>) {
        self.sets.push((col, v.into()));
    }

    /// Curated text field: written (and flagged) unless `mode` must keep it.
    fn curated_text(
        &mut self,
        mode: CuratedWrite,
        user_flag: bool,
        bit: i64,
        col: &'static str,
        current: Option<&str>,
        incoming: Option<String>,
    ) {
        let Some(v) = incoming else { return };
        if !self.allowed(mode, user_flag, bit) {
            return;
        }
        if current != Some(v.as_str()) {
            self.set(col, v);
        }
        self.mark_curated(bit, user_flag);
    }

    fn curated_int(
        &mut self,
        mode: CuratedWrite,
        user_flag: bool,
        bit: i64,
        col: &'static str,
        current: Option<i64>,
        incoming: Option<i64>,
    ) {
        let Some(v) = incoming.filter(|n| *n > 0) else {
            return;
        };
        if !self.allowed(mode, user_flag, bit) {
            return;
        }
        if current != Some(v) {
            self.set(col, v);
        }
        self.mark_curated(bit, user_flag);
    }

    /// Plain field: filled when empty, replaced on override / user writes.
    fn plain_text(
        &mut self,
        mode: CuratedWrite,
        col: &'static str,
        current: Option<&str>,
        incoming: Option<String>,
    ) {
        let Some(v) = incoming else { return };
        let empty = current.map(str::trim).unwrap_or("").is_empty();
        let replace = mode != CuratedWrite::FillUncurated;
        if (empty || replace) && current != Some(v.as_str()) {
            self.set(col, v);
        }
    }

    /// Run the UPDATE; returns true when anything (values or flags) changed.
    fn apply(self, conn: &Connection, table: &str, id: i64, old: (i64, i64, i64)) -> Result<bool> {
        let (old_edited, old_user, old_embedded) = old;
        let mut sets = self.sets;
        if self.edited != old_edited {
            sets.push(("edited_fields", self.edited.into()));
        }
        if self.user != old_user {
            sets.push(("user_fields", self.user.into()));
        }
        if self.embedded != old_embedded {
            sets.push(("embedded_fields", self.embedded.into()));
        }
        if sets.is_empty() {
            return Ok(false);
        }
        let assignments: Vec<String> = sets
            .iter()
            .enumerate()
            .map(|(i, (col, _))| format!("{col} = ?{}", i + 2))
            .collect();
        let sql = format!(
            "UPDATE {table} SET {} WHERE id = ?1",
            assignments.join(", ")
        );
        let mut values: Vec<rusqlite::types::Value> = vec![id.into()];
        values.extend(sets.into_iter().map(|(_, v)| v));
        conn.execute(&sql, rusqlite::params_from_iter(values))?;
        Ok(true)
    }
}

type AlbumRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
    Option<String>,
    i64,
    i64,
    Option<String>,
    i64,
);

pub(super) fn apply_curated_album(
    conn: &Connection,
    folder_key: &str,
    meta: &CuratedAlbumMeta,
    mode: CuratedWrite,
    protect_embedded: bool,
) -> Result<bool> {
    let row: Option<AlbumRow> = conn
        .query_row(
            r#"
            SELECT id, name, release_date, genre, label, country, musicbrainz_release_id,
                   discogs_release_id, discogs_extra_json, expected_track_count,
                   edited_fields, user_fields, added_at, track_count, has_album_meta, updated_at,
                   embedded_fields
            FROM albums WHERE folder_key = ?1
            "#,
            params![folder_key],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                    r.get(14)?,
                    r.get(15)?,
                    r.get(16)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        name,
        date,
        genre,
        label,
        country,
        mb,
        discogs,
        extra,
        expected,
        edited,
        user,
        added_at,
        track_count,
        had_meta,
        updated_at,
        embedded,
    )) = row
    else {
        return Ok(false);
    };
    let user_flag = meta.user_edited || mode == CuratedWrite::User;
    let mut w = RowWrite {
        sets: Vec::new(),
        edited,
        user,
        embedded,
        protect_embedded,
    };
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::TITLE != 0,
        field::TITLE,
        "name",
        Some(name.as_str()),
        clean(&meta.title),
    );
    let incoming_date =
        clean_date(&meta.release_date).map(|d| keep_more_precise(date.as_deref(), d));
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::RELEASE_DATE != 0,
        field::RELEASE_DATE,
        "release_date",
        date.as_deref(),
        incoming_date,
    );
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::GENRE != 0,
        field::GENRE,
        "genre",
        genre.as_deref(),
        clean_genre(&meta.genre),
    );
    w.plain_text(mode, "label", label.as_deref(), clean(&meta.label));
    w.plain_text(mode, "country", country.as_deref(), clean(&meta.country));
    w.plain_text(
        mode,
        "musicbrainz_release_id",
        mb.as_deref(),
        clean(&meta.musicbrainz_release_id),
    );
    w.plain_text(
        mode,
        "discogs_release_id",
        discogs.as_deref(),
        clean(&meta.discogs_release_id),
    );
    w.plain_text(
        mode,
        "discogs_extra_json",
        extra.as_deref(),
        clean(&meta.discogs_extra_json),
    );
    // A tracklist far shorter than the files on disk is a wrong match (an
    // 8-track album matched to its "- Single"); a few bonus files are fine.
    if let Some(n) = meta
        .expected_track_count
        .filter(|n| *n > 0 && n * 2 >= track_count)
    {
        if expected != Some(n) && (expected.is_none() || mode != CuratedWrite::FillUncurated) {
            w.set("expected_track_count", n);
        }
    }
    let has_meta = clean(&meta.release_date).is_some()
        || clean_genre(&meta.genre).is_some()
        || clean(&meta.label).is_some()
        || clean(&meta.country).is_some()
        || clean(&meta.musicbrainz_release_id).is_some()
        || clean(&meta.discogs_release_id).is_some()
        || clean(&meta.title).is_some();
    if has_meta && had_meta == 0 {
        w.set("has_album_meta", 1i64);
    }
    if let Some(a) = clean(&meta.added_at) {
        if added_at.as_deref().is_none_or(|cur| a.as_str() < cur) {
            w.set("added_at", a);
        }
    }
    if mode == CuratedWrite::Override {
        if let Some(u) = clean(&meta.updated_at) {
            if updated_at.as_deref() != Some(u.as_str()) {
                w.set("updated_at", u);
            }
        }
    }
    let title_changed = w.sets.iter().any(|(c, _)| *c == "name");
    let changed = w.apply(conn, "albums", id, (edited, user, embedded))?;
    if title_changed {
        conn.execute(
            r#"
            UPDATE tracks SET album_name = (SELECT name FROM albums WHERE id = ?1)
            WHERE album_id = ?1 AND album_name IS NOT (SELECT name FROM albums WHERE id = ?1)
            "#,
            params![id],
        )?;
    }
    Ok(changed)
}

type TrackRowCur = (
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
    Option<String>,
    i64,
);

pub(super) fn apply_curated_track(
    conn: &Connection,
    rel_path: &str,
    meta: &CuratedTrackMeta,
    mode: CuratedWrite,
    protect_embedded: bool,
) -> Result<bool> {
    let row: Option<TrackRowCur> = conn
        .query_row(
            r#"
            SELECT id, title, release_date, genre, track_number, disc_number,
                   lyrics, source, url, duration_ms, edited_fields, user_fields, added_at,
                   embedded_fields
            FROM tracks WHERE rel_path = ?1
            "#,
            params![rel_path],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        title,
        date,
        genre,
        track_no,
        disc_no,
        lyrics,
        source,
        url,
        duration,
        edited,
        user,
        added_at,
        embedded,
    )) = row
    else {
        return Ok(false);
    };
    let user_flag = meta.user_edited || mode == CuratedWrite::User;
    let mut w = RowWrite {
        sets: Vec::new(),
        edited,
        user,
        embedded,
        protect_embedded,
    };
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::TITLE != 0,
        field::TITLE,
        "title",
        Some(title.as_str()),
        clean(&meta.title),
    );
    let incoming_date =
        clean_date(&meta.release_date).map(|d| keep_more_precise(date.as_deref(), d));
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::RELEASE_DATE != 0,
        field::RELEASE_DATE,
        "release_date",
        date.as_deref(),
        incoming_date,
    );
    w.curated_text(
        mode,
        user_flag || meta.user_mask & field::GENRE != 0,
        field::GENRE,
        "genre",
        genre.as_deref(),
        clean_genre(&meta.genre),
    );
    w.curated_int(
        mode,
        user_flag || meta.user_mask & field::TRACK_NUMBER != 0,
        field::TRACK_NUMBER,
        "track_number",
        track_no,
        meta.track_number,
    );
    w.curated_int(
        mode,
        user_flag || meta.user_mask & field::DISC_NUMBER != 0,
        field::DISC_NUMBER,
        "disc_number",
        disc_no,
        meta.disc_number,
    );
    // Lyrics a person kept are never replaced by a fetch.
    let lyrics_mode = if mode == CuratedWrite::User {
        mode
    } else {
        CuratedWrite::FillUncurated
    };
    let before = w.sets.len();
    w.plain_text(
        lyrics_mode,
        "lyrics",
        lyrics.as_deref(),
        clean(&meta.lyrics),
    );
    if w.sets.len() > before {
        // Curated from now on: a re-read of the tags keeps it.
        w.mark_curated(field::LYRICS, mode == CuratedWrite::User);
    }
    w.plain_text(mode, "source", source.as_deref(), clean(&meta.source));
    w.plain_text(mode, "url", url.as_deref(), clean(&meta.url));
    if let Some(d) = meta.duration_ms.filter(|d| *d > 0) {
        if duration <= 0 {
            w.set("duration_ms", d);
        }
    }
    if let Some(a) = clean(&meta.added_at) {
        if added_at.as_deref().is_none_or(|cur| a.as_str() < cur) {
            w.set("added_at", a);
        }
    }
    let touched_text = w.sets.iter().any(|(c, _)| matches!(*c, "title" | "genre"));
    let touched_genre = w.sets.iter().any(|(c, _)| *c == "genre");
    let changed = w.apply(conn, "tracks", id, (edited, user, embedded))?;
    if touched_genre {
        genres::refresh_genres_where(conn, "tracks", "id = ?1", &id)?;
    }
    if touched_text {
        refresh_fts_row(conn, id)?;
    }
    Ok(changed)
}

/// Re-index one track in `tracks_fts` after an edit.
pub(super) fn refresh_fts_row(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM tracks_fts WHERE rowid = ?1", params![id])?;
    conn.execute(
        r#"
        INSERT INTO tracks_fts(rowid, title, artist_name, album_name, genre)
          SELECT id, title, artist_name, album_name, COALESCE(genre, '') FROM tracks WHERE id = ?1
        "#,
        params![id],
    )?;
    Ok(())
}

impl Db {
    /// Write curated album metadata (see [`CuratedWrite`]). Returns true when
    /// anything changed.
    pub fn apply_curated_album(
        &self,
        folder_key: &str,
        meta: &CuratedAlbumMeta,
        mode: CuratedWrite,
    ) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = apply_curated_album(&tx, folder_key, meta, mode, self.prefers_embedded())?;
        if changed {
            genres::refresh_genres_where(&tx, "albums", "folder_key = ?1", &folder_key)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Write curated track metadata (see [`CuratedWrite`]).
    pub fn apply_curated_track(
        &self,
        rel_path: &str,
        meta: &CuratedTrackMeta,
        mode: CuratedWrite,
    ) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = apply_curated_track(&tx, rel_path, meta, mode, self.prefers_embedded())?;
        tx.commit()?;
        Ok(changed)
    }

    /// Bulk variant for imports: one transaction for many rows, no per-row
    /// genre / FTS refresh (call [`Db::rebuild_genres`] / [`Db::rebuild_fts`]).
    pub fn apply_curated_batch(
        &self,
        albums: &[(String, CuratedAlbumMeta)],
        tracks: &[(String, CuratedTrackMeta)],
        mode: CuratedWrite,
    ) -> Result<(u32, u32)> {
        let protect = self.prefers_embedded();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut a = 0u32;
        let mut t = 0u32;
        for (key, meta) in albums {
            if apply_curated_album(&tx, key, meta, mode, protect)? {
                a += 1;
            }
        }
        for (rel, meta) in tracks {
            if apply_curated_track(&tx, rel, meta, mode, protect)? {
                t += 1;
            }
        }
        tx.commit()?;
        Ok((a, t))
    }
}
