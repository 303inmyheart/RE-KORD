import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import UpdateBanner from "./components/UpdateBanner.svelte";
import { i18n, t } from "./lib/i18n.svelte";
import { watchHubCompat } from "./lib/platform/compatState.svelte";
import { installDownloadBridge, onDownloadSaved } from "./lib/platform/downloads";
import { installExternalLinkHandler } from "./lib/platform/externalLinks";
import { registerServiceWorker } from "./lib/platform/pwa";
import { toasts } from "./lib/toasts.svelte";
import { applyTheme, loadUserPrefs } from "./lib/userPrefs";

// `index.html` ships `lang="it"`: fix `<html lang>` from the saved locale first
// thing, before the theme and before anything is mounted (no first paint yet).
i18n.applySaved();

const prefs = loadUserPrefs();
applyTheme(prefs.theme, prefs.customTheme, {
  glassSurfaces: prefs.glassSurfaces,
  glassOpacity: prefs.glassOpacity,
});

// Solo nei gusci Tauri (le funzioni lo controllano da sole): link esterni nel
// browser di sistema, download generati nella pagina salvati come file veri.
installExternalLinkHandler();
installDownloadBridge();
onDownloadSaved((outcome) => {
  if (outcome.status === "saved") toasts.ok(t("platform.download.saved", { path: outcome.path }));
  else if (outcome.status === "error") {
    toasts.error(t("platform.download.failed", { error: outcome.message }));
  }
});

// Solo nel browser servito dall'hub, in contesto sicuro.
registerServiceWorker();

mount(App, { target: document.getElementById("app")! });

// Avviso di compatibilita' client/hub in una radice sua, sopra qualunque schermata.
const bannerHost = document.createElement("div");
bannerHost.id = "rk-update-root";
document.body.appendChild(bannerHost);
mount(UpdateBanner, { target: bannerHost });
// Il primo controllo lo fa la procedura di connessione (connect.svelte.ts); qui
// solo i successivi, perche' l'hub puo' essere aggiornato con l'app aperta.
watchHubCompat();
