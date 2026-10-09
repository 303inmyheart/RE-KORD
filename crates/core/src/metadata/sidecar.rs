//! Library sidecar JSON files (`kord-albuminfo.json`, `kord-trackinfo.json`,
//! `kord-artistinfo.json`): read, atomic write, and one process-wide lock
//! per file so metadata fetches, manual saves and curiosità saves never
//! interleave their read-modify-write cycles (legacy `withMetaMutation`).

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const FILE_ALBUM: &str = "kord-albuminfo.json";
pub const FILE_ALBUM_WPP: &str = "wpp-albuminfo.json";
pub const FILE_TRACK: &str = "kord-trackinfo.json";
pub const FILE_TRACK_WPP: &str = "wpp-trackinfo.json";
pub const FILE_ARTIST_INFO: &str = "kord-artistinfo.json";
pub const FILE_ARTIST_IMAGE: &str = "kord-artistinfo.jpg";

/// RE-KORD's own sidecar files (and the legacy `wpp-*` ones): never read by
/// the scan, so the library watcher ignores them.
pub fn is_sidecar_name(name: &str) -> bool {
    [
        FILE_ALBUM,
        FILE_ALBUM_WPP,
        FILE_TRACK,
        FILE_TRACK_WPP,
        FILE_ARTIST_INFO,
        FILE_ARTIST_IMAGE,
    ]
    .contains(&name)
}

fn locks() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_key(path: &Path) -> PathBuf {
    // The file may not exist yet: canonicalize the parent.
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => dir
            .canonicalize()
            .map(|d| d.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// Run `f` while holding the lock of `path` (blocking; keep `f` short and
/// free of `.await`).
pub fn with_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    let entry = {
        let mut map = locks().lock().unwrap_or_else(|p| p.into_inner());
        map.entry(lock_key(path))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = entry.lock().unwrap_or_else(|p| p.into_inner());
    f()
}

/// Parse a JSON object file; `{}` when missing, unreadable or not an object.
pub fn read_object(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// Write bytes through a temp file in the same folder + rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("sidecar without parent")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("sidecar without name")?;
    let tmp = dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let res = (|| -> Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Pretty JSON, atomically.
pub fn write_json_atomic(path: &Path, v: &Value) -> Result<()> {
    write_atomic(path, serde_json::to_string_pretty(v)?.as_bytes())
}

/// Read-modify-write one sidecar under its lock. `read_from` lets a legacy
/// `wpp-*` file seed the new `kord-*` file. `f` returns whether to write.
pub fn mutate_object<T>(
    write_path: &Path,
    read_from: Option<&Path>,
    f: impl FnOnce(&mut serde_json::Map<String, Value>) -> Result<(bool, T)>,
) -> Result<T> {
    with_lock(write_path, || {
        let src = read_from.unwrap_or(write_path);
        let mut v = read_object(src);
        let obj = v.as_object_mut().expect("read_object returns an object");
        let (write, out) = f(obj)?;
        if write {
            write_json_atomic(write_path, &v)?;
        }
        Ok(out)
    })
}

/// Album sidecar to read: `kord-albuminfo.json`, else legacy `wpp-…`.
pub fn album_read_path(album_dir: &Path) -> PathBuf {
    let k = album_dir.join(FILE_ALBUM);
    let w = album_dir.join(FILE_ALBUM_WPP);
    if !k.is_file() && w.is_file() {
        w
    } else {
        k
    }
}

/// Track sidecar to read: `kord-trackinfo.json`, else legacy `wpp-…`.
pub fn track_read_path(album_dir: &Path) -> PathBuf {
    let k = album_dir.join(FILE_TRACK);
    let w = album_dir.join(FILE_TRACK_WPP);
    if !k.is_file() && w.is_file() {
        w
    } else {
        k
    }
}

/// Mutate `kord-albuminfo.json` (seeded from `wpp-albuminfo.json` when only
/// that one exists). Always writes the `kord-` name.
pub fn mutate_album<T>(
    album_dir: &Path,
    f: impl FnOnce(&mut serde_json::Map<String, Value>) -> Result<(bool, T)>,
) -> Result<T> {
    let write = album_dir.join(FILE_ALBUM);
    let read = album_read_path(album_dir);
    mutate_object(&write, Some(&read), f)
}

/// Mutate `kord-trackinfo.json` (seeded from the `wpp-` file when needed).
pub fn mutate_tracks<T>(
    album_dir: &Path,
    f: impl FnOnce(&mut serde_json::Map<String, Value>) -> Result<(bool, T)>,
) -> Result<T> {
    let write = album_dir.join(FILE_TRACK);
    let read = track_read_path(album_dir);
    mutate_object(&write, Some(&read), f)
}

/// Fields of a sidecar row the user set by hand: `userEdited` may be `true`
/// (everything), a list of field names, or an object `{field: true}`.
/// Rows saved by the manual editor (`source: "manual"`) count as edited for
/// every field they carry.
pub fn row_user_edited(row: &Value, field: &str) -> bool {
    match row.get("userEdited") {
        Some(Value::Bool(true)) => return true,
        Some(Value::Array(a)) if a.iter().any(|v| v.as_str() == Some(field)) => return true,
        Some(Value::Object(o)) if o.get(field).and_then(Value::as_bool) == Some(true) => {
            return true
        }
        _ => {}
    }
    row.get("source").and_then(Value::as_str) == Some("manual")
        && row.get(field).is_some_and(|v| !v.is_null())
}

/// Add `fields` to a row's `userEdited` list.
pub fn mark_user_edited(row: &mut serde_json::Map<String, Value>, fields: &[&str]) {
    let mut list: Vec<String> = match row.get("userEdited") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::Object(o)) => o
            .iter()
            .filter(|(_, v)| v.as_bool() == Some(true))
            .map(|(k, _)| k.clone())
            .collect(),
        _ => Vec::new(),
    };
    if row.get("userEdited") == Some(&Value::Bool(true)) {
        return;
    }
    for f in fields {
        if !list.iter().any(|x| x == f) {
            list.push((*f).to_string());
        }
    }
    row.insert("userEdited".into(), json!(list));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_edited_shapes() {
        assert!(row_user_edited(&json!({"userEdited": true}), "title"));
        assert!(row_user_edited(&json!({"userEdited": ["title"]}), "title"));
        assert!(!row_user_edited(&json!({"userEdited": ["genre"]}), "title"));
        assert!(row_user_edited(
            &json!({"userEdited": {"title": true}}),
            "title"
        ));
        assert!(row_user_edited(
            &json!({"source": "manual", "title": "x"}),
            "title"
        ));
        assert!(!row_user_edited(
            &json!({"source": "deezer", "title": "x"}),
            "title"
        ));
        let mut row = serde_json::Map::new();
        mark_user_edited(&mut row, &["title"]);
        mark_user_edited(&mut row, &["title", "genre"]);
        assert_eq!(row["userEdited"], json!(["title", "genre"]));
    }

    #[test]
    fn atomic_write_and_wpp_seed() {
        let dir = std::env::temp_dir().join(format!("rekord-sidecar-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE_ALBUM_WPP),
            r#"{"title":"Old","infoItems":[]}"#,
        )
        .unwrap();
        mutate_album(&dir, |o| {
            o.insert("genre".into(), json!("Rock"));
            Ok((true, ()))
        })
        .unwrap();
        let k = read_object(&dir.join(FILE_ALBUM));
        assert_eq!(k["title"], "Old");
        assert_eq!(k["genre"], "Rock");
        // No temp files left behind.
        let leftovers = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
