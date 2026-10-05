//! Mass-deletion guard, unreadable folders and user links that survive files
//! disappearing for a while (unmounted NAS, interrupted rebuilds).

use rekord_core::db::{Db, PlaylistBackup, PlaylistBackupTrack};
use rekord_core::scan::{
    scan_library, scan_library_opts, scan_library_with, ScanMode, ScanOptions, ScanTrigger,
};
use std::fs;
use std::path::PathBuf;

struct TempLibrary {
    root: PathBuf,
    db_dir: PathBuf,
}

impl TempLibrary {
    fn new(tag: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("rekord-guard-{tag}-{}", uuid::Uuid::new_v4()));
        let root = base.join("music");
        let db_dir = base.join("data");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&db_dir).unwrap();
        Self { root, db_dir }
    }

    fn track(&self, rel: &str) -> PathBuf {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"not-a-real-mp3-but-indexable").unwrap();
        path
    }

    fn db(&self) -> Db {
        Db::open(self.db_dir.join("test.db")).unwrap()
    }
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        if let Some(base) = self.root.parent() {
            let _ = fs::remove_dir_all(base);
        }
    }
}

fn automatic() -> ScanOptions {
    ScanOptions {
        mode: ScanMode::Incremental,
        trigger: ScanTrigger::Automatic,
    }
}

fn track_count(db: &Db) -> i64 {
    db.stats(None).unwrap().track_count
}

