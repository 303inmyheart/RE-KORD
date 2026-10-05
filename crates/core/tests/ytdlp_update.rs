//! "Update yt-dlp" against a fake GitHub release: checksum verified, atomic
//! install into `<data_dir>/tools`, resolution prefers the new copy; a bad
//! checksum installs nothing.

#![cfg(unix)]

use axum::routing::get;
use axum::Router;
use rekord_core::tools::{self, Tool, ToolContext};
use rekord_core::ytdlp_update::{platform_asset_name, update_ytdlp, UpdateError};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn update_installs_verified_release_and_rejects_bad_checksum() {
    let Some(asset) = platform_asset_name() else {
        return;
    };
    let script = b"#!/bin/sh\necho 2099.12.31\n".to_vec();
    let sums = Arc::new(Mutex::new(format!("{}  {asset}\n", hex(&script))));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let release = serde_json::json!({
        "tag_name": "2099.12.31",
        "assets": [
            { "name": asset, "browser_download_url": format!("{base}/dl/{asset}") },
            { "name": "SHA2-256SUMS", "browser_download_url": format!("{base}/dl/SHA2-256SUMS") },
        ],
    });
    let sums_route = sums.clone();
    let body = script.clone();
    let app = Router::new()
        .route(
            "/latest",
            get(move || {
                let r = release.clone();
                async move { axum::Json(r) }
            }),
        )
        .route(
            "/dl/SHA2-256SUMS",
            get(move || {
                let s = sums_route.lock().unwrap().clone();
                async move { s }
            }),
        )
        .route(
            &format!("/dl/{asset}"),
            get(move || {
                let b = body.clone();
                async move { b }
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    std::env::set_var("REKORD_YTDLP_RELEASE_API", format!("{base}/latest"));

    let data = std::env::temp_dir().join(format!("rekord-upd-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data).unwrap();
    let ctx = ToolContext {
        config_path: None,
        data_dir: Some(data.clone()),
    };

    // Bad checksum first: refused, nothing installed, no temp file left.
    *sums.lock().unwrap() = format!("{}  {asset}\n", "0".repeat(64));
    let e = update_ytdlp(&ctx, true).await.unwrap_err();
    assert!(matches!(e, UpdateError::ChecksumMismatch(_)), "{e}");
    assert_eq!(e.code(), "ytdlp_checksum_mismatch");
    let installed = tools::app_data_ytdlp_path(&data);
    assert!(!installed.exists());
    let leftovers: Vec<_> = std::fs::read_dir(tools::tools_dir(&data))
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Good checksum: installed, executable, chosen.
    *sums.lock().unwrap() = format!("{}  {asset}\n", hex(&script));
    let report = update_ytdlp(&ctx, false).await.unwrap();
    assert!(report.updated);
    assert_eq!(report.latest_version, "2099.12.31");
    assert_eq!(report.version.as_deref(), Some("2099.12.31"));
    assert_eq!(report.source, Some("app_data"));
    assert!(installed.is_file());
    let resolved = tools::resolve(Tool::Ytdlp, &ctx).await;
    assert_eq!(resolved.path, installed);

    // Already current: no download.
    let again = update_ytdlp(&ctx, false).await.unwrap();
    assert!(!again.updated);
    assert!(again.up_to_date);
    let _ = std::fs::remove_dir_all(&data);
}
