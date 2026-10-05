//! Entity info / curiosità stored in the library folders (legacy
//! `server/entityInfo.mjs`): artist items in `kord-artistinfo.json`
//! (`{ items, image }`, photo `kord-artistinfo.jpg`), album items under the
//! `infoItems` key of `kord-albuminfo.json`.
//!
//! Every write goes through [`crate::metadata::sidecar`]: atomic, and under
//! the same per-file lock as the album metadata writes. A legacy single
//! `info` item is migrated into the list on save; items without an id get a
//! stable content id (so ids do not change between reads). New items are
//! de-duplicated by language + normalized text.

use crate::db::Db;
use crate::metadata::entity_search::{
    dedupe_key, item_content_id, search_entity_sources, EntityCandidate, EntitySearchOptions,
};
use crate::metadata::error::{MetaError, SourceError};
use crate::metadata::sidecar::{self, FILE_ARTIST_IMAGE, FILE_ARTIST_INFO};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ITEMS: usize = 40;
const ITEM_TEXT_MAX: usize = 6000;
/// Album key used by the client for loose tracks (no album folder).
pub const LOOSE_ALBUM_KEY: &str = "__loose__";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityInfoItem {
    pub id: String,
    pub lang: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    /// `wikipedia`, `wikiquote`, `lastfm`, `theaudiodb`, `discogs`, `manual`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `bio`, `desc`, `section`, `trivia`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntityInfoBundle {
    pub items: Vec<EntityInfoItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Save only: items added by this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<usize>,
    /// Save only: requested items skipped as duplicates of saved ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicates: Option<usize>,
    /// Save only: items removed / edited by this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited: Option<usize>,
    /// Save only: the artist photo could not be saved (`{code, message}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_error: Option<Value>,
}

fn safe_seg(raw: &str) -> Result<String> {
    // One plain folder name: no separators, `..`/`.` segment, NUL, device
    // name or reserved metadata folder.
    let s = crate::path_util::safe_dir_name(raw).map_err(|_| MetaError::invalid_path().err())?;
    if crate::path_util::has_reserved_segment(&s) {
        return Err(MetaError::invalid_path().err());
    }
    Ok(s)
}

fn under_root(path: &Path, root: &Path) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(p), Ok(r)) => p.starts_with(&r),
        _ => false,
    }
}

/// Album folder name from what the client sent: a folder name, or an
/// `Artist/Album` folder key of this artist.
fn album_segment<'a>(artist: &str, album: &'a str) -> &'a str {
    let a = album.trim();
    match a.split_once('/') {
        Some((first, rest)) if first.trim() == artist.trim() && !rest.contains('/') => rest,
        _ => a,
    }
}

fn resolve_dirs(
    root: &Path,
    artist: &str,
    album: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    if artist.trim().is_empty() {
        return Err(MetaError::artist_required().err());
    }
    let artist_seg = safe_seg(artist)?;
    let artist_dir = root.join(&artist_seg);
    if !artist_dir.is_dir() || !under_root(&artist_dir, root) {
        return Err(MetaError::artist_not_found().err());
    }
    let Some(album_raw) = album.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok((artist_dir, None));
    };
    let album_seg = safe_seg(album_segment(&artist_seg, album_raw))?;
    let album_dir = artist_dir.join(&album_seg);
    if !album_dir.is_dir() || !under_root(&album_dir, root) {
        return Err(MetaError::album_not_found().err());
    }
    Ok((artist_dir, Some(album_dir)))
}

fn str_field(obj: &serde_json::Map<String, Value>, key: &str, max: usize) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().chars().take(max).collect::<String>())
        .filter(|s| !s.is_empty())
}

