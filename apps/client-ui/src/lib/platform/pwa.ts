/**
 * PWA for the client served by the hub (browser): manifest in `public/`, service
 * worker generated at build time (`vite.config.ts`). Never in the Tauri shells, which have
 * the UI already local and an origin (`tauri://`) where service workers
 * are not needed; never in development, where a cache would confuse HMR.
 *
 * Service workers exist only in secure contexts: HTTPS (Cloudflare tunnel,
 * reverse proxy) and localhost. On `http://192.168.x.x:7420` the browser does not
 * offer them: the app works the same, just without install/offline startup.
 */

import { isTauri } from "./env";

export function pwaSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    !isTauri() &&
    import.meta.env.PROD &&
    window.isSecureContext &&
    "serviceWorker" in navigator
  );
}

export function registerServiceWorker(): void {
  if (!pwaSupported()) return;
  const register = () => {
    navigator.serviceWorker
      .register("/sw.js", { scope: "/" })
      .catch((e) => console.warn("[rekord] service worker not registered", e));
  };
  // After load: registration must not compete for the network on first startup.
  if (document.readyState === "complete") register();
  else window.addEventListener("load", register, { once: true });
}

/** Asks the service worker to re-download the shell, then reloads the page. */
export async function reloadWithFreshShell(): Promise<void> {
  try {
    if (pwaSupported()) {
      const reg = await navigator.serviceWorker.getRegistration();
      await reg?.update();
    }
  } catch {
    /* reload anyway */
  }
  location.reload();
}
