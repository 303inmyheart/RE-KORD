//! Path safety helpers for music_root-relative operations.

use anyhow::{bail, Result};
use std::path::{Component, Path, PathBuf};

/// Folder names that never belong to the playable library (hub metadata,
/// legacy sidecar stores). Compared case-insensitively, like legacy
/// `hasReservedPathSegment`.
const RESERVED_SEGMENTS: &[&str] = &[".kord", "kord", ".wpp", "wpp", ".rekord"];

/// Normalize and validate a relative path under music root (no `..`, absolute, etc.).
pub fn safe_rel_path(raw: &str) -> Result<String> {
    let s = raw.replace('\\', "/").trim().trim_matches('/').to_string();
    if s.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::new();
    for seg in s.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains('\0') {
            bail!("invalid relative path");
        }
        parts.push(seg);
    }
    let joined = parts.join("/");
    // Every component must be a plain name: a Windows drive (`C:`) or UNC prefix
    // would otherwise make `root.join(rel)` escape the root.
    if !Path::new(&joined)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        bail!("invalid relative path");
    }
    Ok(joined)
}

/// True when a segment of `rel` names a reserved metadata folder.
pub fn has_reserved_segment(rel: &str) -> bool {
    rel.replace('\\', "/").split('/').any(|seg| {
        RESERVED_SEGMENTS
            .iter()
            .any(|r| r.eq_ignore_ascii_case(seg.trim()))
    })
}

/// Windows device names (also with an extension: `CON.txt`).
fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.len() == 4
        && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

/// One folder name supplied by a client (artist / album segment).
///
/// Parity legacy `safeRelSeg` (`server/pathSafety.mjs`): names may contain
/// `:` and `..` (`From Zero: A Cappellas`, `...Baby One More Time`); refused
/// are path separators, an exact `.` / `..`, NUL, Windows device names and
/// anything the platform would not treat as one plain path component
/// (a bare drive like `C:`, `a:b` alternate streams on Windows).
pub fn safe_dir_name(raw: &str) -> Result<String> {
    let s = raw.trim();
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.contains('/')
        || s.contains('\\')
        || s.contains('\0')
        || is_windows_device_name(s)
    {
        bail!("invalid directory name");
    }
    // A drive designator joined onto the root would escape it on Windows.
    let b = s.as_bytes();
    if b.len() == 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        bail!("invalid directory name");
    }
    if cfg!(windows) && s.contains(':') {
        bail!("invalid directory name");
    }
    let mut comps = Path::new(s).components();
    if !matches!(
        (comps.next(), comps.next()),
        (Some(Component::Normal(_)), None)
    ) {
        bail!("invalid directory name");
    }
    Ok(s.to_string())
}

pub fn join_under_root(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = safe_rel_path(rel)?;
    let path = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(PathBuf::from(&rel))
    };
    // Reject absolute components sneaked in via weird paths.
    for c in path.components() {
        if matches!(c, Component::ParentDir) {
            bail!("invalid path");
        }
    }
    Ok(path)
}

pub fn under_root(path: &Path, root: &Path) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(p), Ok(r)) => p.starts_with(&r),
        _ => {
            // Fallback when path does not exist yet (mkdir): resolve the nearest
            // existing ancestor so a symlinked parent cannot point outside.
            let Ok(r) = root.canonicalize() else {
                return false;
            };
            let mut probe = path.to_path_buf();
            let mut tail = Vec::new();
            while !probe.exists() {
                let Some(name) = probe.file_name().map(|n| n.to_os_string()) else {
                    return false;
                };
                tail.push(name);
                if !probe.pop() {
                    return false;
                }
            }
            let Ok(mut resolved) = probe.canonicalize() else {
                return false;
            };
            for name in tail.into_iter().rev() {
                resolved.push(name);
            }
            resolved.starts_with(&r)
        }
    }
}

pub fn rel_path_looks_like_album_folder(rel: &str) -> bool {
    safe_rel_path(rel)
        .map(|s| s.split('/').filter(|p| !p.is_empty()).count() >= 2)
        .unwrap_or(false)
}

/// Why a client-supplied library path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryPathError {
    /// Traversal, absolute, reserved segment: never valid.
    Invalid,
    /// Well-formed but nothing servable there.
    NotFound,
}

