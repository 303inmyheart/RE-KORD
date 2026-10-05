/**
 * Android native Cast bridge — wire format and its mapping to `CastStatus`.
 *
 * The Kotlin side (`RekordCast.kt`) sends raw Cast SDK names; everything here is
 * pure so it can be unit-tested (`src/lib/castAndroid.test.mjs`).
 */

import type { CastIdleReason, CastPlayerState, CastSessionState, CastStatus } from "./types";

/** Status as serialised by `RekordCast.statusJson()`. */
export type RawAndroidCastStatus = {
  /** Native init finished (Play Services checked, CastContext resolved or failed). */
  ready?: boolean;
  /** Play Services + Cast SDK usable on this device. */
  supported?: boolean;
  /** Why it is unsupported ("play-services-2", "cast-context"…). */
  reason?: string;
  /** `CastState` name: NO_DEVICES_AVAILABLE | NOT_CONNECTED | CONNECTING | CONNECTED. */
  castState?: string;
  deviceName?: string | null;
  /** `MediaStatus.PLAYER_STATE_*` name, null when unknown. */
  playerState?: string | null;
  /** `MediaStatus.IDLE_REASON_*` name, null when none. */
  idleReason?: string | null;
  mediaLoaded?: boolean;
  positionMs?: number;
  durationMs?: number;
  contentId?: string | null;
  /** Receiver volume 0..1. */
  volume?: number;
};

/** `detail` of the `rekord:cast` DOM event. */
export type AndroidCastEvent =
  | { type: "status"; status: RawAndroidCastStatus }
  | { type: "session"; event: "cancelled" | "startFailed" | "unavailable"; code?: number }
  | { type: "result"; id: number; ok: boolean; error?: string };

/** The JS interface installed by MainActivity as `window.RekordCastNative`. */
export type AndroidCastBridge = {
  getStatus(): string;
  requestSession(): void;
  endSession(stopReceiver: boolean): void;
  load(json: string): void;
  play(): void;
  pause(): void;
  stop(): void;
  seek(seconds: number): void;
};

export const ANDROID_CAST_EVENT = "rekord:cast";
export const ANDROID_CAST_BRIDGE = "RekordCastNative";

/** The bridge when running inside the Android shell, else null. */
export function androidCastBridge(w: unknown): AndroidCastBridge | null {
  if (!w || typeof w !== "object") return null;
  const raw = (w as Record<string, unknown>)[ANDROID_CAST_BRIDGE] as Partial<AndroidCastBridge> | undefined;
  if (!raw || typeof raw.getStatus !== "function" || typeof raw.requestSession !== "function") return null;
  return raw as AndroidCastBridge;
}

export function mapAndroidSessionState(raw: RawAndroidCastStatus): CastSessionState {
  if (!raw.supported) return "unavailable";
  switch (raw.castState) {
    case "CONNECTED":
      return "connected";
    case "CONNECTING":
      return "connecting";
    case "NOT_CONNECTED":
      return "available";
    default:
      return "unavailable";
  }
}

export function mapAndroidPlayerState(raw: string | null | undefined, loaded: boolean): CastPlayerState {
  switch (raw) {
    case "PLAYING":
      return "playing";
    case "PAUSED":
      return "paused";
    case "BUFFERING":
      return "buffering";
    case "LOADING":
      return "loading";
    case "IDLE":
      return "idle";
    default:
      // PLAYER_STATE_UNKNOWN: same fallback as the web sender.
      return loaded ? "paused" : "idle";
  }
}

export function mapAndroidIdleReason(raw: string | null | undefined): CastIdleReason {
  switch (raw) {
    case "FINISHED":
      return "finished";
    case "CANCELLED":
    case "CANCELED":
      return "cancelled";
    case "INTERRUPTED":
      return "interrupted";
    case "ERROR":
      return "error";
    default:
      return null;
  }
}

function seconds(ms: unknown): number {
  return typeof ms === "number" && Number.isFinite(ms) && ms > 0 ? ms / 1000 : 0;
}

export function mapAndroidCastStatus(raw: RawAndroidCastStatus): CastStatus {
  const session = mapAndroidSessionState(raw);
  const player = session === "connected" ? mapAndroidPlayerState(raw.playerState, raw.mediaLoaded === true) : "idle";
  return {
    session,
    deviceName: session === "unavailable" ? null : (raw.deviceName?.trim() || null),
    player,
    idleReason: player === "idle" && session === "connected" ? mapAndroidIdleReason(raw.idleReason) : null,
    currentTime: session === "connected" ? seconds(raw.positionMs) : 0,
    duration: session === "connected" ? seconds(raw.durationMs) : 0,
    mediaUrl: session === "connected" ? (raw.contentId || null) : null,
  };
}

/** Parses the bridge's `getStatus()` string; null when it is not valid JSON. */
export function parseAndroidCastStatus(json: unknown): RawAndroidCastStatus | null {
  if (typeof json !== "string") return null;
  try {
    const o = JSON.parse(json) as unknown;
    return o && typeof o === "object" ? (o as RawAndroidCastStatus) : null;
  } catch {
    return null;
  }
}

/** Validates a `rekord:cast` event detail. */
export function parseAndroidCastEvent(detail: unknown): AndroidCastEvent | null {
  if (!detail || typeof detail !== "object") return null;
  const d = detail as Record<string, unknown>;
  switch (d.type) {
    case "status":
      return d.status && typeof d.status === "object"
        ? { type: "status", status: d.status as RawAndroidCastStatus }
        : null;
    case "session":
      return d.event === "cancelled" || d.event === "startFailed" || d.event === "unavailable"
        ? { type: "session", event: d.event, code: typeof d.code === "number" ? d.code : undefined }
        : null;
    case "result":
      return typeof d.id === "number"
        ? {
            type: "result",
            id: d.id,
            ok: d.ok === true,
            error: typeof d.error === "string" ? d.error : undefined,
          }
        : null;
    default:
      return null;
  }
}

/** JSON payload for the bridge's `load()`. */
export function androidLoadPayload(
  id: number,
  url: string,
  contentType: string,
  metadata: { title: string; artist: string; album: string; coverUrl: string | null },
  opts: { startTime: number; autoplay?: boolean },
): string {
  return JSON.stringify({
    id,
    url,
    contentType,
    title: metadata.title,
    artist: metadata.artist,
    album: metadata.album,
    coverUrl: metadata.coverUrl,
    startTime: Number.isFinite(opts.startTime) ? Math.max(0, opts.startTime) : 0,
    autoplay: opts.autoplay !== false,
  });
}
