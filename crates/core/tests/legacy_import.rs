//! Import of legacy RE-KORD personal data (`<music>/.kord`) into a hub, with
//! fixtures shaped like a real legacy install (`tests/fixtures/legacy_kord`,
//! paths and titles anonymised): favorites, playlists, moods, blocked tracks
//! and albums, play counts, settings and Plectr records, per account.

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use rekord_core::accounts::{self, Account};
use rekord_core::backup::{
    auto_import_legacy_once, legacy_import_pending, read_legacy_import_marker, run_legacy_import,
    sync_legacy_library_data, LegacyAccountReport, LegacyAccountStatus, LegacyImportOptions,
    LegacyImportReport, LegacyImportTrigger,
};
use rekord_core::db::Db;
use rekord_core::scan::{scan_library, ScanMode};
use rekord_core::user_state;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use support::{req, From, Hub};

const ALICE: &str = "0b6d1c52-3f7e-4c1a-9d2e-5a8b7c6d4e3f";
const TESTER: &str = "7f3e2a1b-8c9d-4e5f-a6b7-c8d9e0f1a2b3";
const DELETED: &str = "5c4b3a29-1817-4615-9a4b-3c2d1e0f9a8b";

const U1: &str = "Artista Uno/Primo Album/01 - Brano Uno.mp3";
const U2: &str = "Artista Uno/Primo Album/02 - Brano Due.mp3";
const U3: &str = "Artista Uno/Primo Album/03 - Brano Tre.mp3";
const U4: &str = "Artista Uno/Primo Album/04 - Brano Quattro.mp3";
const B1: &str = "Band Due/Live： Night？/01 - Canzone ⧸ Remix.mp3";
/// On disk in NFD (as macOS writes it); legacy recorded it in NFC.
const B2_DISK: &str = "Band Due/Citta\u{300}/02 - Perche\u{301}.mp3";
const SINGLE: &str = "Artista Uno/Tracks/05 - Singolo.mp3";
const GONE: &str = "Artista Uno/Primo Album/99 - Cancellato.mp3";

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy_kord")
}

