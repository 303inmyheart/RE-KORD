//! Origin policy: Tauri shells, same-origin SPA and dev servers work (incl.
//! preflights with custom headers); any other web page is refused on writes,
//! gets no ACAO on reads and never counts as a local admin.

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{req, From, Hub, LAN_HOST};

#[tokio::test]
async fn tauri_origins_can_read_with_cors_headers() {
    let hub = Hub::new("cors-tauri");
    for origin in [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ] {
        let res = hub
            .send(
                req(Method::GET, "/api/v1/health")
                    .header("origin", origin)
                    .build(),
            )
            .await;
        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(
            res.header("access-control-allow-origin").as_deref(),
            Some(origin),
            "{origin}"
        );
    }
}

#[tokio::test]
async fn preflight_with_account_header_is_allowed_for_tauri() {
    let hub = Hub::new("cors-preflight");
    for method in ["PATCH", "DELETE", "POST", "PUT"] {
        let res = hub
            .send(
                req(Method::OPTIONS, "/api/v1/user-state")
                    .header("origin", "http://tauri.localhost")
                    .header("access-control-request-method", method)
                    .header(
                        "access-control-request-headers",
                        "content-type,x-rekord-account-id",
                    )
                    .from(From::Lan)
                    .build(),
            )
            .await;
        assert!(res.status.is_success(), "{method}: {}", res.status);
        assert_eq!(
            res.header("access-control-allow-origin").as_deref(),
            Some("http://tauri.localhost")
        );
        let allow_methods = res
            .header("access-control-allow-methods")
            .unwrap_or_default();
        assert!(allow_methods.contains(method), "{allow_methods}");
        let allow_headers = res
            .header("access-control-allow-headers")
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(
            allow_headers.contains("x-rekord-account-id"),
            "{allow_headers}"
        );
    }
}

#[tokio::test]
async fn tauri_android_can_patch_from_lan() {
    let hub = Hub::new("cors-android");
    let res = hub
        .send(
            req(Method::PATCH, "/api/v1/user-state")
                .from(From::Lan)
                .header("origin", "http://tauri.localhost")
                .json(json!({ "recentRelPaths": ["A/B/01.mp3"] }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.header("access-control-allow-origin").as_deref(),
        Some("http://tauri.localhost")
    );
}

#[tokio::test]
async fn same_origin_spa_and_dev_servers_are_allowed() {
    let hub = Hub::new("cors-same");
    let res = hub
        .send(
            req(Method::PATCH, "/api/v1/user-state")
                .from(From::Lan)
                .header("origin", &format!("http://{LAN_HOST}"))
                .json(json!({ "settings": { "theme": "dark" } }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    let res = hub
        .send(
            req(Method::PATCH, "/api/v1/user-state")
                .header("origin", "http://localhost:7422")
                .json(json!({ "settings": { "theme": "light" } }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn env_allowlist_adds_origins() {
    std::env::set_var(
        rekord_core::origin::ALLOWED_ORIGINS_ENV,
        "https://music.example.test, http://other.example.test:8080",
    );
    let hub = Hub::new("cors-env");
    let res = hub
        .send(
            req(Method::GET, "/api/v1/health")
                .header("origin", "https://music.example.test")
                .build(),
        )
        .await;
    assert_eq!(
        res.header("access-control-allow-origin").as_deref(),
        Some("https://music.example.test")
    );
}

#[tokio::test]
async fn foreign_origin_writes_are_forbidden() {
    let hub = Hub::new("cors-foreign");
    for origin in ["https://evil.example", "null", "http://localhost:3000"] {
        let res = hub
            .send(
                req(Method::PATCH, "/api/v1/user-state")
                    .header("origin", origin)
                    .json(json!({ "settings": { "theme": "pwned" } }))
                    .build(),
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{origin}");
        assert_eq!(res.json()["error"], "cross_origin_forbidden");
    }
    // The state was not touched.
    let res = hub
        .send(req(Method::GET, "/api/v1/user-state").build())
        .await;
    assert_eq!(res.json()["data"]["revision"], 0);
}

#[tokio::test]
async fn foreign_origin_reads_get_no_acao_and_no_preflight() {
    let hub = Hub::new("cors-foreign-read");
    let res = hub
        .send(
            req(Method::GET, "/api/v1/health")
                .header("origin", "https://evil.example")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("access-control-allow-origin").is_none());

    let res = hub
        .send(
            req(Method::OPTIONS, "/api/v1/user-state")
                .header("origin", "https://evil.example")
                .header("access-control-request-method", "PATCH")
                .build(),
        )
        .await;
    assert!(res.header("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn foreign_page_on_the_hub_machine_is_not_local_admin() {
    let hub = Hub::new("cors-not-local");
    let res = hub
        .send(
            req(Method::GET, "/api/v1/system/machine-access")
                .header("origin", "https://evil.example")
                .build(),
        )
        .await;
    assert_eq!(res.json()["data"]["local"], false);

    // `<img src>` style request: no Origin, but fetch metadata says cross-site.
    let res = hub
        .send(
            req(Method::GET, "/api/v1/backup/kord-data")
                .header("sec-fetch-site", "cross-site")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    let res = hub
        .send(
            req(Method::GET, "/api/v1/system/machine-access")
                .header("origin", "tauri://localhost")
                .build(),
        )
        .await;
    assert_eq!(res.json()["data"]["local"], true);
}

#[tokio::test]
async fn security_headers_are_set() {
    let hub = Hub::new("sec-headers");
    let res = hub.send(req(Method::GET, "/api/v1/health").build()).await;
    assert_eq!(
        res.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(res.header("x-frame-options").as_deref(), Some("DENY"));
    assert_eq!(
        res.header("referrer-policy").as_deref(),
        Some("no-referrer")
    );
    assert!(res.header("x-request-id").is_some());
    let res = hub.send(req(Method::GET, "/api/v1/nope").build()).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(
        res.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
}
