//! MP3 durations are exact even for VBR files without a Xing/VBRI header
//! (long DJ-set rips), where lofty and `ffmpeg -i` only extrapolate the first
//! frame's bitrate.

use lofty::file::AudioFile;
use rekord_core::db::Db;
use rekord_core::scan::{
    mp3_count, mp3_counted_duration_ms, mp3_frame_walk_duration_ms, scan_library,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct TempLibrary {
    root: PathBuf,
}

impl TempLibrary {
    fn new(tag: &str) -> Self {
        Self {
            root: std::env::temp_dir().join(format!("rekord-{tag}-{}", uuid::Uuid::new_v4())),
        }
    }
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// ID3v2.3 tag with ISO-8859-1 text frames.
fn id3v2(frames: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, text) in frames {
        let mut payload = vec![0u8];
        payload.extend_from_slice(text.as_bytes());
        body.extend_from_slice(id.as_bytes());
        body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&payload);
    }
    let size = body.len() as u32;
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend_from_slice(&[
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ]);
    out.extend_from_slice(&body);
    out
}

/// MPEG-1 Layer III bitrates (kbps) by header index.
const BITRATES: [u32; 14] = [
    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];

/// One silent MPEG-1 Layer III, 44.1 kHz stereo frame at bitrate index `ix`.
fn frame(ix: usize) -> Vec<u8> {
    let len = (144 * BITRATES[ix - 1] * 1000 / 44100) as usize;
    let mut f = vec![0u8; len];
    f[..4].copy_from_slice(&[0xFF, 0xFB, (ix as u8) << 4, 0x00]);
    f
}

/// A Xing (`Info` when `cbr`) header frame announcing `frames` audio frames.
fn xing_frame(frames: u32, cbr: bool) -> Vec<u8> {
    let mut f = frame(9);
    f[36..40].copy_from_slice(if cbr { b"Info" } else { b"Xing" });
    f[40..44].copy_from_slice(&3u32.to_be_bytes()); // frames + bytes present
    f[44..48].copy_from_slice(&frames.to_be_bytes());
    f[48..52].copy_from_slice(&(frames * 417).to_be_bytes());
    f
}

const FRAMES: usize = 3000;
/// 3000 frames × 1152 samples / 44100 Hz.
const EXACT_MS: i64 = 78_367;

/// VBR audio: a loud 320 kbps first frame (what a bitrate estimate trusts),
/// then mostly low bitrates, like a set with a quiet stretch.
fn vbr_frames() -> Vec<u8> {
    let mut out = frame(14);
    for i in 1..FRAMES {
        out.extend_from_slice(&frame([1, 2, 3, 5, 9, 1, 1, 4][i % 8]));
    }
    out
}

fn tags() -> Vec<u8> {
    id3v2(&[
        ("TIT2", "Six Hour Set"),
        ("TPE1", "TMOCS"),
        ("TCON", "Techno"),
        ("TYER", "2023"),
    ])
}

fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}

fn lofty_ms(path: &Path) -> i64 {
    lofty::read_from_path(path)
        .unwrap()
        .properties()
        .duration()
        .as_millis() as i64
}