/// Legacy `sanitizeEntityInfoItem`, with a stable content id instead of a
/// random one when the item has none.
fn sanitize_item(raw: &Value) -> Option<EntityInfoItem> {
    let obj = raw.as_object()?;
    let text = str_field(obj, "text", ITEM_TEXT_MAX)?;
    let lang = obj
        .get("lang")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(8).collect::<String>())
        .unwrap_or_else(|| "it".to_string());
    let id = str_field(obj, "id", 64).unwrap_or_else(|| item_content_id(&lang, &text));
    let http_only =
        |v: Option<String>| v.filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    Some(EntityInfoItem {
        id,
        title: str_field(obj, "title", 200),
        saved_at: str_field(obj, "savedAt", 64),
        edited_at: str_field(obj, "editedAt", 64),
        source: str_field(obj, "source", 32),
        kind: str_field(obj, "kind", 32),
        url: http_only(str_field(obj, "url", 1000)),
        image_url: http_only(
            str_field(obj, "imageUrl", 1000).or_else(|| str_field(obj, "thumbnail", 1000)),
        ),
        lang,
        text,
    })
}

fn sanitize_items_list(raw: Option<&Value>) -> Vec<EntityInfoItem> {
    let Some(arr) = raw.and_then(|v| v.as_array()) else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in arr {
        let Some(item) = sanitize_item(row) else {
            continue;
        };
        if !seen.insert(item.id.clone()) {
            continue;
        }
        out.push(item);
        if out.len() >= MAX_ITEMS {
            break;
        }
    }
    out
}

/// Stored items plus a legacy single `info` item (stable id), de-duplicated.
fn items_with_legacy(list: Option<&Value>, j: &Value) -> Vec<EntityInfoItem> {
    let mut items = sanitize_items_list(list);
    if let Some(legacy) = j.get("info").and_then(sanitize_item) {
        let key = dedupe_key(&legacy.lang, &legacy.text);
        if !items
            .iter()
            .any(|x| x.id == legacy.id || dedupe_key(&x.lang, &x.text) == key)
        {
            items.push(legacy);
        }
    }
    items.truncate(MAX_ITEMS);
    items
}

fn image_name(j: &Value) -> Option<String> {
    j.get("image")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains('/') && !s.contains('\\') && !s.contains(".."))
        .map(|s| s.to_string())
}

pub fn load_artist_info_bundle(artist_dir: &Path) -> EntityInfoBundle {
    let j = sidecar::read_object(&artist_dir.join(FILE_ARTIST_INFO));
    EntityInfoBundle {
        items: items_with_legacy(j.get("items"), &j),
        image: image_name(&j),
        ..Default::default()
    }
}

pub fn load_album_info_items(album_dir: &Path) -> Vec<EntityInfoItem> {
    let j = sidecar::read_object(&sidecar::album_read_path(album_dir));
    items_with_legacy(j.get("infoItems"), &j)
}

pub fn get_entity_info(
    music_root: &Path,
    artist: &str,
    album: Option<&str>,
) -> Result<EntityInfoBundle> {
    if album.map(str::trim) == Some(LOOSE_ALBUM_KEY) {
        // Loose tracks have no album folder: nothing stored, not an error.
        resolve_dirs(music_root, artist, None)?;
        return Ok(EntityInfoBundle::default());
    }
    let (artist_dir, album_dir) = resolve_dirs(music_root, artist, album)?;
    if let Some(album_dir) = album_dir {
        return Ok(EntityInfoBundle {
            items: load_album_info_items(&album_dir),
            ..Default::default()
        });
    }
    Ok(load_artist_info_bundle(&artist_dir))
}

/// Edit of a saved item (`text` / `title`; an empty title clears it).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntityInfoEdit {
    pub id: String,
    pub text: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntityInfoSaveRequest {
    /// Artist folder name.
    pub artist: String,
    /// Album folder name (or `Artist/Album` folder key); none = artist level.
    pub album: Option<String>,
    /// Items to add: `{lang, text, title?, source?, kind?, url?, imageUrl?, id?}`.
    #[serde(default)]
    pub add: Vec<Value>,
    #[serde(default)]
    pub remove_ids: Vec<String>,
    #[serde(default)]
    pub edit: Vec<EntityInfoEdit>,
    /// Artist photo to download into `kord-artistinfo.jpg` (artist level).
    pub image_url: Option<String>,
}

