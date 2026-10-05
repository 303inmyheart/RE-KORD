/**
 * Download generati nella pagina (`<a download href="blob:...">`) nei gusci Tauri.
 *
 * Il client costruisce in memoria backup, profilo e pacchetto tema e li "scarica"
 * con un click programmatico su un `<a download>` (vedi `api.ts`). Nel browser
 * funziona; nelle webview di sistema no: WebKitGTK e WKWebView ignorano il click,
 * la WebView di Android non sa scaricare un `blob:`. Qui, solo dentro Tauri:
 *
 * - si ricorda quale Blob sta dietro ogni `blob:` creato (il chiamante lo revoca
 *   subito dopo il click, quando la lettura non e' ancora partita);
 * - si intercettano il click programmatico (`HTMLAnchorElement.prototype.click`)
 *   e quello dell'utente su un `<a download>` nella pagina;
 * - desktop: i byte vanno al comando Rust `save_download`, che apre «Salva con
 *   nome»; Android: al ponte Kotlin `RekordFilesNative`, che scrive in Download.
 *
 * Limiti: iOS non e' coperto (il guscio iOS non esiste ancora); su Android il file
 * va sempre in Download senza chiedere dove.
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

/** 384 KiB per pezzo: multiplo di 3, quindi il base64 dei pezzi si concatena. */
const CHUNK = 3 * 128 * 1024;

const blobs = new Map<string, Blob>();
let listener: SaveListener | null = null;
let installed = false;

/** Chi vuole mostrare un esito (toast) si registra qui. */
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

/** Salva un Blob come file; fuori da Tauri ripiega sul download del browser. */
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
 * Prova a gestire un `<a download>`; vero se l'ha preso in carico (il chiamante
 * allora non deve lasciar fare alla webview).
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
