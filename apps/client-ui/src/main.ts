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
import { initLayoutWidth } from "./lib/layoutWidth";
import { applyTheme, loadUserPrefs } from "./lib/userPrefs";

// `index.html` ships `lang="it"`: fix `<html lang>` from the saved locale first
// thing, before the theme and before anything is mounted (no first paint yet).
i18n.applySaved();

const prefs = loadUserPrefs();
applyTheme(prefs.theme, prefs.customTheme, {
  glassSurfaces: prefs.glassSurfaces,
  glassOpacity: prefs.glassOpacity,
});
// Desktop content / player widths of this device (CSS variables, no layout work).
initLayoutWidth();

// Only in the Tauri shells (the functions check this themselves): external links in the
// system browser, downloads generated in the page saved as real files.
installExternalLinkHandler();
installDownloadBridge();
onDownloadSaved((outcome) => {
  if (outcome.status === "saved") toasts.ok(t("platform.download.saved", { path: outcome.path }));
  else if (outcome.status === "error") {
    toasts.error(t("platform.download.failed", { error: outcome.message }));
  }
});

// Only in the browser served by the hub, in a secure context.
registerServiceWorker();

mount(App, { target: document.getElementById("app")! });

// Client/hub compatibility notice in its own root, above any screen.
const bannerHost = document.createElement("div");
bannerHost.id = "rk-update-root";
document.body.appendChild(bannerHost);
mount(UpdateBanner, { target: bannerHost });
// The first check is done by the connection flow (connect.svelte.ts); here
// only the following ones, because the hub can be updated while the app is open.
watchHubCompat();
