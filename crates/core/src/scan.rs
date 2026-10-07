use crate::cover::find_cover_in_dir;
use crate::db::{Db, TrackRow};
use crate::layout::{self, is_audio_name, is_excluded_dir, LibraryLayout, LOOSE_ALBUM_FOLDER};
use anyhow::{bail, Context, Result};
use lofty::file::AudioFile;
use lofty::picture::{MimeType, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{info, warn};

const LOOSE_ALBUM: &str = LOOSE_ALBUM_FOLDER;
/// Guard against pathological trees / symlink loops while collecting album files.
const MAX_ALBUM_DEPTH: usize = 6;
/// Below this many vanished tracks the mass-deletion guard never triggers.
const MASS_PRUNE_MIN_TRACKS: u64 = 20;
/// Parked favorites / playlist entries whose file never came back are dropped
/// after this many days.
const PARKED_LINKS_TTL_DAYS: i64 = 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanMode {
    #[default]
    /// Upsert what is on disk and drop rows whose files disappeared, unless
    /// that would remove a suspicious share of the library (see [`ScanOptions`]).
    Incremental,
    /// Re-read every file's tags and drop vanished files without the
    /// mass-deletion guard: the explicit "yes, those files are really gone".
    /// Track ids, favorites and playlists are kept (nothing is wiped up front).
    Full,
}

impl ScanMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incremental => "incremental",
            Self::Full => "full",
        }
    }

    pub fn from_query(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("full") | Some("rebuild") => Self::Full,
            _ => Self::Incremental,
        }
    }
}

/// Who asked for the scan; automatic scans are held to a stricter
/// mass-deletion guard because nobody is looking at the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanTrigger {
    /// A person pressed "scan" (or restored a backup).
    #[default]
    Manual,
    /// Watcher, post-download or startup re-index.
    Automatic,
}

impl ScanTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Automatic => "automatic",
        }
    }

    /// Largest share of the indexed tracks an incremental scan may delete in one go.
    fn max_prune_fraction(self) -> f64 {
        match self {
            Self::Manual => 0.5,
            Self::Automatic => 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    pub mode: ScanMode,
    pub trigger: ScanTrigger,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned_files: u64,
    pub indexed_tracks: u64,
    /// Files whose size+mtime matched the previous scan (tags not re-read).
    pub unchanged: u64,
    pub skipped: u64,
    pub errors: u64,
    pub removed_tracks: u64,
    pub removed_albums: u64,
    pub removed_artists: u64,
    pub mode: String,
    #[serde(rename = "music_root")]
    pub music_root: String,
    /// Indexed tracks whose file was not found this time.
    pub missing_tracks: u64,
    /// Set when the mass-deletion guard kept the missing tracks in the catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_skipped: Option<String>,
    /// Folders that could not be listed; their tracks were left untouched.
    pub unreadable_dirs: Vec<String>,
    /// Library index epoch after this scan.
    pub index_epoch: u64,
}

/// Incremental scan (default): safe to run repeatedly, keeps favorites/playlists.
pub fn scan_library(db: &Db, music_root: &Path) -> Result<ScanReport> {
    scan_library_with(db, music_root, ScanMode::Incremental)
}

/// Manual scan in the given mode.
pub fn scan_library_with(db: &Db, music_root: &Path, mode: ScanMode) -> Result<ScanReport> {
    scan_library_opts(
        db,
        music_root,
        ScanOptions {
            mode,
            trigger: ScanTrigger::Manual,
        },
    )
}

pub fn scan_library_opts(db: &Db, music_root: &Path, opts: ScanOptions) -> Result<ScanReport> {
    let mode = opts.mode;
    if !music_root.is_dir() {
        bail!("music root is not a directory: {}", music_root.display());
    }

    let root = music_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", music_root.display()))?;
    let layout = layout::load_layout(&root);

    // An unreadable root (permissions, a NAS that dropped) must never look
    // like an empty library: that would prune everything.
    let collected = collect_groups(&root, &layout)?;

    // Full scans re-read every file, but still need the previous states to
    // tell a real change ("recently updated") from a forced re-read.
    let known_files = db.file_states()?;
    let force_reread = mode == ScanMode::Full;

    let mut stats = Stats::default();
    let mut seen: HashSet<String> = HashSet::new();
    let legacy_art = legacy_artwork_map(&root);

    for group in &collected.groups {
        if let Err(err) = index_group(
            db,
            group,
            &known_files,
            force_reread,
            &legacy_art,
            &mut seen,
            &mut stats,
        ) {
            stats.errors += 1;
            warn!(album = %group.folder_key, error = %err, "album index failed");
        }
    }

    // Whatever lives under a folder we could not list is "seen": we simply
    // do not know, so it is kept. An unreadable entry may also be a file
    // itself (dangling link to an unmounted drive, I/O error on stat).
    for prefix in &collected.unreadable {
        seen.insert(prefix.clone());
        for rel in db.track_rel_paths_under(prefix)? {
            seen.insert(rel);
        }
    }

    let scanned_files = stats.scanned;
    let trigger = opts.trigger;
    let outcome = db.prune_tracks_outside_guarded(&seen, |indexed, missing| {
        prune_guard(mode, trigger, scanned_files, indexed, missing)
    })?;
    let removed_tracks = outcome.removed;
    let mut removed_albums = 0u64;
    let mut removed_artists = 0u64;
    if let Some(reason) = &outcome.skipped {
        warn!(
            missing = outcome.missing,
            indexed = outcome.indexed,
            trigger = trigger.as_str(),
            reason = %reason,
            "scan kept vanished tracks (mass-deletion guard)"
        );
        db.set_meta("last_prune_skipped", reason)?;
    } else {
        removed_albums = db.prune_empty_albums()?;
        removed_artists = db.prune_empty_artists()?;
        db.set_meta("last_prune_skipped", "")?;
    }
    if let Err(err) = db.purge_parked_user_links(PARKED_LINKS_TTL_DAYS) {
        warn!(error = %err, "could not purge old parked user links");
    }

    db.rebuild_fts()?;
    db.refresh_counts()?;
    db.rebuild_genres()?;

    let now = chrono::Utc::now().to_rfc3339();
    db.set_meta("last_scan_at", &now)?;
    db.set_meta("music_root", &root.to_string_lossy())?;
    db.set_meta("schema_scan", "folder-first-v4")?;
    db.set_meta("last_scan_mode", mode.as_str())?;
    let index_epoch = db.bump_index_epoch()?;

    info!(
        scanned_files = stats.scanned,
        indexed_tracks = stats.indexed,
        unchanged = stats.unchanged,
        skipped = stats.skipped,
        errors = stats.errors,
        removed_tracks,
        removed_albums,
        removed_artists,
        missing = outcome.missing,
        unreadable_dirs = collected.unreadable.len(),
        mode = mode.as_str(),
        trigger = trigger.as_str(),
        layout = layout.preferred_layout.as_str(),
        "library scan complete"
    );

    Ok(ScanReport {
        scanned_files: stats.scanned,
        indexed_tracks: stats.indexed,
        unchanged: stats.unchanged,
        skipped: stats.skipped,
        errors: stats.errors,
        removed_tracks,
        removed_albums,
        removed_artists,
        mode: mode.as_str().to_string(),
        music_root: root.to_string_lossy().into_owned(),
        missing_tracks: outcome.missing,
        prune_skipped: outcome.skipped,
        unreadable_dirs: collected.unreadable,
        index_epoch,
    })
}

/// Re-index one folder of the library (`artist` or `artist/album[/…]`), e.g.
/// right after a download into it. Only tracks under that folder can be
/// pruned; the rest of the catalog is untouched.
pub fn scan_subtree(db: &Db, music_root: &Path, rel_dir: &str) -> Result<ScanReport> {
    let rel_dir = rel_dir.replace('\\', "/");
    let rel_dir = rel_dir.trim_matches('/');
    let parts: Vec<&str> = rel_dir.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|p| *p == ".." || *p == ".") {
        bail!("invalid folder: {rel_dir}");
    }
    let root = music_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", music_root.display()))?;
    let layout = layout::load_layout(&root);
    let mut collected = Collected::default();
    let artist = parts[0];
    // The prefix whose tracks this scan is responsible for.
    let prefix = if parts.len() == 1 {
        artist.to_string()
    } else {
        format!("{artist}/{}", parts[1])
    };
    let dir = root.join(&prefix);
    if dir.is_dir() {
        if parts.len() == 1 {
            collect_artist_dir(&dir, artist, &layout, &mut collected);
        } else {
            collect_album_dir(&dir, artist, parts[1], &layout, &mut collected);
        }
    }
    let known_files = db.file_states()?;
    let legacy_art = legacy_artwork_map(&root);
    let mut stats = Stats::default();
    let mut seen: HashSet<String> = HashSet::new();
    for group in &collected.groups {
        if let Err(err) = index_group(
            db,
            group,
            &known_files,
            false,
            &legacy_art,
            &mut seen,
            &mut stats,
        ) {
            stats.errors += 1;
            warn!(album = %group.folder_key, error = %err, "album index failed");
        }
    }
    for p in &collected.unreadable {
        seen.insert(p.clone());
        for rel in db.track_rel_paths_under(p)? {
            seen.insert(rel);
        }
    }
    // Everything indexed outside the folder counts as seen.
    let mut keep: HashSet<String> = db.all_track_rel_paths()?.into_iter().collect();
    let under: HashSet<String> = db.track_rel_paths_under(&prefix)?.into_iter().collect();
    keep.retain(|rel| !under.contains(rel));
    let missing = under.iter().filter(|r| !seen.contains(*r)).count() as u64;
    keep.extend(seen);
    let outcome = db.prune_tracks_outside_guarded(&keep, |indexed, _| {
        prune_guard(
            ScanMode::Incremental,
            ScanTrigger::Automatic,
            stats.scanned.max(1),
            indexed,
            missing,
        )
    })?;
    let (mut removed_albums, mut removed_artists) = (0, 0);
    if outcome.skipped.is_none() {
        removed_albums = db.prune_empty_albums()?;
        removed_artists = db.prune_empty_artists()?;
    }
    db.rebuild_fts()?;
    db.refresh_counts()?;
    db.rebuild_genres()?;
    let index_epoch = db.bump_index_epoch()?;
    info!(
        folder = %prefix,
        scanned_files = stats.scanned,
        indexed_tracks = stats.indexed,
        removed_tracks = outcome.removed,
        index_epoch,
        "folder rescan complete"
    );
    Ok(ScanReport {
        scanned_files: stats.scanned,
        indexed_tracks: stats.indexed,
        unchanged: stats.unchanged,
        skipped: stats.skipped,
        errors: stats.errors,
        removed_tracks: outcome.removed,
        removed_albums,
        removed_artists,
        mode: "folder".to_string(),
        music_root: root.to_string_lossy().into_owned(),
        missing_tracks: missing,
        prune_skipped: outcome.skipped,
        unreadable_dirs: collected.unreadable,
        index_epoch,
    })
}

