//! `PRAGMA user_version` migrations from every pre-versioning schema shape.

use rekord_core::db::{Db, SCHEMA_VERSION};
use rusqlite::Connection;
use std::path::PathBuf;

struct TempDb {
    dir: PathBuf,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("rekord-migrate-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("rekord.db")
    }

    fn raw(&self) -> Connection {
        Connection::open(self.path()).unwrap()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect()
}

/// The oldest hub layout: single-account favorites/playlists and albums keyed
/// by name only.
fn create_single_account_schema(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE artists (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          album_count INTEGER NOT NULL DEFAULT 0,
          track_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE albums (
          id INTEGER PRIMARY KEY,
          artist_id INTEGER REFERENCES artists(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          cover_path TEXT,
          has_cover INTEGER NOT NULL DEFAULT 0,
          track_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE tracks (
          id INTEGER PRIMARY KEY,
          rel_path TEXT NOT NULL UNIQUE,
          file_path TEXT NOT NULL,
          album_id INTEGER,
          artist_id INTEGER REFERENCES artists(id) ON DELETE SET NULL,
          title TEXT NOT NULL,
          artist_name TEXT NOT NULL DEFAULT '',
          album_name TEXT NOT NULL DEFAULT '',
          duration_ms INTEGER NOT NULL DEFAULT 0,
          track_number INTEGER,
          size INTEGER NOT NULL DEFAULT 0,
          mtime INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE favorites (
          track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
          created_at TEXT NOT NULL
        );
        CREATE TABLE playlists (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE playlist_tracks (
          playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
          track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
          position INTEGER NOT NULL,
          PRIMARY KEY (playlist_id, track_id)
        );
        CREATE VIRTUAL TABLE tracks_fts USING fts5(
          title, artist_name, album_name, rel_path, content='tracks', content_rowid='id'
        );

        INSERT INTO artists(id, name) VALUES (1, 'Artist');
        INSERT INTO albums(id, artist_id, name, cover_path, has_cover)
          VALUES (7, 1, 'Album', '/music/Artist/Album/cover.jpg', 1);
        INSERT INTO albums(id, artist_id, name) VALUES (8, 1, 'Ghost');
        INSERT INTO tracks(id, rel_path, file_path, album_id, artist_id, title)
          VALUES (1, 'Artist/Album/01.mp3', '/music/Artist/Album/01.mp3', 7, 1, 'One'),
                 (2, 'Artist/Album/CD2/01.mp3', '/music/Artist/Album/CD2/01.mp3', 7, 1, 'Two');
        INSERT INTO favorites(track_id, created_at) VALUES (1, '2024-01-01T00:00:00Z');
        INSERT INTO playlists(id, name, created_at) VALUES ('p1', 'Set', '2024-01-01T00:00:00Z');
        INSERT INTO playlist_tracks(playlist_id, track_id, position) VALUES ('p1', 2, 0), ('p1', 1, 1);
        "#,
    )
    .unwrap();
}

#[test]
fn the_single_account_schema_migrates_without_losing_data() {
    let tmp = TempDb::new("v0-old");
    create_single_account_schema(&tmp.raw());

    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);

    // Favorites / playlists moved to the default account.
    let favs = db.list_favorites("default").unwrap();
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0].rel_path, "Artist/Album/01.mp3");
    let pls = db.list_playlists("default").unwrap();
    assert_eq!(pls.len(), 1);
    let order: Vec<String> = db
        .playlist_tracks("default", "p1")
        .unwrap()
        .into_iter()
        .map(|t| t.rel_path)
        .collect();
    assert_eq!(
        order,
        vec!["Artist/Album/CD2/01.mp3", "Artist/Album/01.mp3"]
    );

    // The album keeps its id and studio-independent fields, keyed by folder.
    let album = db.get_album(7).unwrap().expect("album kept");
    assert_eq!(album.folder_key, "Artist/Album");
    assert!(album.has_cover);
    // An album no track points at cannot be keyed; the next scan recreates it.
    assert!(db.get_album(8).unwrap().is_none());

    let conn = tmp.raw();
    assert!(columns(&conn, "albums").contains(&"discogs_extra_json".to_string()));
    assert!(columns(&conn, "tracks").contains(&"updated_at".to_string()));
    // The contentless FTS table was rebuilt as a standalone index.
    let fts_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'tracks_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!fts_sql.contains("content="));
    assert_eq!(db.search_tracks("One", 10).unwrap().len(), 1);
}

#[test]
fn a_current_unversioned_database_is_adopted_in_place() {
    let tmp = TempDb::new("v0-current");
    {
        let db = Db::open(tmp.path()).unwrap();
        db.create_playlist("default", "Mine").unwrap();
        db.set_meta("last_scan_at", "2025-01-01T00:00:00Z").unwrap();
    }
    // Make it look like a DB written by a build before versioning existed.
    {
        let conn = tmp.raw();
        conn.execute_batch(
            r#"
            DROP TRIGGER IF EXISTS tracks_reattach_user_links;
            DROP TABLE IF EXISTS parked_playlist_tracks;
            DROP TABLE IF EXISTS parked_favorites;
            PRAGMA user_version = 0;
            "#,
        )
        .unwrap();
    }

    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(db.list_playlists("default").unwrap().len(), 1);
    assert_eq!(
        db.get_meta("last_scan_at").unwrap().as_deref(),
        Some("2025-01-01T00:00:00Z")
    );
    assert_eq!(db.parked_user_link_counts().unwrap(), (0, 0));
}

#[test]
fn reopening_a_migrated_database_is_a_no_op() {
    let tmp = TempDb::new("reopen");
    create_single_account_schema(&tmp.raw());
    drop(Db::open(tmp.path()).unwrap());
    let before: i64 = tmp
        .raw()
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
        .unwrap();

    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    let after: i64 = tmp
        .raw()
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
}

#[test]
fn a_database_from_a_newer_build_still_opens() {
    let tmp = TempDb::new("newer");
    drop(Db::open(tmp.path()).unwrap());
    tmp.raw()
        .execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 5))
        .unwrap();

    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION + 5);
    let raw: i64 = tmp
        .raw()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        raw,
        SCHEMA_VERSION + 5,
        "an older build never downgrades the marker"
    );
}

