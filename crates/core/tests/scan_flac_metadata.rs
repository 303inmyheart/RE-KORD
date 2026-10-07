//! FLAC scanner regression tests: merge Vorbis metadata and prefer embedded artwork.

use rekord_core::db::Db;
use rekord_core::scan::scan_library;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct TempLibrary {
    root: PathBuf,
}

impl TempLibrary {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rekord-flac-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for TempLibrary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn make_png(path: &Path) {
    let img = image::RgbImage::from_pixel(2, 2, image::Rgb([240, 20, 20]));
    img.save(path).unwrap();
}

#[test]
fn flac_reads_tags_and_prefers_embedded_front_cover() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found: skipping");
        return;
    }

    let lib = TempLibrary::new();
    let album_dir = lib.root.join("Folder Artist/Folder Album");
    fs::create_dir_all(&album_dir).unwrap();

    // A loose folder cover exists, but the embedded cover must win.
    fs::write(album_dir.join("cover.jpg"), b"folder-cover").unwrap();
    let embedded = lib.root.join("embedded.png");
    make_png(&embedded);

    let flac = album_dir.join("01.flac");
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-loop",
            "1",
            "-i",
        ])
        .arg(&embedded)
        .args([
            "-t",
            "0.2",
            "-map",
            "0:a",
            "-map",
            "1:v",
            "-c:a",
            "flac",
            "-c:v",
            "png",
            "-disposition:v:0",
            "attached_pic",
            "-metadata:s:v:0",
            "comment=Cover (front)",
            "-metadata",
            "title=Flac Song",
            "-metadata",
            "artist=Tag Artist",
            "-metadata",
            "album=Tag Album",
            "-metadata",
            "genre=Electronic",
            "-metadata",
            "date=2024-05-06",
            "-metadata",
            "track=7/12",
            "-metadata",
            "disc=2/2",
            "-metadata",
            "BPM=123.5",
        ])
        .arg(&flac)
        .status()
        .unwrap();
    assert!(status.success());

    let db = Db::open(lib.root.join("test.db")).unwrap();
    scan_library(&db, &lib.root).unwrap();

    let track = db
        .track_by_rel("Folder Artist/Folder Album/01.flac")
        .unwrap()
        .unwrap();
    assert_eq!(track.title, "Flac Song");
    assert_eq!(track.artist_name, "Tag Artist");
    assert_eq!(track.album_name, "Tag Album");
    assert_eq!(track.genre.as_deref(), Some("Electronic"));
    assert_eq!(track.release_date.as_deref(), Some("2024-05-06"));
    assert_eq!(track.track_number, Some(7));
    assert_eq!(track.bpm, Some(123.5));

    let (album_id, disc_number): (i64, Option<i64>) = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT album_id, disc_number FROM tracks WHERE rel_path = ?1",
                ["Folder Artist/Folder Album/01.flac"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(disc_number, Some(2));

    let cover = db.album_cover_path(album_id).unwrap().unwrap();
    assert!(
        cover
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".rekord-embedded-cover.")),
        "embedded cover should win, got {}",
        cover.display()
    );
    assert_ne!(cover, album_dir.join("cover.jpg"));
}
