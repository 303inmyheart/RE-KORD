//! Studio downloads through the HTTP API with a fake yt-dlp (unix): per-item
//! summary in `done`, no absolute paths, `indexEpoch` after the folder
//! refresh, kill on client disconnect, background jobs, re-attach.

#![cfg(unix)]

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use futures::StreamExt;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use support::{req, Hub};
use tower::ServiceExt;

/// Fake yt-dlp (`--version` → far-future version so it wins resolution).
fn install_fake_ytdlp(hub: &Hub, body: &str) {
    let path = hub.base.join("fake-bin/yt-dlp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 2099.12.31; exit 0; fi\n{body}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    hub.state.config.lock().unwrap().ytdlp_path = Some(path);
}

fn ndjson(body: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(body)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn start(id: &str, background: bool) -> axum::http::Request<axum::body::Body> {
    req(Method::POST, "/api/v1/download")
        .json(json!({
            "url": "https://www.youtube.com/playlist?list=PLfake",
            "downloadId": id,
            "downloadKind": "download_playlist",
            "outputDir": "Out",
            "background": background,
        }))
        .build()
}

const TWO_ITEMS: &str = r#"
echo "[download] Downloading item 1 of 2"
printf '[rekord-item] aaaaaaaaaaa\t1\tSong A\n'
echo "[youtube] aaaaaaaaaaa: Downloading webpage"
echo "[info] aaaaaaaaaaa: Downloading 1 format(s): 140"
mkdir -p Out && printf 'x' > "Out/01 - Song A.m4a"
echo "[download] Destination: Out/01 - Song A.m4a"
echo "[download] 100% of 1.00KiB in 00:00:00"
echo "[download] Downloading item 2 of 2"
printf '[rekord-item] bbbbbbbbbbb\t2\tCanzone è 日本\n'
echo "ERROR: [youtube] bbbbbbbbbbb: Requested format is not available. Use --list-formats for a list of available formats" >&2
exit 1
"#;

#[tokio::test]
async fn done_event_summarises_items_without_absolute_paths() {
    let hub = Hub::new("dl-summary");
    install_fake_ytdlp(&hub, TWO_ITEMS);
    let res = hub
        .send(start("11111111-2222-4333-8444-555555555555", false))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let events = ndjson(&res.body);
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();
    assert_eq!(kinds.first(), Some(&"started"));
    assert!(kinds.contains(&"progress"));
    assert!(kinds.contains(&"item"));
    assert!(kinds.contains(&"indexing"));
    let done = events.last().unwrap();
    assert_eq!(done["type"], "done");
    assert_eq!(done["ok"], true);
    assert_eq!(done["partial"], true);
    assert_eq!(done["summary"]["downloaded"], 1);
    assert_eq!(done["summary"]["failed"], 1);
    assert_eq!(done["summary"]["total"], 2);
    let failed = &done["failedItems"][0];
    assert_eq!(failed["id"], "bbbbbbbbbbb");
    assert_eq!(failed["index"], 2);
    assert_eq!(failed["title"], "Canzone è 日本");
    assert_eq!(failed["code"], "no_audio_format");
    assert_eq!(done["downloadedItems"][0], "Out/01 - Song A.m4a");
    assert_eq!(done["formats"][0], "140");
    assert_eq!(done["rescanned"], true);
    assert!(done.get("musicRoot").is_none());
    let raw = String::from_utf8_lossy(&res.body);
    let root = hub.root.to_string_lossy();
    assert!(!raw.contains(root.as_ref()), "absolute music root leaked");
    assert!(
        !raw.contains(hub.base.to_string_lossy().as_ref()),
        "absolute path leaked"
    );
    assert!(
        done["indexEpoch"].as_u64().unwrap() > 0,
        "{}",
        done["indexEpoch"]
    );

    // The finished job stays listed for a returning client.
    let res = hub
        .send(req(Method::GET, "/api/v1/download/active").build())
        .await;
    let list = res.json()["data"]["downloads"].clone();
    assert_eq!(list[0]["status"], "done");
    assert_eq!(list[0]["canCancel"], false);
    assert_eq!(list[0]["done"]["summary"]["failed"], 1);
}

#[tokio::test]
async fn all_items_failing_is_not_ok_and_skips_the_rescan() {
    let hub = Hub::new("dl-fail");
    install_fake_ytdlp(
        &hub,
        r#"
for i in 1 2 3; do
  echo "[download] Downloading item $i of 3"
  printf '[rekord-item] vid0000000%s\t%s\tT%s\n' $i $i $i
  echo "ERROR: [youtube] vid0000000$i: Requested format is not available" >&2
done
exit 1
"#,
    );
    let res = hub
        .send(start("21111111-2222-4333-8444-555555555555", false))
        .await;
    let events = ndjson(&res.body);
    assert!(!events.iter().any(|e| e["type"] == "indexing"));
    let done = events.last().unwrap();
    assert_eq!(done["ok"], false);
    assert_eq!(done["error"], "no_audio_format");
    assert_eq!(done["summary"]["failed"], 3);
    assert_eq!(done["failedItems"].as_array().unwrap().len(), 3);
    assert_eq!(done["rescanned"], false);
}

async fn wait_status(hub: &Hub, id: &str, want: &str) -> Value {
    for _ in 0..100 {
        let res = hub
            .send(
                req(
                    Method::GET,
                    &format!("/api/v1/download/active?downloadId={id}"),
                )
                .build(),
            )
            .await;
        let job = res.json()["data"]["downloads"][0].clone();
        if job["status"] == want {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("download {id} never reached {want}");
}

fn pid_alive(pid_file: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(pid_file) else {
        return false;
    };
    Path::new(&format!("/proc/{}", pid.trim())).exists()
}

const SLOW: &str = r#"
echo $$ > "$REKORD_TEST_PID_FILE"
echo "[download] Downloading item 1 of 1"
exec sleep 30
"#;

#[tokio::test]
async fn client_disconnect_kills_foreground_download() {
    let hub = Hub::new("dl-disconnect");
    let pid_file = hub.base.join("pid-fg");
    install_fake_ytdlp(
        &hub,
        &SLOW.replace("$REKORD_TEST_PID_FILE", &pid_file.to_string_lossy()),
    );
    let id = "31111111-2222-4333-8444-555555555555";
    let res = hub.app.clone().oneshot(start(id, false)).await.unwrap();
    let mut body = res.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first).contains("started"));
    // Wait for the process, then go away.
    for _ in 0..50 {
        if pid_alive(&pid_file) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(pid_alive(&pid_file));
    drop(body);
    let job = wait_status(&hub, id, "cancelled").await;
    assert_eq!(job["cancelReason"], "client_disconnected");
    assert_eq!(job["done"]["error"], "client_disconnected");
    assert!(!pid_alive(&pid_file), "yt-dlp still running");
}

#[tokio::test]
async fn background_download_survives_disconnect_and_can_be_reattached() {
    let hub = Hub::new("dl-background");
    let pid_file = hub.base.join("pid-bg");
    install_fake_ytdlp(
        &hub,
        &SLOW.replace("$REKORD_TEST_PID_FILE", &pid_file.to_string_lossy()),
    );
    let id = "41111111-2222-4333-8444-555555555555";
    let res = hub.app.clone().oneshot(start(id, true)).await.unwrap();
    let mut body = res.into_body().into_data_stream();
    let _ = body.next().await;
    for _ in 0..50 {
        if pid_alive(&pid_file) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(body);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let job = wait_status(&hub, id, "running").await;
    assert_eq!(job["canCancel"], true);
    assert_eq!(job["background"], true);
    assert!(pid_alive(&pid_file));

    // Re-attach: snapshot first, then live events up to `done`.
    let res = hub
        .app
        .clone()
        .oneshot(
            req(
                Method::GET,
                &format!("/api/v1/download/active?downloadId={id}&stream=1"),
            )
            .build(),
        )
        .await
        .unwrap();
    let mut stream = res.into_body().into_data_stream();
    let first = stream.next().await.unwrap().unwrap();
    let snap: Value = serde_json::from_slice(first.split(|b| *b == b'\n').next().unwrap()).unwrap();
    assert_eq!(snap["type"], "snapshot");
    assert_eq!(snap["download"]["downloadId"], id);

    // Cancel explicitly: the attached stream ends with `done`.
    let res = hub
        .send(
            req(Method::POST, "/api/v1/download-cancel")
                .json(json!({ "downloadId": id }))
                .build(),
        )
        .await;
    assert_eq!(res.json()["data"]["found"], true);
    let mut rest = Vec::new();
    while let Some(Ok(chunk)) = stream.next().await {
        rest.extend_from_slice(&chunk);
    }
    let events = ndjson(&rest);
    let done = events.last().unwrap();
    assert_eq!(done["type"], "done");
    assert_eq!(done["cancelled"], true);
    assert_eq!(done["cancelReason"], "user");
    assert!(!pid_alive(&pid_file));
}

#[tokio::test]
async fn duplicate_download_id_is_refused_with_a_code() {
    let hub = Hub::new("dl-dup");
    install_fake_ytdlp(&hub, "exec sleep 5");
    let id = "51111111-2222-4333-8444-555555555555";
    let res = hub.app.clone().oneshot(start(id, true)).await.unwrap();
    let mut body = res.into_body().into_data_stream();
    let _ = body.next().await;
    let res = hub.send(start(id, true)).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.json()["error"], "download_id_active");
    let _ = hub
        .send(
            req(Method::POST, "/api/v1/download-cancel")
                .json(json!({ "downloadId": id }))
                .build(),
        )
        .await;
}