struct Mutation {
    items: Vec<EntityInfoItem>,
    added: usize,
    duplicates: usize,
    removed: usize,
    edited: usize,
}

fn apply_mutation(prev: Vec<EntityInfoItem>, req: &EntityInfoSaveRequest, now: &str) -> Mutation {
    let remove: HashSet<&str> = req.remove_ids.iter().map(|s| s.trim()).collect();
    let before = prev.len();
    let mut items: Vec<EntityInfoItem> = prev
        .into_iter()
        .filter(|it| !remove.contains(it.id.as_str()))
        .collect();
    let removed = before - items.len();
    let mut edited = 0;
    for e in &req.edit {
        let Some(it) = items.iter_mut().find(|x| x.id == e.id.trim()) else {
            continue;
        };
        let mut changed = false;
        if let Some(t) = e
            .text
            .as_deref()
            .map(|s| s.trim().chars().take(ITEM_TEXT_MAX).collect::<String>())
            .filter(|s| !s.is_empty())
        {
            if t != it.text {
                it.text = t;
                changed = true;
            }
        }
        if let Some(title) = e.title.as_deref() {
            let t: String = title.trim().chars().take(200).collect();
            let t = (!t.is_empty()).then_some(t);
            if t != it.title {
                it.title = t;
                changed = true;
            }
        }
        if changed {
            it.edited_at = Some(now.to_string());
            edited += 1;
        }
    }
    let mut have: HashSet<String> = items.iter().map(|x| dedupe_key(&x.lang, &x.text)).collect();
    let mut ids: HashSet<String> = items.iter().map(|x| x.id.clone()).collect();
    let (mut added, mut duplicates) = (0, 0);
    for raw in &req.add {
        let Some(mut item) = sanitize_item(raw) else {
            continue;
        };
        if items.len() >= MAX_ITEMS {
            break;
        }
        let key = dedupe_key(&item.lang, &item.text);
        if have.contains(&key) {
            duplicates += 1;
            continue;
        }
        // Client ids are accepted only when free; otherwise the content id.
        if ids.contains(&item.id) {
            item.id = item_content_id(&item.lang, &item.text);
            if ids.contains(&item.id) {
                duplicates += 1;
                continue;
            }
        }
        item.saved_at = Some(now.to_string());
        item.edited_at = None;
        have.insert(key);
        ids.insert(item.id.clone());
        items.push(item);
        added += 1;
    }
    Mutation {
        items,
        added,
        duplicates,
        removed,
        edited,
    }
}

