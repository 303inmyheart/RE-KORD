//! Backup / restore ZIP for the next hub (kordBackup: 3) + restore of legacy v2 ZIPs.
//!
//! - `export`: backup and theme ZIPs (and theme import)
//! - `restore`: restore of v3 / legacy v2 backups
//! - `legacy_import`: studio metadata and personal data from a legacy `.kord`
//! - `legacy_config`: machine settings of the legacy server (`music-root.config.json`)

use crate::accounts::{self, Account, DEFAULT_ACCOUNT_ID};
use crate::db::{
    CuratedAlbumMeta, CuratedTrackMeta, CuratedWrite, Db, PlaylistBackup, PlaylistBackupTrack,
};
use crate::metadata::providers::{DiscogsAlbumExtra, FetchedAlbumMeta, FetchedTrackMeta};
use crate::scan;
use crate::selection;
use crate::state::AppState;
use crate::user_state::{self, UserStateV1};
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

mod export;
pub mod legacy_config;
mod legacy_import;
mod restore;

pub use export::*;
pub use legacy_import::*;
pub use restore::*;

const BACKUP_VERSION: u32 = 3;
const THEME_EXPORT_JSON: &str = "rekord-theme.json";

const LIBRARY_SIDECAR_NAMES: &[&str] = &[
    "kord-albuminfo.json",
    "wpp-albuminfo.json",
    "kord-trackinfo.json",
    "wpp-trackinfo.json",
    "kord-artistinfo.json",
    "kord-artistinfo.jpg",
    "linked-source.json",
    "cover.jpg",
    "folder.jpg",
    "front.jpg",
    "cover.png",
    "folder.png",
    "artwork.jpg",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    #[serde(rename = "kordBackup", alias = "rekordBackup")]
    pub kord_backup: u32,
    pub created_at: String,
    #[serde(default)]
    pub library_root: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub restored: bool,
    pub version: u32,
    pub favorites: u32,
    pub playlists: u32,
    pub playlist_tracks: u32,
    pub library_files: u32,
    pub scanned_tracks: u64,
    pub album_meta_merged: u32,
    pub track_meta_merged: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeImportReport {
    pub theme_imported: bool,
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass_surfaces: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass_opacity: Option<f64>,
}

fn zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

fn add_bytes<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    data: &[u8],
) -> Result<()> {
    zip.start_file(name, zip_options())?;
    zip.write_all(data)?;
    Ok(())
}

fn add_file_path<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    abs: &Path,
    zip_name: &str,
) -> Result<()> {
    let data = fs::read(abs).with_context(|| format!("read {}", abs.display()))?;
    add_bytes(zip, zip_name, &data)
}

fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => bail!("unsafe path in zip: {rel}"),
        }
    }
    Ok(out)
}

fn read_zip_string(
    archive: &mut ZipArchive<impl Read + std::io::Seek>,
    name: &str,
) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(mut f) => {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            Ok(Some(s))
        }
        Err(_) => Ok(None),
    }
}

fn extract_prefix(
    archive: &mut ZipArchive<impl Read + std::io::Seek>,
    prefix: &str,
    dest_root: &Path,
) -> Result<u32> {
    let mut n = 0u32;
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    for name in names {
        if !name.starts_with(prefix) || name.ends_with('/') {
            continue;
        }
        let rel = &name[prefix.len()..];
        if rel.is_empty() {
            continue;
        }
        let dest = safe_join(dest_root, rel)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = archive.by_name(&name)?;
        let mut out = File::create(&dest)?;
        std::io::copy(&mut file, &mut out)?;
        n += 1;
    }
    Ok(n)
}

