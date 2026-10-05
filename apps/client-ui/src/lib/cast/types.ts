/**
 * Backend-agnostic casting contract.
 *
 * The web client ships a Google Cast Web Sender backend (`googleCast.ts`); a
 * native backend (e.g. the Android RekordCastManager bridged through Tauri) can
 * implement the same interface and be installed with `registerCastBackend()`
 * from `castController.svelte.ts` — the UI and the player integration do not
 * change.
 */

/** Connection state of the casting session. */
export type CastSessionState =
  /** No backend, unsupported environment or no receivers on the network. */
  | "unavailable"
  /** Receivers found, not connected. */
  | "available"
  | "connecting"
  | "connected";

/** Playback state reported by the receiver. */
export type CastPlayerState = "idle" | "loading" | "buffering" | "playing" | "paused";

/** Why the receiver went idle (only meaningful when `player === "idle"`). */
export type CastIdleReason = "finished" | "cancelled" | "interrupted" | "error" | null;

export type CastStatus = {
  session: CastSessionState;
  /** Friendly name of the receiver ("Living room speaker"). */
  deviceName: string | null;
  player: CastPlayerState;
  idleReason: CastIdleReason;
  /** Seconds. */
  currentTime: number;
  /** Seconds (0 when unknown). */
  duration: number;
  /** Media URL currently loaded on the receiver, if any. */
  mediaUrl: string | null;
};

export const INITIAL_CAST_STATUS: CastStatus = {
  session: "unavailable",
  deviceName: null,
  player: "idle",
  idleReason: null,
  currentTime: 0,
  duration: 0,
  mediaUrl: null,
};

export type CastMediaMetadata = {
  title: string;
  artist: string;
  album: string;
  /** Absolute, receiver-reachable cover URL. */
  coverUrl: string | null;
};

export type CastLoadOptions = {
  /** Start position (s). */
  startTime: number;
  /** Start playing once loaded (default true). */
  autoplay?: boolean;
};

export interface CastBackend {
  /** Short id for logs / diagnostics ("google-cast-web", "android-native"). */
  readonly id: string;
  /**
   * Whether this backend can work in the current environment. May load an SDK
   * lazily; resolves false (never throws) when unsupported.
   */
  isAvailable(): Promise<boolean>;
  /** Opens the device picker / connects. Rejects if the user cancels or it fails. */
  requestSession(): Promise<void>;
  /** Disconnects; `stopReceiver` also stops playback on the device. */
  endSession(stopReceiver?: boolean): Promise<void>;
  /** Loads a media URL (absolute, reachable by the receiver). */
  loadMedia(
    url: string,
    contentType: string,
    metadata: CastMediaMetadata,
    opts: CastLoadOptions,
  ): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(seconds: number): Promise<void>;
  /** Stops the media on the receiver (session stays open). */
  stop(): Promise<void>;
  getStatus(): CastStatus;
  /** Status updates (session, player state, progress ~1 Hz). Returns unsubscribe. */
  subscribe(listener: (status: CastStatus) => void): () => void;
}
