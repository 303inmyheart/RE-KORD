/**
 * Bridge to the Android shell's media notification.
 *
 * Android's WebView has no Media Session API: `navigator.mediaSession`
 * does not exist, so nothing that `mediaSession.ts` writes reaches the
 * system. The shell therefore exposes `window.RekordMediaNative`, and from here we
 * tell it the same state the other clients send to the operating system.
 * The audio stays in the page: over there only the notification is drawn, which sends the
 * commands back with the `rekord:media-action` event.
 *
 * Outside Android the bridge is absent and every function here does nothing.
 */

/** State the Kotlin side can read (`NowPlaying.fromJson`). */
type NativeSnapshot = {
  title: string;
  artist: string;
  album: string;
  artworkUrl: string;
  playing: boolean;
  durationMs: number;
  positionMs: number;
  /**
   * The player means to play even while nothing sounds yet (loading, retrying,
   * reconnecting): the shell keeps its foreground service, CPU and Wi-Fi awake.
   */
  wantsPlay: boolean;
  /** Why it is paused: `external` (another app took the audio), `user`, `""`. */
  pauseReason: string;
};

type NativeMediaBridge = {
  update: (json: string) => void;
  stop: () => void;
  /** Diagnostics into the shell's logcat (`RekordMedia` tag). Older shells lack it. */
  log?: (message: string) => void;
};

/**
 * Metadata, state and position arrive from three separate calls on every
 * track change: wait a moment and cross the bridge only once.
 */
const PUSH_DELAY_MS = 80;

function bridge(): NativeMediaBridge | null {
  if (typeof window === "undefined") return null;
  const raw = (window as unknown as { RekordMediaNative?: NativeMediaBridge })
    .RekordMediaNative;
  if (!raw || typeof raw.update !== "function") return null;
  return raw;
}

let snapshot: NativeSnapshot | null = null;
/** Last intent, kept for the snapshot that the next track's metadata starts. */
const intent = { wantsPlay: false, pauseReason: "" };
let pending: ReturnType<typeof setTimeout> | null = null;
let lastSent = "";

function flush(): void {
  pending = null;
  const target = bridge();
  if (!target) return;
  if (!snapshot) {
    if (lastSent === "") return;
    lastSent = "";
    try {
      target.stop();
    } catch {
      /* The shell has gone away: there is nothing to recover. */
    }
    return;
  }
  const json = JSON.stringify(snapshot);
  if (json === lastSent) return;
  lastSent = json;
  try {
    target.update(json);
    // The shell delivers the notification's commands only when this flag is present:
    // it means the player is mounted and listening for `rekord:media-action`
    // (see RekordMediaBridge in MainActivity/RekordMedia.kt).
    (window as unknown as { __rekordNativeMediaReady?: boolean }).__rekordNativeMediaReady = true;
  } catch {
    /* */
  }
}

function schedule(): void {
  if (!bridge()) return;
  if (pending != null) return;
  pending = setTimeout(flush, PUSH_DELAY_MS);
}

export function pushNativeMetadata(
  track: { title: string; artist: string; album: string } | null,
  artworkUrl: string,
): void {
  if (!bridge()) return;
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
    // New metadata without a known state yet: keep the previous one,
    // otherwise the track change would make the notification flash to "paused".
    playing: snapshot?.playing ?? false,
    durationMs: 0,
    positionMs: 0,
    wantsPlay: intent.wantsPlay,
    pauseReason: intent.pauseReason,
  };
  schedule();
}

export function pushNativePlaybackState(
  state: "none" | "paused" | "playing",
): void {
  if (!bridge()) return;
  if (state === "none") {
    snapshot = null;
    schedule();
    return;
  }
  if (!snapshot) return;
  snapshot = { ...snapshot, playing: state === "playing" };
  schedule();
}

/**
 * What the player means to do, beyond what it does: see `wantsPlay` and
 * `pauseReason` in NativeSnapshot. Crosses the bridge only when it changes.
 */
export function pushNativeIntent(wantsPlay: boolean, pauseReason: string): void {
  if (!bridge()) return;
  intent.wantsPlay = wantsPlay;
  intent.pauseReason = pauseReason;
  if (!snapshot) return;
  if (snapshot.wantsPlay === wantsPlay && snapshot.pauseReason === pauseReason) return;
  snapshot = { ...snapshot, wantsPlay, pauseReason };
  schedule();
}

/**
 * Playback diagnostics (stalls, retries, reconnects) into the Android logcat,
 * next to the shell's own lines: `adb logcat -s RekordMedia`. Elsewhere a no-op,
 * so callers need not check the platform; keep it to transitions, never per frame.
 */
export function nativeLog(message: string): void {
  const target = bridge();
  if (!target || typeof target.log !== "function") return;
  try {
    target.log(message);
  } catch {
    /* */
  }
}

export function pushNativePosition(duration: number, position: number): void {
  if (!bridge()) return;
  if (!snapshot) return;
  const durationMs = Number.isFinite(duration) && duration > 0
    ? Math.round(duration * 1000)
    : 0;
  const positionMs = Number.isFinite(position) && position > 0
    ? Math.round(position * 1000)
    : 0;
  snapshot = {
    ...snapshot,
    durationMs,
    positionMs: durationMs > 0 ? Math.min(positionMs, durationMs) : positionMs,
  };
  schedule();
}
