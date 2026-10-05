/**
 * Google Cast Web Sender backend (Default Media Receiver).
 *
 * The SDK (`cast_sender.js?loadCastFramework=1`) is loaded lazily, only in
 * Chrome-family browsers in a secure context (see `isWebCastSenderEnvironment`).
 * Elsewhere — Tauri webviews, Firefox, Safari, plain-http LAN pages —
 * `isAvailable()` resolves false and the Cast button stays hidden.
 */

import { isWebCastSenderEnvironment } from "./castMedia";
import {
  INITIAL_CAST_STATUS,
  type CastBackend,
  type CastIdleReason,
  type CastLoadOptions,
  type CastMediaMetadata,
  type CastPlayerState,
  type CastStatus,
} from "./types";

const CAST_SDK_URL = "https://www.gstatic.com/cv/js/sender/v1/cast_sender.js?loadCastFramework=1";
const SDK_TIMEOUT_MS = 10_000;

// Minimal typings for the bits of the SDK we use (no @types dependency).
type ChromeCastMedia = {
  DEFAULT_MEDIA_RECEIVER_APP_ID: string;
  MediaInfo: new (contentId: string, contentType: string) => {
    metadata: unknown;
    streamType: string;
  };
  MusicTrackMediaMetadata: new () => {
    title: string;
    artist: string;
    albumName: string;
    images: { url: string }[];
  };
  StreamType: { BUFFERED: string };
  LoadRequest: new (info: unknown) => { currentTime: number; autoplay: boolean };
};
type ChromeCast = {
  media: ChromeCastMedia;
  Image: new (url: string) => { url: string };
  AutoJoinPolicy: { ORIGIN_SCOPED: string };
};
type CastSessionObj = {
  loadMedia(request: unknown): Promise<string | undefined>;
  getCastDevice(): { friendlyName: string } | null;
  getMediaSession(): { idleReason?: string | null; media?: { contentId?: string } } | null;
  endSession(stopCasting: boolean): void;
};
type CastContextObj = {
  setOptions(opts: Record<string, unknown>): void;
  requestSession(): Promise<string | undefined>;
  getCurrentSession(): CastSessionObj | null;
  getCastState(): string;
  endCurrentSession(stopCasting: boolean): void;
  addEventListener(type: string, handler: (ev: { sessionState?: string; castState?: string }) => void): void;
};
type RemotePlayerObj = {
  isConnected: boolean;
  isMediaLoaded: boolean;
  isPaused: boolean;
  currentTime: number;
  duration: number;
  playerState: string | null;
  mediaInfo: { contentId?: string } | null;
};
type RemotePlayerControllerObj = {
  addEventListener(type: string, handler: () => void): void;
  playOrPause(): void;
  seek(): void;
  stop(): void;
};
type CastFramework = {
  CastContext: { getInstance(): CastContextObj };
  CastContextEventType: { SESSION_STATE_CHANGED: string; CAST_STATE_CHANGED: string };
  CastState: { NO_DEVICES_AVAILABLE: string; NOT_CONNECTED: string; CONNECTING: string; CONNECTED: string };
  RemotePlayer: new () => RemotePlayerObj;
  RemotePlayerController: new (p: RemotePlayerObj) => RemotePlayerControllerObj;
  RemotePlayerEventType: { ANY_CHANGE: string };
};
type CastWindow = Window & {
  cast?: { framework?: CastFramework };
  chrome?: { cast?: ChromeCast };
  __onGCastApiAvailable?: (available: boolean, reason?: string) => void;
  __TAURI_INTERNALS__?: unknown;
  __TAURI__?: unknown;
};

function castWindow(): CastWindow | null {
  return typeof window === "undefined" ? null : (window as CastWindow);
}

let sdkPromise: Promise<boolean> | null = null;

function loadSdk(): Promise<boolean> {
  const w = castWindow();
  if (!w) return Promise.resolve(false);
  if (w.cast?.framework && w.chrome?.cast) return Promise.resolve(true);
  if (sdkPromise) return sdkPromise;
  sdkPromise = new Promise<boolean>((resolve) => {
    let done = false;
    const finish = (ok: boolean) => {
      if (done) return;
      done = true;
      window.clearTimeout(timer);
      resolve(ok && Boolean(w.cast?.framework && w.chrome?.cast));
    };
    const timer = window.setTimeout(() => finish(false), SDK_TIMEOUT_MS);
    // The SDK calls this global once the framework is ready (or not available).
    w.__onGCastApiAvailable = (available) => finish(Boolean(available));
    const script = document.createElement("script");
    script.src = CAST_SDK_URL;
    script.async = true;
    script.onerror = () => finish(false);
    document.head.appendChild(script);
  });
  return sdkPromise;
}

function mapPlayerState(raw: string | null | undefined, loaded: boolean): CastPlayerState {
  switch (raw) {
    case "PLAYING":
      return "playing";
    case "PAUSED":
      return "paused";
    case "BUFFERING":
      return "buffering";
    case "LOADING":
      return "loading";
    default:
      return loaded ? "paused" : "idle";
  }
}

function mapIdleReason(raw: string | null | undefined): CastIdleReason {
  switch (raw) {
    case "FINISHED":
      return "finished";
    case "CANCELLED":
      return "cancelled";
    case "INTERRUPTED":
      return "interrupted";
    case "ERROR":
      return "error";
    default:
      return null;
  }
}

