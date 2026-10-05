//! Local multi-account registry (parity with old `.kord/global_info/accounts.json`).
//!
//! Layout under `data_dir`:
//! - `accounts.json` — registry
//! - `accounts/{id}/library-selection.json` — per-account library selection
//! - `accounts/{id}_info/` — user-state, custom theme background
//!
//! Registry reads-modify-writes are serialised per data dir and written
//! atomically (unique temp file, fsync, rename), so concurrent creates never
//! lose entries.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use uuid::Uuid;

pub const DEFAULT_ACCOUNT_ID: &str = "default";
/// Legacy name of the default account (kept when the registry is created;
/// names chosen by the user are never rewritten).
pub const DEFAULT_ACCOUNT_NAME: &str = "Default";

/// Per-data-dir registry lock.
fn registry_lock(data_dir: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let key = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let mut map = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn lock_guard(lock: &Mutex<()>) -> MutexGuard<'_, ()> {
    lock.lock().unwrap_or_else(|e| e.into_inner())
}
const ACCOUNTS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountsFile {
    schema_version: u32,
    accounts: Vec<Account>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSnapshot {
    pub default_account_id: String,
    pub accounts: Vec<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_account_id: Option<String>,
}

/// Longest accepted account id (UUIDs are 36 chars).
pub const MAX_ACCOUNT_ID_LEN: usize = 64;

/// Strict account id shape: `[A-Za-z0-9_-]{1,64}`. Ids end up in file paths
/// (`accounts/{id}`, `accounts/{id}_info`), so nothing else is accepted.
pub fn is_valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ACCOUNT_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Registry normalisation: map stray characters to `_` (never `.`/`/`), so a
/// hand-edited registry cannot produce a path outside `accounts/`.
fn safe_account_id(account_id: &str) -> Option<String> {
    let id = account_id.trim();
    if id.is_empty() {
        return None;
    }
    let safe: String = id
        .chars()
        .take(MAX_ACCOUNT_ID_LEN)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if is_valid_account_id(&safe) {
        Some(safe)
    } else {
        None
    }
}

/// Why a client-supplied account id was refused.
#[derive(Debug, thiserror::Error)]
pub enum AccountIdError {
    #[error("invalid account id")]
    Invalid,
    #[error("account not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AccountIdError {
    /// HTTP status for the error (400 malformed, 404 unknown, 500 I/O).
    pub fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::Invalid => axum::http::StatusCode::BAD_REQUEST,
            Self::NotFound => axum::http::StatusCode::NOT_FOUND,
            Self::Other(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable machine-readable code for the JSON envelope.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid => "invalid_account_id",
            Self::NotFound => "account_not_found",
            Self::Other(_) => "account_lookup_failed",
        }
    }
}

/// Resolve a requested id against the registry: absent → `default`, malformed
/// → `Invalid`, unknown → `NotFound` (no silent fallback to Default, which is
/// the account allowed to run machine operations).
pub fn lookup_account_id(
    data_dir: &Path,
    requested: Option<&str>,
) -> std::result::Result<String, AccountIdError> {
    let Some(id) = requested.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(DEFAULT_ACCOUNT_ID.to_string());
    };
    if !is_valid_account_id(id) {
        return Err(AccountIdError::Invalid);
    }
    if id == DEFAULT_ACCOUNT_ID {
        return Ok(id.to_string());
    }
    let accounts = ensure_accounts(data_dir)?;
    if accounts.iter().any(|a| a.id == id) {
        Ok(id.to_string())
    } else {
        Err(AccountIdError::NotFound)
    }
}

/// Request account from query/header, validated against the registry.
pub fn account_from_request(
    data_dir: &Path,
    headers: &axum::http::HeaderMap,
    query_account_id: Option<&str>,
) -> std::result::Result<String, AccountIdError> {
    let requested = account_id_from_headers_and_query(headers, query_account_id);
    lookup_account_id(data_dir, requested.as_deref())
}

fn clean_account_name(value: &str, fallback: &str) -> String {
    let t = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.trim();
    if t.is_empty() {
        fallback.to_string()
    } else {
        t.chars().take(80).collect()
    }
}

pub fn accounts_registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("accounts.json")
}

pub fn account_dir(data_dir: &Path, account_id: &str) -> Option<PathBuf> {
    safe_account_id(account_id).map(|id| data_dir.join("accounts").join(id))
}

