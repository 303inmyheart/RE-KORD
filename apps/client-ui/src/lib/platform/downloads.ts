/**
 * Downloads generated in the page (`<a download href="blob:...">`) in the Tauri shells.
 *
 * The client builds backups, profiles and theme packages in memory and "downloads" them
 * with a programmatic click on an `<a download>` (see `api.ts`). In the browser
 * it works; in the system webviews it does not: WebKitGTK and WKWebView ignore the click,
 * Android's WebView cannot download a `blob:`. Here, only inside Tauri:
 *
 * - we remember which Blob is behind every `blob:` created (the caller revokes it
 *   right after the click, when the read has not started yet);
 * - we intercept the programmatic click (`HTMLAnchorElement.prototype.click`)
 *   and the user's click on an `<a download>` in the page;
 * - desktop: the bytes go to the Rust command `save_download`, which opens «Save
 *   As»; Android: to the Kotlin bridge `RekordFilesNative`, which writes to Download.
 *
 * Limits: iOS is not covered (the iOS shell does not exist yet); on Android the file
 * always goes to Download without asking where.
 */

import { isAndroidShell, isTauri } from "./env";

type FilesBridge = {
  begin: (name: string, mime: string) => number;
  append: (id: number, base64: string) => boolean;
  finish: (id: number) => string;
  abort: (id: number) => void;
};

export type SaveOutcome =
  | { status: "saved"; path: string }
  | { status: "cancelled" }
  | { status: "error"; message: string };

type SaveListener = (outcome: SaveOutcome, name: string) => void;

/** 384 KiB per chunk: a multiple of 3, so the chunks' base64 concatenates. */
const CHUNK = 3 * 128 * 1024;

const blobs = new Map<string, Blob>();
let listener: SaveListener | null = null;
let installed = false;

/** Whoever wants to show an outcome (toast) registers here. */
export function onDownloadSaved(fn: SaveListener | null): void {
  listener = fn;
}

function filesBridge(): FilesBridge | null {
  const raw = (window as unknown as { RekordFilesNative?: FilesBridge }).RekordFilesNative;
  return raw && typeof raw.begin === "function" ? raw : null;
}

function toBase64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}

async function saveOnAndroid(blob: Blob, name: string): Promise<SaveOutcome> {
  const bridge = filesBridge();
  if (!bridge) return { status: "error", message: "files bridge unavailable" };
  const id = bridge.begin(name, blob.type || "application/octet-stream");
  if (id < 0) return { status: "error", message: "cannot create file" };
  try {
    for (let offset = 0; offset < blob.size; offset += CHUNK) {
      const part = new Uint8Array(await blob.slice(offset, offset + CHUNK).arrayBuffer());
      if (!bridge.append(id, toBase64(part))) {
        return { status: "error", message: "write failed" };
      }
    }
    const path = bridge.finish(id);
    return path ? { status: "saved", path } : { status: "error", message: "close failed" };
  } catch (e) {
    bridge.abort(id);
    return { status: "error", message: e instanceof Error ? e.message : String(e) };
  }
}

async function saveOnDesktop(blob: Blob, name: string): Promise<SaveOutcome> {
  const { invoke } = await import("@tauri-apps/api/core");
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const path = await invoke<string | null>("save_download", bytes, {
    headers: { "x-rekord-filename": encodeURIComponent(name) },
  });
  return path ? { status: "saved", path } : { status: "cancelled" };
}

/** Saves a Blob as a file; outside Tauri it falls back to the browser download. */
export async function saveBlob(blob: Blob, name: string): Promise<SaveOutcome> {
  const fileName = name.trim() || "rekord-download";
  let outcome: SaveOutcome;
  try {
    if (!isTauri()) {
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = fileName;
      a.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
      return { status: "saved", path: fileName };
    }
    outcome = isAndroidShell()
      ? await saveOnAndroid(blob, fileName)
      : await saveOnDesktop(blob, fileName);
  } catch (e) {
    outcome = { status: "error", message: e instanceof Error ? e.message : String(e) };
  }
  listener?.(outcome, fileName);
  return outcome;
}

function nameFor(anchor: HTMLAnchorElement): string {
  const explicit = anchor.getAttribute("download")?.trim();
  if (explicit) return explicit;
  try {
    const last = new URL(anchor.href).pathname.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : "rekord-download";
  } catch {
    return "rekord-download";
  }
}

/**
 * Tries to handle an `<a download>`; true if it took charge of it (the caller
 * then must not let the webview do it).
 */
function handleAnchor(anchor: HTMLAnchorElement): boolean {
  if (!anchor.hasAttribute("download")) return false;
  const href = anchor.href;
  const name = nameFor(anchor);
  if (href.startsWith("blob:")) {
    const blob = blobs.get(href);
    if (!blob) return false;
    void saveBlob(blob, name);
    return true;
  }
  if (href.startsWith("data:") || href.startsWith("http:") || href.startsWith("https:")) {
    void fetch(href)
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.blob();
      })
      .then((blob) => saveBlob(blob, name))
      .catch((e) =>
        listener?.({ status: "error", message: e instanceof Error ? e.message : String(e) }, name),
      );
    return true;
  }
  return false;
}

export function installDownloadBridge(): void {
  if (installed || !isTauri()) return;
  installed = true;

  const create = URL.createObjectURL.bind(URL);
  const revoke = URL.revokeObjectURL.bind(URL);
  URL.createObjectURL = (obj: Blob | MediaSource) => {
    const url = create(obj);
    if (obj instanceof Blob) blobs.set(url, obj);
    return url;
  };
  URL.revokeObjectURL = (url: string) => {
    blobs.delete(url);
    revoke(url);
  };

  const nativeClick = HTMLAnchorElement.prototype.click;
  HTMLAnchorElement.prototype.click = function (this: HTMLAnchorElement) {
    if (handleAnchor(this)) return;
    nativeClick.call(this);
  };

  window.addEventListener("click", (ev) => {
    if (ev.defaultPrevented || ev.button !== 0) return;
    const anchor = (ev.target as Element | null)?.closest?.("a[download]") as
      | HTMLAnchorElement
      | null;
    if (anchor && handleAnchor(anchor)) ev.preventDefault();
  });
}