/// Covers registered by the legacy server (`.kord/artwork`), by album folder:
/// used when the folder itself has no `cover.*` / `folder.*` file. Covers
/// fetched by legacy Studio often only live there.
fn legacy_artwork_map(root: &Path) -> HashMap<String, PathBuf> {
    let db_path = root.join(".kord").join("rekord.db");
    let art_dir = root.join(".kord").join("artwork");
    let mut out = HashMap::new();
    if !db_path.is_file() || !art_dir.is_dir() {
        return out;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return out;
    };
    let Ok(mut stmt) = conn.prepare(
        r#"
        SELECT a.folder_rel_path, w.full_path
        FROM albums a JOIN artwork w ON w.id = a.cover_art_id
        WHERE a.folder_rel_path IS NOT NULL
        "#,
    ) else {
        return out;
    };
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
    let Ok(rows) = rows else {
        return out;
    };
    for (folder, full) in rows.flatten() {
        // Absolute paths point at wherever the legacy library lived: the file
        // name inside this library's `.kord/artwork` is what counts.
        let Some(name) = Path::new(&full.replace('\\', "/"))
            .file_name()
            .map(|n| n.to_owned())
        else {
            continue;
        };
        let candidate = art_dir.join(name);
        if candidate.is_file() {
            out.insert(folder.replace('\\', "/"), candidate);
        }
    }
    out
}

/// Decide whether `missing` of `indexed` tracks may be deleted.
fn prune_guard(
    mode: ScanMode,
    trigger: ScanTrigger,
    scanned_files: u64,
    indexed: u64,
    missing: u64,
) -> std::result::Result<(), String> {
    if mode == ScanMode::Full || missing == 0 {
        return Ok(());
    }
    if scanned_files == 0 {
        return Err(format!(
            "no audio files found under the music root while {missing} tracks are indexed \
             (drive not mounted?); run a full rescan to confirm the removal"
        ));
    }
    let fraction = missing as f64 / indexed.max(1) as f64;
    if missing >= MASS_PRUNE_MIN_TRACKS && fraction > trigger.max_prune_fraction() {
        return Err(format!(
            "{missing} of {indexed} indexed tracks vanished in one {} scan; \
             run a full rescan to confirm the removal",
            trigger.as_str()
        ));
    }
    Ok(())
}

#[derive(Default)]
struct Stats {
    scanned: u64,
    indexed: u64,
    unchanged: u64,
    skipped: u64,
    errors: u64,
}

/// One album worth of files, already resolved to display names.
struct AlbumGroup {
    artist: String,
    album: String,
    /// `<artist>/<album folder>` — stable album key, also the rel_path prefix.
    folder_key: String,
    /// Directory used for cover lookup (None for synthetic groups).
    cover_dir: Option<PathBuf>,
    loose: bool,
    /// (absolute path, rel_path)
    files: Vec<(PathBuf, String)>,
}

/// Everything found on disk, plus the folders that could not be listed.
#[derive(Default)]
struct Collected {
    groups: Vec<AlbumGroup>,
    /// rel_path prefixes (`artist`, `artist/album`, …) whose listing failed.
    unreadable: Vec<String>,
}