#[test]
fn vbr_without_xing_is_counted_exactly_and_keeps_its_tags() {
    let lib = TempLibrary::new("mp3-vbr");
    let mut bytes = tags();
    bytes.extend(vbr_frames());
    let path = write(&lib.root, "Sets/TMOCS/set.mp3", &bytes);

    // The premise: lofty extrapolates the first frame's bitrate.
    let estimate = lofty_ms(&path);
    assert!(
        (estimate - EXACT_MS).abs() > EXACT_MS / 2,
        "lofty estimate {estimate} unexpectedly close to {EXACT_MS}"
    );
    assert_eq!(mp3_counted_duration_ms(&path), Some(EXACT_MS));

    // The Xing frame it lacks, for `/media` to splice in after the ID3 tag.
    let header = mp3_count(&path).unwrap().seek_header.unwrap();
    assert_eq!(header.insert_at, tags().len() as u64);
    let f = &header.frame;
    assert_eq!(&f[..2], &[0xFF, 0xFB]);
    assert_eq!(&f[36..40], b"Xing");
    assert_eq!(
        u32::from_be_bytes(f[44..48].try_into().unwrap()),
        FRAMES as u32
    );
    let toc = &f[52..152];
    assert_eq!(
        toc[0],
        (f.len() as f64 * 256.0 / (bytes.len() - tags().len() + f.len()) as f64).round() as u8
    );
    assert!(
        toc.windows(2).all(|w| w[0] <= w[1]),
        "TOC must not go back: {toc:?}"
    );

    let db = Db::open(lib.root.join("t.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();
    let t = db.track_by_rel("Sets/TMOCS/set.mp3").unwrap().unwrap();
    assert_eq!(t.duration_ms, EXACT_MS);
    assert_eq!(t.title, "Six Hour Set");
    assert_eq!(t.genre.as_deref(), Some("Techno"));
    assert_eq!(t.release_date.as_deref(), Some("2023"));
    assert_eq!(db.stats(None).unwrap().tracks_without_meta, 0);
}

#[test]
fn junk_and_trailing_tags_do_not_break_the_count() {
    let lib = TempLibrary::new("mp3-junk");
    let frames = vbr_frames();
    let cut = frames.len() / 2;
    // Land on a frame boundary for the junk insertion.
    let mut at = 0;
    while at < cut {
        let ix = (frames[at + 2] >> 4) as usize;
        at += 144 * BITRATES[ix - 1] as usize * 1000 / 44100;
    }
    let mut bytes = tags();
    bytes.extend_from_slice(&frames[..at]);
    bytes.extend_from_slice(&[0xFF, 0xFB, 0x12, 0x34, 0xFF, 0xE0, 7, 7, 7]); // false syncs
    bytes.extend_from_slice(&frames[at..]);
    let mut id3v1 = b"TAG".to_vec();
    id3v1.resize(128, 0);
    bytes.extend_from_slice(&id3v1);
    let path = write(&lib.root, "A/B/junk.mp3", &bytes);
    assert_eq!(mp3_counted_duration_ms(&path), Some(EXACT_MS));
}

#[test]
fn a_xing_frame_count_is_trusted_and_not_counted_as_audio() {
    let lib = TempLibrary::new("mp3-xing");
    let mut bytes = tags();
    bytes.extend(xing_frame(FRAMES as u32, false));
    bytes.extend(vbr_frames());
    let path = write(&lib.root, "A/B/xing.mp3", &bytes);
    // lofty's header-based value is already exact: no full read.
    assert_eq!(mp3_counted_duration_ms(&path), None);
    assert_eq!(lofty_ms(&path), EXACT_MS);
    // The walk skips the header frame and agrees.
    assert_eq!(mp3_frame_walk_duration_ms(&path), Some(EXACT_MS));
}

#[test]
fn small_cbr_files_keep_the_cheap_estimate() {
    let lib = TempLibrary::new("mp3-cbr");
    let mut bytes = tags();
    for _ in 0..FRAMES {
        bytes.extend(frame(9));
    }
    let path = write(&lib.root, "A/B/cbr.mp3", &bytes);
    assert_eq!(mp3_counted_duration_ms(&path), None);
    assert_eq!(mp3_frame_walk_duration_ms(&path), Some(EXACT_MS));
    let estimate = lofty_ms(&path);
    // Within 0.5%: these frames skip the padding byte real encoders add.
    assert!(
        (estimate - EXACT_MS).abs() <= EXACT_MS / 200,
        "cbr estimate {estimate}"
    );

    let mut info = tags();
    info.extend(xing_frame(FRAMES as u32, true));
    for _ in 0..FRAMES {
        info.extend(frame(9));
    }
    let path = write(&lib.root, "A/B/info.mp3", &info);
    assert_eq!(mp3_counted_duration_ms(&path), None);
}

/// Real encoder output: the same LAME VBR stream with and without the Xing
/// header must time identically. Skipped when ffmpeg is not installed.
#[test]
fn lame_vbr_without_xing_matches_the_xing_frame_count() {
    let ffmpeg_ok = Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ffmpeg_ok {
        eprintln!("ffmpeg not found: skipping");
        return;
    }
    let lib = TempLibrary::new("mp3-lame");
    fs::create_dir_all(lib.root.join("A/B")).unwrap();
    let noxing = lib.root.join("A/B/noxing.mp3");
    let xing = lib.root.join("A/B/xing.mp3");
    let src = "anoisesrc=d=20:c=pink:a=0.3,aformat=channel_layouts=stereo,\
               volume='0.02+0.98*pow(sin(t*0.3),2)':eval=frame";
    let run = |args: &[&str], out: &Path| {
        let ok = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(args)
            .arg(out)
            .status()
            .unwrap()
            .success();
        assert!(ok, "ffmpeg failed for {}", out.display());
    };
    run(
        &[
            "-f",
            "lavfi",
            "-i",
            src,
            "-c:a",
            "libmp3lame",
            "-q:a",
            "5",
            "-write_xing",
            "0",
            "-metadata",
            "genre=Techno",
        ],
        &noxing,
    );
    run(
        &[
            "-i",
            noxing.to_str().unwrap(),
            "-c",
            "copy",
            "-write_xing",
            "1",
        ],
        &xing,
    );
    let exact = lofty_ms(&xing);
    assert!((exact - 20_000).abs() < 100, "xing duration {exact}");
    assert_eq!(mp3_counted_duration_ms(&noxing), Some(exact));
}

/// Libraries indexed by an older build kept the bitrate estimate: the v5
/// migration marks MP3s stale so the next incremental scan recounts them.
#[test]
fn mp3s_indexed_by_an_older_build_are_recounted_after_the_upgrade() {
    let lib = TempLibrary::new("mp3-upgrade");
    let mut bytes = tags();
    bytes.extend(vbr_frames());
    write(&lib.root, "Sets/TMOCS/set.mp3", &bytes);
    let db_path = lib.root.join("t.db");
    {
        let db = Db::open(&db_path).unwrap();
        scan_library(&db, &lib.root).unwrap();
    }
    {
        // What a v4 build left behind: the estimate, at schema version 4.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("UPDATE tracks SET duration_ms = 25000; PRAGMA user_version = 4;")
            .unwrap();
    }
    let db = Db::open(&db_path).unwrap();
    scan_library(&db, &lib.root).unwrap();
    let t = db.track_by_rel("Sets/TMOCS/set.mp3").unwrap().unwrap();
    assert_eq!(t.duration_ms, EXACT_MS);
}
