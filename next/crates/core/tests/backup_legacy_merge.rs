//! `sync-legacy-meta` merges `.kord` personal data into the hub instead of
//! replacing it: re-running after using next keeps next-only data.

use rekord_core::accounts;
use rekord_core::backup::{
    sync_legacy_library_data, sync_legacy_library_data_with, LegacyImportMode,
};
use rekord_core::db::Db;
use rekord_core::scan::scan_library;
use rekord_core::user_state;
use std::fs;
use std::path::PathBuf;

struct Setup {
    base: PathBuf,
    root: PathBuf,
    data: PathBuf,
}

impl Setup {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "rekord-legacy-merge-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        let root = base.join("music");
        let data = base.join("data");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&data).unwrap();
        for rel in [
            "Artist/Album/01.mp3",
            "Artist/Album/02.mp3",
            "Artist/Album/03.mp3",
        ] {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"audio").unwrap();
        }
        let kord = root.join(".kord");
        fs::create_dir_all(kord.join("global_info")).unwrap();
        fs::write(
            kord.join("global_info/accounts.json"),
            r#"{"accounts":[{"id":"default","name":"Locale"},{"id":"legacy-guest","name":"Ospite"}]}"#,
        )
        .unwrap();
        fs::create_dir_all(kord.join("default_info")).unwrap();
        fs::write(
            kord.join("default_info/user-state.json"),
            r#"{
              "favorites": ["Artist/Album/01.mp3"],
              "playlists": [
                {"name": "Mix", "tracks": [{"relPath": "Artist/Album/01.mp3"}, {"relPath": "Artist/Album/02.mp3"}]},
                {"name": "Legacy Only", "tracks": [{"relPath": "Artist/Album/03.mp3"}]}
              ],
              "trackMoods": {"Artist/Album/01.mp3": ["chill_relax"], "Artist/Album/02.mp3": ["dark_tense"]},
              "trackPlayCounts": {"Artist/Album/01.mp3": 9}
            }"#,
        )
        .unwrap();
        Self { base, root, data }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn rels<T: AsRef<rekord_core::db::Track>>(tracks: Vec<T>) -> Vec<String> {
    tracks
        .into_iter()
        .map(|t| t.as_ref().rel_path.clone())
        .collect()
}

#[test]
fn rerunning_the_sync_keeps_next_only_data() {
    let s = Setup::new("keep");
    let db = Db::open(s.data.join("rekord.db")).unwrap();
    scan_library(&db, &s.root).unwrap();

    // Life in next: an extra account, a favorite, a playlist and a mood.
    let created = accounts::create_account(&s.data, "Solo next").unwrap();
    let next_only = created.created_account_id.unwrap();
    let t3 = db.track_id_by_rel("Artist/Album/03.mp3").unwrap().unwrap();
    db.add_favorite("default", t3).unwrap();
    let mix = db.create_playlist("default", "mix").unwrap();
    let t2 = db.track_id_by_rel("Artist/Album/02.mp3").unwrap().unwrap();
    db.add_to_playlist("default", &mix.id, t2).unwrap();
    let mut ustate = user_state::load_user_state(&s.data, "default");
    ustate.track_moods.insert(
        "Artist/Album/02.mp3".into(),
        serde_json::json!(["fun_quirky"]),
    );
    user_state::save_user_state(&s.data, "default", &ustate).unwrap();

    for _ in 0..2 {
        sync_legacy_library_data(&db, &s.data, &s.root).unwrap();
    }

    let registry = accounts::ensure_accounts(&s.data).unwrap();
    assert!(
        registry.iter().any(|a| a.id == next_only),
        "next-only account kept"
    );
    assert!(
        registry.iter().any(|a| a.id == "legacy-guest"),
        "legacy account added"
    );

    let mut favs = rels(db.list_favorites("default").unwrap());
    favs.sort();
    assert_eq!(favs, vec!["Artist/Album/01.mp3", "Artist/Album/03.mp3"]);

    let pls = db.list_playlists("default").unwrap();
    assert_eq!(pls.len(), 2, "Mix merged by name, Legacy Only added once");
    assert_eq!(
        rels(db.playlist_tracks("default", &mix.id).unwrap()),
        vec!["Artist/Album/02.mp3", "Artist/Album/01.mp3"]
    );

    let after = user_state::load_user_state(&s.data, "default");
    assert_eq!(
        after.track_moods["Artist/Album/02.mp3"],
        serde_json::json!(["fun_quirky"])
    );
    assert_eq!(
        after.track_moods["Artist/Album/01.mp3"],
        serde_json::json!(["chill_relax"])
    );
    assert_eq!(
        after.play_counts["Artist/Album/01.mp3"],
        serde_json::json!(9)
    );
}

#[test]
fn replace_mode_is_still_available_explicitly() {
    let s = Setup::new("replace");
    let db = Db::open(s.data.join("rekord.db")).unwrap();
    scan_library(&db, &s.root).unwrap();
    let t3 = db.track_id_by_rel("Artist/Album/03.mp3").unwrap().unwrap();
    db.add_favorite("default", t3).unwrap();

    sync_legacy_library_data_with(&db, &s.data, &s.root, LegacyImportMode::Replace).unwrap();

    assert_eq!(
        rels(db.list_favorites("default").unwrap()),
        vec!["Artist/Album/01.mp3"]
    );
}

/// A backup entry name picks the account id; separators hidden in it (a
/// Windows `..\`) must not steer the temp theme file out of `accounts/`.
#[tokio::test]
async fn restore_keeps_zip_account_ids_inside_the_accounts_dir() {
    use std::io::Write;
    let s = Setup::new("restore-acc-id");
    let opts = rekord_core::HubOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        data_dir: s.data.clone(),
        client_ui_dir: None,
        admin_ui_dir: None,
    };
    let state = rekord_core::prepare_hub_state(&opts, Some(&s.root), None).unwrap();

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file("config/manifest.json", o).unwrap();
        let manifest = serde_json::json!({
            "kordBackup": 3,
            "createdAt": "2025-01-01T00:00:00Z",
            "libraryRoot": s.root.to_string_lossy(),
        });
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("hub/accounts/..\\..\\evil/theme-bg.png", o)
            .unwrap();
        zip.write_all(b"\x89PNG\r\n\x1a\nfake").unwrap();
        zip.finish().unwrap();
    }
    rekord_core::backup::restore_backup_zip(&state, cursor.into_inner())
        .await
        .unwrap();

    let accounts_dir = s.data.join("accounts");
    for entry in walkdir::WalkDir::new(&s.base) {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains('\\'),
            "zip account id reached the filesystem unsanitised: {}",
            entry.path().display()
        );
        if name.starts_with(".restore-") || name.starts_with("theme-bg") {
            assert!(
                entry.path().starts_with(&accounts_dir),
                "{}",
                entry.path().display()
            );
        }
    }
}
