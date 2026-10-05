//! `/media` and `/api/v1/transcode`: path traversal stays blocked (raw,
//! encoded, double-encoded, reserved folders, hub data dir) and byte ranges
//! behave (first bytes, suffix, 416, HEAD, validators).

#[path = "http_support/mod.rs"]
mod support;

use axum::http::{Method, StatusCode};
use support::{req, Hub};

fn body_of(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
async fn traversal_variants_are_refused() {
    let hub = Hub::new("media-trav");
    hub.file("Artist/Album/01.mp3", b"audio");
    std::fs::write(hub.base.join("secret.txt"), b"top secret").unwrap();
    hub.file(".kord/rekord.db", b"legacy db");
    hub.file("kord/x.json", b"legacy");

    for uri in [
        "/media/..%2Fsecret.txt",
        "/media/..%2F..%2F..%2Fetc%2Fhostname",
        "/media/%2e%2e/secret.txt",
        "/media/%2E%2E%2Fsecret.txt",
        "/media/%252e%252e%252fsecret.txt",
        "/media/Artist/..%2F..%2Fsecret.txt",
        "/media/..%5Csecret.txt",
        "/api/v1/media/..%2Fsecret.txt",
        "/media/%2Fetc%2Fhostname",
        "/media/.kord/rekord.db",
        "/media/.KORD/rekord.db",
        "/media/kord/x.json",
    ] {
        let res = hub.send(req(Method::GET, uri).build()).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            !String::from_utf8_lossy(&res.body).contains("secret"),
            "{uri} leaked the file"
        );
    }
    let ok = hub
        .send(req(Method::GET, "/media/Artist/Album/01.mp3").build())
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body, b"audio");
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_out_of_the_library_is_refused() {
    let hub = Hub::new("media-link");
    std::fs::write(hub.base.join("outside.mp3"), b"outside").unwrap();
    std::fs::create_dir_all(hub.root.join("Artist")).unwrap();
    std::os::unix::fs::symlink(
        hub.base.join("outside.mp3"),
        hub.root.join("Artist/link.mp3"),
    )
    .unwrap();
    let res = hub
        .send(req(Method::GET, "/media/Artist/link.mp3").build())
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hub_data_dir_inside_library_is_not_served() {
    let hub = Hub::with_data_inside_root("media-datadir");
    std::fs::write(
        hub.data.join("youtube-cookies.txt"),
        b"# Netscape HTTP Cookie File",
    )
    .unwrap();
    let res = hub
        .send(req(Method::GET, "/media/hubdata/youtube-cookies.txt").build())
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = hub
        .send(req(Method::GET, "/media/hubdata/settings.json").build())
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ranges_first_bytes_suffix_and_open_ended() {
    let hub = Hub::new("media-range");
    let data = body_of(1000);
    hub.file("A/B/t.flac", &data);

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.flac")
                .header("range", "bytes=0-99")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.header("content-range").as_deref(),
        Some("bytes 0-99/1000")
    );
    assert_eq!(res.header("content-length").as_deref(), Some("100"));
    assert_eq!(res.header("accept-ranges").as_deref(), Some("bytes"));
    assert_eq!(res.body, data[..100]);

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.flac")
                .header("range", "bytes=-10")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.header("content-range").as_deref(),
        Some("bytes 990-999/1000")
    );
    assert_eq!(res.body, data[990..]);

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.flac")
                .header("range", "bytes=900-")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.body, data[900..]);

    let res = hub
        .send(req(Method::GET, "/media/A/B/t.flac").build())
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, data);
    assert!(res.header("etag").is_some());
    assert!(res.header("last-modified").is_some());
}

#[tokio::test]
async fn unsatisfiable_range_reports_length() {
    let hub = Hub::new("media-416");
    hub.file("A/B/t.mp3", &body_of(500));
    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.mp3")
                .header("range", "bytes=500-")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(res.header("content-range").as_deref(), Some("bytes */500"));
}

#[tokio::test]
async fn multi_range_falls_back_to_full_body() {
    let hub = Hub::new("media-multi");
    let data = body_of(300);
    hub.file("A/B/t.mp3", &data);
    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.mp3")
                .header("range", "bytes=0-1,10-20")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, data);
}