/// Copy the fixture `.kord` into `root` and add the theme backgrounds.
fn install_legacy_kord(root: &Path) {
    let src = fixture_dir();
    let dest = root.join(".kord");
    for entry in walkdir::WalkDir::new(&src) {
        let entry = entry.unwrap();
        let rel = entry.path().strip_prefix(&src).unwrap();
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
    for acc in ["default", ALICE] {
        fs::write(
            dest.join(format!("{acc}_info/theme-bg.jpg")),
            b"\xff\xd8\xff\xe0legacy",
        )
        .unwrap();
    }
}

fn library_files() -> [&'static str; 7] {
    [U1, U2, U3, U4, B1, B2_DISK, SINGLE]
}

struct Lib {
    base: PathBuf,
    root: PathBuf,
    data: PathBuf,
    db: Db,
}

impl Lib {
    fn new(tag: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("rekord-legacy-{tag}-{}", uuid::Uuid::new_v4()));
        let root = base.join("music");
        let data = base.join("data");
        fs::create_dir_all(&data).unwrap();
        for rel in library_files() {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"audio").unwrap();
        }
        install_legacy_kord(&root);
        accounts::ensure_accounts(&data).unwrap();
        let db = Db::open(data.join("rekord.db")).unwrap();
        scan_library(&db, &root).unwrap();
        Self {
            base,
            root,
            data,
            db,
        }
    }

    fn run(&self, trigger: LegacyImportTrigger, dry_run: bool, force: bool) -> LegacyImportReport {
        run_legacy_import(
            &self.db,
            &self.data,
            &self.root,
            LegacyImportOptions {
                trigger,
                dry_run,
                force,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn favorites(&self, account: &str) -> Vec<String> {
        let mut v = self.db.export_favorite_rel_paths(account).unwrap();
        v.sort();
        v
    }

    fn playlist_names(&self, account: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .db
            .list_playlists(account)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        v.sort();
        v
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn account<'a>(report: &'a LegacyImportReport, legacy_id: &str) -> &'a LegacyAccountReport {
    report
        .accounts
        .iter()
        .find(|a| a.legacy_id == legacy_id)
        .unwrap_or_else(|| panic!("{legacy_id} missing from the report"))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The reported regression: RE-KORD 5 was used before the automatic import
/// ran, and that import skipped every account "in use" (the default one
/// always has settings). The fixed import merges into them.
#[test]
fn accounts_already_used_in_rekord5_receive_their_legacy_data() {
    let lib = Lib::new("in-use");
    let data = &lib.data;

    // Life in RE-KORD 5 before the import: "Alice" created by hand (another
    // id), the default account with a theme, a queue, a favorite and a playlist.
    let alice = accounts::create_account(data, "Alice")
        .unwrap()
        .created_account_id
        .unwrap();
    assert_ne!(alice, ALICE);
    let mut registry = accounts::ensure_accounts(data).unwrap();
    // Tester was imported by the earlier version.
    registry.push(Account {
        id: TESTER.into(),
        name: "Tester".into(),
    });
    accounts::replace_accounts_registry(data, &registry).unwrap();
    user_state::update_user_state(data, "default", None, |s| {
        s.settings.insert("theme".into(), json!("neon"));
        s.settings
            .insert("queue".into(), json!({ "relPaths": [U4] }));
        s.play_counts.insert(U1.into(), json!(20));
        s.track_moods.insert(U1.into(), json!(["fun_quirky"]));
    })
    .unwrap();
    let t3 = lib.db.track_id_by_rel(U3).unwrap().unwrap();
    lib.db.add_favorite("default", t3).unwrap();
    lib.db.create_playlist("default", "Coda salvata").unwrap();
    user_state::update_user_state(data, &alice, None, |s| {
        s.settings.insert("theme".into(), json!("custom"));
    })
    .unwrap();
    let alice_bg = user_state::theme_bg_path_for_ext(data, &alice, "png");
    fs::create_dir_all(alice_bg.parent().unwrap()).unwrap();
    fs::write(&alice_bg, b"\x89PNGmine").unwrap();

    // The marker the buggy version wrote.
    fs::write(
        data.join("legacy-import.json"),
        serde_json::to_vec_pretty(&json!({
            "importedAt": "2026-10-06T08:47:55Z",
            "musicRoot": lib.root.to_string_lossy(),
            "albumMetaMerged": 0,
            "trackMetaMerged": 0,
            "accounts": {
                "default": { "hubId": "default", "imported": false, "reason": "hub_account_in_use" },
                ALICE: { "hubId": alice, "imported": false, "reason": "hub_account_in_use" },
                TESTER: { "hubId": TESTER, "imported": true },
                DELETED: { "hubId": DELETED, "imported": false, "reason": "not_registered" }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        legacy_import_pending(data, &lib.root),
        "old marker: run again"
    );

    let report = auto_import_legacy_once(&lib.db, data, &lib.root)
        .unwrap()
        .expect("import ran");
    assert_eq!(report.trigger, LegacyImportTrigger::Auto);
    assert_eq!(report.accounts_added, 0);

    // --- default
    let def = account(&report, "default");
    assert_eq!(def.status, LegacyAccountStatus::Imported);
    assert_eq!(def.counts.favorites, 3);
    assert_eq!(def.counts.favorites_parked, 1);
    assert_eq!(def.counts.playlists, 3);
    assert_eq!(def.unmatched_paths, vec![GONE.to_string()]);
    assert_eq!(
        def.unmatched_album_keys,
        vec!["Gruppo Sparito::Album Perso"]
    );
    assert_eq!(
        lib.favorites("default"),
        sorted(vec![B1.into(), U1.into(), U3.into(), GONE.into()])
    );
    assert_eq!(
        lib.playlist_names("default"),
        vec![
            "Coda salvata",
            "Mix",
            "Queue Test Playlist - 01",
            "Queue Test Playlist - 01"
        ]
    );
    let mix = lib
        .db
        .list_playlists("default")
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Mix")
        .unwrap();
    let mix_tracks: Vec<String> = lib
        .db
        .playlist_tracks("default", &mix.id)
        .unwrap()
        .into_iter()
        .map(|t| t.rel_path)
        .collect();
    // Case-insensitive match, then the `Tracce` → `Tracks` rename; the
    // deleted file stays parked.
    assert_eq!(mix_tracks, vec![U4.to_string(), SINGLE.to_string()]);

    let s = user_state::load_user_state(data, "default");
    assert_eq!(s.settings["theme"], json!("neon"), "RE-KORD 5 settings win");
    assert_eq!(
        s.settings["crossfadeSec"],
        json!(5),
        "missing settings are added"
    );
    assert!(
        !s.settings.contains_key("locale"),
        "language left as chosen"
    );
    assert!(
        !s.settings.contains_key("legacyQueue"),
        "current queue kept"
    );
    assert_eq!(s.play_counts[U1], json!(20), "higher count kept");
    assert_eq!(
        s.play_counts[B2_DISK],
        json!(3),
        "NFC legacy path matched NFD file"
    );
    assert_eq!(
        s.track_moods[U1],
        json!(["fun_quirky"]),
        "RE-KORD 5 mood kept"
    );
    assert_eq!(
        s.track_moods[U3],
        json!(["aggressive_heavy", "motivational_drive"])
    );
    assert_eq!(s.track_moods[B2_DISK], json!(["sad_melancholy"]));
    for blocked in [U1, U3, B2_DISK] {
        assert!(
            s.excluded_rel_paths.contains(&blocked.to_string()),
            "{blocked}"
        );
    }
    let live = lib
        .db
        .list_albums()
        .unwrap()
        .into_iter()
        .find(|a| a.folder_key == "Band Due/Live： Night？")
        .unwrap();
    assert_eq!(s.excluded_album_ids, vec![live.id]);
    assert_eq!(
        s.settings["legacyExcludedAlbumKeys"],
        json!(["Gruppo Sparito::Album Perso"])
    );
    assert_eq!(s.settings["plectr"]["bests"][U1]["score"], json!(27630));

    // --- Alice: matched by name, data under the RE-KORD 5 id.
    let a = account(&report, ALICE);
    assert_eq!(a.status, LegacyAccountStatus::Imported);
    assert_eq!(a.hub_id, alice);
    assert_eq!(a.counts.favorites, 4);
    assert_eq!(a.counts.moods, 2);
    assert_eq!(a.counts.excluded_tracks, 1);
    assert_eq!(a.counts.theme_backgrounds, 0, "own background kept");
    assert!(!data.join(format!("accounts/{ALICE}_info")).exists());
    assert_eq!(
        lib.favorites(&alice),
        sorted(vec![U2.into(), U3.into(), B2_DISK.into(), SINGLE.into()])
    );
    assert_eq!(lib.playlist_names(&alice), vec!["test"]);
    let sa = user_state::load_user_state(data, &alice);
    assert_eq!(sa.settings["theme"], json!("custom"));
    assert_eq!(sa.excluded_rel_paths, vec![U2.to_string()]);
    assert_eq!(sa.settings["plectr"]["bests"][B1]["score"], json!(6300));
    assert_eq!(fs::read(&alice_bg).unwrap(), b"\x89PNGmine");

    // --- imported by the earlier version: left alone.
    let t = account(&report, TESTER);
    assert_eq!(t.status, LegacyAccountStatus::Unchanged);
    assert!(lib.db.list_playlists(TESTER).unwrap().is_empty());

    // --- deleted in legacy / test folders: never become accounts.
    for id in [DELETED, "route-test"] {
        let r = account(&report, id);
        assert_eq!(r.status, LegacyAccountStatus::Skipped);
        assert_eq!(r.reason.as_deref(), Some("not_registered"));
        assert!(!data.join(format!("accounts/{id}_info")).exists());
    }

    let marker = read_legacy_import_marker(data).unwrap();
    assert_eq!(marker.version, 2);
    assert!(marker.accounts["default"].imported);
    assert!(marker.accounts["default"].fingerprint.is_some());
    assert!(marker.last_report.is_some());
    assert!(!legacy_import_pending(data, &lib.root));
}

#[test]
fn reruns_add_nothing_twice_and_respect_removals() {
    let lib = Lib::new("rerun");
    let first = auto_import_legacy_once(&lib.db, &lib.data, &lib.root)
        .unwrap()
        .unwrap();
    assert_eq!(first.accounts_added, 2, "Alice and Tester created");
    let registry = accounts::ensure_accounts(&lib.data).unwrap();
    let names: Vec<(&str, &str)> = registry
        .iter()
        .map(|a| (a.id.as_str(), a.name.as_str()))
        .collect();
    assert!(names.contains(&("default", "Default")));
    assert!(names.contains(&(ALICE, "Alice")), "legacy id and name kept");
    assert!(names.contains(&(TESTER, "Tester")));
    assert!(!registry.iter().any(|a| a.id == DELETED));
    assert_eq!(account(&first, ALICE).counts.selections, 1);
    assert_eq!(account(&first, ALICE).counts.theme_backgrounds, 1);
    let s = user_state::load_user_state(&lib.data, ALICE);
    assert_eq!(
        s.settings["locale"],
        json!("it"),
        "fresh account: language too"
    );

    let favs = lib.favorites("default");
    let playlists = lib.playlist_names("default");
    assert_eq!(favs.len(), 3);
    assert_eq!(playlists.len(), 3);

    // Automatic import does not run twice.
    assert!(auto_import_legacy_once(&lib.db, &lib.data, &lib.root)
        .unwrap()
        .is_none());

    // A manual run finds nothing new.
    let again = sync_legacy_library_data(&lib.db, &lib.data, &lib.root).unwrap();
    assert!(again.totals.is_empty(), "{:?}", again.totals);
    assert_eq!(
        account(&again, "default").status,
        LegacyAccountStatus::Unchanged
    );
    assert_eq!(lib.favorites("default"), favs);
    assert_eq!(lib.playlist_names("default"), playlists);

    // Removed in RE-KORD 5: a manual run leaves it removed …
    let t1 = lib.db.track_id_by_rel(U1).unwrap().unwrap();
    lib.db.remove_favorite("default", t1).unwrap();
    user_state::update_user_state(&lib.data, "default", None, |s| {
        s.track_moods.clear();
    })
    .unwrap();
    lib.run(LegacyImportTrigger::Manual, false, false);
    assert!(!lib.favorites("default").contains(&U1.to_string()));
    assert!(user_state::load_user_state(&lib.data, "default")
        .track_moods
        .is_empty());

    // … `force` brings it back, still without duplicates.
    let forced = lib.run(LegacyImportTrigger::Manual, false, true);
    assert_eq!(account(&forced, "default").counts.favorites, 1);
    assert_eq!(account(&forced, "default").counts.playlists, 0);
    assert_eq!(lib.favorites("default"), favs);
    assert_eq!(lib.playlist_names("default"), playlists);
    assert_eq!(
        user_state::load_user_state(&lib.data, "default")
            .track_moods
            .len(),
        3
    );

    // A changed legacy file is merged again (only what is missing).
    let us = lib.root.join(".kord/default_info/user-state.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&us).unwrap()).unwrap();
    v["favorites"].as_array_mut().unwrap().push(json!(U2));
    fs::write(&us, serde_json::to_vec(&v).unwrap()).unwrap();
    let changed = lib.run(LegacyImportTrigger::Manual, false, false);
    assert_eq!(
        account(&changed, "default").status,
        LegacyAccountStatus::Imported
    );
    assert_eq!(account(&changed, "default").counts.favorites, 1);
    assert_eq!(
        account(&changed, ALICE).status,
        LegacyAccountStatus::Unchanged
    );
}

#[test]
fn a_dry_run_reports_the_same_counts_and_writes_nothing() {
    let lib = Lib::new("dry");
    let preview = lib.run(LegacyImportTrigger::Cli, true, false);
    assert!(preview.dry_run);
    assert!(read_legacy_import_marker(&lib.data).is_none());
    assert_eq!(accounts::ensure_accounts(&lib.data).unwrap().len(), 1);
    assert!(lib.favorites("default").is_empty());
    assert!(lib.db.list_playlists("default").unwrap().is_empty());
    assert!(user_state::load_user_state(&lib.data, "default")
        .track_moods
        .is_empty());
    assert!(!lib.data.join(format!("accounts/{ALICE}_info")).exists());

    let real = lib.run(LegacyImportTrigger::Cli, false, false);
    assert_eq!(preview.totals, real.totals);
    assert_eq!(preview.accounts_added, real.accounts_added);
    assert_eq!(preview.unmatched_count, real.unmatched_count);
    let def = account(&real, "default");
    assert_eq!(def.counts.favorites, 3);
    assert_eq!(def.counts.playlists, 3);
    assert_eq!(def.counts.playlist_tracks, 6);
    assert_eq!(def.counts.playlist_tracks_parked, 1);
    assert_eq!(def.counts.moods, 3);
    assert_eq!(def.counts.excluded_tracks, 3);
    assert_eq!(def.counts.excluded_albums, 1);
    assert_eq!(def.counts.play_counts, 4);
    assert_eq!(def.counts.recent, 3);
    assert_eq!(def.counts.plectr_bests, 2);
    assert_eq!(def.counts.theme_backgrounds, 1);
    assert_eq!(def.counts.selections, 0, "default keeps its own selection");
}

#[tokio::test]
async fn first_scan_imports_and_the_admin_api_reports_and_reruns() {
    let hub = Hub::new("legacy-api");
    for rel in library_files() {
        hub.file(rel, b"audio");
    }
    install_legacy_kord(&hub.root);
    hub.state
        .run_scan_mode(ScanMode::Incremental)
        .await
        .unwrap();
    assert_eq!(
        hub.state
            .db
            .export_favorite_rel_paths("default")
            .unwrap()
            .len(),
        3
    );

    let status = hub
        .send(req(Method::GET, "/api/v1/legacy-import").build())
        .await;
    assert_eq!(status.status, StatusCode::OK);
    let body = status.json();
    assert_eq!(body["data"]["kordFound"], json!(true));
    assert_eq!(body["data"]["pending"], json!(false));
    assert_eq!(body["data"]["lastReport"]["trigger"], json!("auto"));
    assert_eq!(body["data"]["lastReport"]["totals"]["favorites"], json!(7));

    let dry = hub
        .send(req(Method::POST, "/api/v1/legacy-import?dryRun=true").build())
        .await;
    assert_eq!(dry.status, StatusCode::OK);
    let body = dry.json();
    assert_eq!(body["data"]["dryRun"], json!(true));
    assert_eq!(body["data"]["totals"]["favorites"], json!(0));

    let forced = hub
        .send(req(Method::POST, "/api/v1/legacy-import?force=true").build())
        .await;
    assert_eq!(forced.status, StatusCode::OK);
    assert_eq!(forced.json()["data"]["trigger"], json!("manual"));
    // The older route answers with the same report.
    let old = hub
        .send(req(Method::POST, "/api/v1/library/sync-legacy-meta").build())
        .await;
    assert_eq!(old.status, StatusCode::OK);
    assert!(old.json()["data"]["accounts"].is_array());

    let lan = hub
        .send(
            req(Method::POST, "/api/v1/legacy-import")
                .from(From::Lan)
                .build(),
        )
        .await;
    assert_eq!(lan.status, StatusCode::FORBIDDEN);
}
