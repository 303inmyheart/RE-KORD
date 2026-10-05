//! Album track lists follow download / filename order (legacy
//! `albumExpectedOrder.mjs`), with numbers compared by value.

use rekord_core::db::{nat_cmp, Db};
use rekord_core::scan::scan_library;
use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

struct TempLibrary {
    root: PathBuf,
}

impl TempLibrary {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("rekord-order-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn track(&self, rel: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"untagged").unwrap();
    }
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn names(tracks: &[rekord_core::db::Track]) -> Vec<String> {
    tracks
        .iter()
        .map(|t| t.rel_path.rsplit('/').next().unwrap().to_string())
        .collect()
}

#[test]
fn untagged_ten_sorts_after_two() {
    let lib = TempLibrary::new("ten-two");
    for name in [
        "10-x.mp3",
        "2-y.mp3",
        "1-z.mp3",
        "11 - Outro.mp3",
        "02b.mp3",
    ] {
        lib.track(&format!("Artist/Album/{name}"));
    }
    let db = Db::open(lib.root.join("t.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();

    let expected = vec![
        "1-z.mp3",
        "2-y.mp3",
        "02b.mp3",
        "10-x.mp3",
        "11 - Outro.mp3",
    ];
    let by_folder = db.tracks_by_album_folder("Artist/Album").unwrap();
    assert_eq!(names(&by_folder), expected);
    let album_id = by_folder[0].album_id.unwrap();
    assert_eq!(names(&db.album_tracks(album_id).unwrap()), expected);
}

#[test]
fn natural_compare_matches_locale_compare_numeric() {
    assert_eq!(nat_cmp("2-y", "10-x"), Ordering::Less);
    assert_eq!(nat_cmp("Track 9", "track 10"), Ordering::Less);
    assert_eq!(nat_cmp("01 - first", "02 - second"), Ordering::Less);
    assert_eq!(nat_cmp("abc", "ABC"), Ordering::Equal);
    assert_eq!(nat_cmp("Énfasis", "énfasis"), Ordering::Equal);
    assert_eq!(nat_cmp("a", "a1"), Ordering::Less);
}