#[test]
fn an_unreadable_root_is_an_error_and_deletes_nothing() {
    let lib = TempLibrary::new("root-gone");
    lib.track("Artist/Album/01.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();

    fs::remove_dir_all(&lib.root).unwrap();
    assert!(scan_library(&db, &lib.root).is_err());
    assert!(scan_library_opts(&db, &lib.root, automatic()).is_err());
    assert_eq!(track_count(&db), 1);
}

#[test]
fn an_empty_mountpoint_keeps_the_library_and_its_favorites() {
    let lib = TempLibrary::new("mountpoint");
    lib.track("Artist/Album/01.mp3");
    lib.track("Artist/Album/02.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    let id = db.track_id_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    db.add_favorite("default", id).unwrap();

    // The NAS is unmounted: the mountpoint is there, but empty.
    fs::remove_dir_all(lib.root.join("Artist")).unwrap();
    let report = scan_library_opts(&db, &lib.root, automatic()).unwrap();

    assert_eq!(report.removed_tracks, 0);
    assert!(report.prune_skipped.is_some());
    assert_eq!(track_count(&db), 2);
    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
    // Same ids: clients holding them keep working.
    assert_eq!(db.track_id_by_rel("Artist/Album/01.mp3").unwrap(), Some(id));
}

#[test]
fn automatic_scans_refuse_to_drop_a_large_share_of_the_library() {
    let lib = TempLibrary::new("mass");
    for i in 0..25 {
        lib.track(&format!("Keep/Album/{i:02}.mp3"));
    }
    for i in 0..35 {
        lib.track(&format!("Gone/Album/{i:02}.mp3"));
    }
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    fs::remove_dir_all(lib.root.join("Gone")).unwrap();

    let report = scan_library_opts(&db, &lib.root, automatic()).unwrap();
    assert_eq!(report.missing_tracks, 35);
    assert_eq!(report.removed_tracks, 0);
    assert!(report.prune_skipped.is_some());
    assert_eq!(track_count(&db), 60);

    // More than half the library is too much for a manual incremental scan as well…
    let report = scan_library(&db, &lib.root).unwrap();
    assert_eq!(report.removed_tracks, 0);

    // …the explicit full rescan confirms it.
    let report = scan_library_with(&db, &lib.root, ScanMode::Full).unwrap();
    assert_eq!(report.removed_tracks, 35);
    assert_eq!(track_count(&db), 25);
}

#[test]
fn a_manual_scan_prunes_a_moderate_removal() {
    let lib = TempLibrary::new("moderate");
    for i in 0..40 {
        lib.track(&format!("Keep/Album/{i:02}.mp3"));
    }
    for i in 0..25 {
        lib.track(&format!("Gone/Album/{i:02}.mp3"));
    }
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    fs::remove_dir_all(lib.root.join("Gone")).unwrap();

    // 25 of 65 (38%): over the automatic threshold, under the manual one.
    let auto = scan_library_opts(&db, &lib.root, automatic()).unwrap();
    assert_eq!(auto.removed_tracks, 0);
    let manual = scan_library(&db, &lib.root).unwrap();
    assert_eq!(manual.removed_tracks, 25);
    assert_eq!(track_count(&db), 40);
}

#[test]
fn small_removals_are_never_blocked() {
    let lib = TempLibrary::new("small");
    lib.track("Artist/Album/01.mp3");
    lib.track("Artist/Album/02.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    fs::remove_file(lib.root.join("Artist/Album/02.mp3")).unwrap();

    let report = scan_library_opts(&db, &lib.root, automatic()).unwrap();
    assert_eq!(report.removed_tracks, 1);
    assert!(report.prune_skipped.is_none());
}

#[cfg(unix)]
#[test]
fn tracks_under_an_unreadable_folder_are_kept() {
    use std::os::unix::fs::PermissionsExt;
    // root ignores permission bits: nothing to test there.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let lib = TempLibrary::new("unreadable");
    lib.track("Artist/Album/01.mp3");
    lib.track("Other/Album/01.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();

    let locked = lib.root.join("Artist");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let report = scan_library(&db, &lib.root);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let report = report.unwrap();

    assert_eq!(report.removed_tracks, 0);
    assert_eq!(report.unreadable_dirs, vec!["Artist".to_string()]);
    assert_eq!(track_count(&db), 2);
}

#[test]
fn playlist_entries_come_back_in_place_when_the_file_returns() {
    let lib = TempLibrary::new("playlist-park");
    lib.track("Artist/Album/01.mp3");
    lib.track("Artist/Album/02.mp3");
    lib.track("Artist/Album/03.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    let pl = db.create_playlist("default", "Set").unwrap();
    for rel in [
        "Artist/Album/03.mp3",
        "Artist/Album/01.mp3",
        "Artist/Album/02.mp3",
    ] {
        let id = db.track_id_by_rel(rel).unwrap().unwrap();
        db.add_to_playlist("default", &pl.id, id).unwrap();
    }

    fs::remove_file(lib.root.join("Artist/Album/01.mp3")).unwrap();
    scan_library(&db, &lib.root).unwrap();
    assert_eq!(db.playlist_tracks("default", &pl.id).unwrap().len(), 2);
    assert_eq!(db.parked_user_link_counts().unwrap(), (0, 1));

    lib.track("Artist/Album/01.mp3");
    scan_library(&db, &lib.root).unwrap();
    let order: Vec<String> = db
        .playlist_tracks("default", &pl.id)
        .unwrap()
        .into_iter()
        .map(|t| t.rel_path)
        .collect();
    assert_eq!(
        order,
        vec![
            "Artist/Album/03.mp3",
            "Artist/Album/01.mp3",
            "Artist/Album/02.mp3"
        ]
    );
    assert_eq!(db.parked_user_link_counts().unwrap(), (0, 0));
}

#[test]
fn a_full_rescan_keeps_favorites_playlists_and_ids() {
    let lib = TempLibrary::new("full-keeps");
    lib.track("Artist/Album/01.mp3");
    lib.track("Artist/Album/02.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    let id = db.track_id_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    db.add_favorite("default", id).unwrap();
    let pl = db.create_playlist("default", "Set").unwrap();
    db.add_to_playlist("default", &pl.id, id).unwrap();

    scan_library_with(&db, &lib.root, ScanMode::Full).unwrap();

    assert_eq!(db.track_id_by_rel("Artist/Album/01.mp3").unwrap(), Some(id));
    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
    assert_eq!(db.playlist_tracks("default", &pl.id).unwrap().len(), 1);
}

#[test]
fn a_catalog_wipe_interrupted_before_the_rescan_loses_no_user_data() {
    let lib = TempLibrary::new("crash");
    lib.track("Artist/Album/01.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    let id = db.track_id_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    db.add_favorite("default", id).unwrap();
    let pl = db.create_playlist("default", "Set").unwrap();
    db.add_to_playlist("default", &pl.id, id).unwrap();

    // The old full rebuild: wipe, then crash before re-indexing.
    db.clear_catalog().unwrap();
    drop(db);

    let db = lib.db();
    assert_eq!(track_count(&db), 0);
    scan_library(&db, &lib.root).unwrap();
    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
    assert_eq!(db.playlist_tracks("default", &pl.id).unwrap().len(), 1);
}

#[test]
fn restored_links_for_unscanned_files_attach_on_the_next_scan() {
    let lib = TempLibrary::new("restore-park");
    let db = lib.db();
    // Restore runs before the library is indexed.
    let linked = db
        .replace_favorites_by_rel_paths("default", &["Artist/Album/01.mp3".to_string()])
        .unwrap();
    assert_eq!(linked, 0);
    assert_eq!(
        db.export_favorite_rel_paths("default").unwrap(),
        vec!["Artist/Album/01.mp3".to_string()],
        "parked favorites still belong in backups"
    );
    db.replace_playlists_backup(
        "default",
        &[PlaylistBackup {
            id: None,
            name: "Set".into(),
            tracks: vec![PlaylistBackupTrack {
                rel_path: "Artist/Album/01.mp3".into(),
                title: String::new(),
                artist_name: String::new(),
                album_name: String::new(),
            }],
        }],
    )
    .unwrap();

    lib.track("Artist/Album/01.mp3");
    scan_library(&db, &lib.root).unwrap();

    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
    let pl = &db.list_playlists("default").unwrap()[0];
    assert_eq!(db.playlist_tracks("default", &pl.id).unwrap().len(), 1);
}

#[test]
fn merging_playlists_appends_without_duplicates() {
    let lib = TempLibrary::new("merge");
    lib.track("Artist/Album/01.mp3");
    lib.track("Artist/Album/02.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    let mine = db.create_playlist("default", "Road Trip").unwrap();
    let id1 = db.track_id_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    db.add_to_playlist("default", &mine.id, id1).unwrap();
    let fav = db.track_id_by_rel("Artist/Album/02.mp3").unwrap().unwrap();
    db.add_favorite("default", fav).unwrap();

    let row = |rel: &str| PlaylistBackupTrack {
        rel_path: rel.into(),
        title: String::new(),
        artist_name: String::new(),
        album_name: String::new(),
    };
    let incoming = vec![
        PlaylistBackup {
            id: None,
            name: " road  trip ".into(),
            tracks: vec![row("Artist/Album/01.mp3"), row("Artist/Album/02.mp3")],
        },
        PlaylistBackup {
            id: None,
            name: "Legacy only".into(),
            tracks: vec![row("Artist/Album/02.mp3")],
        },
    ];
    let (created, added) = db.merge_playlists_backup("default", &incoming).unwrap();
    assert_eq!((created, added), (1, 2));
    // Running it again changes nothing.
    assert_eq!(
        db.merge_playlists_backup("default", &incoming).unwrap(),
        (0, 0)
    );

    let road: Vec<String> = db
        .playlist_tracks("default", &mine.id)
        .unwrap()
        .into_iter()
        .map(|t| t.rel_path)
        .collect();
    assert_eq!(road, vec!["Artist/Album/01.mp3", "Artist/Album/02.mp3"]);
    assert_eq!(db.list_playlists("default").unwrap().len(), 2);

    db.merge_favorites_by_rel_paths("default", &["Artist/Album/01.mp3".to_string()])
        .unwrap();
    assert_eq!(db.list_favorites("default").unwrap().len(), 2);
}

/// A track file that cannot be stat'ed (symlink to a drive that is not
/// mounted right now) is "unknown", like an unreadable folder: kept.
#[cfg(unix)]
#[test]
fn a_track_whose_file_cannot_be_stated_is_kept() {
    let lib = TempLibrary::new("dangling-file");
    let ext = lib.root.parent().unwrap().join("external");
    fs::create_dir_all(&ext).unwrap();
    fs::write(ext.join("01.mp3"), b"not-a-real-mp3-but-indexable").unwrap();
    fs::write(ext.join("02.mp3"), b"not-a-real-mp3-but-indexable").unwrap();
    fs::create_dir_all(lib.root.join("Artist/Album")).unwrap();
    std::os::unix::fs::symlink(ext.join("01.mp3"), lib.root.join("Artist/Album/01.mp3")).unwrap();
    std::os::unix::fs::symlink(ext.join("02.mp3"), lib.root.join("Artist/loose.mp3")).unwrap();
    lib.track("Artist/Album/02.mp3");
    let db = lib.db();
    scan_library(&db, &lib.root).unwrap();
    assert_eq!(track_count(&db), 3);
    let id = db.track_id_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    db.add_favorite("default", id).unwrap();

    // The "drive" goes away: both links dangle.
    fs::remove_dir_all(&ext).unwrap();
    let report = scan_library(&db, &lib.root).unwrap();
    assert_eq!(report.removed_tracks, 0);
    assert_eq!(track_count(&db), 3);
    assert_eq!(db.list_favorites("default").unwrap().len(), 1);
}