/// List a directory, sorted. `None` (and a recorded prefix) when it cannot be
/// read, so its tracks are protected from pruning instead of looking deleted.
fn list_dir(dir: &Path, rel_prefix: &str, out: &mut Collected) -> Option<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Ok(entries) => {
            let mut children: Vec<PathBuf> = Vec::new();
            for entry in entries {
                match entry {
                    Ok(e) => children.push(e.path()),
                    Err(err) => {
                        // A half-listed folder is as untrustworthy as an unlisted one.
                        warn!(path = %dir.display(), error = %err, "directory listing interrupted");
                        out.unreadable.push(rel_prefix.to_string());
                        return None;
                    }
                }
            }
            children.sort();
            Some(children)
        }
        Err(err) => {
            warn!(path = %dir.display(), error = %err, "cannot list folder; keeping its tracks");
            out.unreadable.push(rel_prefix.to_string());
            None
        }
    }
}

/// File, directory, or something we cannot stat (dropped mount, dangling link).
enum EntryKind {
    File,
    Dir,
    Other,
    Unreadable,
}

fn entry_kind(path: &Path) -> EntryKind {
    match fs::metadata(path) {
        Ok(m) if m.is_file() => EntryKind::File,
        Ok(m) if m.is_dir() => EntryKind::Dir,
        Ok(_) => EntryKind::Other,
        Err(_) => EntryKind::Unreadable,
    }
}

fn collect_groups(root: &Path, layout: &LibraryLayout) -> Result<Collected> {
    let mut out = Collected::default();
    let mut root_loose: Vec<PathBuf> = Vec::new();

    let entries =
        fs::read_dir(root).with_context(|| format!("cannot list music root {}", root.display()))?;
    let mut top: Vec<PathBuf> = Vec::new();
    for entry in entries {
        top.push(
            entry
                .with_context(|| format!("cannot list music root {}", root.display()))?
                .path(),
        );
    }
    top.sort();

    for path in top {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if is_excluded_dir(name) {
            continue;
        }
        match entry_kind(&path) {
            EntryKind::File => {
                if is_audio_name(name) {
                    root_loose.push(path);
                }
            }
            EntryKind::Dir => collect_artist_dir(&path, name, layout, &mut out),
            EntryKind::Unreadable => out.unreadable.push(name.to_string()),
            EntryKind::Other => {}
        }
    }

    if !root_loose.is_empty() {
        // Flat layout: audio directly in the root. Artist comes from tags when the
        // layout allows it, otherwise from the configured virtual artist.
        for file in root_loose {
            let Some(file_name) = file.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let artist = if layout.uses_tags() {
                read_tag_artist(&file).unwrap_or_else(|| layout.virtual_artist.clone())
            } else {
                layout.virtual_artist.clone()
            };
            let folder_key = format!("{artist}/{LOOSE_ALBUM}");
            let rel = format!("{folder_key}/{file_name}");
            match out.groups.iter_mut().find(|g| g.folder_key == folder_key) {
                Some(existing) => existing.files.push((file, rel)),
                None => out.groups.push(AlbumGroup {
                    artist: artist.clone(),
                    album: LOOSE_ALBUM.to_string(),
                    folder_key,
                    cover_dir: Some(root.to_path_buf()),
                    loose: true,
                    files: vec![(file, rel)],
                }),
            }
        }
    }

    Ok(out)
}

fn collect_artist_dir(
    artist_path: &Path,
    artist_name: &str,
    layout: &LibraryLayout,
    out: &mut Collected,
) {
    let Some(children) = list_dir(artist_path, artist_name, out) else {
        return;
    };

    let mut loose: Vec<(PathBuf, String)> = Vec::new();

    for child in children {
        let Some(name) = child.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if is_excluded_dir(name) {
            continue;
        }
        match entry_kind(&child) {
            // Even in `artist/track` libraries subfolders are indexed as albums, so
            // nothing on disk is ever dropped; the artist's own files stay loose.
            EntryKind::Dir => collect_album_dir(&child, artist_name, name, layout, out),
            EntryKind::File if is_audio_name(name) => {
                let rel = format!("{artist_name}/{LOOSE_ALBUM}/{name}");
                loose.push((child, rel));
            }
            EntryKind::Unreadable => {
                out.unreadable.push(format!("{artist_name}/{name}"));
                if is_audio_name(name) {
                    // Could be one of the artist's loose tracks.
                    out.unreadable
                        .push(format!("{artist_name}/{LOOSE_ALBUM}/{name}"));
                }
            }
            _ => {}
        }
    }

    if !loose.is_empty() {
        out.groups.push(AlbumGroup {
            artist: artist_name.to_string(),
            album: LOOSE_ALBUM.to_string(),
            folder_key: format!("{artist_name}/{LOOSE_ALBUM}"),
            cover_dir: Some(artist_path.to_path_buf()),
            loose: true,
            files: loose,
        });
    }
}

fn collect_album_dir(
    album_path: &Path,
    artist_name: &str,
    album_folder: &str,
    _layout: &LibraryLayout,
    out: &mut Collected,
) {
    let folder_key = format!("{artist_name}/{album_folder}");
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    // CD1/CD2, bonus-disc and other nested directories belong to the album
    // represented by this top-level album folder. `deep_scan` means "walk
    // deeper", not "turn every nested directory into a separate album".
    collect_audio_recursive(
        album_path,
        &folder_key,
        0,
        false,
        &mut files,
        out,
        artist_name,
    );

    if !files.is_empty() {
        out.groups.push(AlbumGroup {
            artist: artist_name.to_string(),
            album: album_folder.to_string(),
            folder_key,
            cover_dir: Some(album_path.to_path_buf()),
            loose: false,
            files,
        });
    }
}

fn collect_audio_recursive(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    split_subfolders: bool,
    files: &mut Vec<(PathBuf, String)>,
    out: &mut Collected,
    artist_name: &str,
) {
    if depth > MAX_ALBUM_DEPTH {
        return;
    }
    let Some(children) = list_dir(dir, rel_prefix, out) else {
        return;
    };

    for child in children {
        let Some(name) = child.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if is_excluded_dir(name) {
            continue;
        }
        match entry_kind(&child) {
            EntryKind::File => {
                if is_audio_name(name) {
                    let rel = format!("{rel_prefix}/{name}");
                    files.push((child, rel));
                }
            }
            EntryKind::Unreadable => out.unreadable.push(format!("{rel_prefix}/{name}")),
            EntryKind::Other => {}
            EntryKind::Dir if split_subfolders => {
                let nested_key = format!("{rel_prefix}/{name}");
                let mut nested_files = Vec::new();
                collect_audio_recursive(
                    &child,
                    &nested_key,
                    depth + 1,
                    split_subfolders,
                    &mut nested_files,
                    out,
                    artist_name,
                );
                if !nested_files.is_empty() {
                    out.groups.push(AlbumGroup {
                        artist: artist_name.to_string(),
                        album: name.to_string(),
                        folder_key: nested_key,
                        cover_dir: Some(child.clone()),
                        loose: false,
                        files: nested_files,
                    });
                }
            }
            EntryKind::Dir => {
                collect_audio_recursive(
                    &child,
                    &format!("{rel_prefix}/{name}"),
                    depth + 1,
                    split_subfolders,
                    files,
                    out,
                    artist_name,
                );
            }
        }
    }
}

