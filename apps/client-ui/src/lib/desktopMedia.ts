/**
 * Now-playing state for the Linux desktop shell's MPRIS player
 * (`apps/client-shell/src-tauri/src/mpris.rs`).
 *
 * WebKitGTK does not publish the Media Session to the desktop, so GNOME's
 * media widget, shell extensions and media keys need the shell's own player.
 * `mediaSession.ts` hands the same three updates it gives the OS (metadata,
 * state, position) to this module too; they are merged into one snapshot and
 * cross the IPC bridge only when something the desktop shows changed: a
 * position the widget can extrapolate (same track, same state) is not sent.
 * The widget's buttons come back as the usual `rekord:media-action` event.
 *
 * The cover is the 256 px thumbnail the page already shows, stored once per
 * album in the shell's cache and declared as a `file://` URL (desktop shells
 * load local files reliably, remote ones not always).
 *
 * Anywhere but the Linux desktop shell every function here does nothing.
 */

import { positionChanged, type SentPosition } from "./mediaPosition";
import { platformCaps } from "./platformCaps";

type Snapshot = {
  title: string;
  artist: string;
  album: string;
  /** The page's cover URL (http), turned into `artUrl` by the shell. */
  artworkUrl: string;
  playing: boolean;
  durationMs: number;
  positionMs: number;
};

/** One IPC hop for the three updates of a track change. */
const PUSH_DELAY_MS = 80;

type Invoke = <T>(cmd: string, args?: unknown, options?: { headers: Record<string, string> }) => Promise<T>;

let enabled: boolean | null = null;
let invokeFn: Invoke | null = null;
let snapshot: Snapshot | null = null;
let pending: ReturnType<typeof setTimeout> | null = null;
let shown = false;
let lastKey = "";
let lastPosition: SentPosition | null = null;
/** http cover URL → file URL from the shell (or "" while unavailable). */
const artFiles = new Map<string, string>();

function active(): boolean {
  if (enabled == null) enabled = platformCaps.tauri && platformCaps.linux;
  return enabled;
}

async function invoke<T>(cmd: string, args?: unknown, options?: { headers: Record<string, string> }): Promise<T> {
  if (!invokeFn) invokeFn = (await import("@tauri-apps/api/core")).invoke as Invoke;
  return invokeFn<T>(cmd, args, options);
}

function schedule(): void {
  if (pending != null) return;
  pending = setTimeout(() => {
    pending = null;
    void flush();
  }, PUSH_DELAY_MS);
}

/** File name for a cover: the URL's album id and version, or a hash of it. */
function artKey(url: string): string {
  let h = 0;
  for (let i = 0; i < url.length; i++) h = (Math.imul(h, 31) + url.charCodeAt(i)) | 0;
  return `c${(h >>> 0).toString(36)}`;
}

async function artFile(url: string): Promise<string> {
  if (!url) return "";
  const known = artFiles.get(url);
  if (known != null) return known;
  artFiles.set(url, "");
  try {
    const res = await fetch(url, { credentials: "include" });
    if (!res.ok) return "";
    const bytes = new Uint8Array(await res.arrayBuffer());
    const file = await invoke<string>("media_art", bytes, { headers: { "x-art-key": artKey(url) } });
    artFiles.set(url, file);
    if (artFiles.size > 200) artFiles.delete(artFiles.keys().next().value as string);
    return file;
  } catch {
    return "";
  }
}

async function flush(): Promise<void> {
  const snap = snapshot;
  if (!snap) {
    if (!shown) return;
    shown = false;
    lastKey = "";
    lastPosition = null;
    await invoke("media_clear").catch(() => {});
    return;
  }
  const artUrl = await artFile(snap.artworkUrl);
  if (snap !== snapshot) return; // a newer update is on its way
  const key = [snap.title, snap.artist, snap.album, artUrl, snap.playing, snap.durationMs].join("\u0000");
  const position: SentPosition = {
    duration: snap.durationMs / 1000,
    position: snap.positionMs / 1000,
    rate: 1,
    at: Date.now(),
    playing: snap.playing,
  };
  if (key === lastKey && !positionChanged(lastPosition, position)) return;
  lastKey = key;
  lastPosition = position;
  shown = true;
  const { artworkUrl: _http, ...now } = snap;
  await invoke("media_update", { now: { ...now, artUrl } }).catch(() => {});
}

export function pushDesktopMetadata(
  track: { title: string; artist: string; album: string } | null,
  artworkUrl: string,
): void {
  if (!active()) return;
  if (!track) {
    snapshot = null;
    schedule();
    return;
  }
  snapshot = {
    title: track.title,
    artist: track.artist,
    album: track.album,
    artworkUrl,
    playing: snapshot?.playing ?? false,
    durationMs: 0,
    positionMs: 0,
  };
  schedule();
}

export function pushDesktopPlaybackState(state: "none" | "paused" | "playing"): void {
  if (!active()) return;
  if (state === "none") {
    snapshot = null;
    schedule();
    return;
  }
  if (!snapshot) return;
  snapshot = { ...snapshot, playing: state === "playing" };
  schedule();
}

export function pushDesktopPosition(duration: number, position: number): void {
  if (!active() || !snapshot) return;
  const durationMs = Number.isFinite(duration) && duration > 0 ? Math.round(duration * 1000) : 0;
  const positionMs = Number.isFinite(position) && position > 0 ? Math.round(position * 1000) : 0;
  snapshot = {
    ...snapshot,
    durationMs,
    positionMs: durationMs > 0 ? Math.min(positionMs, durationMs) : positionMs,
  };
  schedule();
}
