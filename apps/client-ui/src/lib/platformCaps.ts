/**
 * Rendering capabilities of the engine the UI runs in.
 *
 * WebKitGTK (the Tauri webview on Linux) composites in software: every
 * infinite CSS animation keeps it repainting at 60 fps and `backdrop-filter`
 * makes each scrolled frame re-blur what is behind it (QA perf report: queue
 * playing 34% CPU with the equaliser icon animating, 0.1% without; glass on
 * pushes scroll frames from 28 to 34 ms). There the UI drops those effects:
 *
 * - `lowEffects`: static equaliser icons, static skeletons.
 * - `backdropFilter`: false → the opaque glass fallback (`data-glass-backdrop="0"`).
 *
 * The flags are computed once at startup. `applyPlatformCapsToDom()` mirrors
 * them on <html> as `data-rk-lowfx` / `data-rk-engine` so CSS can follow.
 *
 * Read the flags, do not re-detect: `lib/userPrefs.ts` should use
 * `platformCaps.backdropFilter` in `probeGlassBackdropWorks()`.
 */

type TauriWindow = Window & {
  __TAURI__?: unknown;
  __TAURI_INTERNALS__?: unknown;
};

type NavigatorUAData = { platform?: string };

function detectTauri(w: TauriWindow | null): boolean {
  if (!w) return false;
  if ("__TAURI_INTERNALS__" in w || "__TAURI__" in w) return true;
  // Tauri's own UA token (some builds add it), checked as a fallback.
  return /\bTauri\b/i.test(w.navigator?.userAgent ?? "");
}

function detectLinux(nav: Navigator | null): boolean {
  if (!nav) return false;
  if (/Android/i.test(nav.userAgent)) return false;
  const uaData = (nav as Navigator & { userAgentData?: NavigatorUAData }).userAgentData;
  if (uaData?.platform) return /linux/i.test(uaData.platform);
  return /Linux/i.test(nav.platform || "") || /X11; Linux|Linux x86_64|Linux aarch64/.test(nav.userAgent);
}

/** WebKitGTK identifies as Safari-like WebKit on X11/Linux, without Chrome. */
function detectWebKitGtk(nav: Navigator | null): boolean {
  if (!nav) return false;
  const ua = nav.userAgent;
  return /AppleWebKit/.test(ua) && /Linux|X11/.test(ua) && !/Chrome|Chromium|Edg\//.test(ua);
}

function detectReducedMotion(): boolean {
  try {
    return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

function detectBackdropSupport(): boolean {
  if (typeof CSS === "undefined" || typeof CSS.supports !== "function") return false;
  return (
    CSS.supports("backdrop-filter", "blur(2px)") ||
    CSS.supports("-webkit-backdrop-filter", "blur(2px)")
  );
}

function compute() {
  const w = typeof window === "undefined" ? null : (window as TauriWindow);
  const nav = typeof navigator === "undefined" ? null : navigator;
  const tauri = detectTauri(w);
  const linux = detectLinux(nav);
  const webkitGtk = detectWebKitGtk(nav);
  /** Tauri on Linux = WebKitGTK; a plain WebKitGTK browser (Epiphany) counts too. */
  const softwareCompositor = (tauri && linux) || webkitGtk;
  return {
    tauri,
    linux,
    /** The engine is WebKitGTK (Tauri on Linux, or Epiphany). */
    webkitGtk: softwareCompositor,
    /** Drop always-on animations and blur. */
    lowEffects: softwareCompositor,
    /** `backdrop-filter` works and is cheap enough to use. */
    backdropFilter: !softwareCompositor && detectBackdropSupport(),
    /** The user asked for less motion (read once; CSS handles live changes). */
    reducedMotion: detectReducedMotion(),
  } as const;
}

export const platformCaps = compute();

export type PlatformCaps = typeof platformCaps;

/** Mirror the flags on <html> for CSS. Idempotent; call once at startup. */
export function applyPlatformCapsToDom(root: HTMLElement | null = typeof document === "undefined" ? null : document.documentElement) {
  if (!root) return;
  if (platformCaps.lowEffects) root.dataset.rkLowfx = "1";
  else delete root.dataset.rkLowfx;
  root.dataset.rkEngine = platformCaps.webkitGtk ? "webkitgtk" : "default";
}

/**
 * Whether the one "live" equaliser icon (the nav rail's Studio entry) may
 * animate. Everything else shows a static "playing" pose.
 */
export function canAnimateLiveIndicators(): boolean {
  return !platformCaps.lowEffects && !platformCaps.reducedMotion;
}