/// Extract the best embedded cover from the album's audio files.
///
/// The scanner historically only looked for cover.jpg/folder.jpg. Lofty exposes
/// embedded FLAC PICTURE blocks, ID3 APIC and MP4 covr through the generic Tag,
/// so use those as a fallback. We intentionally only write JPEG/PNG because the
/// thumbnail pipeline is built with those codecs enabled.
fn extract_embedded_cover(group: &AlbumGroup) -> Option<PathBuf> {
    let dir = group.cover_dir.as_deref()?;

    for want_front in [true, false] {
        for (path, _) in &group.files {
            let Ok(tagged) = Probe::open(path).and_then(|p| p.read()) else {
                continue;
            };
            for tag in tagged.tags() {
                for picture in tag.pictures() {
                    if want_front && picture.pic_type() != PictureType::CoverFront {
                        continue;
                    }
                    let ext = match picture.mime_type() {
                        Some(MimeType::Jpeg) => "jpg",
                        Some(MimeType::Png) => "png",
                        _ => continue,
                    };
                    let dest = dir.join(format!(".rekord-embedded-cover.{ext}"));
                    let tmp = dir.join(format!(".rekord-embedded-cover.{ext}.tmp"));
                    if fs::write(&tmp, picture.data()).is_err() {
                        continue;
                    }
                    if fs::rename(&tmp, &dest).is_ok() {
                        // Remove only our stale alternate cache, never cover.jpg/folder.jpg.
                        let stale_ext = if ext == "jpg" { "png" } else { "jpg" };
                        let _ = fs::remove_file(
                            dir.join(format!(".rekord-embedded-cover.{stale_ext}")),
                        );
                        info!(
                            source = %path.display(),
                            cover = %dest.display(),
                            front = want_front,
                            "extracted embedded album cover"
                        );
                        return Some(dest);
                    }
                    let _ = fs::remove_file(&tmp);
                }
            }
        }
    }
    None
}

/// What to write for one file of an album, decided before taking the DB lock.
enum FileWork {
    /// size+mtime unchanged: only re-point at the current album/artist rows.
    Relink,
    Index {
        meta: Box<AudioMeta>,
        size: u64,
        mtime: i64,
    },
}

/// Index one album: all filesystem I/O (stat, tags, cover lookup) happens
/// first, then every row is written in a single transaction.
fn index_group(
    db: &Db,
    group: &AlbumGroup,
    known_files: &HashMap<String, (i64, i64)>,
    force_reread: bool,
    legacy_art: &HashMap<String, PathBuf>,
    seen: &mut HashSet<String>,
    stats: &mut Stats,
) -> Result<()> {
    // Embedded artwork is authoritative: prefer FLAC PICTURE / ID3 APIC /
    // MP4 covr over a loose cover.jpg. Folder and legacy artwork are fallbacks.
    let cover = extract_embedded_cover(group)
        .or_else(|| group.cover_dir.as_deref().and_then(find_cover_in_dir))
        .or_else(|| legacy_art.get(&group.folder_key).cloned());

    let mut work: Vec<(&Path, &str, FileWork)> = Vec::with_capacity(group.files.len());
    let mut changed_files = 0usize;
    for (path, rel) in &group.files {
        stats.scanned += 1;
        seen.insert(rel.clone());

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(err) => {
                stats.errors += 1;
                warn!(path = %path.display(), error = %err, "stat failed");
                continue;
            }
        };
        let size = meta.len() as i64;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        match known_files.get(rel) {
            Some((known_size, known_mtime)) if *known_size == size && *known_mtime == mtime => {
                if !force_reread {
                    work.push((path, rel, FileWork::Relink));
                    continue;
                }
            }
            // `-1`: marked stale by a schema upgrade, not changed on disk.
            Some((_, -1)) => {}
            _ => changed_files += 1,
        }
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        work.push((
            path,
            rel,
            FileWork::Index {
                meta: Box::new(read_audio_meta(path, &file_stem, &group.artist)),
                size: size as u64,
                mtime,
            },
        ));
    }

    // Loose tracks share a synthetic album: its name is the folder constant.
    let folder_title = if group.loose {
        group.album.clone()
    } else {
        crate::db::text::album_display_title(&group.album)
    };
    let (indexed, unchanged) = db.write_batch(|batch| {
        let artist_id = batch.upsert_artist(&group.artist)?;
        let album_id = batch.upsert_album(
            &folder_title,
            &group.artist,
            Some(artist_id),
            &group.folder_key,
            cover.as_deref(),
            group.loose,
        )?;
        let album_name = batch.album_name(album_id)?;
        let mut indexed = 0u64;
        let mut unchanged = 0u64;
        for (path, rel, item) in &work {
            match item {
                // Unchanged on disk: keep DB row (and any Studio edits) untouched,
                // just make sure it points at the current album/artist rows.
                FileWork::Relink => {
                    batch.relink_track_album(rel, album_id, &album_name)?;
                    unchanged += 1;
                }
                FileWork::Index { meta, size, mtime } => {
                    let track_artist = meta.artist.as_deref().unwrap_or(&group.artist);
                    let track_artist_id = batch.upsert_artist(track_artist)?;
                    batch.upsert_track(&TrackRow {
                        rel_path: rel,
                        file_path: path,
                        title: &meta.title,
                        artist_name: track_artist,
                        album_name: &album_name,
                        duration_ms: meta.duration_ms,
                        track_number: meta.track_number,
                        album_id: Some(album_id),
                        artist_id: Some(track_artist_id),
                        size: *size,
                        mtime: *mtime,
                        genre: meta.genre.as_deref(),
                        release_date: meta.release_date.as_deref(),
                        lyrics: meta.lyrics.as_deref(),
                        bpm: meta.bpm,
                        disc_number: meta.disc_number,
                        tag_album: meta.album.as_deref(),
                    })?;
                    batch.set_mp3_seek_header(
                        rel,
                        *size,
                        *mtime,
                        meta.mp3_seek_header
                            .as_ref()
                            .map(|h| (h.insert_at, h.frame.as_slice())),
                    )?;
                    indexed += 1;
                }
            }
        }
        batch.sync_album_display(album_id, &folder_title, !group.loose)?;
        if indexed > 0 {
            batch.backfill_album_meta_from_tracks(album_id)?;
        }
        if changed_files > 0 {
            batch.touch_album(album_id)?;
        }
        Ok((indexed, unchanged))
    })?;
    stats.indexed += indexed;
    stats.unchanged += unchanged;
    Ok(())
}

struct AudioMeta {
    title: String,
    artist: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
    duration_ms: i64,
    genre: Option<String>,
    release_date: Option<String>,
    lyrics: Option<String>,
    bpm: Option<f64>,
    album: Option<String>,
    /// Boxed: rare, and it would bloat every queued file.
    mp3_seek_header: Option<Box<Mp3SeekHeader>>,
}

/// Numeric Vorbis/ID3 index, accepting the common `2/3` form.
fn parse_tag_index(raw: &str) -> Option<i64> {
    raw.trim()
        .split('/')
        .next()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}

