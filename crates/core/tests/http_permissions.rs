//! Permission matrix (studio QA item 10, legacy `requestAccess.mjs:61-99`):
//!
//! | caller                         | library / Studio write | machine op |
//! |--------------------------------|------------------------|------------|
//! | local, Default                 | yes                    | yes        |
//! | local, other account           | yes                    | `forbidden_default_account` |
//! | LAN / tunnel                   | `forbidden_remote`     | `forbidden_remote` |
//! | LAN + allow_remote_admin, other| yes                    | `forbidden_default_account` |
//! | LAN + allow_remote_admin, Default | yes                 | yes        |

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{req, From, Hub, Req};

async fn guest(hub: &Hub) -> String {
    let res = hub
        .send(
            req(Method::POST, "/api/v1/accounts")
                .json(json!({ "name": "Ospite" }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    res.json()["data"]["createdAccountId"]
        .as_str()
        .unwrap()
        .to_string()
}

fn as_account(r: Req, account: Option<&str>) -> Req {
    match account {
        Some(id) => r.header("x-rekord-account-id", id),
        None => r,
    }
}

/// Library write: create a folder.
fn library_write(name: &str) -> Req {
    req(Method::POST, "/api/v1/fs/mkdir").json(json!({ "name": name }))
}

/// Machine operation: clear the YouTube cookies.
fn machine_op() -> Req {
    req(Method::DELETE, "/api/v1/config/youtube-cookies")
}

async fn check(
    hub: &Hub,
    from: From,
    account: Option<&str>,
    folder: &str,
    library: Result<(), &str>,
    machine: Result<(), &str>,
) {
    let res = hub
        .send(
            as_account(library_write(folder), account)
                .from(from)
                .build(),
        )
        .await;
    match library {
        Ok(()) => assert_eq!(
            res.status,
            StatusCode::OK,
            "library {folder}: {:?}",
            res.json()
        ),
        Err(code) => {
            assert_eq!(res.status, StatusCode::FORBIDDEN, "library {folder}");
            assert_eq!(res.json()["error"], code, "library {folder}");
        }
    }
    let res = hub
        .send(as_account(machine_op(), account).from(from).build())
        .await;
    match machine {
        Ok(()) => assert_ne!(res.status, StatusCode::FORBIDDEN, "machine {folder}"),
        Err(code) => {
            assert_eq!(res.status, StatusCode::FORBIDDEN, "machine {folder}");
            assert_eq!(res.json()["error"], code, "machine {folder}");
        }
    }
}

#[tokio::test]
async fn permission_matrix() {
    let hub = Hub::new("perm-matrix");
    let guest = guest(&hub).await;
    let g = Some(guest.as_str());

    check(&hub, From::Local, None, "LocalDefault", Ok(()), Ok(())).await;
    check(
        &hub,
        From::Local,
        g,
        "LocalGuest",
        Ok(()),
        Err("forbidden_default_account"),
    )
    .await;
    for (from, tag) in [(From::Lan, "Lan"), (From::Tunnel, "Tunnel")] {
        for acc in [None, g] {
            check(
                &hub,
                from,
                acc,
                &format!("{tag}Off"),
                Err("forbidden_remote"),
                Err("forbidden_remote"),
            )
            .await;
        }
    }
    hub.set_allow_remote_admin(true);
    check(
        &hub,
        From::Lan,
        g,
        "LanGuest",
        Ok(()),
        Err("forbidden_default_account"),
    )
    .await;
    check(&hub, From::Lan, None, "LanDefault", Ok(()), Ok(())).await;

    assert!(hub.root.join("LocalGuest").is_dir());
    assert!(!hub.root.join("LanOff").exists());
}

#[tokio::test]
async fn machine_access_reports_library_and_machine_rights() {
    let hub = Hub::new("perm-status");
    let guest = guest(&hub).await;
    let get = |from: From, acc: Option<&str>| {
        as_account(req(Method::GET, "/api/v1/system/machine-access"), acc)
            .from(from)
            .build()
    };
    let v = hub.send(get(From::Local, None)).await.json();
    assert_eq!(v["data"]["canManageLibrary"], true);
    assert_eq!(v["data"]["canManageMachine"], true);
    assert!(v["data"]["machineDeniedReason"].is_null());

    let v = hub.send(get(From::Local, Some(&guest))).await.json();
    assert_eq!(v["data"]["canManageLibrary"], true);
    assert_eq!(v["data"]["canManageMachine"], false);
    assert_eq!(
        v["data"]["machineDeniedReason"],
        "forbidden_default_account"
    );

    let v = hub.send(get(From::Lan, None)).await.json();
    assert_eq!(v["data"]["canManageLibrary"], false);
    assert_eq!(v["data"]["canManageMachine"], false);
    assert_eq!(v["data"]["libraryDeniedReason"], "forbidden_remote");
}

#[tokio::test]
async fn studio_writes_from_a_local_guest_reach_the_handler() {
    let hub = Hub::new("perm-studio");
    let guest = guest(&hub).await;
    hub.file("A/B/01.mp3", b"x");
    // Validation error, not 403: the guard let the guest through.
    let res = hub
        .send(
            req(Method::POST, "/api/v1/download")
                .header("x-rekord-account-id", &guest)
                .json(json!({
                    "url": "https://example.com/x",
                    "downloadId": "6f1c2a8e-3b7d-4c1e-9a2b-1d2e3f4a5b6c",
                }))
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.json()["error"], "url_not_allowed");
    let res = hub
        .send(
            req(Method::POST, "/api/v1/track-info/save")
                .header("x-rekord-account-id", &guest)
                .json(json!({ "relPath": "A/B/01.mp3", "patch": { "title": "x" } }))
                .build(),
        )
        .await;
    assert_ne!(res.status, StatusCode::FORBIDDEN, "{:?}", res.json());
    // The yt-dlp update stays a machine operation.
    let res = hub
        .send(
            req(Method::POST, "/api/v1/tools/ytdlp/update")
                .header("x-rekord-account-id", &guest)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.json()["error"], "forbidden_default_account");
}
