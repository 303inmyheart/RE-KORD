//! Regression: nested disc folders must remain one album.

use rekord_core::db::Db;
use rekord_core::layout::{save_layout, LibraryLayout};
use rekord_core::scan::scan_library;
use std::fs;
use std::path::PathBuf;

struct TempLibrary {
    root: PathBuf,
}

impl TempLibrary {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rekord-album-grouping-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn deep_scan_keeps_cd_subfolders_in_one_album() {
    let lib = TempLibrary::new();
    fs::create_dir_all(lib.root.join("Artist/Album/CD1")).unwrap();
    fs::create_dir_all(lib.root.join("Artist/Album/CD2")).unwrap();
    fs::write(lib.root.join("Artist/Album/CD1/01 - One.mp3"), b"x").unwrap();
    fs::write(lib.root.join("Artist/Album/CD2/02 - Two.mp3"), b"x").unwrap();

    save_layout(
        &lib.root,
        &LibraryLayout {
            deep_scan: true,
            ..LibraryLayout::default()
        },
    )
    .unwrap();

    let db = Db::open(lib.root.join("test.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();

    let albums = db.list_albums().unwrap();
    assert_eq!(albums.len(), 1, "{albums:#?}");
    assert_eq!(albums[0].folder_key, "Artist/Album");

    let tracks = db.library_album_tracks(albums[0].id).unwrap();
    assert_eq!(tracks.len(), 2);
    assert!(tracks
        .iter()
        .any(|track| track.rel_path.ends_with("CD1/01 - One.mp3")));
    assert!(tracks
        .iter()
        .any(|track| track.rel_path.ends_with("CD2/02 - Two.mp3")));
}