/// `"128"`, `"127.5"`, `"127,5"`, `"120 BPM"` → tempo; nonsense → None.
pub fn parse_bpm(raw: &str) -> Option<f64> {
    let t = raw.trim().replace(',', ".");
    let num: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v: f64 = num.parse().ok()?;
    (v.is_finite() && v > 0.0 && v < 1000.0).then(|| (v * 100.0).round() / 100.0)
}

/// Tempo from ID3 `TBPM` / MP4 `tmpo` (integer BPM), Vorbis/APE `BPM`, or a
/// free-form `TEMPO` field.
fn read_bpm(tag: &lofty::tag::Tag) -> Option<f64> {
    [
        ItemKey::Bpm,
        ItemKey::IntegerBpm,
        ItemKey::Unknown("TEMPO".into()),
        ItemKey::Unknown("tempo".into()),
    ]
    .iter()
    .find_map(|k| tag.get_string(k).and_then(parse_bpm))
}

fn read_tag_artist(path: &Path) -> Option<String> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    tagged
        .tags()
        .iter()
        .filter_map(|tag| tag.artist())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Most precise release date in the tags: recording / release dates are full
/// dates more often than the year field (yt-dlp writes `YYYYMMDD` there).
fn read_tag_date(tag: &lofty::tag::Tag) -> Option<String> {
    let mut best: Option<String> = None;
    let candidates = [
        ItemKey::RecordingDate,
        ItemKey::ReleaseDate,
        ItemKey::OriginalReleaseDate,
        ItemKey::Year,
    ];
    for key in &candidates {
        let Some(date) = tag
            .get_string(key)
            .and_then(crate::db::text::normalize_date)
        else {
            continue;
        };
        let better = best.as_deref().is_none_or(|b| {
            crate::db::text::date_precision(&date) > crate::db::text::date_precision(b)
        });
        if better {
            best = Some(date);
        }
    }
    best.or_else(|| {
        tag.year()
            .filter(|y| *y > 0)
            .and_then(|y| crate::db::text::normalize_date(&y.to_string()))
    })
}

/// Duration from `ffmpeg -i` ("Duration: HH:MM:SS.cc"), for files lofty can't
/// time. Runs only on those files, on the scan's blocking thread.
fn ffmpeg_duration_ms(path: &Path) -> Option<i64> {
    let ffmpeg = crate::tools::resolve_blocking(
        crate::tools::Tool::Ffmpeg,
        &crate::tools::ToolContext::default(),
    );
    if !ffmpeg.available {
        return None;
    }
    let out = std::process::Command::new(&ffmpeg.path)
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let raw = text.split("Duration: ").nth(1)?.split(',').next()?.trim();
    let mut parts = raw.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    let ms = ((h * 3600.0 + m * 60.0 + sec) * 1000.0).round() as i64;
    (ms > 0).then_some(ms)
}

#[derive(Default)]
struct FfprobeMeta {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
    duration_ms: Option<i64>,
    genre: Option<String>,
    release_date: Option<String>,
    lyrics: Option<String>,
    bpm: Option<f64>,
}

fn merge_ffprobe_tags(
    dst: &mut HashMap<String, String>,
    value: Option<&serde_json::Value>,
) {
    let Some(obj) = value.and_then(serde_json::Value::as_object) else {
        return;
    };
    for (key, value) in obj {
        let Some(raw) = value.as_str() else {
            continue;
        };
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        dst.entry(key.to_ascii_lowercase())
            .or_insert_with(|| value.to_string());
    }
}

fn ffprobe_metadata_value(root: &serde_json::Value) -> Option<FfprobeMeta> {
    let mut tags = HashMap::<String, String>::new();
    merge_ffprobe_tags(&mut tags, root.pointer("/format/tags"));

    let streams = root
        .get("streams")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for stream in streams {
        if stream.get("codec_type").and_then(serde_json::Value::as_str) == Some("audio") {
            merge_ffprobe_tags(&mut tags, stream.get("tags"));
        }
    }

    let get = |keys: &[&str]| -> Option<String> {
        keys.iter().find_map(|key| tags.get(*key).cloned())
    };
    let duration_ms = root
        .pointer("/format/duration")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| raw.parse::<f64>().ok())
        .or_else(|| {
            streams.iter().find_map(|stream| {
                (stream
                    .get("codec_type")
                    .and_then(serde_json::Value::as_str)
                    == Some("audio"))
                .then(|| stream.get("duration")?.as_str()?.parse::<f64>().ok())
                .flatten()
            })
        })
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(|seconds| (seconds * 1000.0).round() as i64);

    let meta = FfprobeMeta {
        title: get(&["title"]),
        artist: get(&["artist", "album_artist", "albumartist"]),
        album: get(&["album"]),
        track_number: get(&["track", "tracknumber", "track_number"])
            .and_then(|raw| parse_tag_index(&raw)),
        disc_number: get(&["disc", "discnumber", "disc_number"])
            .and_then(|raw| parse_tag_index(&raw)),
        duration_ms,
        genre: get(&["genre"]),
        release_date: get(&[
            "date",
            "release_date",
            "releasedate",
            "originaldate",
            "original_date",
            "year",
        ])
        .and_then(|raw| crate::db::text::normalize_date(&raw)),
        lyrics: get(&[
            "lyrics",
            "unsyncedlyrics",
            "unsynced_lyrics",
            "syncedlyrics",
            "synced_lyrics",
        ]),
        bpm: get(&["bpm", "tempo", "tbpm"]).and_then(|raw| parse_bpm(&raw)),
    };

    let has_any = meta.title.is_some()
        || meta.artist.is_some()
        || meta.album.is_some()
        || meta.track_number.is_some()
        || meta.disc_number.is_some()
        || meta.duration_ms.is_some()
        || meta.genre.is_some()
        || meta.release_date.is_some()
        || meta.lyrics.is_some()
        || meta.bpm.is_some();
    has_any.then_some(meta)
}

/// ffprobe is deliberately only a fallback: spawning a process for every
/// healthy file would make large-library scans unnecessarily slow. It is
/// valuable for FLAC/Vorbis files that Lofty rejects or only partially parses.
fn ffprobe_metadata(path: &Path) -> Option<FfprobeMeta> {
    let tool = crate::tools::resolve_blocking(
        crate::tools::Tool::Ffprobe,
        &crate::tools::ToolContext::default(),
    );
    if !tool.available {
        return None;
    }
    let out = std::process::Command::new(&tool.path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:format_tags:stream=codec_type,duration:stream_tags",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    ffprobe_metadata_value(&value)
}

fn is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
}

