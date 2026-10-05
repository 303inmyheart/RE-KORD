//! Account registry under concurrency, and account deletion cleanup
//! (accounts QA items 10, 16, 18).

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{req, Hub};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn parallel_creates_keep_every_account() {
    let hub = Hub::new("acc-race");
    let mut tasks = Vec::new();
    for i in 0..8 {
        let app = hub.app.clone();
        tasks.push(tokio::spawn(async move {
            let res = app
                .oneshot(
                    req(Method::POST, "/api/v1/accounts")
                        .json(json!({ "name": format!("Racer {i}") }))
                        .build(),
                )
                .await
                .unwrap();
            res.status()
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::CREATED);
    }
    let res = hub.send(req(Method::GET, "/api/v1/accounts").build()).await;
    let accounts = res.json()["data"]["accounts"].as_array().unwrap().clone();
    assert_eq!(accounts.len(), 9, "{accounts:?}");
    for i in 0..8 {
        assert!(
            accounts.iter().any(|a| a["name"] == format!("Racer {i}")),
            "Racer {i} lost"
        );
    }
    // No temp files left behind.
    let stray: Vec<_> = std::fs::read_dir(&hub.data)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

#[tokio::test]
async fn default_account_is_named_default() {
    let hub = Hub::new("acc-name");
    let res = hub.send(req(Method::GET, "/api/v1/accounts").build()).await;
    let accounts = res.json()["data"]["accounts"].clone();
    assert_eq!(accounts[0]["id"], "default");
    assert_eq!(accounts[0]["name"], "Default");
}

#[tokio::test]
async fn deleting_an_account_removes_its_state_folder_and_rows() {
    let hub = Hub::new("acc-delete");
    let res = hub
        .send(
            req(Method::POST, "/api/v1/accounts")
                .json(json!({ "name": "Temp" }))
                .build(),
        )
        .await;
    let id = res.json()["data"]["createdAccountId"]
        .as_str()
        .unwrap()
        .to_string();
    // Personal state: user-state + theme background.
    let res = hub
        .send(
            req(Method::PATCH, "/api/v1/user-state")
                .header("x-rekord-account-id", &id)
                .json(json!({ "trackMoods": { "A/B/01.mp3": ["chill_relax"] } }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let info = hub.data.join("accounts").join(format!("{id}_info"));
    assert!(info.join("user-state.json").is_file());
    std::fs::write(info.join("theme-bg.jpg"), b"x").unwrap();
    let pl = hub
        .send(
            req(Method::POST, "/api/v1/playlists")
                .header("x-rekord-account-id", &id)
                .json(json!({ "name": "Mine" }))
                .build(),
        )
        .await;
    assert!(pl.status.is_success(), "{:?}", pl.json());

    let res = hub
        .send(req(Method::DELETE, &format!("/api/v1/accounts/{id}")).build())
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(!info.exists(), "accounts/<id>_info left behind");
    assert!(!hub.data.join("accounts").join(&id).exists());
    assert!(hub.state.db.list_playlists(&id).unwrap().is_empty());

    let res = hub
        .send(req(Method::DELETE, &format!("/api/v1/accounts/{id}")).build())
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.json()["error"], "account_not_found");
    let res = hub
        .send(req(Method::DELETE, "/api/v1/accounts/default").build())
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.json()["error"], "cannot_delete_default_account");
}
