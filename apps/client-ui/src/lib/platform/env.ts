/**
 * Dove sta girando l'interfaccia: browser (pagina servita dall'hub o dev server),
 * guscio Tauri desktop, guscio Tauri Android.
 */

type ShellWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
  RekordMediaNative?: unknown;
  RekordFilesNative?: unknown;
};

function win(): ShellWindow | null {
  return typeof window === "undefined" ? null : (window as ShellWindow);
}

/** Dentro un guscio Tauri (desktop o telefono). */
export function isTauri(): boolean {
  const w = win();
  return !!w && "__TAURI_INTERNALS__" in w;
}

/** Guscio Android: e' l'unico che espone i ponti Kotlin. */
export function isAndroidShell(): boolean {
  const w = win();
  return !!w && isTauri() && (!!w.RekordFilesNative || /Android/i.test(navigator.userAgent));
}

/** Guscio desktop (Windows, macOS, Linux). */
export function isDesktopShell(): boolean {
  return isTauri() && !isAndroidShell() && !/iPhone|iPad/i.test(navigator.userAgent);
}

/**
 * L'interfaccia e' impacchettata nell'app (e quindi si aggiorna con l'app) invece
 * di arrivare dall'hub a ogni apertura.
 */
export function isBundledUi(): boolean {
  return isTauri();
}