/// Tags of one file, with the display title and track numbers resolved:
/// a missing title (or one that only repeats the file name) is cleaned up
/// from the file name, and numbers fall back to the file name too.
fn read_audio_meta(path: &Path, file_stem: &str, artist_folder: &str) -> AudioMeta {
    let (guess_disc, guess_track) = crate::db::text::guess_track_numbers(file_stem);
    let tagged = match Probe::open(path).and_then(|p| p.read()) {
        Ok(tagged) => tagged,
        Err(lofty_error) => {
            if let Some(meta) = ffprobe_metadata(path) {
                info!(
                    path = %path.display(),
                    error = %lofty_error,
                    "Lofty metadata read failed; using ffprobe fallback"
                );
                let title = crate::db::text::track_display_title(
                    meta.title.as_deref(),
                    file_stem,
                    meta.artist.as_deref().unwrap_or(artist_folder),
                );
                return AudioMeta {
                    title,
                    artist: meta.artist,
                    track_number: meta.track_number.or(guess_track),
                    disc_number: meta.disc_number.or(guess_disc),
                    duration_ms: meta
                        .duration_ms
                        .or_else(|| ffmpeg_duration_ms(path))
                        .unwrap_or(0),
                    genre: meta.genre,
                    release_date: meta.release_date,
                    lyrics: meta.lyrics,
                    bpm: meta.bpm,
                    album: meta.album,
                    mp3_seek_header: None,
                };
            }

            warn!(
                path = %path.display(),
                error = %lofty_error,
                "audio metadata unreadable by Lofty and ffprobe"
            );
            let counted = is_mp3(path)
                .then(|| mp3_frames::count(path, true))
                .flatten();
            return AudioMeta {
                title: crate::db::text::track_display_title(None, file_stem, artist_folder),
                artist: None,
                track_number: guess_track,
                disc_number: guess_disc,
                duration_ms: counted
                    .as_ref()
                    .map(|c| c.duration_ms)
                    .or_else(|| ffmpeg_duration_ms(path))
                    .unwrap_or(0),
                genre: None,
                release_date: None,
                lyrics: None,
                bpm: None,
                album: None,
                mp3_seek_header: counted.and_then(|c| c.seek_header).map(Box::new),
            };
        }
    };

    // FLAC can legally contain Vorbis Comments plus a read-only ID3v2 tag.
    // Merge every parsed tag and take the first non-empty value per field.
    let tags = tagged.tags();
    let mut artist = tags
        .iter()
        .filter_map(|tag| tag.artist())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty());
    let mut raw_title = tags
        .iter()
        .filter_map(|tag| tag.title())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty());
    let mut album = tags
        .iter()
        .filter_map(|tag| tag.album())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty());
    let mut genre = tags
        .iter()
        .filter_map(|tag| tag.genre())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty());
    let mut track_number = tags
        .iter()
        .find_map(|tag| {
            tag.track()
                .filter(|number| *number > 0)
                .map(|number| number as i64)
                .or_else(|| {
                    tag.get_string(&ItemKey::TrackNumber)
                        .and_then(parse_tag_index)
                })
        })
        .or(guess_track);
    let mut disc_number = tags
        .iter()
        .find_map(|tag| {
            tag.disk()
                .filter(|number| *number > 0)
                .map(|number| number as i64)
                .or_else(|| {
                    tag.get_string(&ItemKey::DiscNumber)
                        .and_then(parse_tag_index)
                })
        })
        .or(guess_disc);
    let mut release_date = tags
        .iter()
        .filter_map(read_tag_date)
        .max_by_key(|date| crate::db::text::date_precision(date));
    let mut lyrics = tags
        .iter()
        .find_map(|tag| tag.get_string(&ItemKey::Lyrics))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut bpm = tags.iter().find_map(read_bpm);

    let mut duration_ms = tagged.properties().duration().as_millis() as i64;
    let mut mp3_seek_header = None;
    if tagged.file_type() == lofty::file::FileType::Mpeg {
        // lofty only estimates VBR files without a Xing/VBRI header.
        if let Some(counted) = mp3_count(path) {
            duration_ms = counted.duration_ms;
            mp3_seek_header = counted.seek_header.map(Box::new);
        }
    }

    // A partially parsed FLAC is nearly as harmful as a rejected one: missing
    // TITLE/ARTIST/ALBUM makes matching and album display fall back to folder
    // names. Ask ffprobe only when one of those core fields is absent.
    let is_flac = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("flac"));
    if (tags.is_empty() || (is_flac && (raw_title.is_none() || artist.is_none() || album.is_none())))
        && let Some(fallback) = ffprobe_metadata(path)
    {
        raw_title = raw_title.or(fallback.title);
        artist = artist.or(fallback.artist);
        album = album.or(fallback.album);
        genre = genre.or(fallback.genre);
        track_number = track_number.or(fallback.track_number);
        disc_number = disc_number.or(fallback.disc_number);
        release_date = release_date.or(fallback.release_date);
        lyrics = lyrics.or(fallback.lyrics);
        bpm = bpm.or(fallback.bpm);
        if duration_ms <= 0 {
            duration_ms = fallback.duration_ms.unwrap_or(0);
        }
    }

    if duration_ms <= 0 {
        // lofty can't time some containers (WMA/ASF, WebM): ask ffmpeg.
        duration_ms = ffmpeg_duration_ms(path).unwrap_or(0);
    }

    let title = crate::db::text::track_display_title(
        raw_title.as_deref(),
        file_stem,
        artist.as_deref().unwrap_or(artist_folder),
    );

    AudioMeta {
        title,
        artist,
        track_number,
        disc_number,
        duration_ms,
        genre,
        release_date,
        lyrics,
        bpm,
        album,
        mp3_seek_header,
    }
}

/// Frame count of an MP3 whose duration lofty (and `ffmpeg -i`) can only
/// *estimate*, see [`mp3_count`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3Count {
    pub duration_ms: i64,
    /// Xing header the file lacks; `/media` splices it in so players get the
    /// exact length and seek by its table of contents instead of guessing.
    pub seek_header: Option<Mp3SeekHeader>,
}

/// A synthetic Xing frame (frame count, byte count, TOC) and the file offset
/// of the first MPEG frame, where it belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3SeekHeader {
    pub insert_at: u64,
    pub frame: Vec<u8>,
}

/// Exact MP3 duration where lofty (and `ffmpeg -i`) only estimate it.
///
/// Without a Xing/VBRI header both extrapolate a bitrate over the whole file,
/// which is right for CBR and wrong — by many minutes on a long DJ set — for
/// VBR (typical of YouTube rips). `None` when the estimate is trustworthy: a
/// Xing/VBRI frame count or an `Info` (CBR) header is present, or sampling
/// shows a constant bitrate on a file small enough for an error not to
/// matter. Otherwise every frame header is walked (sequential read, no
/// decoding) and the samples are summed.
pub fn mp3_count(path: &Path) -> Option<Mp3Count> {
    mp3_frames::count(path, false)
}

/// [`mp3_count`]'s duration.
pub fn mp3_counted_duration_ms(path: &Path) -> Option<i64> {
    mp3_count(path).map(|c| c.duration_ms)
}

/// Like [`mp3_counted_duration_ms`], but always walks the frames.
pub fn mp3_frame_walk_duration_ms(path: &Path) -> Option<i64> {
    mp3_frames::count(path, true).map(|c| c.duration_ms)
}

mod mp3_frames {
    use super::{Mp3Count, Mp3SeekHeader};
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::Path;

    /// Files at least this big are always counted when they have no frame
    /// count header: an estimate error on a multi-hour set is minutes.
    const ALWAYS_COUNT_BYTES: u64 = 48 * 1024 * 1024;
    /// Frames sampled (at the start and in the middle) to tell CBR from VBR.
    const SAMPLE_FRAMES: usize = 600;
    const BUF_BYTES: usize = 1024 * 1024;
    /// Every this many frames the walk remembers an offset (for the TOC).
    const TOC_STRIDE: u64 = 32;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Header {
        version: u8, // 3 = MPEG-1, 2 = MPEG-2, 0 = MPEG-2.5
        layer: u8,   // 1..=3
        mono: bool,
        bitrate_kbps: u32,
        sample_rate: u32,
        samples: u32,
        len: usize,
    }

