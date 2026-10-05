//! «Salva con nome» per i file che la pagina costruisce in memoria (backup
//! dell'hub, profilo, pacchetto tema).
//!
//! Nel browser basta `<a download href="blob:...">`; nelle webview di sistema
//! (WebKitGTK, WKWebView) quel click non produce niente. La pagina manda qui i
//! byte come corpo grezzo dell'IPC e il nome nell'intestazione `x-rekord-filename`
//! (percent-encoded), e qui si chiede dove salvare.

use std::path::PathBuf;
use tauri::ipc::{InvokeBody, Request};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const NAME_HEADER: &str = "x-rekord-filename";

/// Torna il percorso scritto, oppure `None` se l'utente ha annullato.
#[tauri::command]
pub async fn save_download(app: AppHandle, request: Request<'_>) -> Result<Option<String>, String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected a raw body".into());
    };
    let name = request
        .headers()
        .get(NAME_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .map(|n| sanitize_file_name(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "rekord-download".to_string());

    let mut dialog = app.dialog().file().set_file_name(&name);
    if let Some(dir) = default_dir() {
        dialog = dialog.set_directory(dir);
    }
    // blocking_* va bene qui: i comandi async girano sul runtime, non sul thread
    // della finestra, che resta libero di disegnare il dialogo.
    let Some(target) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = target.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn default_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| {
            let downloads = home.join("Downloads");
            if downloads.is_dir() {
                downloads
            } else {
                home
            }
        })
}

/// Solo il nome: niente cartelle, niente caratteri che Windows rifiuta.
fn sanitize_file_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    base.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string()
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_sanitizes() {
        assert_eq!(percent_decode("rekord%20backup.zip"), "rekord backup.zip");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("a:b?.zip"), "a_b_.zip");
    }
}
