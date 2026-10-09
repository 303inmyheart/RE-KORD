//! The hub's store of covers taken from the files themselves:
//! `<data dir>/covers/embedded/<album key>.jpg`.
//!
//! The scan never writes into the music folders (they may be read-only, on a
//! NAS, or watched: a new `cover.jpg` would trigger another rescan). Only a
//! person choosing a cover in Studio writes `cover.jpg` there. An album row
//! points at a stored file through `cover_path` like at a folder image, so
//! thumbnails, `cover_version` and cache busting work unchanged.

use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Longest edge of a stored cover; bigger pictures are scaled down.
pub const MAX_COVER_EDGE: u32 = 1500;
/// Pictures above this are not even decoded.
pub const MAX_PICTURE_BYTES: usize = 32 * 1024 * 1024;
/// Largest picture edge decoded (scaled to [`MAX_COVER_EDGE`] afterwards).
const MAX_DECODE_EDGE: u32 = 8192;
/// Memory one decode may take (a small NAS scans too).
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
/// Stored covers younger than this are never pruned: a scan or the backfill
/// may have saved one and not recorded it on its album yet.
const PRUNE_GRACE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

pub fn store_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("covers").join("embedded")
}

/// FNV-1a: stable across builds (std's hasher is not), so stored files keep
/// their names after an upgrade.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Where the embedded cover of the album `folder_key` is kept.
pub fn stored_path(store: &Path, folder_key: &str) -> PathBuf {
    store.join(format!("{:016x}.jpg", fnv1a(folder_key)))
}

/// The bytes to store for a picture: a JPEG that fits stays as it is, the
/// rest (PNG, WebP, big JPEGs) is decoded, scaled to [`MAX_COVER_EDGE`] and
/// encoded as JPEG. Anything that does not decode is refused.
pub fn normalize_picture(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 32 {
        bail!("picture too small ({} bytes)", data.len());
    }
    if data.len() > MAX_PICTURE_BYTES {
        bail!("picture too large ({} bytes)", data.len());
    }
    let format = image::guess_format(data).context("unknown picture format")?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP
    ) {
        bail!("unsupported picture format {format:?}");
    }
    // A small file can declare a huge canvas: bounded before decoding.
    let mut reader = image::ImageReader::with_format(Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_EDGE);
    limits.max_image_height = Some(MAX_DECODE_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let img = reader.decode().context("decode picture")?;
    if img.width() == 0 || img.height() == 0 {
        bail!("empty picture");
    }
    if format == image::ImageFormat::Jpeg && img.width().max(img.height()) <= MAX_COVER_EDGE {
        return Ok(data.to_vec());
    }
    let img = if img.width().max(img.height()) > MAX_COVER_EDGE {
        img.resize(MAX_COVER_EDGE, MAX_COVER_EDGE, FilterType::CatmullRom)
    } else {
        img
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut out), 90)
        .encode_image(&img.to_rgb8())
        .context("encode cover")?;
    Ok(out)
}

/// Store `data` as the embedded cover of `folder_key`. An identical file is
/// left untouched, so its `cover_version` (and the clients' caches) stay valid.
pub fn save(store: &Path, folder_key: &str, data: &[u8]) -> Result<PathBuf> {
    let bytes = normalize_picture(data)?;
    let dest = stored_path(store, folder_key);
    if std::fs::read(&dest).is_ok_and(|cur| cur == bytes) {
        return Ok(dest);
    }
    std::fs::create_dir_all(store).with_context(|| format!("create {}", store.display()))?;
    // Unique temp name: a scan and the backfill may save the same album.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dest.with_extension(format!("{}.{seq}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &dest).with_context(|| format!("rename into {}", dest.display()))?;
    Ok(dest)
}

/// Delete stored covers no album points at any more. Returns how many.
pub fn prune(store: &Path, referenced: &HashSet<PathBuf>) -> usize {
    let Ok(entries) = std::fs::read_dir(store) else {
        return 0;
    };
    let mut removed = 0;
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_ours = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "jpg" || e == "tmp");
        // In-flight or just saved (not yet on its album row): kept for now.
        let recent = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| now.duration_since(t).unwrap_or_default() < PRUNE_GRACE);
        if is_ours && !recent && !referenced.contains(&path) && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn big_pictures_are_scaled_and_become_jpeg() {
        let out = normalize_picture(&png(3000, 1500)).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Jpeg);
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (1500, 750));
    }

    #[test]
    fn huge_declared_canvas_is_refused() {
        // PNG header declaring 20000×20000, no pixel data.
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xffff_ffffu32;
            for &b in data {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xedb8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend_from_slice(&20000u32.to_be_bytes());
        ihdr.extend_from_slice(&20000u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(&ihdr);
        data.extend_from_slice(&crc32(&ihdr).to_be_bytes());
        data.extend_from_slice(&[0u8; 64]);
        assert!(normalize_picture(&data).is_err());
    }

    #[test]
    fn prune_keeps_covers_saved_moments_ago() {
        let store = std::env::temp_dir().join(format!("rekord-prune-{}", uuid::Uuid::new_v4()));
        let saved = save(&store, "A/B", &png(64, 64)).unwrap();
        // Not referenced yet (the album row is written right after).
        assert_eq!(prune(&store, &HashSet::new()), 0);
        assert!(saved.is_file());
        // An old orphan goes.
        let old = store.join("0000000000000000.jpg");
        std::fs::write(&old, b"x").unwrap();
        let past = std::time::SystemTime::now() - PRUNE_GRACE - std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(past)
            .unwrap();
        assert_eq!(prune(&store, &HashSet::from([saved.clone()])), 1);
        assert!(!old.exists() && saved.is_file());
        let _ = std::fs::remove_dir_all(&store);
    }

    #[test]
    fn corrupt_pictures_are_refused() {
        assert!(normalize_picture(b"not a picture at all, just some text bytes").is_err());
        let mut broken = png(64, 64);
        broken.truncate(60);
        assert!(normalize_picture(&broken).is_err());
    }

    #[test]
    fn stored_names_are_stable_and_per_album() {
        let store = Path::new("/data/covers/embedded");
        assert_eq!(stored_path(store, "A/B"), stored_path(store, "A/B"));
        assert_ne!(stored_path(store, "A/B"), stored_path(store, "A/C"));
        assert_eq!(
            stored_path(store, "A/B").file_name().unwrap().len(),
            "0123456789abcdef.jpg".len()
        );
    }
}