/// Add / remove / edit items (no photo download; see
/// [`save_entity_info_full`]). Blocking filesystem work.
pub fn save_entity_info(music_root: &Path, req: EntityInfoSaveRequest) -> Result<EntityInfoBundle> {
    if req.album.as_deref().map(str::trim) == Some(LOOSE_ALBUM_KEY) {
        return Err(MetaError::invalid_request("loose tracks have no album folder").err());
    }
    let (artist_dir, album_dir_opt) = resolve_dirs(music_root, &req.artist, req.album.as_deref())?;
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(album_dir) = album_dir_opt {
        let m = sidecar::mutate_album(&album_dir, |obj| {
            let j = Value::Object(obj.clone());
            let prev = items_with_legacy(obj.get("infoItems"), &j);
            obj.remove("info");
            let m = apply_mutation(prev, &req, &now);
            if m.items.is_empty() {
                obj.remove("infoItems");
            } else {
                obj.insert("infoItems".into(), serde_json::to_value(&m.items)?);
            }
            obj.insert("editedAt".into(), json!(now));
            Ok((true, m))
        })?;
        return Ok(EntityInfoBundle {
            items: m.items,
            image: None,
            added: Some(m.added),
            duplicates: Some(m.duplicates),
            removed: Some(m.removed),
            edited: Some(m.edited),
            image_error: None,
        });
    }

    let path = artist_dir.join(FILE_ARTIST_INFO);
    let (m, image) = sidecar::mutate_object(&path, None, |obj| {
        let j = Value::Object(obj.clone());
        let prev = items_with_legacy(obj.get("items"), &j);
        let image = image_name(&j);
        obj.remove("info");
        let m = apply_mutation(prev, &req, &now);
        if m.items.is_empty() {
            // Legacy: no items left → the file and the photo go.
            return Ok((false, (m, None)));
        }
        obj.insert("items".into(), serde_json::to_value(&m.items)?);
        obj.insert("editedAt".into(), json!(now));
        Ok((true, (m, image)))
    })?;
    if m.items.is_empty() {
        sidecar::with_lock(&path, || {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(artist_dir.join(FILE_ARTIST_IMAGE));
        });
    }
    Ok(EntityInfoBundle {
        items: m.items,
        image,
        added: Some(m.added),
        duplicates: Some(m.duplicates),
        removed: Some(m.removed),
        edited: Some(m.edited),
        image_error: None,
    })
}

/// Re-encode downloaded bytes as a JPEG (≤ 1600 px), so the file really is
/// what its `.jpg` name says.
fn to_jpeg(bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes).map_err(|_| MetaError::image_invalid().err())?;
    let img = if img.width() > 1600 || img.height() > 1600 {
        img.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&rgb)
        .map_err(|_| MetaError::image_invalid().err())?;
    Ok(out)
}

/// Write the artist photo (`kord-artistinfo.jpg`, re-encoded) and point the
/// sidecar at it. Blocking.
pub fn set_artist_image_bytes(artist_dir: &Path, bytes: &[u8]) -> Result<String> {
    let jpeg = to_jpeg(bytes)?;
    let path = artist_dir.join(FILE_ARTIST_INFO);
    sidecar::with_lock(&path, || -> Result<()> {
        sidecar::write_atomic(&artist_dir.join(FILE_ARTIST_IMAGE), &jpeg)?;
        let mut j = sidecar::read_object(&path);
        if let Some(obj) = j.as_object_mut() {
            obj.insert("image".into(), Value::String(FILE_ARTIST_IMAGE.into()));
        }
        sidecar::write_json_atomic(&path, &j)
    })?;
    Ok(FILE_ARTIST_IMAGE.into())
}

/// Download `url` (public hosts only, size-capped) and store it as the
/// artist photo.
pub async fn save_artist_image_from_url(artist_dir: &Path, url: &str) -> Result<String> {
    let (bytes, _) = crate::metadata::artwork::fetch_public_image(
        url,
        crate::metadata::artwork::MAX_IMAGE_BYTES,
    )
    .await?;
    let dir = artist_dir.to_path_buf();
    tokio::task::spawn_blocking(move || set_artist_image_bytes(&dir, &bytes)).await?
}

/// Save items and, at artist level with items left, the photo from
/// `image_url`. A photo failure does not undo the text save: it is reported
/// in `imageError`.
pub async fn save_entity_info_full(
    music_root: &Path,
    req: EntityInfoSaveRequest,
) -> Result<EntityInfoBundle> {
    let image_url = req
        .image_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let artist = req.artist.clone();
    let artist_level = req
        .album
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty);
    let root = music_root.to_path_buf();
    let mut bundle = tokio::task::spawn_blocking(move || save_entity_info(&root, req)).await??;
    if let (Some(url), true) = (image_url, artist_level && !bundle.items.is_empty()) {
        let dir = resolve_artist_dir(music_root, &artist)?;
        match save_artist_image_from_url(&dir, &url).await {
            Ok(name) => bundle.image = Some(name),
            Err(e) => {
                let (_, code, message) = crate::metadata::error::classify(&e);
                tracing::warn!(error = %message, "artist image not saved");
                bundle.image_error = Some(json!({ "code": code, "message": message }));
            }
        }
    }
    Ok(bundle)
}