    impl Header {
        fn parse(b: &[u8]) -> Option<Header> {
            if b.len() < 4 || b[0] != 0xFF || b[1] & 0xE0 != 0xE0 {
                return None;
            }
            let version = (b[1] >> 3) & 3;
            let layer = match (b[1] >> 1) & 3 {
                0 => return None,
                bits => 4 - bits,
            };
            let br_ix = (b[2] >> 4) as usize;
            let sr_ix = ((b[2] >> 2) & 3) as usize;
            if version == 1 || br_ix == 0 || br_ix == 15 || sr_ix == 3 {
                return None;
            }
            const V1: [[u32; 14]; 3] = [
                [
                    32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
                ],
                [
                    32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
                ],
                [
                    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
                ],
            ];
            const V2_L1: [u32; 14] = [
                32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
            ];
            const V2_L23: [u32; 14] = [8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
            let bitrate_kbps = match (version, layer) {
                (3, l) => V1[l as usize - 1][br_ix - 1],
                (_, 1) => V2_L1[br_ix - 1],
                _ => V2_L23[br_ix - 1],
            };
            let base_sr = [44100u32, 48000, 32000][sr_ix];
            let sample_rate = match version {
                3 => base_sr,
                2 => base_sr / 2,
                _ => base_sr / 4,
            };
            let samples = match (layer, version) {
                (1, _) => 384,
                (2, _) | (3, 3) => 1152,
                _ => 576,
            };
            let pad = u32::from((b[2] >> 1) & 1);
            let len = if layer == 1 {
                (12 * bitrate_kbps * 1000 / sample_rate + pad) * 4
            } else {
                samples / 8 * bitrate_kbps * 1000 / sample_rate + pad
            } as usize;
            if len < 4 {
                return None;
            }
            Some(Header {
                version,
                layer,
                mono: b[3] >> 6 == 3,
                bitrate_kbps,
                sample_rate,
                samples,
                len,
            })
        }

        /// Same stream: frames of one file never change these.
        fn same_stream(&self, other: &Header) -> bool {
            self.version == other.version
                && self.layer == other.layer
                && self.sample_rate == other.sample_rate
        }

        /// Offset of a Xing header inside the frame (after the side info).
        fn xing_offset(&self) -> usize {
            4 + match (self.version == 3, self.mono) {
                (true, false) => 32,
                (true, true) | (false, false) => 17,
                (false, true) => 9,
            }
        }
    }

    /// Sequential frame reader over a large buffer (no decoding).
    struct Frames {
        file: File,
        buf: Vec<u8>,
        /// File offset of `buf[0]`.
        base: u64,
        lo: usize,
        hi: usize,
        eof: bool,
        stream: Option<Header>,
        locked: bool,
    }

    impl Frames {
        fn open(path: &Path) -> Option<Frames> {
            let mut file = File::open(path).ok()?;
            let start = id3v2_len(&mut file)?;
            let mut frames = Frames {
                file,
                buf: vec![0; BUF_BYTES],
                base: 0,
                lo: 0,
                hi: 0,
                eof: false,
                stream: None,
                locked: false,
            };
            frames.seek(start)?;
            Some(frames)
        }

        /// Jump to `offset` and look for a frame boundary again.
        fn seek(&mut self, offset: u64) -> Option<()> {
            self.file.seek(SeekFrom::Start(offset)).ok()?;
            self.base = offset;
            self.lo = 0;
            self.hi = 0;
            self.eof = false;
            self.locked = false;
            Some(())
        }

        /// At least `need` bytes buffered from `lo` (false at end of file).
        fn fill(&mut self, need: usize) -> bool {
            if self.hi - self.lo >= need {
                return true;
            }
            self.buf.copy_within(self.lo..self.hi, 0);
            self.base += self.lo as u64;
            self.hi -= self.lo;
            self.lo = 0;
            while self.hi < need && !self.eof {
                match self.file.read(&mut self.buf[self.hi..]) {
                    Ok(0) | Err(_) => self.eof = true,
                    Ok(n) => self.hi += n,
                }
            }
            self.hi - self.lo >= need
        }

        /// Next frame (header, file offset, its bytes; `lo` advanced past
        /// it), or `None` at end of file. Out of sync (start, junk, a seek),
        /// a candidate is only accepted when the header after it fits too.
        fn next_frame(&mut self) -> Option<(Header, u64, &[u8])> {
            loop {
                if !self.fill(4) {
                    return None;
                }
                let at = self.lo;
                let Some(h) = Header::parse(&self.buf[at..at + 4]) else {
                    self.locked = false;
                    self.lo += 1;
                    continue;
                };
                if !self.stream.is_none_or(|s| s.same_stream(&h)) {
                    self.locked = false;
                    self.lo += 1;
                    continue;
                }
                if !self.fill(h.len) {
                    return None; // truncated last frame
                }
                if !self.locked {
                    // Confirm with the following header (when there is one).
                    let at = self.lo;
                    let confirmed = self.fill(h.len + 4) && {
                        let next = &self.buf[at + h.len..at + h.len + 4];
                        Header::parse(next).is_some_and(|n| n.same_stream(&h))
                    };
                    if !confirmed {
                        self.lo += 1;
                        continue;
                    }
                    self.locked = true;
                    self.stream.get_or_insert(h);
                }
                let at = self.lo;
                self.lo += h.len;
                return Some((h, self.base + at as u64, &self.buf[at..at + h.len]));
            }
        }

        /// File offset of the next unread byte.
        fn position(&self) -> u64 {
            self.base + self.lo as u64
        }
    }

    /// Bytes taken by leading ID3v2 tags (there may be more than one).
    fn id3v2_len(file: &mut File) -> Option<u64> {
        let mut offset = 0u64;
        loop {
            let mut head = [0u8; 10];
            file.seek(SeekFrom::Start(offset)).ok()?;
            if file.read_exact(&mut head).is_err() || &head[..3] != b"ID3" {
                return Some(offset);
            }
            let size = head[6..10]
                .iter()
                .fold(0u64, |acc, b| (acc << 7) | u64::from(b & 0x7F));
            let footer = if head[5] & 0x10 != 0 { 10 } else { 0 };
            offset += 10 + size + footer;
        }
    }

    enum InfoFrame {
        /// Xing (VBR) or VBRI header with usable counts: lofty times it.
        FrameCount,
        /// `Info`: LAME's CBR header.
        Cbr,
        /// A Xing/VBRI frame without usable counts: not audio, not counted.
        Empty,
    }

    fn be32(b: &[u8]) -> u32 {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }

