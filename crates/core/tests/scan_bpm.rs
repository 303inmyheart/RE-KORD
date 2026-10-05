//! Track tempo (`bpm`) is read from the tags during scans and exposed on Track.

use rekord_core::db::Db;
use rekord_core::scan::{parse_bpm, scan_library};
use std::fs;
use std::path::{Path, PathBuf};

struct TempLibrary {
    root: PathBuf,
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Minimal MP3: an ID3v2.3 tag with the given text frames, then a few silent
/// MPEG-1 Layer III frames (128 kbps, 44.1 kHz) so the file probes as audio.
fn mp3_with_frames(frames: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, text) in frames {
        let mut payload = vec![0u8]; // ISO-8859-1
        payload.extend_from_slice(text.as_bytes());
        body.extend_from_slice(id.as_bytes());
        body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&payload);
    }
    let size = body.len() as u32;
    let syncsafe = [
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ];
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend_from_slice(&syncsafe);
    out.extend_from_slice(&body);
    for _ in 0..8 {
        out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        out.extend_from_slice(&[0u8; 417 - 4]);
    }
    out
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

#[test]
fn bpm_is_read_from_id3_and_exposed_as_null_when_missing() {
    let lib = TempLibrary {
        root: std::env::temp_dir().join(format!("rekord-bpm-{}", uuid::Uuid::new_v4())),
    };
    write(
        &lib.root,
        "Artist/Album/01.mp3",
        &mp3_with_frames(&[("TIT2", "Fast One"), ("TBPM", "128")]),
    );
    write(
        &lib.root,
        "Artist/Album/02.mp3",
        &mp3_with_frames(&[("TIT2", "No Tempo")]),
    );
    let db = Db::open(lib.root.join("t.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();

    let fast = db.track_by_rel("Artist/Album/01.mp3").unwrap().unwrap();
    assert_eq!(fast.title, "Fast One");
    assert_eq!(fast.bpm, Some(128.0));
    let json = serde_json::to_value(&fast).unwrap();
    assert_eq!(json["bpm"], serde_json::json!(128.0));

    let slow = db.track_by_rel("Artist/Album/02.mp3").unwrap().unwrap();
    assert_eq!(slow.bpm, None);
    assert!(serde_json::to_value(&slow).unwrap()["bpm"].is_null());

    // A re-tagged file is re-read by the next incremental scan.
    write(
        &lib.root,
        "Artist/Album/02.mp3",
        &mp3_with_frames(&[("TIT2", "No Tempo"), ("TBPM", "96")]),
    );
    scan_library(&db, &lib.root).unwrap();
    let slow = db.track_by_rel("Artist/Album/02.mp3").unwrap().unwrap();
    assert_eq!(slow.bpm, Some(96.0));
}

#[test]
fn bpm_strings_are_parsed_leniently() {
    assert_eq!(parse_bpm("128"), Some(128.0));
    assert_eq!(parse_bpm(" 127,5 "), Some(127.5));
    assert_eq!(parse_bpm("120 BPM"), Some(120.0));
    assert_eq!(parse_bpm("0"), None);
    assert_eq!(parse_bpm("fast"), None);
    assert_eq!(parse_bpm("12000"), None);
}