pub fn resolve_artist_dir(music_root: &Path, artist: &str) -> Result<PathBuf> {
    let (artist_dir, _) = resolve_dirs(music_root, artist, None)?;
    Ok(artist_dir)
}

// ---------------------------------------------------------------------------
// Search + batch
// ---------------------------------------------------------------------------

/// Search candidates for an artist / album and flag the ones already saved
/// there (`alreadySaved`, `savedId`) when `music_root` is given and the
/// folders exist. Partial results come with per-source `errors`.
pub async fn search_entity_info(
    music_root: Option<&Path>,
    opts: &EntitySearchOptions,
    folder_artist: Option<&str>,
    folder_album: Option<&str>,
) -> Result<crate::metadata::entity_search::EntitySearchResult> {
    let mut res = search_entity_sources(opts).await?;
    if let (Some(root), Some(artist)) = (music_root, folder_artist.or(Some(opts.artist.as_str()))) {
        if let Ok(bundle) = get_entity_info(root, artist, folder_album) {
            mark_saved(&mut res.candidates, &bundle.items);
        }
    }
    Ok(res)
}

/// Flag candidates whose text (or id) is already saved.
pub fn mark_saved(candidates: &mut [EntityCandidate], saved: &[EntityInfoItem]) {
    for c in candidates.iter_mut() {
        let key = dedupe_key(&c.lang, &c.text);
        if let Some(it) = saved
            .iter()
            .find(|it| it.id == c.id || dedupe_key(&it.lang, &it.text) == key)
        {
            c.already_saved = true;
            c.saved_id = Some(it.id.clone());
        }
    }
}

/// Which albums a batch covers.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "scope", content = "albums")]
pub enum BatchScope {
    /// The artist itself plus every album folder of the artist.
    Artist,
    /// Only these album folders (folder names or `Artist/Album` keys).
    Albums(Vec<String>),
}

/// One batch target.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatchTarget {
    /// `"artist"` or the album folder key `Artist/Album`.
    pub key: String,
    /// Album folder name; `None` for the artist row.
    pub album: Option<String>,
    /// Name used for the search (curated album title when known).
    pub label: String,
}

/// Album folders of an artist (sub-directories, hidden / reserved skipped),
/// in natural order.
pub fn artist_album_folders(music_root: &Path, artist: &str) -> Result<Vec<String>> {
    let dir = resolve_artist_dir(music_root, artist)?;
    let mut out: Vec<String> = fs::read_dir(&dir)?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| {
            !n.starts_with('.')
                && !crate::path_util::has_reserved_segment(n)
                && !crate::layout::is_excluded_dir(n)
        })
        .collect();
    out.sort_by(|a, b| crate::db::nat_cmp(&a.to_lowercase(), &b.to_lowercase()));
    Ok(out)
}

