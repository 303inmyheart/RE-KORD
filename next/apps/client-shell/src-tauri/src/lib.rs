//! Guscio Tauri del client RE-KORD.
//!
//! L'interfaccia e' `apps/client-ui` impacchettata nell'app; l'hub e' un indirizzo
//! HTTP(S) scelto al primo avvio. Qui stanno solo le cose che la pagina non sa
//! fare da sola: aprire i link nel browser di sistema (plugin opener), salvare un
//! file generato nella pagina (desktop), tenere una sola istanza aperta (desktop),
//! leggere un QR (telefono) e — con la feature `hub` — far girare l'hub stesso.

#[cfg(desktop)]
mod downloads;
#[cfg(all(desktop, feature = "hub"))]
mod embedded_hub;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Va registrato per primo: il secondo processo deve uscire prima di
    // inizializzare il resto (e, con l'hub incorporato, prima di tentare la porta).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        focus_main_window(app);
    }));

    let builder = builder.plugin(tauri_plugin_opener::init());

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![downloads::save_download]);

    // La fotocamera serve solo per leggere il QR dell'hub al primo avvio, e solo
    // dove una fotocamera c'e': sul desktop l'indirizzo si scrive con la tastiera.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    #[cfg(all(desktop, feature = "hub"))]
    let builder = builder.setup(|app| {
        embedded_hub::start(app.handle());
        Ok(())
    });

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building RE-KORD client");

    app.run(|_app, _event| {
        #[cfg(all(desktop, feature = "hub"))]
        if let tauri::RunEvent::Exit = _event {
            embedded_hub::stop(_app);
        }
    });
}

#[cfg(desktop)]
fn focus_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
