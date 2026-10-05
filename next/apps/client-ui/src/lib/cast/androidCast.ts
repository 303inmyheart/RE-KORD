/**
 * Native Google Cast backend for the Android shell.
 *
 * The Tauri WebView has no Cast extension, so the web sender (`googleCast.ts`)
 * cannot run there. Instead `MainActivity` installs `window.RekordCastNative`
 * (`RekordCast.kt`, Google Cast SDK with the Default Media Receiver): commands
 * go through it, and the receiver status comes back as `rekord:cast` DOM events
 * — the same pattern as the media notification (`nativeMedia.ts`).
 *
 * `castController` picks this backend automatically when the bridge exists.
 * Media URLs are still decided by `castMedia.ts` (LAN origin, transcoding).
 */

import {
  ANDROID_CAST_EVENT,
  androidCastBridge,
  androidLoadPayload,
  mapAndroidCastStatus,
  parseAndroidCastEvent,
  parseAndroidCastStatus,
  type AndroidCastBridge,
  type RawAndroidCastStatus,
} from "./androidCastStatus";
import {
  INITIAL_CAST_STATUS,
  type CastBackend,
  type CastLoadOptions,
  type CastMediaMetadata,
  type CastStatus,
} from "./types";

/** Native init (Play Services check + CastContext) normally takes well under this. */
const READY_TIMEOUT_MS = 10_000;
/** Device picker open + connection: generous, the user may take a while to choose. */
const SESSION_TIMEOUT_MS = 120_000;
const LOAD_TIMEOUT_MS = 30_000;

/** User-facing error texts (castController passes translated ones). */
export type AndroidCastMessages = {
  sessionFailed(code: number | null): string;
  timeout(): string;
};

const DEFAULT_MESSAGES: AndroidCastMessages = {
  sessionFailed: (code) => `Cast session failed${code == null ? "" : ` (code ${code})`}`,
  timeout: () => "the Cast device did not answer",
};

type Pending = { resolve: () => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> };

export class AndroidCastBackend implements CastBackend {
  readonly id = "android-native";
  private status: CastStatus = { ...INITIAL_CAST_STATUS };
  private raw: RawAndroidCastStatus = {};
  private listeners = new Set<(s: CastStatus) => void>();
  private readyWaiters = new Set<(raw: RawAndroidCastStatus) => void>();
  private pendingSession: Pending | null = null;
  private pendingLoads = new Map<number, Pending>();
  private nextLoadId = 1;
  private availablePromise: Promise<boolean> | null = null;
  private listening = false;

  private readonly bridge: AndroidCastBridge;
  private readonly messages: AndroidCastMessages;

  // No parameter properties: the node tests run this file with strip-only TS.
  constructor(bridge: AndroidCastBridge, messages: AndroidCastMessages = DEFAULT_MESSAGES) {
    this.bridge = bridge;
    this.messages = messages;
  }

  isAvailable(): Promise<boolean> {
    this.availablePromise ??= this.init();
    return this.availablePromise;
  }

  private async init(): Promise<boolean> {
    this.listen();
    const initial = parseAndroidCastStatus(this.safe(() => this.bridge.getStatus()));
    if (initial) this.applyRaw(initial);
    const raw = this.raw.ready ? this.raw : await this.waitReady();
    return raw.ready === true && raw.supported === true;
  }

  private waitReady(): Promise<RawAndroidCastStatus> {
    return new Promise((resolve) => {
      const done = (raw: RawAndroidCastStatus) => {
        clearTimeout(timer);
        this.readyWaiters.delete(done);
        resolve(raw);
      };
      const timer = setTimeout(() => done(this.raw), READY_TIMEOUT_MS);
      this.readyWaiters.add(done);
    });
  }

  private listen() {
    if (this.listening || typeof window === "undefined") return;
    this.listening = true;
    window.addEventListener(ANDROID_CAST_EVENT, (ev) => {
      const event = parseAndroidCastEvent((ev as CustomEvent).detail);
      if (!event) return;
      switch (event.type) {
        case "status":
          this.applyRaw(event.status);
          break;
        case "session":
          if (event.event === "cancelled") this.settleSession(new Error("cancel"));
          else if (event.event === "startFailed")
            this.settleSession(new Error(this.messages.sessionFailed(event.code ?? null)));
          else this.settleSession(new Error("cast-unavailable"));
          break;
        case "result": {
          const p = this.pendingLoads.get(event.id);
          if (!p) return;
          this.pendingLoads.delete(event.id);
          clearTimeout(p.timer);
          if (event.ok) p.resolve();
          else p.reject(new Error(event.error || "cast-load-failed"));
          break;
        }
      }
    });
  }

  private applyRaw(raw: RawAndroidCastStatus) {
    this.raw = raw;
    this.status = mapAndroidCastStatus(raw);
    if (raw.ready) for (const fn of [...this.readyWaiters]) fn(raw);
    if (this.status.session === "connected") this.settleSession(null);
    for (const fn of this.listeners) fn(this.status);
  }

  private settleSession(err: Error | null) {
    const p = this.pendingSession;
    if (!p) return;
    this.pendingSession = null;
    clearTimeout(p.timer);
    if (err) p.reject(err);
    else p.resolve();
  }

  private safe<T>(fn: () => T): T | undefined {
    try {
      return fn();
    } catch {
      return undefined;
    }
  }

  async requestSession(): Promise<void> {
    if (!(await this.isAvailable())) throw new Error("cast-unavailable");
    if (this.status.session === "connected") return;
    // An older wait is superseded (castController's busy flag normally prevents it).
    if (this.pendingSession) this.settleSession(new Error("cancel"));
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(
        () => this.settleSession(new Error(this.messages.timeout())),
        SESSION_TIMEOUT_MS,
      );
      this.pendingSession = { resolve, reject, timer };
      try {
        this.bridge.requestSession();
      } catch (e) {
        this.settleSession(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  async endSession(stopReceiver = true): Promise<void> {
    this.safe(() => this.bridge.endSession(stopReceiver));
  }

  loadMedia(url: string, contentType: string, metadata: CastMediaMetadata, opts: CastLoadOptions): Promise<void> {
    const id = this.nextLoadId++;
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingLoads.delete(id);
        reject(new Error(this.messages.timeout()));
      }, LOAD_TIMEOUT_MS);
      this.pendingLoads.set(id, { resolve, reject, timer });
      try {
        this.bridge.load(androidLoadPayload(id, url, contentType, metadata, opts));
      } catch (e) {
        this.pendingLoads.delete(id);
        clearTimeout(timer);
        reject(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  async play(): Promise<void> {
    this.safe(() => this.bridge.play());
  }

  async pause(): Promise<void> {
    this.safe(() => this.bridge.pause());
  }

  async seek(seconds: number): Promise<void> {
    if (!Number.isFinite(seconds)) return;
    this.safe(() => this.bridge.seek(Math.max(0, seconds)));
  }

  async stop(): Promise<void> {
    this.safe(() => this.bridge.stop());
  }

  getStatus(): CastStatus {
    return this.status;
  }

  subscribe(listener: (s: CastStatus) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}

/** The native backend when running in the Android shell (bridge present), else null. */
export function createAndroidCastBackend(messages?: AndroidCastMessages): CastBackend | null {
  const bridge = androidCastBridge(typeof window === "undefined" ? null : window);
  return bridge ? new AndroidCastBackend(bridge, messages) : null;
}