/// Targets of a batch: artist row first for [`BatchScope::Artist`], album
/// labels from the DB (curated name) when available.
pub fn batch_targets(
    music_root: &Path,
    db: Option<&Db>,
    artist: &str,
    scope: &BatchScope,
) -> Result<Vec<BatchTarget>> {
    let artist_seg = safe_seg(artist)?;
    let folders: Vec<String> = match scope {
        BatchScope::Artist => artist_album_folders(music_root, &artist_seg)?,
        BatchScope::Albums(list) => list
            .iter()
            .map(|a| album_segment(&artist_seg, a).to_string())
            .filter(|a| !a.is_empty() && a != LOOSE_ALBUM_KEY)
            .collect(),
    };
    let names: std::collections::HashMap<String, String> = db
        .and_then(|d| d.list_albums().ok())
        .map(|albums| {
            albums
                .into_iter()
                .filter(|a| !a.loose)
                .map(|a| (a.folder_key, a.name))
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    if *scope == BatchScope::Artist {
        out.push(BatchTarget {
            key: "artist".into(),
            album: None,
            label: artist_seg.clone(),
        });
    }
    let mut seen = HashSet::new();
    for f in folders {
        let seg = safe_seg(&f)?;
        if !seen.insert(seg.clone()) {
            continue;
        }
        let key = format!("{artist_seg}/{seg}");
        let label = names
            .get(&key)
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| seg.clone());
        out.push(BatchTarget {
            key,
            album: Some(seg),
            label,
        });
    }
    Ok(out)
}

/// Search result of one batch target.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSearchRow {
    #[serde(flatten)]
    pub target: BatchTarget,
    pub candidates: Vec<EntityCandidate>,
    pub errors: Vec<SourceError>,
    /// Items already saved there.
    pub saved: Vec<EntityInfoItem>,
    /// Target-level failure (`{code, message}`), e.g. album folder missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// Progress of a batch: `done` of `total`, current target key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgress {
    pub done: usize,
    pub total: usize,
    pub key: String,
}

fn coded(e: &anyhow::Error) -> Value {
    let (_, code, message) = crate::metadata::error::classify(e);
    json!({ "code": code, "message": message })
}

/// Search every target of the batch (sequential, to stay polite with the
/// public APIs). `on_progress` runs after each target.
pub async fn batch_search_entity_info(
    music_root: &Path,
    db: Option<&Db>,
    artist: &str,
    scope: &BatchScope,
    base: &EntitySearchOptions,
    on_progress: &(dyn Fn(BatchProgress) + Send + Sync),
) -> Result<Vec<BatchSearchRow>> {
    let targets = batch_targets(music_root, db, artist, scope)?;
    let total = targets.len();
    let mut rows = Vec::with_capacity(total);
    for (i, t) in targets.into_iter().enumerate() {
        let saved = get_entity_info(music_root, artist, t.album.as_deref());
        let row = match saved {
            Err(e) => BatchSearchRow {
                target: t.clone(),
                candidates: vec![],
                errors: vec![],
                saved: vec![],
                error: Some(coded(&e)),
            },
            Ok(bundle) => {
                let opts = EntitySearchOptions {
                    artist: base.artist.clone(),
                    album: t.album.as_ref().map(|_| t.label.clone()),
                    ..base.clone()
                };
                match search_entity_sources(&opts).await {
                    Ok(mut res) => {
                        mark_saved(&mut res.candidates, &bundle.items);
                        BatchSearchRow {
                            target: t.clone(),
                            candidates: res.candidates,
                            errors: res.errors,
                            saved: bundle.items,
                            error: None,
                        }
                    }
                    Err(e) => BatchSearchRow {
                        target: t.clone(),
                        candidates: vec![],
                        errors: vec![],
                        saved: bundle.items,
                        error: Some(coded(&e)),
                    },
                }
            }
        };
        rows.push(row);
        on_progress(BatchProgress {
            done: i + 1,
            total,
            key: t.key,
        });
    }
    Ok(rows)
}

/// What to save for one batch target.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatchSaveRow {
    /// Album folder name / key; none = the artist.
    pub album: Option<String>,
    #[serde(default)]
    pub add: Vec<Value>,
    #[serde(default)]
    pub remove_ids: Vec<String>,
    #[serde(default)]
    pub edit: Vec<EntityInfoEdit>,
    pub image_url: Option<String>,
}

