/**
 * Link esterni nel guscio Tauri.
 *
 * Nella finestra dell'app un `<a href="https://...">` o un `window.open(...)`
 * aprirebbero la pagina *dentro* l'app (o niente, secondo la webview), senza
 * barra degli indirizzi ne' modo di tornare indietro. Qui si intercettano e si
 * passano al browser di sistema col plugin opener. Nel browser non si tocca nulla.
 *
 * "Esterno" = http/https verso un'origine diversa da quella della pagina, piu'
 * mailto: e tel:. Il pannello admin dell'hub (`http://hub:7420/admin`) e' esterno
 * per l'app, e si apre infatti nel browser.
 */

import { isTauri } from "./env";

const EXTERNAL_PROTOCOLS = new Set(["http:", "https:", "mailto:", "tel:"]);

export function externalUrl(raw: string | URL | null | undefined): string | null {
  if (raw == null || raw === "") return null;
  let url: URL;
  try {
    url = new URL(String(raw), location.href);
  } catch {
    return null;
  }
  if (!EXTERNAL_PROTOCOLS.has(url.protocol)) return null;
  if ((url.protocol === "http:" || url.protocol === "https:") && url.origin === location.origin) {
    return null;
  }
  return url.href;
}

/** Apre `url` nel browser (o nell'app di posta/telefono) del sistema. */
export async function openExternal(url: string): Promise<void> {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("plugin:opener|open_url", { url });
}

let installed = false;

export function installExternalLinkHandler(): void {
  if (installed || !isTauri()) return;
  installed = true;

  const onClick = (ev: MouseEvent) => {
    // Chi ha gia' gestito il click (router, menu) ha la precedenza.
    if (ev.defaultPrevented) return;
    if (ev.type === "click" && ev.button !== 0) return;
    if (ev.type === "auxclick" && ev.button !== 1) return;
    const target = ev.target as Element | null;
    const anchor = target?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!anchor || anchor.hasAttribute("download")) return;
    const url = externalUrl(anchor.getAttribute("href"));
    if (!url) return;
    ev.preventDefault();
    void openExternal(url).catch((e) => console.warn("[rekord] open external link", e));
  };
  // Fase di bubbling su window: i gestori di Svelte (delegati sulla radice)
  // girano prima e possono chiamare preventDefault.
  window.addEventListener("click", onClick);
  window.addEventListener("auxclick", onClick);

  const nativeOpen = window.open.bind(window);
  window.open = (url?: string | URL, target?: string, features?: string) => {
    const external = externalUrl(url ?? null);
    if (external) {
      void openExternal(external).catch((e) => console.warn("[rekord] window.open", e));
      return null;
    }
    return nativeOpen(url, target, features);
  };
}
