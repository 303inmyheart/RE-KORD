//! Curiosità storage: kord-* files, legacy migration, dedupe, edit/delete,
//! batch targets and safe folder names (no network).

use rekord_core::entity_info::{
    batch_save_entity_info, batch_targets, get_entity_info, save_entity_info, BatchSaveRow,
    BatchScope, EntityInfoEdit, EntityInfoSaveRequest,
};
use rekord_core::metadata::entity_search::{item_content_id, strip_wikitext, wikiquote_quotes};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn library() -> PathBuf {
    let root = std::env::temp_dir().join(format!("rekord-einfo-it-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("Salmo/Playlist")).unwrap();
    fs::create_dir_all(root.join("Salmo/Hellvisback")).unwrap();
    fs::create_dir_all(root.join("Caparezza/Exuvia")).unwrap();
    root
}

fn read(p: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn album_items_always_go_to_kord_albuminfo() {
    let root = library();
    let dir = root.join("Caparezza/Exuvia");
    fs::write(
        dir.join("wpp-albuminfo.json"),
        r#"{"title":"Exuvia","genre":"Rap","infoItems":[{"lang":"it","text":"Primo."}]}"#,
    )
    .unwrap();
    let b = save_entity_info(
        &root,
        EntityInfoSaveRequest {
            artist: "Caparezza".into(),
            album: Some("Exuvia".into()),
            add: vec![json!({"lang": "it", "text": "Secondo.", "source": "wikipedia"})],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(b.items.len(), 2);
    let k = read(dir.join("kord-albuminfo.json"));
    assert_eq!(k["genre"], "Rap", "other album keys survive");
    assert_eq!(k["infoItems"].as_array().unwrap().len(), 2);
    // The legacy file is not written.
    let w = read(dir.join("wpp-albuminfo.json"));
    assert_eq!(w["infoItems"].as_array().unwrap().len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ids_are_stable_and_items_editable() {
    let root = library();
    fs::write(
        root.join("Salmo/kord-artistinfo.json"),
        r#"{"info":{"lang":"it","text":"Salmo è un rapper."}}"#,
    )
    .unwrap();
    let a = get_entity_info(&root, "Salmo", None).unwrap();
    assert_eq!(a.items[0].id, item_content_id("it", "Salmo è un rapper."));
    let id = a.items[0].id.clone();
    let b = save_entity_info(
        &root,
        EntityInfoSaveRequest {
            artist: "Salmo".into(),
            edit: vec![EntityInfoEdit {
                id: id.clone(),
                text: Some("Salmo è un rapper sardo.".into()),
                title: None,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(b.edited, Some(1));
    let j = read(root.join("Salmo/kord-artistinfo.json"));
    assert!(j.get("info").is_none(), "legacy key migrated");
    assert_eq!(j["items"][0]["id"], json!(id));
    assert_eq!(j["items"][0]["text"], "Salmo è un rapper sardo.");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn traversal_and_unknown_folders_are_refused() {
    let root = library();
    for (artist, album) in [
        ("..", None),
        ("Salmo", Some("..")),
        ("Salmo", Some("../Caparezza")),
        ("Salmo/../Caparezza", None),
        ("kord", None),
    ] {
        assert!(
            get_entity_info(&root, artist, album).is_err(),
            "{artist} {album:?}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn batch_save_reports_per_album_counts() {
    let root = library();
    let targets = batch_targets(&root, None, "Salmo", &BatchScope::Artist).unwrap();
    let keys: Vec<&str> = targets.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys, vec!["artist", "Salmo/Hellvisback", "Salmo/Playlist"]);
    let selected = batch_targets(
        &root,
        None,
        "Salmo",
        &BatchScope::Albums(vec!["Salmo/Playlist".into()]),
    )
    .unwrap();
    assert_eq!(selected.len(), 1);
    let res = batch_save_entity_info(
        &root,
        "Salmo",
        vec![
            BatchSaveRow {
                album: Some("Playlist".into()),
                add: vec![
                    json!({"lang": "it", "text": "Uno."}),
                    json!({"lang": "it", "text": "Due."}),
                    json!({"lang": "it", "text": "uno"}),
                ],
                ..Default::default()
            },
            BatchSaveRow {
                album: Some("Missing".into()),
                add: vec![json!({"lang": "it", "text": "x"})],
                ..Default::default()
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(res[0].saved, 2);
    assert_eq!(res[0].duplicates, 1);
    assert_eq!(res[1].saved, 0);
    assert_eq!(res[1].error.as_ref().unwrap()["code"], "album_not_found");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn wikitext_and_quotes_parsing() {
    assert_eq!(
        strip_wikitext("'''Exuvia''' è un [[album]] di [[Caparezza|Capa]].<ref name=\"a\"/>"),
        "Exuvia è un album di Capa."
    );
    assert!(wikiquote_quotes("en", "Psalms are hymns.\nA line that is long enough to count as a quote here.\nAnother line that is long enough to count as a quote.").is_none());
}