/// Outcome of one batch save row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSaveResult {
    pub key: String,
    pub album: Option<String>,
    /// Items added ("N salvate").
    pub saved: usize,
    pub duplicates: usize,
    /// Items stored there after the save.
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// Save several targets of one artist; one failing target does not stop
/// the others.
pub async fn batch_save_entity_info(
    music_root: &Path,
    artist: &str,
    rows: Vec<BatchSaveRow>,
) -> Result<Vec<BatchSaveResult>> {
    let artist_seg = safe_seg(artist)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let album = row
            .album
            .as_deref()
            .map(|a| album_segment(&artist_seg, a).to_string())
            .filter(|a| !a.trim().is_empty());
        let key = album
            .as_ref()
            .map(|a| format!("{artist_seg}/{a}"))
            .unwrap_or_else(|| "artist".into());
        let req = EntityInfoSaveRequest {
            artist: artist_seg.clone(),
            album: album.clone(),
            add: row.add,
            remove_ids: row.remove_ids,
            edit: row.edit,
            image_url: row.image_url,
        };
        match save_entity_info_full(music_root, req).await {
            Ok(b) => out.push(BatchSaveResult {
                key,
                album,
                saved: b.added.unwrap_or(0),
                duplicates: b.duplicates.unwrap_or(0),
                total: b.items.len(),
                image: b.image,
                error: b.image_error,
            }),
            Err(e) => out.push(BatchSaveResult {
                key,
                album,
                saved: 0,
                duplicates: 0,
                total: 0,
                image: None,
                error: Some(coded(&e)),
            }),
        }
    }
    Ok(out)
}