#[test]
fn a_fresh_database_starts_at_the_latest_version() {
    let tmp = TempDb::new("fresh");
    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    let conn = tmp.raw();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal");
}

/// Exactly what the pre-versioning `next` hub (`db.rs` before the `db/`
/// split) created on a fresh file: multi-account user data, folder-keyed
/// albums, standalone FTS, Studio columns added by ALTER, delta triggers.
fn create_previous_next_schema(conn: &Connection) {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS artists (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          album_count INTEGER NOT NULL DEFAULT 0,
          track_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS tracks (
          id INTEGER PRIMARY KEY,
          rel_path TEXT NOT NULL UNIQUE,
          file_path TEXT NOT NULL,
          album_id INTEGER,
          artist_id INTEGER REFERENCES artists(id) ON DELETE SET NULL,
          title TEXT NOT NULL,
          artist_name TEXT NOT NULL DEFAULT '',
          album_name TEXT NOT NULL DEFAULT '',
          duration_ms INTEGER NOT NULL DEFAULT 0,
          track_number INTEGER,
          size INTEGER NOT NULL DEFAULT 0,
          mtime INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS files (
          rel_path TEXT PRIMARY KEY,
          size INTEGER NOT NULL,
          mtime INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS library_meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS favorites (
          account_id TEXT NOT NULL,
          track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
          created_at TEXT NOT NULL,
          PRIMARY KEY (account_id, track_id)
        );
        CREATE TABLE IF NOT EXISTS playlists (
          id TEXT PRIMARY KEY,
          account_id TEXT NOT NULL,
          name TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS playlist_tracks (
          playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
          track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
          position INTEGER NOT NULL,
          PRIMARY KEY (playlist_id, track_id)
        );
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE albums (
          id INTEGER PRIMARY KEY,
          artist_id INTEGER REFERENCES artists(id) ON DELETE SET NULL,
          name TEXT NOT NULL,
          artist_name TEXT NOT NULL DEFAULT '',
          folder_key TEXT NOT NULL UNIQUE,
          cover_path TEXT,
          has_cover INTEGER NOT NULL DEFAULT 0,
          loose INTEGER NOT NULL DEFAULT 0,
          track_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE VIRTUAL TABLE tracks_fts USING fts5(
          title, artist_name, album_name, rel_path
        );
        ALTER TABLE albums ADD COLUMN release_date TEXT;
        ALTER TABLE albums ADD COLUMN genre TEXT;
        ALTER TABLE albums ADD COLUMN label TEXT;
        ALTER TABLE albums ADD COLUMN country TEXT;
        ALTER TABLE albums ADD COLUMN musicbrainz_release_id TEXT;
        ALTER TABLE albums ADD COLUMN discogs_release_id TEXT;
        ALTER TABLE albums ADD COLUMN discogs_extra_json TEXT;
        ALTER TABLE albums ADD COLUMN has_album_meta INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE albums ADD COLUMN expected_track_count INTEGER;
        ALTER TABLE tracks ADD COLUMN genre TEXT;
        ALTER TABLE tracks ADD COLUMN release_date TEXT;
        ALTER TABLE tracks ADD COLUMN disc_number INTEGER;
        ALTER TABLE tracks ADD COLUMN source TEXT;
        ALTER TABLE tracks ADD COLUMN url TEXT;
        ALTER TABLE tracks ADD COLUMN lyrics TEXT;
        ALTER TABLE tracks ADD COLUMN updated_at TEXT;
        CREATE TABLE IF NOT EXISTS track_tombstones (
          rel_path TEXT PRIMARY KEY,
          removed_at TEXT NOT NULL
        );
        CREATE TRIGGER tracks_touch_insert
        AFTER INSERT ON tracks BEGIN
          UPDATE tracks SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = NEW.id;
          DELETE FROM track_tombstones WHERE rel_path = NEW.rel_path;
        END;
        CREATE TRIGGER IF NOT EXISTS tracks_touch_update
        AFTER UPDATE ON tracks BEGIN
          UPDATE tracks SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = NEW.id;
        END;
        CREATE TRIGGER IF NOT EXISTS tracks_tombstone
        AFTER DELETE ON tracks BEGIN
          INSERT INTO track_tombstones(rel_path, removed_at)
            VALUES (OLD.rel_path, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(rel_path) DO UPDATE SET removed_at = excluded.removed_at;
        END;
        CREATE INDEX IF NOT EXISTS idx_tracks_updated_at ON tracks(updated_at);
        CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album_id);
        CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist_id);
        "#,
    )
    .unwrap();
}

#[test]
fn a_database_from_the_previous_next_build_keeps_user_data_through_scans() {
    let tmp = TempDb::new("prev-next");
    let root = tmp.dir.join("music");
    let rels = [
        "Artist/Album/01.mp3",
        "Artist/Album/02.mp3",
        "Artist/Other/01.mp3",
    ];
    for rel in rels {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"not-a-real-mp3-but-indexable").unwrap();
    }
    {
        let conn = tmp.raw();
        create_previous_next_schema(&conn);
        let canon = root.canonicalize().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO artists(id, name) VALUES (1, 'Artist');
            INSERT INTO albums(id, artist_id, name, artist_name, folder_key, genre)
              VALUES (10, 1, 'Album', 'Artist', 'Artist/Album', 'Jazz'),
                     (11, 1, 'Other', 'Artist', 'Artist/Other', NULL);
            "#,
        )
        .unwrap();
        for (i, rel) in rels.iter().enumerate() {
            let album = if rel.starts_with("Artist/Album/") {
                10
            } else {
                11
            };
            conn.execute(
                "INSERT INTO tracks(id, rel_path, file_path, album_id, artist_id, title, lyrics)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, 'la la')",
                rusqlite::params![
                    (i + 1) as i64,
                    rel,
                    canon.join(rel).to_string_lossy(),
                    album,
                    format!("T{i}")
                ],
            )
            .unwrap();
        }
        conn.execute_batch(
            r#"
            INSERT INTO favorites(account_id, track_id, created_at) VALUES
              ('default', 1, '2024-01-01T00:00:00Z'),
              ('default', 3, '2024-01-02T00:00:00Z'),
              ('guest', 1, '2024-01-03T00:00:00Z');
            INSERT INTO playlists(id, account_id, name, created_at) VALUES
              ('p1', 'default', 'Set', '2024-01-01T00:00:00Z'),
              ('p2', 'guest', 'Mine', '2024-01-01T00:00:00Z');
            INSERT INTO playlist_tracks(playlist_id, track_id, position) VALUES
              ('p1', 3, 0), ('p1', 1, 1), ('p1', 2, 2), ('p2', 2, 0);
            "#,
        )
        .unwrap();
    }

    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    let favs = |acc: &str| -> Vec<String> {
        let mut v: Vec<String> = db
            .list_favorites(acc)
            .unwrap()
            .into_iter()
            .map(|t| t.track.rel_path)
            .collect();
        v.sort();
        v
    };
    let order = |acc: &str, id: &str| -> Vec<String> {
        db.playlist_tracks(acc, id)
            .unwrap()
            .into_iter()
            .map(|t| t.rel_path)
            .collect()
    };
    assert_eq!(
        favs("default"),
        vec!["Artist/Album/01.mp3", "Artist/Other/01.mp3"]
    );
    assert_eq!(favs("guest"), vec!["Artist/Album/01.mp3"]);
    assert_eq!(
        order("default", "p1"),
        vec![
            "Artist/Other/01.mp3",
            "Artist/Album/01.mp3",
            "Artist/Album/02.mp3"
        ]
    );
    assert_eq!(order("guest", "p2"), vec!["Artist/Album/02.mp3"]);
    assert!(columns(&tmp.raw(), "tracks").contains(&"bpm".to_string()));

    // A first incremental scan on the migrated DB keeps ids, links and Studio edits.
    rekord_core::scan::scan_library(&db, &root).unwrap();
    assert_eq!(
        favs("default"),
        vec!["Artist/Album/01.mp3", "Artist/Other/01.mp3"]
    );
    assert_eq!(favs("guest"), vec!["Artist/Album/01.mp3"]);
    assert_eq!(order("guest", "p2"), vec!["Artist/Album/02.mp3"]);
    let album = db.get_album(10).unwrap().expect("album kept");
    assert_eq!(album.folder_key, "Artist/Album");
    assert_eq!(album.genre.as_deref(), Some("Jazz"));

    // The album disappears (full scan prunes it), then comes back: links park
    // and re-attach exactly once, to the same accounts and playlists.
    let moved = tmp.dir.join("moved-away");
    std::fs::rename(root.join("Artist/Album"), &moved).unwrap();
    rekord_core::scan::scan_library_with(&db, &root, rekord_core::scan::ScanMode::Full).unwrap();
    assert_eq!(favs("default"), vec!["Artist/Other/01.mp3"]);
    assert!(favs("guest").is_empty());
    assert_eq!(db.parked_user_link_counts().unwrap(), (2, 3));
    std::fs::rename(&moved, root.join("Artist/Album")).unwrap();
    rekord_core::scan::scan_library(&db, &root).unwrap();
    rekord_core::scan::scan_library(&db, &root).unwrap();
    assert_eq!(
        favs("default"),
        vec!["Artist/Album/01.mp3", "Artist/Other/01.mp3"]
    );
    assert_eq!(favs("guest"), vec!["Artist/Album/01.mp3"]);
    assert_eq!(
        order("default", "p1"),
        vec![
            "Artist/Other/01.mp3",
            "Artist/Album/01.mp3",
            "Artist/Album/02.mp3"
        ]
    );
    assert_eq!(order("guest", "p2"), vec!["Artist/Album/02.mp3"]);
    assert_eq!(db.parked_user_link_counts().unwrap(), (0, 0));
}