pub fn account_library_selection_path(data_dir: &Path, account_id: &str) -> Option<PathBuf> {
    account_dir(data_dir, account_id).map(|d| d.join("library-selection.json"))
}

fn normalize_accounts(raw: &[Account]) -> Vec<Account> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in raw {
        let id = safe_account_id(&item.id).unwrap_or_else(|| Uuid::new_v4().to_string());
        if !seen.insert(id.clone()) {
            continue;
        }
        let fallback = if id == DEFAULT_ACCOUNT_ID {
            DEFAULT_ACCOUNT_NAME
        } else {
            "Account"
        };
        out.push(Account {
            id,
            name: clean_account_name(&item.name, fallback),
        });
    }
    out
}

/// Atomic write (caller holds the registry lock).
fn write_accounts_file(data_dir: &Path, accounts: &[Account]) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(data_dir)?;
    let path = accounts_registry_path(data_dir);
    let body = AccountsFile {
        schema_version: ACCOUNTS_SCHEMA,
        accounts: accounts.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&body)?;
    let tmp = data_dir.join(format!(".accounts.{}.tmp", Uuid::new_v4().simple()));
    let res = (|| -> Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(raw.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &path)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

fn read_accounts_file(data_dir: &Path) -> Result<Option<Vec<Account>>> {
    let path = accounts_registry_path(data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed: AccountsFile = serde_json::from_str(&raw).unwrap_or(AccountsFile {
        schema_version: ACCOUNTS_SCHEMA,
        accounts: Vec::new(),
    });
    let list = normalize_accounts(&parsed.accounts);
    if list.is_empty() {
        Ok(None)
    } else {
        Ok(Some(list))
    }
}

/// Ensure registry + default account dir exist; migrate legacy global selection once.
pub fn ensure_accounts(data_dir: &Path) -> Result<Vec<Account>> {
    fs::create_dir_all(data_dir)?;
    let lock = registry_lock(data_dir);
    let _guard = lock_guard(&lock);
    ensure_accounts_locked(data_dir)
}

fn ensure_accounts_locked(data_dir: &Path) -> Result<Vec<Account>> {
    let mut accounts = match read_accounts_file(data_dir)? {
        Some(list) => list,
        None => {
            let def = vec![Account {
                id: DEFAULT_ACCOUNT_ID.to_string(),
                name: DEFAULT_ACCOUNT_NAME.to_string(),
            }];
            write_accounts_file(data_dir, &def)?;
            def
        }
    };

    // Always keep a default id present as first account when missing.
    if !accounts.iter().any(|a| a.id == DEFAULT_ACCOUNT_ID) {
        accounts.insert(
            0,
            Account {
                id: DEFAULT_ACCOUNT_ID.to_string(),
                name: DEFAULT_ACCOUNT_NAME.to_string(),
            },
        );
        write_accounts_file(data_dir, &accounts)?;
    }

    for acc in &accounts {
        ensure_account_layout(data_dir, &acc.id, acc.id == DEFAULT_ACCOUNT_ID)?;
    }

    migrate_legacy_global_selection(data_dir)?;
    Ok(accounts)
}

fn ensure_account_layout(data_dir: &Path, account_id: &str, is_default: bool) -> Result<()> {
    let Some(dir) = account_dir(data_dir, account_id) else {
        bail!("invalid account id");
    };
    fs::create_dir_all(&dir)?;
    let Some(sel_path) = account_library_selection_path(data_dir, account_id) else {
        bail!("invalid account id");
    };
    if !sel_path.exists() {
        // New non-default accounts start empty; default gets includeAll when no prior file.
        let body = if is_default {
            serde_json::json!({
                "version": 1,
                "includeAll": true,
                "artists": [],
                "albums": [],
                "tracks": []
            })
        } else {
            serde_json::json!({
                "version": 1,
                "includeAll": false,
                "artists": [],
                "albums": [],
                "tracks": []
            })
        };
        let tmp = sel_path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&body)?)?;
        fs::rename(&tmp, &sel_path)?;
    }
    Ok(())
}