/// Resolve a client-supplied rel path to an existing file inside `music_root`.
///
/// The path must be relative, free of `..`/reserved segments and hidden
/// folders; after resolving symlinks it must stay under the music root and
/// outside the hub data dir (which may live inside the library).
pub fn resolve_library_file(
    music_root: &Path,
    data_dir: Option<&Path>,
    raw_rel: &str,
) -> std::result::Result<PathBuf, LibraryPathError> {
    if raw_rel.contains('\0') {
        return Err(LibraryPathError::Invalid);
    }
    // A raw `..` must be refused even when it would normalise away.
    if raw_rel.replace('\\', "/").split('/').any(|s| s == "..") {
        return Err(LibraryPathError::Invalid);
    }
    let rel = safe_rel_path(raw_rel).map_err(|_| LibraryPathError::Invalid)?;
    if rel.is_empty() || has_reserved_segment(&rel) {
        return Err(LibraryPathError::Invalid);
    }
    if rel.split('/').any(crate::layout::is_excluded_dir) {
        return Err(LibraryPathError::Invalid);
    }
    let candidate = music_root.join(&rel);
    let Ok(resolved) = candidate.canonicalize() else {
        return Err(LibraryPathError::NotFound);
    };
    ensure_servable(music_root, data_dir, &resolved)?;
    Ok(resolved)
}

/// Check an absolute (already resolved or DB-provided) file path: it must be a
/// file under the music root and not inside the hub data dir.
pub fn ensure_servable(
    music_root: &Path,
    data_dir: Option<&Path>,
    path: &Path,
) -> std::result::Result<(), LibraryPathError> {
    let resolved = path
        .canonicalize()
        .map_err(|_| LibraryPathError::NotFound)?;
    let root = music_root
        .canonicalize()
        .map_err(|_| LibraryPathError::NotFound)?;
    if !resolved.starts_with(&root) {
        return Err(LibraryPathError::Invalid);
    }
    if let Some(dir) = data_dir.and_then(|d| d.canonicalize().ok()) {
        if resolved.starts_with(&dir) {
            return Err(LibraryPathError::Invalid);
        }
    }
    if let Ok(rest) = resolved.strip_prefix(&root) {
        let rest = rest.to_string_lossy().replace('\\', "/");
        if has_reserved_segment(&rest) {
            return Err(LibraryPathError::Invalid);
        }
    }
    if !resolved.is_file() {
        return Err(LibraryPathError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_prefixes() {
        assert!(safe_rel_path("../etc/passwd").is_err());
        assert!(safe_rel_path("a/../../b").is_err());
        assert_eq!(safe_rel_path("/etc/hostname").unwrap(), "etc/hostname");
        assert_eq!(safe_rel_path("A/./B").unwrap(), "A/B");
        assert!(safe_dir_name("..").is_err());
        assert!(safe_dir_name("C:").is_err());
    }

    #[test]
    fn dir_names_allow_colons_and_inner_dots() {
        for ok in [
            "From Zero: A Cappellas",
            "Caparezza: Exuvia",
            "...Baby One More Time",
            "Vol. 2..",
            "a..b",
            "Re:Stacks",
            "CONCERTO",
            "COM10",
        ] {
            #[cfg(windows)]
            if ok.contains(':') {
                continue;
            }
            assert_eq!(safe_dir_name(ok).unwrap(), ok, "{ok}");
        }
        for bad in [
            "", " ", ".", "..", " .. ", "a/b", "../x", "a\\b", "x\0y", "C:", "z:", "CON", "nul",
            "Com1", "LPT9.txt", "aux.mp3",
        ] {
            assert!(safe_dir_name(bad).is_err(), "{bad:?} must be refused");
        }
        // Whatever passes stays a single child of the root.
        let root = Path::new("/music");
        for ok in ["From Zero: A Cappellas", "...Baby One More Time", "a..b"] {
            let joined = root.join(safe_dir_name(ok).unwrap());
            assert_eq!(joined.parent(), Some(root));
        }
    }

    #[test]
    fn reserved_segments_are_case_insensitive() {
        assert!(has_reserved_segment("Artist/.KORD/rekord.db"));
        assert!(has_reserved_segment("wpp/x"));
        assert!(!has_reserved_segment("Kordelia/Album/01.mp3"));
    }

    #[test]
    fn resolve_library_file_stays_under_root() {
        let base = std::env::temp_dir().join(format!("rekord-pathutil-{}", uuid::Uuid::new_v4()));
        let root = base.join("music");
        std::fs::create_dir_all(root.join("A/B")).unwrap();
        std::fs::write(root.join("A/B/01.mp3"), b"x").unwrap();
        std::fs::write(base.join("secret.txt"), b"s").unwrap();
        assert!(resolve_library_file(&root, None, "A/B/01.mp3").is_ok());
        assert_eq!(
            resolve_library_file(&root, None, "../secret.txt"),
            Err(LibraryPathError::Invalid)
        );
        assert_eq!(
            resolve_library_file(&root, None, "A/B/missing.mp3"),
            Err(LibraryPathError::NotFound)
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("secret.txt"), root.join("A/link.mp3")).unwrap();
            assert_eq!(
                resolve_library_file(&root, None, "A/link.mp3"),
                Err(LibraryPathError::Invalid)
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
