//! Machine-level settings of the legacy Node server (`music-root.config.json`,
//! parity `server/musicRootConfig.mjs`): Discogs token, Cloudflare login flag
//! and YouTube cookies. Imported once in place at startup, and from the
//! `config/` folder of a restored backup.
//!
//! Hub values always win: a setting is only filled when the hub has none, so
//! re-running never undoes something changed in next.

use crate::config::AppConfig;
use crate::db::Db;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub const LEGACY_CONFIG_FILE: &str = "music-root.config.json";
const LEGACY_DISCOGS_TOKEN_FILE: &str = "discogs-token";
/// `library_meta` key recording that the in-place import already ran.
const IMPORTED_META_KEY: &str = "legacy_config_imported_at";

/// What a legacy-config import filled in.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConfigImport {
    pub source: Option<String>,
    pub discogs_token: bool,
    pub cloudflare_logged_in: bool,
    pub youtube_cookies: bool,
}

/// Parsed bits of a legacy `music-root.config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyConfig {
    pub music_root: Option<PathBuf>,
    pub discogs_token_path: Option<PathBuf>,
    pub youtube_cookies_path: Option<PathBuf>,
    pub cloudflare_logged_in: bool,
}

fn non_empty_path(v: &Value, key: &str) -> Option<PathBuf> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn parse_legacy_config(raw: &str) -> Option<LegacyConfig> {
    let v: Value = serde_json::from_str(raw).ok()?;
    if !v.is_object() {
        return None;
    }
    Some(LegacyConfig {
        music_root: non_empty_path(&v, "musicRoot").or_else(|| non_empty_path(&v, "libraryRoot")),
        discogs_token_path: non_empty_path(&v, "discogsTokenPath"),
        youtube_cookies_path: non_empty_path(&v, "youtubeCookiesPath"),
        cloudflare_logged_in: v.get("cloudflareLoggedIn").and_then(|x| x.as_bool()) == Some(true),
    })
}

/// Where the legacy server kept its config: `REKORD_USER_CONFIG_DIR` (and
/// older names), the Electron `userData` folders, or next to a dev checkout's
/// `server/`.
pub fn legacy_config_candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for key in [
        "REKORD_USER_CONFIG_DIR",
        "KORD_USER_CONFIG_DIR",
        "WPP_USER_CONFIG_DIR",
    ] {
        if let Ok(v) = std::env::var(key) {
            let t = v.trim();
            if !t.is_empty() {
                dirs.push(PathBuf::from(t));
            }
        }
    }
    if let Some(base) = dirs::config_dir() {
        for app in ["RE-KORD Server", "RE-KORD", "rekord", "Kord", "kord"] {
            dirs.push(base.join(app));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for rel in ["server", "../server", "../../server"] {
            dirs.push(cwd.join(rel));
        }
    }
    dirs.into_iter()
        .map(|d| d.join(LEGACY_CONFIG_FILE))
        .collect()
}

pub fn find_legacy_config_file() -> Option<PathBuf> {
    legacy_config_candidates().into_iter().find(|p| p.is_file())
}

fn read_token(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let t = raw.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Fill hub settings that are still empty from a legacy config. `config_dir`
/// is where the legacy file lived (default location of `discogs-token`);
/// `zip_discogs_token` is a token shipped inside a backup.
pub fn apply_legacy_config(
    cfg: &mut AppConfig,
    legacy: &LegacyConfig,
    config_dir: Option<&Path>,
    zip_discogs_token: Option<&str>,
) -> LegacyConfigImport {
    let mut out = LegacyConfigImport::default();

    if cfg.discogs_token.is_none() && !cfg.discogs_token_from_env {
        let token = zip_discogs_token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .or_else(|| legacy.discogs_token_path.as_deref().and_then(read_token))
            .or_else(|| config_dir.and_then(|d| read_token(&d.join(LEGACY_DISCOGS_TOKEN_FILE))));
        if let Some(token) = token {
            match cfg.set_discogs_token(&token) {
                Ok(()) => out.discogs_token = true,
                Err(e) => warn!(error = %e, "could not import legacy Discogs token"),
            }
        }
    }

    if legacy.cloudflare_logged_in
        && !crate::remote_access::load_cloudflare_logged_in(&cfg.data_dir)
    {
        match crate::remote_access::set_cloudflare_logged_in(&cfg.data_dir, true) {
            Ok(()) => out.cloudflare_logged_in = true,
            Err(e) => warn!(error = %e, "could not import legacy Cloudflare login state"),
        }
    }

    if cfg.youtube_cookies_path.is_none() && !cfg.youtube_cookies_from_env {
        let cookies = legacy
            .youtube_cookies_path
            .clone()
            .or_else(|| config_dir.map(|d| d.join("youtube-cookies.txt")))
            .filter(|p| p.is_file());
        if let Some(src) = cookies {
            match fs::read(&src)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| cfg.set_youtube_cookies_bytes(&bytes))
            {
                Ok(_) => out.youtube_cookies = true,
                Err(e) => {
                    warn!(error = %e, path = %src.display(), "could not import legacy YouTube cookies")
                }
            }
        }
    }
    out
}

