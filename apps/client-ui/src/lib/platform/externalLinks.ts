/**
 * External links in the Tauri shell.
 *
 * In the app window an `<a href="https://...">` or a `window.open(...)` would
 * open the page *inside* the app (or nothing, depending on the webview), with no
 * address bar nor a way to go back. Here they are intercepted and
 * handed to the system browser with the opener plugin. In the browser nothing is touched.
 *
 * "External" = http/https towards an origin other than the page's, plus
 * mailto: and tel:. The hub's admin panel (`http://hub:7420/admin`) is external
 * for the app, and indeed opens in the browser.
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

/** Opens `url` in the system browser (or mail/phone app). */
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
    // Whoever already handled the click (router, menu) takes precedence.
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
  // Bubbling phase on window: Svelte's handlers (delegated on the root)
  // run first and can call preventDefault.
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
