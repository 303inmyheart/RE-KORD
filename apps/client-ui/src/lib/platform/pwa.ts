/**
 * PWA per il client servito dall'hub (browser): manifest in `public/`, service
 * worker generato in build (`vite.config.ts`). Mai nei gusci Tauri, che hanno
 * l'interfaccia gia' in locale e un'origine (`tauri://`) dove i service worker
 * non servono; mai in sviluppo, dove una cache confonderebbe l'HMR.
 *
 * I service worker esistono solo in contesti sicuri: HTTPS (tunnel Cloudflare,
 * reverse proxy) e localhost. Su `http://192.168.x.x:7420` il browser non li
 * offre: l'app funziona uguale, solo senza installazione/avvio offline.
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
      .catch((e) => console.warn("[rekord] service worker non registrato", e));
  };
  // Dopo il load: la registrazione non deve contendere la rete al primo avvio.
  if (document.readyState === "complete") register();
  else window.addEventListener("load", register, { once: true });
}

/** Chiede al service worker di riscaricare il guscio, poi ricarica la pagina. */
export async function reloadWithFreshShell(): Promise<void> {
  try {
    if (pwaSupported()) {
      const reg = await navigator.serviceWorker.getRegistration();
      await reg?.update();
    }
  } catch {
    /* si ricarica comunque */
  }
  location.reload();
}