    /// Xing / Info / VBRI header inside the first frame.
    fn info_frame(h: &Header, frame: &[u8]) -> Option<InfoFrame> {
        let xing = h.xing_offset();
        if let Some(tag) = frame.get(xing..xing + 16) {
            if &tag[..4] == b"Xing" || &tag[..4] == b"Info" {
                // Same rule as lofty: frame and byte counts both present.
                let usable =
                    be32(&tag[4..]) & 3 == 3 && be32(&tag[8..]) > 0 && be32(&tag[12..]) > 0;
                return Some(match (usable, &tag[..4] == b"Info") {
                    (true, true) => InfoFrame::Cbr,
                    (true, false) => InfoFrame::FrameCount,
                    (false, _) => InfoFrame::Empty,
                });
            }
        }
        if let Some(tag) = frame.get(36..36 + 18) {
            if &tag[..4] == b"VBRI" {
                let usable = be32(&tag[10..]) > 0 && be32(&tag[14..]) > 0;
                return Some(if usable {
                    InfoFrame::FrameCount
                } else {
                    InfoFrame::Empty
                });
            }
        }
        None
    }

    /// Whether `SAMPLE_FRAMES` frames from the reader's position all have
    /// `bitrate_kbps` (true at end of file).
    fn constant_bitrate(frames: &mut Frames, bitrate_kbps: u32) -> bool {
        for _ in 0..SAMPLE_FRAMES {
            match frames.next_frame() {
                Some((h, _, _)) if h.bitrate_kbps != bitrate_kbps => return false,
                Some(_) => {}
                None => break,
            }
        }
        true
    }

    /// A Layer III Xing frame matching `first` (same version, sample rate and
    /// channel mode, so decoders read it at the usual offset), as small as the
    /// header allows. `toc` gets the frame's own length.
    fn xing_frame(
        first_raw: [u8; 4],
        first: &Header,
        audio_frames: u64,
        stream_bytes: u64,
        toc: impl Fn(u64) -> [u8; 100],
    ) -> Option<Vec<u8>> {
        if first.layer != 3 {
            return None;
        }
        let at = first.xing_offset();
        let need = at + 4 + 4 + 4 + 4 + 100;
        let (raw, h) = (1u8..=14).find_map(|br_ix| {
            let raw = [
                0xFF,
                first_raw[1] | 1, // no CRC
                (br_ix << 4) | (first_raw[2] & 0x0C),
                first_raw[3] & 0xC0,
            ];
            Header::parse(&raw)
                .filter(|h| h.len >= need)
                .map(|h| (raw, h))
        })?;
        let frames = u32::try_from(audio_frames).ok()?;
        let bytes = u32::try_from(stream_bytes + h.len as u64).ok()?;
        let mut out = vec![0u8; h.len];
        out[..4].copy_from_slice(&raw);
        out[at..at + 4].copy_from_slice(b"Xing");
        out[at + 4..at + 8].copy_from_slice(&7u32.to_be_bytes()); // frames | bytes | TOC
        out[at + 8..at + 12].copy_from_slice(&frames.to_be_bytes());
        out[at + 12..at + 16].copy_from_slice(&bytes.to_be_bytes());
        out[at + 16..at + 116].copy_from_slice(&toc(h.len as u64));
        Some(out)
    }

    pub(super) fn count(path: &Path, always: bool) -> Option<Mp3Count> {
        let size = std::fs::metadata(path).ok()?.len();
        let mut frames = Frames::open(path)?;
        let (first, first_off, first_frame) = frames.next_frame()?;
        let first_raw = [
            first_frame[0],
            first_frame[1],
            first_frame[2],
            first_frame[3],
        ];
        let info = info_frame(&first, first_frame);
        if !always && matches!(info, Some(InfoFrame::FrameCount | InfoFrame::Cbr)) {
            return None;
        }
        if !always && size < ALWAYS_COUNT_BYTES {
            // Small file: worth a full read only when the bitrate varies.
            let after_first = frames.position();
            let cbr = constant_bitrate(&mut frames, first.bitrate_kbps) && {
                frames.seek(size / 2)?;
                constant_bitrate(&mut frames, first.bitrate_kbps)
            };
            if cbr {
                return None;
            }
            frames.seek(after_first)?;
        }
        // Offsets (from `first_off`) of every TOC_STRIDE-th audio frame.
        let mut marks: Vec<u64> = Vec::new();
        let mut audio_frames = 0u64;
        let mut samples = 0u64;
        // A Xing/Info/VBRI frame carries no audio.
        if info.is_none() {
            marks.push(0);
            audio_frames = 1;
            samples = u64::from(first.samples);
        }
        while let Some((h, off, _)) = frames.next_frame() {
            if audio_frames.is_multiple_of(TOC_STRIDE) {
                marks.push(off - first_off);
            }
            audio_frames += 1;
            samples += u64::from(h.samples);
        }
        let rate = u64::from(first.sample_rate);
        let ms = (samples * 1000 + rate / 2) / rate;
        if ms == 0 {
            return None;
        }
        // Files with a usable header keep it; the others get one at serve time.
        let seek_header = matches!(info, None | Some(InfoFrame::Empty))
            .then(|| {
                let stream_bytes = size - first_off;
                xing_frame(first_raw, &first, audio_frames, stream_bytes, |xing_len| {
                    let total = (stream_bytes + xing_len) as f64;
                    let mut toc = [0u8; 100];
                    for (i, slot) in toc.iter_mut().enumerate() {
                        let frame = i as u64 * audio_frames / 100;
                        let mark = marks[((frame / TOC_STRIDE) as usize).min(marks.len() - 1)];
                        *slot = (((xing_len + mark) as f64 * 256.0 / total).round() as u64).min(255)
                            as u8;
                    }
                    toc
                })
            })
            .flatten()
            .map(|frame| Mp3SeekHeader {
                insert_at: first_off,
                frame,
            });
        Some(Mp3Count {
            duration_ms: ms as i64,
            seek_header,
        })
    }
}


#[cfg(test)]
mod ffprobe_metadata_tests {
    use super::*;

    #[test]
    fn parses_flac_vorbis_tags_from_ffprobe_json() {
        let value = serde_json::json!({
            "streams": [
                {"codec_type": "audio", "duration": "42.125", "tags": {"encoder": "flac"}}
            ],
            "format": {
                "duration": "42.125",
                "tags": {
                    "TITLE": "Song",
                    "ARTIST": "Artist",
                    "ALBUM": "Album",
                    "GENRE": "Electronic",
                    "DATE": "2024-05-06",
                    "TRACK": "7/12",
                    "DISC": "2/2",
                    "BPM": "123.5",
                    "LYRICS": "hello"
                }
            }
        });
        let meta = ffprobe_metadata_value(&value).expect("metadata");
        assert_eq!(meta.title.as_deref(), Some("Song"));
        assert_eq!(meta.artist.as_deref(), Some("Artist"));
        assert_eq!(meta.album.as_deref(), Some("Album"));
        assert_eq!(meta.genre.as_deref(), Some("Electronic"));
        assert_eq!(meta.release_date.as_deref(), Some("2024-05-06"));
        assert_eq!(meta.track_number, Some(7));
        assert_eq!(meta.disc_number, Some(2));
        assert_eq!(meta.bpm, Some(123.5));
        assert_eq!(meta.lyrics.as_deref(), Some("hello"));
        assert_eq!(meta.duration_ms, Some(42_125));
    }
}