/// Move `data_dir/library-selection.json` → default account dir (once).
fn migrate_legacy_global_selection(data_dir: &Path) -> Result<()> {
    let legacy = data_dir.join("library-selection.json");
    if !legacy.is_file() {
        return Ok(());
    }
    let Some(dest) = account_library_selection_path(data_dir, DEFAULT_ACCOUNT_ID) else {
        return Ok(());
    };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    // Prefer legacy content (existing user data) over the bootstrap empty/default file.
    if dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::rename(&legacy, &dest).or_else(|_| {
        fs::copy(&legacy, &dest)?;
        fs::remove_file(&legacy)?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

pub fn get_accounts_snapshot(data_dir: &Path) -> Result<AccountsSnapshot> {
    let accounts = ensure_accounts(data_dir)?;
    Ok(AccountsSnapshot {
        default_account_id: DEFAULT_ACCOUNT_ID.to_string(),
        accounts,
        created_account_id: None,
    })
}

/// Replace the accounts registry (used by backup restore). Always keeps `default`.
pub fn replace_accounts_registry(data_dir: &Path, accounts: &[Account]) -> Result<Vec<Account>> {
    fs::create_dir_all(data_dir)?;
    let lock = registry_lock(data_dir);
    let _guard = lock_guard(&lock);
    let mut list = normalize_accounts(accounts);
    if list.is_empty() {
        list.push(Account {
            id: DEFAULT_ACCOUNT_ID.to_string(),
            name: DEFAULT_ACCOUNT_NAME.to_string(),
        });
    } else if !list.iter().any(|a| a.id == DEFAULT_ACCOUNT_ID) {
        list.insert(
            0,
            Account {
                id: DEFAULT_ACCOUNT_ID.to_string(),
                name: DEFAULT_ACCOUNT_NAME.to_string(),
            },
        );
    }
    write_accounts_file(data_dir, &list)?;
    for acc in &list {
        ensure_account_layout(data_dir, &acc.id, acc.id == DEFAULT_ACCOUNT_ID)?;
    }
    Ok(list)
}

/// Parse accounts list from legacy/next JSON (`{ accounts: [...] }` or bare array).
pub fn accounts_from_json_value(v: &serde_json::Value) -> Vec<Account> {
    let arr = v
        .get("accounts")
        .and_then(|x| x.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();
    let mut raw = Vec::new();
    for item in arr {
        let id = item
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let name = item
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("Account")
            .to_string();
        if id.trim().is_empty() {
            continue;
        }
        raw.push(Account { id, name });
    }
    normalize_accounts(&raw)
}

/// Absent id → `default`; malformed or unknown ids are errors (see
/// [`lookup_account_id`]).
pub fn resolve_account_id(data_dir: &Path, requested: Option<&str>) -> Result<String> {
    lookup_account_id(data_dir, requested).map_err(|e| anyhow::anyhow!(e.to_string()))
}

pub fn create_account(data_dir: &Path, name: &str) -> Result<AccountsSnapshot> {
    fs::create_dir_all(data_dir)?;
    let lock = registry_lock(data_dir);
    let _guard = lock_guard(&lock);
    let mut accounts = ensure_accounts_locked(data_dir)?;
    let id = Uuid::new_v4().to_string();
    let account = Account {
        id: id.clone(),
        name: clean_account_name(name, "Nuovo account"),
    };
    ensure_account_layout(data_dir, &account.id, false)?;
    accounts.push(account);
    write_accounts_file(data_dir, &accounts)?;
    Ok(AccountsSnapshot {
        default_account_id: DEFAULT_ACCOUNT_ID.to_string(),
        accounts,
        created_account_id: Some(id),
    })
}

pub fn update_account(data_dir: &Path, id: &str, name: Option<&str>) -> Result<AccountsSnapshot> {
    if !is_valid_account_id(id.trim()) {
        bail!("invalid account id");
    }
    let lock = registry_lock(data_dir);
    let _guard = lock_guard(&lock);
    let mut accounts = ensure_accounts_locked(data_dir)?;
    let account = accounts
        .iter_mut()
        .find(|a| a.id == id.trim())
        .with_context(|| format!("account not found: {id}"))?;
    if let Some(n) = name {
        account.name = clean_account_name(n, &account.name);
    }
    write_accounts_file(data_dir, &accounts)?;
    Ok(AccountsSnapshot {
        default_account_id: DEFAULT_ACCOUNT_ID.to_string(),
        accounts,
        created_account_id: None,
    })
}

pub fn delete_account(data_dir: &Path, id: &str) -> Result<AccountsSnapshot> {
    let account_id = id.trim();
    if !is_valid_account_id(account_id) {
        bail!("invalid account id");
    }
    if account_id == DEFAULT_ACCOUNT_ID {
        bail!("cannot remove the default account");
    }
    let lock = registry_lock(data_dir);
    let _guard = lock_guard(&lock);
    let mut accounts = ensure_accounts_locked(data_dir)?;
    if !accounts.iter().any(|a| a.id == account_id) {
        bail!("account not found");
    }
    if accounts.len() <= 1 {
        bail!("keep at least one account");
    }
    accounts.retain(|a| a.id != account_id);
    write_accounts_file(data_dir, &accounts)?;
    remove_account_files(data_dir, account_id);
    Ok(AccountsSnapshot {
        default_account_id: DEFAULT_ACCOUNT_ID.to_string(),
        accounts,
        created_account_id: None,
    })
}

/// Per-account folders: `accounts/{id}` (library selection) and
/// `accounts/{id}_info` (user-state, theme background). The id is validated,
/// so neither path can leave `accounts/`.
fn remove_account_files(data_dir: &Path, account_id: &str) {
    if !is_valid_account_id(account_id) || account_id == DEFAULT_ACCOUNT_ID {
        return;
    }
    let base = data_dir.join("accounts");
    for dir in [
        base.join(account_id),
        base.join(format!("{account_id}_info")),
    ] {
        if dir.exists() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                tracing::warn!(path = %dir.display(), error = %e, "account folder not removed");
            }
        }
    }
}

/// Account id from query `accountId` or headers (compat with old client).
pub fn account_id_from_headers_and_query(
    headers: &axum::http::HeaderMap,
    query_account_id: Option<&str>,
) -> Option<String> {
    if let Some(q) = query_account_id.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(q.to_string());
    }
    for name in ["x-rekord-account-id", "x-kord-account-id"] {
        if let Some(v) = headers.get(name).and_then(|h| h.to_str().ok()) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Hub-wide integrations / remote-access mutations are owned by the default account.
pub fn is_default_account_id(id: &str) -> bool {
    id.trim() == DEFAULT_ACCOUNT_ID
}

/// Resolve the request account and require it to be the default (`default`).
pub fn require_default_account(
    data_dir: &Path,
    headers: &axum::http::HeaderMap,
    query_account_id: Option<&str>,
) -> Result<String> {
    let id = account_from_request(data_dir, headers, query_account_id)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if !is_default_account_id(&id) {
        bail!("only the default account can manage this setting");
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("rekord-accounts-{}", Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn account_ids_are_strict() {
        assert!(is_valid_account_id("default"));
        assert!(is_valid_account_id("8c703652-df64-430f-aa66-970d68e82dce"));
        assert!(!is_valid_account_id("../../x"));
        assert!(!is_valid_account_id("a.b"));
        assert!(!is_valid_account_id(""));
        assert!(!is_valid_account_id(&"a".repeat(65)));
        assert_eq!(safe_account_id("../x").as_deref(), Some("___x"));
    }

    #[test]
    fn delete_removes_account_folders() {
        let dir = temp_dir();
        let snap = create_account(&dir, "Ospite").unwrap();
        let id = snap.created_account_id.unwrap();
        let info = dir.join("accounts").join(format!("{id}_info"));
        fs::create_dir_all(&info).unwrap();
        fs::write(info.join("user-state.json"), "{}").unwrap();
        fs::write(info.join("theme-bg.jpg"), b"x").unwrap();
        assert!(dir.join("accounts").join(&id).is_dir());
        delete_account(&dir, &id).unwrap();
        assert!(!info.exists());
        assert!(!dir.join("accounts").join(&id).exists());
        assert!(dir.join("accounts").join("default").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_account_keeps_legacy_name() {
        let dir = temp_dir();
        let list = ensure_accounts(&dir).unwrap();
        assert_eq!(list[0].name, "Default");
        // A name the user chose stays as is.
        update_account(&dir, "default", Some("Locale")).unwrap();
        assert_eq!(ensure_accounts(&dir).unwrap()[0].name, "Locale");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_ids_do_not_fall_back_to_default() {
        let dir = temp_dir();
        assert_eq!(lookup_account_id(&dir, None).unwrap(), "default");
        assert!(matches!(
            lookup_account_id(&dir, Some("../../etc")),
            Err(AccountIdError::Invalid)
        ));
        assert!(matches!(
            lookup_account_id(&dir, Some("nobody")),
            Err(AccountIdError::NotFound)
        ));
        let snap = create_account(&dir, "Ospite").unwrap();
        let id = snap.created_account_id.unwrap();
        assert_eq!(lookup_account_id(&dir, Some(&id)).unwrap(), id);
        let _ = fs::remove_dir_all(&dir);
    }
}
