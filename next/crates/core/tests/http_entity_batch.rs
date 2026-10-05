//! Curiosità batch endpoints and write guard (no network: targets + manual
//! items only).

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{req, From, Hub};

#[tokio::test]
async fn batch_targets_and_batch_save() {
    let hub = Hub::new("einfo-batch");
    hub.file("Caparezza/Exuvia/01.mp3", b"x");
    hub.file("Caparezza/Prisoner 709/01.mp3", b"x");

    let res = hub
        .send(
            req(Method::POST, "/api/v1/entity-info/batch-targets")
                .json(json!({ "artist": "Caparezza", "scope": "artist" }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.json());
    let targets = res.json()["data"]["targets"].as_array().unwrap().clone();
    assert_eq!(targets.len(), 3);
    assert_eq!(targets[0]["key"], "artist");
    assert_eq!(targets[1]["key"], "Caparezza/Exuvia");

    let res = hub
        .send(
            req(Method::POST, "/api/v1/entity-info/batch-targets")
                .json(json!({ "artist": "Caparezza", "scope": "nope" }))
                .build(),
        )
        .await;
    assert_eq!(res.json()["error"], "invalid_scope");

    let save = json!({
        "artist": "Caparezza",
        "rows": [{ "album": "Exuvia", "add": [{ "lang": "it", "text": "Album del 2021." }] }],
    });
    // Library write: refused from the LAN, allowed locally to any account.
    let res = hub
        .send(
            req(Method::POST, "/api/v1/entity-info/batch-save")
                .from(From::Lan)
                .json(save.clone())
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.json()["error"], "forbidden_remote");
    let res = hub
        .send(
            req(Method::POST, "/api/v1/entity-info/batch-save")
                .json(save)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.json());
    let r = &res.json()["data"]["results"][0];
    assert_eq!(r["key"], "Caparezza/Exuvia");
    assert_eq!(r["saved"], 1);
}
