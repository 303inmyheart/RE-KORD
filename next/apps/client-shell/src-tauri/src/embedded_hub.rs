//! Hub incorporato ("server flavor", feature `hub`, solo desktop).
//!
//! L'app Electron "Server" della 5.0 avviava l'hub insieme alla finestra. Qui
//! `rekord_core::run_hub` gira su un runtime tokio in un thread suo, cosi' la
//! finestra non aspetta mai l'hub e l'hub non dipende dal ciclo degli eventi.
//! Il client nella finestra lo trova da solo: all'avvio prova gia'
//! `http://127.0.0.1:7420`.
//!
//! Configurazione, in ordine di precedenza:
//! 1. variabili d'ambiente `REKORD_EMBEDDED_HUB=0` (spegne), `REKORD_BIND`,
//!    `REKORD_DATA_DIR` (stessi nomi di `rekord-server`);
//! 2. `<app data dir>/hub.json` → `{ "enabled": true, "bind": "0.0.0.0:7420",
//!    "dataDir": null }`, creato con questi valori al primo avvio;
//! 3. predefiniti: `0.0.0.0:7420` (raggiungibile in LAN, come l'app 5.0) e dati
//!    in `<app data dir>/hub`.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

const DEFAULT_BIND: &str = "0.0.0.0:7420";
const CONFIG_FILE: &str = "hub.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubFileConfig {
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default)]
    data_dir: Option<PathBuf>,
}

fn default_enabled() -> bool {
    true
}

fn default_bind() -> String {
    DEFAULT_BIND.to_string()
}

impl Default for HubFileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: default_bind(),
            data_dir: None,
        }
    }
}

struct Running {
    shutdown: Option<oneshot::Sender<()>>,
    done: std::sync::mpsc::Receiver<()>,
}

#[derive(Default)]
struct HubHandle(Mutex<Option<Running>>);

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn load_config(app_data: &Path) -> HubFileConfig {
    let path = app_data.join(CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            eprintln!(
                "[rekord] {} non valido ({e}): uso i valori predefiniti",
                path.display()
            );
            HubFileConfig::default()
        }),
        Err(_) => {
            let cfg = HubFileConfig::default();
            // Scritto subito: chi vuole cambiare porta o cartella trova il file
            // gia' pronto invece di doverne indovinare il formato.
            if std::fs::create_dir_all(app_data).is_ok() {
                if let Ok(raw) = serde_json::to_string_pretty(&cfg) {
                    let _ = std::fs::write(&path, raw);
                }
            }
            cfg
        }
    }
}

/// Interfaccia impacchettata come risorsa (`tauri.hub.conf.json`), se c'e':
/// `admin-ui` per il pannello su /admin, `client-ui` per i telefoni e i browser
/// in LAN o dal tunnel che aprono http://<pc>:7420/.
fn bundled_ui(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join(name);
    dir.join("index.html").is_file().then_some(dir)
}

/// yt-dlp, cloudflared e ffmpeg impacchettati in `bin/` (pnpm pack:*:server):
/// l'hub li cerca tramite le stesse variabili d'ambiente di `rekord-server`, che
/// restano prioritarie se l'utente le ha gia' impostate.
fn export_bundled_tools(app: &AppHandle) {
    let Ok(bin) = app.path().resource_dir().map(|d| d.join("bin")) else {
        return;
    };
    let exe = |name: &str| {
        let file = if cfg!(windows) {
            bin.join(format!("{name}.exe"))
        } else {
            bin.join(name)
        };
        file.is_file().then_some(file)
    };
    for (var, name) in [
        ("YTDLP_PATH", "yt-dlp"),
        ("REKORD_CLOUDFLARED_BIN", "cloudflared"),
        ("REKORD_FFMPEG", "ffmpeg"),
    ] {
        if env_nonempty(var).is_some() {
            continue;
        }
        if let Some(path) = exe(name) {
            std::env::set_var(var, path);
        }
    }
}

pub fn start(app: &AppHandle) {
    app.manage(HubHandle::default());

    let app_data = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!(
                "[rekord] hub incorporato non avviato: cartella dati dell'app sconosciuta ({e})"
            );
            return;
        }
    };
    let file = load_config(&app_data);
    let enabled = match env_nonempty("REKORD_EMBEDDED_HUB") {
        Some(v) => !matches!(v.as_str(), "0" | "false" | "off"),
        None => file.enabled,
    };
    if !enabled {
        eprintln!("[rekord] hub incorporato spento (hub.json / REKORD_EMBEDDED_HUB)");
        return;
    }

    let bind_raw = env_nonempty("REKORD_BIND").unwrap_or(file.bind.clone());
    let bind: SocketAddr = match bind_raw.parse() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("[rekord] indirizzo hub non valido «{bind_raw}» ({e}): uso {DEFAULT_BIND}");
            DEFAULT_BIND.parse().expect("default bind")
        }
    };
    let data_dir = env_nonempty("REKORD_DATA_DIR")
        .map(PathBuf::from)
        .or(file.data_dir.clone())
        .unwrap_or_else(|| app_data.join("hub"));

    let opts = rekord_core::HubOptions {
        bind,
        data_dir,
        // La finestra usa la copia di Tauri; questa e' per gli altri dispositivi.
        client_ui_dir: bundled_ui(app, "client-ui"),
        admin_ui_dir: bundled_ui(app, "admin-ui"),
    };
    // Prima di avviare il runtime dell'hub: nessun altro thread dell'hub legge
    // ancora l'ambiente.
    export_bundled_tools(app);

    let (tx, rx) = oneshot::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let spawned = std::thread::Builder::new()
        .name("rekord-hub".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("rekord-hub-worker")
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[rekord] runtime dell'hub non creato: {e}");
                    let _ = done_tx.send(());
                    return;
                }
            };
            let shutdown = async move {
                let _ = rx.await;
            };
            if let Err(e) = runtime.block_on(rekord_core::run_hub(opts, shutdown)) {
                // Tipicamente la porta e' gia' occupata da un rekord-server
                // standalone: il client si colleghera' a quello.
                eprintln!("[rekord] hub incorporato fermo: {e:#}");
            }
            runtime.shutdown_timeout(Duration::from_secs(2));
            let _ = done_tx.send(());
        });

    match spawned {
        Ok(_) => {
            if let Some(handle) = app.try_state::<HubHandle>() {
                *handle.0.lock().unwrap() = Some(Running {
                    shutdown: Some(tx),
                    done: done_rx,
                });
            }
        }
        Err(e) => eprintln!("[rekord] thread dell'hub non avviato: {e}"),
    }
}

/// Chiusura ordinata all'uscita dell'app: niente DB lasciato a meta' scrittura.
pub fn stop(app: &AppHandle) {
    let Some(handle) = app.try_state::<HubHandle>() else {
        return;
    };
    let Some(mut running) = handle.0.lock().unwrap().take() else {
        return;
    };
    if let Some(tx) = running.shutdown.take() {
        let _ = tx.send(());
    }
    if running.done.recv_timeout(Duration::from_secs(8)).is_err() {
        eprintln!("[rekord] l'hub non si e' fermato in tempo: chiusura forzata");
    }
}