/// One-shot batch: search every target and save all candidates not already
/// saved (artist photo from the first candidate image). Returns per-target
/// saved counts.
pub async fn batch_search_and_save_entity_info(
    music_root: &Path,
    db: Option<&Db>,
    artist: &str,
    scope: &BatchScope,
    base: &EntitySearchOptions,
    on_progress: &(dyn Fn(BatchProgress) + Send + Sync),
) -> Result<Vec<BatchSaveResult>> {
    let rows = batch_search_entity_info(music_root, db, artist, scope, base, on_progress).await?;
    let mut save_rows = Vec::new();
    let mut failed = Vec::new();
    for r in rows {
        if let Some(err) = r.error {
            failed.push(BatchSaveResult {
                key: r.target.key,
                album: r.target.album,
                saved: 0,
                duplicates: 0,
                total: r.saved.len(),
                image: None,
                error: Some(err),
            });
            continue;
        }
        let image_url = if r.target.album.is_none() {
            r.candidates.iter().find_map(|c| c.image_url.clone())
        } else {
            None
        };
        let add = r
            .candidates
            .iter()
            .filter(|c| !c.already_saved)
            .filter_map(|c| serde_json::to_value(c).ok())
            .collect();
        save_rows.push(BatchSaveRow {
            album: r.target.album,
            add,
            image_url,
            ..Default::default()
        });
    }
    let mut out = batch_save_entity_info(music_root, artist, save_rows).await?;
    out.extend(failed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("rekord-einfo-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(d.join("Caparezza/Exuvia")).unwrap();
        d
    }

    fn req(album: Option<&str>) -> EntityInfoSaveRequest {
        EntityInfoSaveRequest {
            artist: "Caparezza".into(),
            album: album.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn legacy_info_gets_a_stable_id_and_is_migrated() {
        let root = tmp();
        let album = root.join("Caparezza/Exuvia");
        fs::write(
            album.join("wpp-albuminfo.json"),
            r#"{"title":"Exuvia","info":{"lang":"it","text":"Un album sulla muta."}}"#,
        )
        .unwrap();
        let a = get_entity_info(&root, "Caparezza", Some("Exuvia")).unwrap();
        let b = get_entity_info(&root, "Caparezza", Some("Exuvia")).unwrap();
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].id, b.items[0].id, "id stable across reads");
        // Saving migrates into kord-albuminfo.json/infoItems with the same id.
        let mut r = req(Some("Exuvia"));
        r.add = vec![json!({"lang": "it", "text": "Un album sulla muta!"})]; // duplicate text
        let saved = save_entity_info(&root, r).unwrap();
        assert_eq!(saved.items.len(), 1);
        use crate::metadata::sidecar::FILE_ALBUM;
        assert_eq!(saved.duplicates, Some(1));
        let k = sidecar::read_object(&album.join(FILE_ALBUM));
        assert!(k.get("info").is_none());
        assert_eq!(k["title"], "Exuvia");
        assert_eq!(k["infoItems"][0]["id"], json!(a.items[0].id));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn add_edit_remove_artist_items() {
        let root = tmp();
        let mut r = req(None);
        r.add = vec![
            json!({"lang": "it", "text": "Caparezza è un rapper di Molfetta.", "source": "wikipedia", "kind": "bio", "url": "https://it.wikipedia.org/wiki/Caparezza", "imageUrl": "https://x/y.jpg"}),
            json!({"lang": "it", "text": "Caparezza  è un rapper di Molfetta"}),
            json!({"lang": "it", "text": "Ha vinto molte Targhe Tenco."}),
        ];
        let b = save_entity_info(&root, r).unwrap();
        assert_eq!((b.added, b.duplicates), (Some(2), Some(1)));
        assert_eq!(b.items[0].source.as_deref(), Some("wikipedia"));
        assert_eq!(b.items[0].image_url.as_deref(), Some("https://x/y.jpg"));
        let id = b.items[1].id.clone();
        let mut r = req(None);
        r.edit = vec![EntityInfoEdit {
            id: id.clone(),
            text: Some("Ha vinto quattro Targhe Tenco.".into()),
            title: Some("Premi".into()),
        }];
        let b = save_entity_info(&root, r).unwrap();
        assert_eq!(b.edited, Some(1));
        assert_eq!(b.items[1].text, "Ha vinto quattro Targhe Tenco.");
        assert_eq!(b.items[1].id, id, "edit keeps the id");
        let mut r = req(None);
        r.remove_ids = b.items.iter().map(|i| i.id.clone()).collect();
        let b = save_entity_info(&root, r).unwrap();
        assert!(b.items.is_empty());
        assert!(!root.join("Caparezza").join(FILE_ARTIST_INFO).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_names_with_colons_and_dots_resolve() {
        let root = tmp();
        fs::create_dir_all(root.join("Caparezza/From Zero: A Cappellas")).unwrap();
        fs::create_dir_all(root.join("Caparezza/...Baby One More Time")).unwrap();
        assert!(get_entity_info(&root, "Caparezza", Some("From Zero: A Cappellas")).is_ok());
        assert!(get_entity_info(&root, "Caparezza", Some("...Baby One More Time")).is_ok());
        assert!(get_entity_info(&root, "Caparezza", Some("Caparezza/Exuvia")).is_ok());
        assert!(get_entity_info(&root, "Caparezza", Some("..")).is_err());
        assert!(get_entity_info(&root, "Caparezza", Some("../Caparezza")).is_err());
        assert!(get_entity_info(&root, "Caparezza", Some(LOOSE_ALBUM_KEY))
            .unwrap()
            .items
            .is_empty());
        let (_, code, _) = crate::metadata::error::classify(
            &get_entity_info(&root, "Caparezza", Some("Nope")).unwrap_err(),
        );
        assert_eq!(code, "album_not_found");
        let t = batch_targets(&root, None, "Caparezza", &BatchScope::Artist).unwrap();
        assert_eq!(t[0].key, "artist");
        // Hidden folders (leading ".") are skipped like legacy.
        assert_eq!(t.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn mark_saved_by_text() {
        let saved = vec![sanitize_item(&json!({"lang":"it","text":"Uno due tre."})).unwrap()];
        let mut c = vec![EntityCandidate {
            id: "zzz".into(),
            source: "wikipedia".into(),
            kind: "bio".into(),
            lang: "it".into(),
            title: None,
            text: "uno, due, tre".into(),
            url: None,
            image_url: None,
            thumbnail: None,
            already_saved: false,
            saved_id: None,
        }];
        mark_saved(&mut c, &saved);
        assert!(c[0].already_saved);
        assert_eq!(c[0].saved_id.as_deref(), Some(saved[0].id.as_str()));
    }
}
