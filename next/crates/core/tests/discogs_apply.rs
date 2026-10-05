//! Applying a Discogs release: album meta keeps every genre/style and each
//! file matched to the tracklist gets track/disc numbers, duration and (when
//! untitled) the Discogs title. Network-free: the release JSON is inline.

use rekord_core::db::Db;
use rekord_core::metadata::discogs::plan_discogs_track_deltas;
use rekord_core::metadata::providers::{discogs_release_from_json, FetchedTrackMeta};
use rekord_core::scan::scan_library;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

struct TempLibrary {
    root: PathBuf,
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn library(files: &[&str]) -> TempLibrary {
    let root = std::env::temp_dir().join(format!("rekord-discogs-{}", uuid::Uuid::new_v4()));
    for rel in files {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"untagged").unwrap();
    }
    TempLibrary { root }
}

#[test]
fn a_release_is_applied_track_by_track() {
    let lib = library(&[
        "Nirvana/Nevermind/02 - In Bloom.mp3",
        "Nirvana/Nevermind/01 Smells Like Teen Spirit.mp3",
        "Nirvana/Nevermind/CD2/01 Endless Nameless.mp3",
        "Nirvana/Nevermind/bonus interview.mp3",
    ]);
    let db = Db::open(lib.root.join("t.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();

    let release = discogs_release_from_json(
        json!({
            "id": 42,
            "title": "Nevermind",
            "year": 1991,
            "uri": "https://www.discogs.com/release/42",
            "genres": ["Rock"],
            "styles": ["Grunge", "Alternative Rock"],
            "labels": [{"name": "DGC", "catno": "DGCD-24425"}],
            "tracklist": [
                {"type_": "heading", "title": "Disc 1"},
                {"position": "1-1", "title": "Smells Like Teen Spirit", "duration": "5:01"},
                {"position": "1-2", "title": "In Bloom", "duration": "4:14"},
                {"position": "2-1", "title": "Endless, Nameless", "duration": "6:43"},
            ],
        }),
        None,
        42,
        "",
    );
    assert_eq!(
        release.meta.genre.as_deref(),
        Some("Rock; Grunge; Alternative Rock")
    );
    assert_eq!(release.meta.expected_track_count, Some(3));
    db.apply_album_meta("Nirvana/Nevermind", &release.meta)
        .unwrap();

    let tracks = db.tracks_by_album_folder("Nirvana/Nevermind").unwrap();
    let deltas = plan_discogs_track_deltas(
        &tracks,
        "Nirvana",
        &release.tracklist,
        release.meta.discogs_uri.as_deref(),
    );
    assert_eq!(deltas.len(), 3, "the interview has no tracklist row");
    for d in &deltas {
        db.apply_discogs_track_meta(
            &d.rel_path,
            &FetchedTrackMeta {
                ok: true,
                title: d.title.clone(),
                track_number: Some(d.track_number),
                disc_number: Some(d.disc_number),
                duration_ms: d.duration_ms,
                source: Some(d.source.clone()),
                url: d.url.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let bloom = db
        .track_by_rel("Nirvana/Nevermind/02 - In Bloom.mp3")
        .unwrap()
        .unwrap();
    assert_eq!(bloom.track_number, Some(2));
    assert_eq!(bloom.title, "In Bloom");
    assert_eq!(
        bloom.duration_ms, 254_000,
        "unknown duration filled from Discogs"
    );
    assert_eq!(bloom.source.as_deref(), Some("discogs"));

    let endless = db
        .track_by_rel("Nirvana/Nevermind/CD2/01 Endless Nameless.mp3")
        .unwrap()
        .unwrap();
    assert_eq!(endless.track_number, Some(1));
    assert_eq!(endless.title, "Endless, Nameless");

    let album = db
        .list_albums()
        .unwrap()
        .into_iter()
        .find(|a| a.folder_key == "Nirvana/Nevermind")
        .unwrap();
    assert_eq!(
        album.genre.as_deref(),
        Some("Rock; Grunge; Alternative Rock")
    );
    assert_eq!(album.expected_track_count, Some(3));
}