/// How `.kord` personal data meets what the hub already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LegacyImportMode {
    /// Union with the hub (default, safe to re-run after using next): favorites
    /// are added, playlists merged by name without duplicates, accounts added
    /// (never removed), moods / settings / excludes only fill gaps, selection
    /// and theme only written where the hub has none.
    #[default]
    Merge,
    /// Legacy data replaces the hub's personal data (registry, favorites,
    /// playlists, user-state, selection, theme). Next-only data is lost.
    Replace,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_theme_zip_payload(zip_bytes: &[u8]) -> Result<Option<serde_json::Value>> {
        let mut archive = match ZipArchive::new(Cursor::new(zip_bytes.to_vec())) {
            Ok(a) => a,
            Err(_) => return Ok(None),
        };
        let Some(json_name) = find_theme_json_entry(&mut archive) else {
            return Ok(None);
        };
        let raw = {
            let mut f = archive
                .by_name(&json_name)
                .with_context(|| format!("read {json_name}"))?;
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            s
        };
        let payload: serde_json::Value =
            serde_json::from_str(&raw).context("Invalid theme archive: bad rekord-theme.json")?;
        if payload.get("kind").and_then(|v| v.as_str()) != Some("rekord-theme") {
            bail!("Invalid theme archive: bad rekord-theme.json");
        }
        Ok(Some(payload))
    }

    fn build_legacy_theme_zip() -> Vec<u8> {
        let payload = json!({
            "kind": "rekord-theme",
            "version": 1,
            "theme": "custom",
            "glassSurfaces": true,
            "glassOpacity": 100,
            "customTheme": {
                "bg": "#181818",
                "section": "#181818",
                "accent": "#8b5cf6",
                "accent2": "#c4b5fd",
                "bgImageFit": "contain",
                "bgMode": "image"
            },
            "backgroundFile": "background.jpg"
        });
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            add_bytes(
                &mut zip,
                "rekord-theme/rekord-theme.json",
                serde_json::to_string_pretty(&payload).unwrap().as_bytes(),
            )
            .unwrap();
            add_bytes(&mut zip, "rekord-theme/background.jpg", b"fake-jpeg").unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn detects_legacy_theme_zip_without_manifest() {
        let bytes = build_legacy_theme_zip();
        let parsed = parse_theme_zip_payload(&bytes).unwrap().expect("theme zip");
        assert_eq!(parsed["kind"], "rekord-theme");
        assert_eq!(parsed["theme"], "custom");
        assert_eq!(parsed["customTheme"]["accent"], "#8b5cf6");
        assert_eq!(parsed["backgroundFile"], "background.jpg");
    }

    #[test]
    fn restore_remaps_accounts_by_matching_name() {
        let existing = vec![
            Account {
                id: "default".into(),
                name: accounts::DEFAULT_ACCOUNT_NAME.into(),
            },
            Account {
                id: "hub-diego".into(),
                name: "Diego".into(),
            },
        ];
        let backup = vec![
            Account {
                id: "default".into(),
                name: "Default".into(),
            },
            Account {
                id: "bak-diego".into(),
                name: "diego".into(), // case-insensitive
            },
            Account {
                id: "bak-new".into(),
                name: "Nuovo".into(),
            },
        ];
        let (reg, map) = resolve_restore_account_targets(&backup, &existing);
        assert_eq!(map.get("default").map(String::as_str), Some("default"));
        assert_eq!(map.get("bak-diego").map(String::as_str), Some("hub-diego"));
        assert_eq!(map.get("bak-new").map(String::as_str), Some("bak-new"));
        assert_eq!(reg.len(), 3);
        assert!(reg.iter().any(|a| a.id == "hub-diego" && a.name == "diego"));
        assert!(reg.iter().any(|a| a.id == "bak-new" && a.name == "Nuovo"));
    }

    #[test]
    fn restore_keeps_same_id_without_name_steal() {
        let existing = vec![
            Account {
                id: "default".into(),
                name: "Default".into(),
            },
            Account {
                id: "aaa".into(),
                name: "Diego".into(),
            },
        ];
        let backup = vec![
            Account {
                id: "default".into(),
                name: "Default".into(),
            },
            Account {
                id: "aaa".into(),
                name: "Diego".into(),
            },
        ];
        let (_reg, map) = resolve_restore_account_targets(&backup, &existing);
        assert_eq!(map.get("aaa").map(String::as_str), Some("aaa"));
    }

    #[test]
    fn non_theme_zip_returns_none() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            add_bytes(&mut zip, "readme.txt", b"hi").unwrap();
            zip.finish().unwrap();
        }
        let bytes = cursor.into_inner();
        assert!(parse_theme_zip_payload(&bytes).unwrap().is_none());
    }

    #[test]
    fn real_legacy_theme_fixture_if_present() {
        let path = PathBuf::from("/home/diego-ubuntu/Scaricati/rekord-theme-custom-2026-07-09.zip");
        if !path.is_file() {
            return;
        }
        let bytes = fs::read(&path).unwrap();
        let parsed = parse_theme_zip_payload(&bytes)
            .unwrap()
            .expect("fixture theme zip");
        assert_eq!(parsed["kind"], "rekord-theme");
        assert_eq!(parsed["theme"], "custom");
        // Must not require config/manifest.json — detection is theme-only.
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(read_zip_string(&mut archive, "config/manifest.json")
            .unwrap()
            .is_none());
    }

    #[test]
    fn imports_sidecar_and_legacy_db_metadata_after_scan_shape() {
        let tmp = tempfile_dir();
        let music = tmp.join("music");
        let album_dir = music.join("Artist").join("Album");
        fs::create_dir_all(&album_dir).unwrap();
        fs::write(
            album_dir.join("kord-albuminfo.json"),
            r#"{"title":"Nice Title","genre":"Rock","label":"Label X","releaseDate":"2001"}"#,
        )
        .unwrap();
        fs::write(
            album_dir.join("kord-trackinfo.json"),
            r#"{"01 - Song.mp3":{"title":"Song","source":"deezer","url":"https://example/t/1","genre":"Rock"}}"#,
        )
        .unwrap();

        let hub_db_path = tmp.join("hub.db");
        let db = Db::open(&hub_db_path).unwrap();
        let artist_id = db.upsert_artist("Artist").unwrap();
        let album_id = db
            .upsert_album(
                "Album",
                "Artist",
                Some(artist_id),
                "Artist/Album",
                None,
                false,
            )
            .unwrap();
        db.upsert_track(
            "Artist/Album/01 - Song.mp3",
            &album_dir.join("01 - Song.mp3"),
            "01 - Song",
            "Artist",
            "Album",
            1000,
            Some(1),
            Some(album_id),
            Some(artist_id),
            10,
            0,
            None,
            None,
            None,
        )
        .unwrap();

        let (a1, t1) = import_sidecar_metadata(&db, &music).unwrap();
        assert!(a1 >= 1);
        assert!(t1 >= 1);

        // Build a mini legacy rekord.db with richer track meta for another album path.
        let legacy_dir = music.join(".kord");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_path = legacy_dir.join("rekord.db");
        {
            let leg = Connection::open(&legacy_path).unwrap();
            leg.execute_batch(
                r#"
                CREATE TABLE albums (
                  id TEXT PRIMARY KEY,
                  artist_id TEXT,
                  folder_rel_path TEXT NOT NULL UNIQUE,
                  name TEXT NOT NULL,
                  title TEXT,
                  release_date TEXT,
                  genre TEXT,
                  label TEXT,
                  country TEXT,
                  musicbrainz_release_id TEXT,
                  expected_track_count INTEGER,
                  has_album_meta INTEGER NOT NULL DEFAULT 0,
                  discogs_release_id INTEGER
                );
                CREATE TABLE tracks (
                  id TEXT PRIMARY KEY,
                  rel_path TEXT NOT NULL UNIQUE,
                  album_id TEXT,
                  title TEXT NOT NULL,
                  artist_name TEXT,
                  album_name TEXT,
                  genre TEXT,
                  release_date TEXT,
                  lyrics TEXT,
                  source TEXT,
                  url TEXT
                );
                INSERT INTO albums(id, artist_id, folder_rel_path, name, title, genre, label, has_album_meta)
                VALUES ('A','Artist','Artist/Album','Album','Nice Title','Metal','Legacy Label',1);
                INSERT INTO tracks(id, rel_path, album_id, title, artist_name, album_name, source, url, lyrics)
                VALUES ('T','Artist/Album/01 - Song.mp3','A','Song','Artist','Album','musicbrainz','https://mb/1','la la');
                "#,
            )
            .unwrap();
        }

        // Clear sidecar-filled genre so legacy DB can demonstrate fill-empty merge of lyrics/source.
        {
            let conn = Connection::open(&hub_db_path).unwrap();
            conn.execute(
                "UPDATE tracks SET source=NULL, url=NULL, lyrics=NULL, genre=NULL",
                [],
            )
            .unwrap();
            conn.execute("UPDATE albums SET genre=NULL, label=NULL", [])
                .unwrap();
        }
        let db2 = Db::open(&hub_db_path).unwrap();
        let (a2, t2) = import_legacy_library_db_metadata(&db2, &legacy_path).unwrap();
        assert!(a2 >= 1);
        assert!(t2 >= 1);

        let album = db2
            .list_albums()
            .unwrap()
            .into_iter()
            .find(|a| a.folder_key == "Artist/Album")
            .expect("album");
        assert_eq!(album.genre.as_deref(), Some("Metal"));
        assert_eq!(album.label.as_deref(), Some("Legacy Label"));

        let tracks = db2.tracks_by_album_folder("Artist/Album").unwrap();
        let tr = tracks
            .iter()
            .find(|t| t.rel_path.ends_with("Song.mp3"))
            .unwrap();
        // source/url/lyrics live in DB columns; list API Track may omit some — query sqlite.
        let conn = Connection::open(&hub_db_path).unwrap();
        let (source, url, lyrics): (Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT source, url, lyrics FROM tracks WHERE rel_path = ?1",
                ["Artist/Album/01 - Song.mp3"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(source.as_deref(), Some("musicbrainz"));
        assert_eq!(url.as_deref(), Some("https://mb/1"));
        assert_eq!(lyrics.as_deref(), Some("la la"));
        let _ = tr;
    }

    #[test]
    fn real_legacy_backup_db_merges_when_fixture_present() {
        let zip = PathBuf::from(
            "/home/diego-ubuntu/Scaricati/rekord-backup-2026-07-29T14-25-06.354Z.zip",
        );
        if !zip.is_file() {
            return;
        }
        let tmp = tempfile_dir();
        let hub_db_path = tmp.join("hub.db");
        let legacy_path = tmp.join("rekord.db");
        // Extract only legacy DB from the real ZIP.
        {
            let bytes = fs::read(&zip).unwrap();
            let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut f = archive.by_name("kord-db/rekord.db").unwrap();
            let mut out = File::create(&legacy_path).unwrap();
            std::io::copy(&mut f, &mut out).unwrap();
        }
        let db = Db::open(&hub_db_path).unwrap();
        // Seed a few albums/tracks that exist in the fixture DB.
        let samples: Vec<(String, String)> = {
            let leg = Connection::open_with_flags(&legacy_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
            let mut stmt = leg
                .prepare(
                    r#"
                    SELECT a.folder_rel_path, t.rel_path
                    FROM tracks t
                    JOIN albums a ON a.id = t.album_id
                    WHERE t.source IS NOT NULL AND trim(t.source) != ''
                    LIMIT 5
                    "#,
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert!(!samples.is_empty());
        for (folder, rel) in &samples {
            let folder = folder.replace('\\', "/");
            let rel = rel.replace('\\', "/");
            let artist = folder.split('/').next().unwrap_or("A");
            let album = folder.split('/').nth(1).unwrap_or("B");
            let artist_id = db.upsert_artist(artist).unwrap();
            let album_id = db
                .upsert_album(album, artist, Some(artist_id), &folder, None, false)
                .unwrap();
            db.upsert_track(
                &rel,
                &Path::new("/tmp").join(&rel),
                "t",
                artist,
                album,
                1,
                None,
                Some(album_id),
                Some(artist_id),
                1,
                0,
                None,
                None,
                None,
            )
            .unwrap();
        }
        let (albums, tracks) = import_legacy_library_db_metadata(&db, &legacy_path).unwrap();
        assert!(albums > 0, "expected album meta from fixture db");
        assert!(tracks > 0, "expected track meta from fixture db");
        let conn = Connection::open(&hub_db_path).unwrap();
        let with_source: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tracks WHERE source IS NOT NULL AND trim(source) != ''",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(with_source > 0);
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rekord-backup-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