/// One-shot in-place import at startup (marker in `library_meta`). Returns
/// `None` when it already ran or no legacy config was found.
pub fn import_legacy_config_in_place(
    cfg: &mut AppConfig,
    db: &Db,
) -> Result<Option<LegacyConfigImport>> {
    // Throwaway hubs (tests, previews under the temp dir) must not pick up the
    // credentials of whatever legacy install lives on the machine.
    let opted_out = std::env::var("REKORD_SKIP_LEGACY_CONFIG_IMPORT")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false);
    if opted_out || cfg.data_dir.starts_with(std::env::temp_dir()) {
        return Ok(None);
    }
    if db.get_meta(IMPORTED_META_KEY)?.is_some() {
        return Ok(None);
    }
    let Some(path) = find_legacy_config_file() else {
        return Ok(None);
    };
    let result = import_legacy_config_from(cfg, &path);
    // Mark even when nothing was filled: values cleared in next later must
    // not come back from the legacy file on the next start.
    db.set_meta(IMPORTED_META_KEY, &chrono::Utc::now().to_rfc3339())?;
    Ok(result)
}

pub fn import_legacy_config_from(cfg: &mut AppConfig, path: &Path) -> Option<LegacyConfigImport> {
    let raw = fs::read_to_string(path).ok()?;
    let legacy = parse_legacy_config(&raw)?;
    let mut out = apply_legacy_config(cfg, &legacy, path.parent(), None);
    out.source = Some(path.to_string_lossy().into_owned());
    if out.discogs_token || out.cloudflare_logged_in || out.youtube_cookies {
        info!(
            source = %path.display(),
            discogs_token = out.discogs_token,
            cloudflare_logged_in = out.cloudflare_logged_in,
            youtube_cookies = out.youtube_cookies,
            "imported legacy server settings"
        );
    }
    Some(out)
}

/// A restored `settings.json` must not grant remote admin on this machine by
/// itself: the flag is reset and the reset reported.
pub fn sanitize_restored_settings(raw: &str) -> (String, bool) {
    let Ok(mut v) = serde_json::from_str::<Value>(raw) else {
        return (raw.to_string(), false);
    };
    let Some(obj) = v.as_object_mut() else {
        return (raw.to_string(), false);
    };
    let was_enabled = obj.get("allow_remote_admin").and_then(|x| x.as_bool()) == Some(true);
    if obj.contains_key("allow_remote_admin") {
        obj.insert("allow_remote_admin".into(), Value::Bool(false));
    }
    let body = serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_string());
    (body, was_enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn temp_dir(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("rekord-legacy-cfg-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn cfg_in(dir: &Path) -> AppConfig {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        AppConfig::resolve(Some(dir.join("data")), bind, None)
    }

    #[test]
    fn parses_the_legacy_keys() {
        let c = parse_legacy_config(
            r#"{"musicRoot":"/m","schemaVersion":3,"cloudflareLoggedIn":true,
                "youtubeCookiesPath":"/c/youtube-cookies.txt","discogsTokenPath":" "}"#,
        )
        .unwrap();
        assert_eq!(c.music_root, Some(PathBuf::from("/m")));
        assert!(c.cloudflare_logged_in);
        assert_eq!(c.discogs_token_path, None);
        assert_eq!(
            c.youtube_cookies_path,
            Some(PathBuf::from("/c/youtube-cookies.txt"))
        );
    }

    #[test]
    fn imports_token_and_login_once_and_never_overrides_the_hub() {
        let dir = temp_dir("import");
        let legacy_dir = dir.join("legacy");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("discogs-token"), "  tok-123\n").unwrap();
        fs::write(
            legacy_dir.join(LEGACY_CONFIG_FILE),
            r#"{"musicRoot":"/m","cloudflareLoggedIn":true}"#,
        )
        .unwrap();
        let mut cfg = cfg_in(&dir);
        cfg.ensure_dirs().unwrap();

        let out =
            import_legacy_config_from(&mut cfg, &legacy_dir.join(LEGACY_CONFIG_FILE)).unwrap();
        assert!(out.discogs_token);
        assert!(out.cloudflare_logged_in);
        assert_eq!(cfg.discogs_token.as_deref(), Some("tok-123"));
        assert!(crate::remote_access::load_cloudflare_logged_in(
            &cfg.data_dir
        ));

        // The hub now has its own token: a different legacy one is ignored.
        fs::write(legacy_dir.join("discogs-token"), "other").unwrap();
        let again =
            import_legacy_config_from(&mut cfg, &legacy_dir.join(LEGACY_CONFIG_FILE)).unwrap();
        assert!(!again.discogs_token);
        assert_eq!(cfg.discogs_token.as_deref(), Some("tok-123"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restored_settings_never_enable_remote_admin() {
        let (body, was) =
            sanitize_restored_settings(r#"{"music_root":"/m","allow_remote_admin":true}"#);
        assert!(was);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["allow_remote_admin"], Value::Bool(false));
        assert_eq!(v["music_root"], Value::from("/m"));

        let (_, was) = sanitize_restored_settings(r#"{"music_root":"/m"}"#);
        assert!(!was);
    }
}
