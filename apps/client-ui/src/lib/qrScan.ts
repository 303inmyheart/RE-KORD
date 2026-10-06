/**
 * Reading the hub's QR with the camera.
 *
 * It exists only inside the native shell on a phone: in the browser and on desktop the
 * plugin is not registered, so the «scan the QR» button is not shown
 * at all instead of appearing and then failing (see `qrScannerAvailable`).
 */

type ScannerModule = typeof import("@tauri-apps/plugin-barcode-scanner");

export type QrScanOutcome =
  | { status: "ok"; text: string }
  /** The user closed the camera: no message to show. */
  | { status: "cancelled" }
  | { status: "denied" }
  | { status: "error"; message: string };

let modulePromise: Promise<ScannerModule | null> | null = null;

function inNativeShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** The module loads on demand: in the browser build it stays a chunk never requested. */
async function loadScanner(): Promise<ScannerModule | null> {
  if (!inNativeShell()) return null;
  if (!modulePromise) {
    modulePromise = import("@tauri-apps/plugin-barcode-scanner").catch(() => null);
  }
  return modulePromise;
}

/**
 * True only where the camera can actually be opened. The permission check
 * acts as a probe: on desktop the plugin is absent and the call fails immediately,
 * without asking the user anything.
 */
export async function qrScannerAvailable(): Promise<boolean> {
  const mod = await loadScanner();
  if (!mod) return false;
  try {
    await mod.checkPermissions();
    return true;
  } catch {
    return false;
  }
}

/**
 * Opens the camera full screen and returns the content of the first QR read.
 * Permission is asked only here, when the user has tapped the button: a
 * request at startup, before explaining what it is for, gets denied by reflex.
 */
export async function scanQrCode(): Promise<QrScanOutcome> {
  const mod = await loadScanner();
  if (!mod) return { status: "error", message: "scanner-unavailable" };
  try {
    // The value is compared as a string: across plugin versions the set
    // of states changes («prompt», «prompt-with-rationale»), and the only one that
    // matters here is «granted».
    let permission = String(await mod.checkPermissions());
    if (permission !== "granted") permission = String(await mod.requestPermissions());
    if (permission !== "granted") return { status: "denied" };
    const result = await mod.scan({
      // Full screen: «windowed» mode draws the camera behind the
      // webview and wants the page transparent, i.e. the whole UI to
      // redo for one button.
      windowed: false,
      formats: [mod.Format.QRCode],
    });
    const text = String(result?.content ?? "").trim();
    if (!text) return { status: "cancelled" };
    return { status: "ok", text };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    // Closing the camera with the back button arrives here as an error.
    if (/cancel/i.test(message)) return { status: "cancelled" };
    if (/permission|denied/i.test(message)) return { status: "denied" };
    return { status: "error", message };
  }
}
