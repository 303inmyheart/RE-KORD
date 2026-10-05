//! `/api/v1/user-state`: optimistic revisions (409 + current), atomic per-
//! account writes under concurrency, mood validation, strict account ids and
//! corrupt-file recovery.

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use std::sync::Arc;
use support::{req, Hub};

async fn patch(hub: &Hub, body: serde_json::Value) -> support::Resp {
    hub.send(req(Method::PATCH, "/api/v1/user-state").json(body).build())
        .await
}

#[tokio::test]
async fn stale_expected_revision_gets_409_with_current_state() {
    let hub = Hub::new("us-409");
    let res = patch(&hub, json!({ "recentRelPaths": ["A/B/01.mp3"] })).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["data"]["revision"], 1);

    let res = patch(
        &hub,
        json!({ "expectedRevision": 1, "settings": { "theme": "dark" } }),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["data"]["revision"], 2);

    let res = patch(
        &hub,
        json!({ "expectedRevision": 1, "settings": { "theme": "light" } }),
    )
    .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    let j = res.json();
    assert_eq!(j["error"], "revision_conflict");
    assert_eq!(j["current"]["revision"], 2);
    assert_eq!(j["current"]["settings"]["theme"], "dark");
    assert_eq!(j["current"]["recentRelPaths"][0], "A/B/01.mp3");

    // PUT honours the same contract.
    let res = hub
        .send(
            req(Method::PUT, "/api/v1/user-state")
                .json(json!({
                    "expectedRevision": 0,
                    "state": { "version": 1, "revision": 0 }
                }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn concurrent_patches_are_serialised_per_account() {
    let hub = Arc::new(Hub::new("us-concurrent"));
    let mut tasks = Vec::new();
    for i in 0..24 {
        let hub = hub.clone();
        tasks.push(tokio::spawn(async move {
            patch(&hub, json!({ "settings": { format!("k{i}"): i } }))
                .await
                .status
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let res = hub
        .send(req(Method::GET, "/api/v1/user-state").build())
        .await;
    let j = res.json();
    assert_eq!(j["data"]["revision"], 24, "every write bumped once");
    assert_eq!(
        j["data"]["settings"].as_object().unwrap().len(),
        24,
        "no lost update"
    );

    // Same expected revision from many clients: exactly one wins.
    let mut tasks = Vec::new();
    for i in 0..12 {
        let hub = hub.clone();
        tasks.push(tokio::spawn(async move {
            patch(
                &hub,
                json!({ "expectedRevision": 24, "settings": { "race": i } }),
            )
            .await
            .status
        }));
    }
    let mut won = 0;
    for t in tasks {
        match t.await.unwrap() {
            StatusCode::OK => won += 1,
            StatusCode::CONFLICT => {}
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(won, 1);
}

#[tokio::test]
async fn track_moods_are_validated() {
    let hub = Hub::new("us-moods");
    let res = patch(
        &hub,
        json!({ "trackMoods": {
            "A/B/01.mp3": ["chill_relax", "bogus", "uplifting_happy", "dark_tense", "fun_quirky"],
            "A/B/02.mp3": ["nope"]
        } }),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK);
    let moods = &res.json()["data"]["trackMoods"];
    assert_eq!(
        moods["A/B/01.mp3"],
        json!(["chill_relax", "motivational_drive", "dark_tense"])
    );
    assert!(moods.get("A/B/02.mp3").is_none());
}

#[tokio::test]
async fn account_ids_are_validated_everywhere() {
    let hub = Hub::new("us-ids");
    for (name, value) in [
        ("x-rekord-account-id", "../../x"),
        ("x-kord-account-id", "..\\..\\x"),
        ("x-rekord-account-id", "a/b"),
        ("x-rekord-account-id", "a.b"),
    ] {
        let res = hub
            .send(
                req(Method::PATCH, "/api/v1/user-state")
                    .header(name, value)
                    .json(json!({ "settings": { "x": 1 } }))
                    .build(),
            )
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{value}");
        assert_eq!(res.json()["error"], "invalid_account_id");
    }
    let res = hub
        .send(req(Method::GET, "/api/v1/user-state?accountId=..%2F..%2Fx").build())
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Well-formed but unknown: 404, never a silent fallback to Default.
    let res = hub
        .send(
            req(Method::GET, "/api/v1/user-state")
                .header("x-rekord-account-id", "ghost")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = hub
        .send(
            req(Method::GET, "/api/v1/favorites")
                .header("x-rekord-account-id", "ghost")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = hub
        .send(
            req(Method::DELETE, "/api/v1/jobs")
                .header("x-rekord-account-id", "ghost")
                .build(),
        )
        .await;
    assert_eq!(
        res.status,
        StatusCode::FORBIDDEN,
        "unknown id is not Default"
    );

    let res = hub
        .send(
            req(Method::PUT, "/api/v1/accounts/..%2Fx")
                .json(json!({ "name": "x" }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Nothing escaped the data dir.
    assert!(!hub.base.join("x_info").exists());
    assert!(!hub.data.join("x_info").exists());
}

#[tokio::test]
async fn corrupt_state_file_is_kept_aside() {
    let hub = Hub::new("us-corrupt");
    let dir = hub.data.join("accounts").join("default_info");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("user-state.json"), b"{ \"revision\": 7, oops").unwrap();
    let res = patch(&hub, json!({ "settings": { "theme": "dark" } })).await;
    assert_eq!(res.status, StatusCode::OK);
    let kept = std::fs::read_dir(&dir).unwrap().flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .starts_with("user-state.json.corrupt-")
    });
    assert!(kept);
    // No temp files left behind by the atomic writer.
    let temps = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(temps, 0);
}

#[tokio::test]
async fn default_body_limit_is_small_but_upload_routes_accept_more() {
    let hub = Hub::new("body-limit");
    // ~3 MiB JSON on a plain route: refused.
    let big = "x".repeat(3 * 1024 * 1024);
    let res = patch(&hub, json!({ "settings": { "blob": big } })).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);

    // ~3 MiB theme background: accepted (route-level limit 33 MiB).
    let mut png = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    png.resize(3 * 1024 * 1024, 0);
    let (ct, body) = support::multipart_file("bg.png", "image/png", &png);
    let res = hub
        .send(
            req(Method::POST, "/api/v1/user-state/custom-theme-bg")
                .raw_body(&ct, body)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.json());

    // ~3 MiB restore upload: past the default limit, rejected only as "not a zip".
    let (ct, body) =
        support::multipart_file("b.zip", "application/zip", &vec![b'z'; 3 * 1024 * 1024]);
    let res = hub
        .send(
            req(Method::POST, "/api/v1/backup/kord-restore")
                .raw_body(&ct, body)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}
