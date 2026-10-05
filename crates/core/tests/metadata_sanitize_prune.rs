//! Sanitize-titles preview order / user edits, and prune keeping fetched
//! track numbers (QA studio #17, library #15). No network.

use rekord_core::db::Db;
use rekord_core::metadata::{prune_album_library_metadata, sanitize_track_titles};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn setup() -> (PathBuf, PathBuf, Db) {
    let base = std::env::temp_dir().join(format!("rekord-meta-it-{}", uuid::Uuid::new_v4()));
    let root = base.join("music");
    for (album, files) in [
        (
            "Eagles/Desperado",
            vec![
                "10 - Bitter Creek (2013 Remaster).m4a",
                "02 - Twenty-One (2013 Remaster).m4a",
                "01 - Eagles - Doolin-Dalton (Official Audio).m4a",
            ],
        ),
        (
            "Aura/Aura Farming",
            vec!["01 - Zero Effort (Official Video).m4a"],
        ),
    ] {
        fs::create_dir_all(root.join(album)).unwrap();
        for f in files {
            fs::write(root.join(album).join(f), b"x").unwrap();
        }
    }
    let db = Db::open(base.join("rekord.db")).unwrap();
    (base, root, db)
}

#[test]
fn sanitize_preview_is_sorted_and_skips_user_titles() {
    let (base, root, db) = setup();
    fs::write(
        root.join("Eagles/Desperado/kord-trackinfo.json"),
        r#"{"02 - Twenty-One (2013 Remaster).m4a": {"title": "21", "userEdited": ["title"]}}"#,
    )
    .unwrap();
    let v = sanitize_track_titles(&root, &db, "album", Some("Eagles/Desperado"), true).unwrap();
    let files: Vec<&str> = v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["fileName"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        vec![
            "01 - Eagles - Doolin-Dalton (Official Audio).m4a",
            "10 - Bitter Creek (2013 Remaster).m4a",
        ]
    );
    assert_eq!(v["changes"][0]["to"], "Doolin-Dalton");
    assert_eq!(v["changes"][1]["to"], "Bitter Creek");
    assert_eq!(v["skipped"][0]["reason"], "user_edited");
    // Dry run writes nothing.
    let side: Value = serde_json::from_str(
        &fs::read_to_string(root.join("Eagles/Desperado/kord-trackinfo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(side.as_object().unwrap().len(), 1);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn prune_keeps_fetched_numbers_and_drops_orphans() {
    let (base, root, _db) = setup();
    let dir = root.join("Eagles/Desperado");
    fs::write(
        dir.join("kord-trackinfo.json"),
        json!({
            "gone.mp3": {"title": "Old"},
            "02 - Twenty-One (2013 Remaster).m4a": {"trackNumber": 2, "discNumber": 1, "source": "musicbrainz", "fetchedAt": "x"},
            "10 - Bitter Creek (2013 Remaster).m4a": {"trackNumber": 9},
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("kord-albuminfo.json"),
        json!({"expectedTrackCount": 11, "source": "musicbrainz"}).to_string(),
    )
    .unwrap();
    let v = prune_album_library_metadata(&root, "Eagles/Desperado").unwrap();
    assert_eq!(v["removed"], json!(["gone.mp3"]));
    assert_eq!(v["trackOrderingFieldsCleared"], 1);
    assert_eq!(v["expectedTracksCleared"], false);
    let side: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("kord-trackinfo.json")).unwrap())
            .unwrap();
    assert_eq!(
        side["02 - Twenty-One (2013 Remaster).m4a"]["trackNumber"],
        2
    );
    assert!(side["10 - Bitter Creek (2013 Remaster).m4a"]
        .get("trackNumber")
        .is_none());
    let album: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("kord-albuminfo.json")).unwrap())
            .unwrap();
    assert_eq!(album["expectedTrackCount"], 11);
    let _ = fs::remove_dir_all(&base);
}