#[tokio::test]
async fn head_reports_length_without_body() {
    let hub = Hub::new("media-head");
    hub.file("A/B/t.mp3", &body_of(1234));
    let res = hub
        .send(req(Method::HEAD, "/media/A/B/t.mp3").build())
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header("content-length").as_deref(), Some("1234"));
    assert!(res.body.is_empty());

    let res = hub
        .send(
            req(Method::HEAD, "/media/A/B/t.mp3")
                .header("range", "bytes=0-9")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.header("content-length").as_deref(), Some("10"));
    assert!(res.body.is_empty());
}

#[tokio::test]
async fn validators_give_304_and_if_range_mismatch_serves_full() {
    let hub = Hub::new("media-etag");
    let data = body_of(200);
    hub.file("A/B/t.mp3", &data);
    let first = hub.send(req(Method::GET, "/media/A/B/t.mp3").build()).await;
    let etag = first.header("etag").unwrap();

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.mp3")
                .header("if-none-match", &etag)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_MODIFIED);
    assert!(res.body.is_empty());

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.mp3")
                .header("range", "bytes=0-9")
                .header("if-range", "\"stale\"")
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, data);

    let res = hub
        .send(
            req(Method::GET, "/media/A/B/t.mp3")
                .header("range", "bytes=0-9")
                .header("if-range", &etag)
                .build(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn transcode_refuses_traversal() {
    let hub = Hub::new("transcode-trav");
    std::fs::write(hub.base.join("secret.flac"), b"x").unwrap();
    for uri in [
        "/api/v1/transcode/..%2Fsecret.flac",
        "/api/v1/transcode/%2e%2e%2Fsecret.flac",
        "/api/v1/transcode/.kord/x.flac?format=aac",
    ] {
        let res = hub.send(req(Method::GET, uri).build()).await;
        // 404 with ffmpeg installed, 503 `ffmpeg_unavailable` without: never 200.
        assert!(
            res.status == StatusCode::NOT_FOUND || res.status == StatusCode::SERVICE_UNAVAILABLE,
            "{uri}: {}",
            res.status
        );
    }
    let res = hub
        .send(req(Method::GET, "/api/v1/transcode/A/x.flac?format=wav").build())
        .await;
    assert!(res.status == StatusCode::BAD_REQUEST || res.status == StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn health_advertises_contract_fields() {
    let hub = Hub::new("health");
    let res = hub.send(req(Method::GET, "/api/v1/health").build()).await;
    assert_eq!(res.status, StatusCode::OK);
    let j = res.json();
    assert_eq!(j["apiVersion"], 1);
    assert_eq!(j["minClientVersion"], "5.0.0");
    assert!(j["transcode"].is_boolean());
    assert_eq!(j["ok"], true);
    assert!(j["version"].is_string());
}

/// Real transcode when ffmpeg is installed (skipped otherwise).
#[tokio::test]
async fn transcode_streams_mp3_when_ffmpeg_exists() {
    let Some(ffmpeg) = rekord_core::transcode::ffmpeg_path() else {
        return;
    };
    let hub = Hub::new("transcode-ok");
    let wav = hub.root.join("A/B/tone.wav");
    std::fs::create_dir_all(wav.parent().unwrap()).unwrap();
    let made = std::process::Command::new(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:duration=1")
        .arg(&wav)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !made {
        return;
    }
    let res = hub
        .send(req(Method::GET, "/api/v1/transcode/A/B/tone.wav").build())
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header("content-type").as_deref(), Some("audio/mpeg"));
    assert!(res.body.len() > 1000, "mp3 bytes streamed");

    let res = hub
        .send(req(Method::GET, "/api/v1/transcode/A/B/tone.wav?format=aac").build())
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header("content-type").as_deref(), Some("audio/aac"));
    assert!(!res.body.is_empty());
}

/// Non-audio files that ship with albums (`.html`, `.svg`) must not run as
/// same-origin documents on the hub (they would count as a local caller).
#[tokio::test]
async fn library_documents_are_served_sandboxed() {
    let hub = Hub::new("media-csp");
    hub.file(
        "A/B/readme.html",
        b"<script>fetch('/api/v1/library/scan',{method:'POST'})</script>",
    );
    hub.file("A/B/01.mp3", b"0123456789");
    for path in ["/media/A/B/readme.html", "/api/v1/media/A/B/01.mp3"] {
        let res = hub.send(req(Method::GET, path).build()).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        let csp = res.header("content-security-policy").unwrap_or_default();
        assert!(csp.contains("sandbox"), "{path}: {csp:?}");
    }
}

/// Silent MPEG-1 Layer III 44.1 kHz frame at bitrate index `ix`.
fn mp3_frame(ix: usize) -> Vec<u8> {
    const KBPS: [usize; 14] = [
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    let mut f = vec![0u8; 144 * KBPS[ix - 1] * 1000 / 44100];
    f[..4].copy_from_slice(&[0xFF, 0xFB, (ix as u8) << 4, 0x00]);
    f
}

#[tokio::test]
async fn vbr_mp3_without_xing_is_served_with_a_spliced_xing_frame() {
    let hub = Hub::new("media-xing");
    // ID3v2 tag (empty), then 3000 VBR frames without a Xing header.
    let mut orig = b"ID3\x03\x00\x00\x00\x00\x00\x00".to_vec();
    let at = orig.len();
    orig.extend(mp3_frame(14));
    for i in 1..3000 {
        orig.extend(mp3_frame([1, 2, 3, 5, 9, 1, 1, 4][i % 8]));
    }
    let path = hub.file("Sets/DJ/set.mp3", &orig);
    rekord_core::scan::scan_library(&hub.state.db, &hub.root).unwrap();
    let track = hub
        .state
        .db
        .track_by_rel("Sets/DJ/set.mp3")
        .unwrap()
        .unwrap();
    assert_eq!(track.duration_ms, 78_367);

    let full = hub
        .send(req(Method::GET, "/media/Sets/DJ/set.mp3").build())
        .await;
    assert_eq!(full.status, StatusCode::OK);
    let n = full.body.len() - orig.len();
    assert!(n > 0, "no frame spliced in");
    assert_eq!(
        full.header("content-length"),
        Some(full.body.len().to_string())
    );
    assert!(full.header("etag").unwrap().contains("-x"));
    // The file around the inserted frame is untouched.
    assert_eq!(&full.body[..at], &orig[..at]);
    assert_eq!(&full.body[at + n..], &orig[at..]);
    let xing = &full.body[at..at + n];
    assert_eq!(&xing[36..40], b"Xing");
    assert_eq!(u32::from_be_bytes(xing[44..48].try_into().unwrap()), 3000);
    assert_eq!(
        u32::from_be_bytes(xing[48..52].try_into().unwrap()) as usize,
        full.body.len() - at
    );
    // Any player reading the stream now gets the exact length.
    let served = hub.base.join("served.mp3");
    std::fs::write(&served, &full.body).unwrap();
    let probed = lofty::read_from_path(&served).unwrap();
    use lofty::file::AudioFile;
    assert_eq!(probed.properties().duration().as_millis(), 78_367);

    // Ranges are cut from the spliced stream, also across the insertion.
    let (a, b) = (at - 2, at + n + 5);
    let part = hub
        .send(
            req(Method::GET, "/media/Sets/DJ/set.mp3")
                .header("range", &format!("bytes={a}-{b}"))
                .build(),
        )
        .await;
    assert_eq!(part.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(part.body, &full.body[a..=b]);
    assert_eq!(
        part.header("content-range"),
        Some(format!("bytes {a}-{b}/{}", full.body.len()))
    );

    // Changed on disk and not rescanned yet: served as is.
    let mut changed = orig.clone();
    changed.extend(mp3_frame(9));
    std::fs::write(&path, &changed).unwrap();
    let res = hub
        .send(req(Method::GET, "/media/Sets/DJ/set.mp3").build())
        .await;
    assert_eq!(res.body, changed);
}