export class GoogleCastWebBackend implements CastBackend {
  readonly id = "google-cast-web";
  private status: CastStatus = { ...INITIAL_CAST_STATUS };
  private listeners = new Set<(s: CastStatus) => void>();
  private ctx: CastContextObj | null = null;
  private fw: CastFramework | null = null;
  private player: RemotePlayerObj | null = null;
  private controller: RemotePlayerControllerObj | null = null;
  private initPromise: Promise<boolean> | null = null;

  isAvailable(): Promise<boolean> {
    this.initPromise ??= this.init();
    return this.initPromise;
  }

  private async init(): Promise<boolean> {
    const w = castWindow();
    if (!w || typeof navigator === "undefined") return false;
    const supported = isWebCastSenderEnvironment({
      userAgent: navigator.userAgent,
      isSecureContext: w.isSecureContext,
      isTauri: Boolean(w.__TAURI_INTERNALS__ || w.__TAURI__),
    });
    if (!supported) return false;
    if (!(await loadSdk())) return false;
    const fw = w.cast!.framework!;
    const chromeCast = w.chrome!.cast!;
    let ctx: CastContextObj;
    try {
      ctx = fw.CastContext.getInstance();
      ctx.setOptions({
        receiverApplicationId: chromeCast.media.DEFAULT_MEDIA_RECEIVER_APP_ID,
        autoJoinPolicy: chromeCast.AutoJoinPolicy.ORIGIN_SCOPED,
      });
    } catch {
      return false;
    }
    this.fw = fw;
    this.ctx = ctx;
    this.player = new fw.RemotePlayer();
    this.controller = new fw.RemotePlayerController(this.player);
    this.controller.addEventListener(fw.RemotePlayerEventType.ANY_CHANGE, () => this.refresh());
    ctx.addEventListener(fw.CastContextEventType.CAST_STATE_CHANGED, () => this.refresh());
    ctx.addEventListener(fw.CastContextEventType.SESSION_STATE_CHANGED, () => this.refresh());
    this.refresh();
    return true;
  }

  private refresh() {
    const ctx = this.ctx;
    const fw = this.fw;
    const p = this.player;
    if (!ctx || !fw || !p) return;
    const castState = ctx.getCastState();
    const session =
      castState === fw.CastState.CONNECTED
        ? "connected"
        : castState === fw.CastState.CONNECTING
          ? "connecting"
          : castState === fw.CastState.NOT_CONNECTED
            ? "available"
            : "unavailable";
    const cs = ctx.getCurrentSession();
    const media = cs?.getMediaSession() ?? null;
    const playerState = session === "connected" ? mapPlayerState(p.playerState, p.isMediaLoaded) : "idle";
    this.status = {
      session,
      deviceName: cs?.getCastDevice()?.friendlyName ?? null,
      player: playerState,
      idleReason: playerState === "idle" ? mapIdleReason(media?.idleReason) : null,
      currentTime: Number.isFinite(p.currentTime) ? p.currentTime : 0,
      duration: Number.isFinite(p.duration) ? p.duration : 0,
      mediaUrl: p.mediaInfo?.contentId ?? media?.media?.contentId ?? null,
    };
    for (const fn of this.listeners) fn(this.status);
  }

  async requestSession(): Promise<void> {
    if (!(await this.isAvailable()) || !this.ctx) throw new Error("cast-unavailable");
    const err = await this.ctx.requestSession();
    // The SDK resolves with an error code string on failure in some versions.
    if (err) throw new Error(String(err));
    this.refresh();
  }

  async endSession(stopReceiver = true): Promise<void> {
    this.ctx?.endCurrentSession(stopReceiver);
    this.refresh();
  }

  async loadMedia(
    url: string,
    contentType: string,
    metadata: CastMediaMetadata,
    opts: CastLoadOptions,
  ): Promise<void> {
    const w = castWindow();
    const chromeCast = w?.chrome?.cast;
    const session = this.ctx?.getCurrentSession();
    if (!chromeCast || !session) throw new Error("cast-no-session");
    const info = new chromeCast.media.MediaInfo(url, contentType);
    const meta = new chromeCast.media.MusicTrackMediaMetadata();
    meta.title = metadata.title;
    meta.artist = metadata.artist;
    meta.albumName = metadata.album;
    meta.images = metadata.coverUrl ? [new chromeCast.Image(metadata.coverUrl)] : [];
    info.metadata = meta;
    info.streamType = chromeCast.media.StreamType.BUFFERED;
    const request = new chromeCast.media.LoadRequest(info);
    request.currentTime = Math.max(0, opts.startTime);
    request.autoplay = opts.autoplay !== false;
    const err = await session.loadMedia(request);
    if (err) throw new Error(String(err));
    this.refresh();
  }

  async play(): Promise<void> {
    if (this.player?.isPaused) this.controller?.playOrPause();
  }

  async pause(): Promise<void> {
    if (this.player && !this.player.isPaused) this.controller?.playOrPause();
  }

  async seek(seconds: number): Promise<void> {
    if (!this.player || !this.controller) return;
    this.player.currentTime = Math.max(0, seconds);
    this.controller.seek();
  }

  async stop(): Promise<void> {
    this.controller?.stop();
  }

  getStatus(): CastStatus {
    return this.status;
  }

  subscribe(listener: (s: CastStatus) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
