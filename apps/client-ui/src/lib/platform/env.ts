/**
 * Where the UI is running: browser (page served by the hub or dev server),
 * Tauri desktop shell, Tauri Android shell.
 */

type ShellWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
  RekordMediaNative?: unknown;
  RekordFilesNative?: unknown;
};

function win(): ShellWindow | null {
  return typeof window === "undefined" ? null : (window as ShellWindow);
}

/** Inside a Tauri shell (desktop or phone). */
export function isTauri(): boolean {
  const w = win();
  return !!w && "__TAURI_INTERNALS__" in w;
}

/** Android shell: the only one exposing the Kotlin bridges. */
export function isAndroidShell(): boolean {
  const w = win();
  return !!w && isTauri() && (!!w.RekordFilesNative || /Android/i.test(navigator.userAgent));
}

/** Desktop shell (Windows, macOS, Linux). */
export function isDesktopShell(): boolean {
  return isTauri() && !isAndroidShell() && !/iPhone|iPad/i.test(navigator.userAgent);
}

/**
 * The UI is bundled in the app (and so updates with the app) instead
 * of coming from the hub on every launch.
 */
export function isBundledUi(): boolean {
  return isTauri();
}
