/**
 * Keeps the app's single audio output stream warm across pauses and track
 * changes (Linux desktop shell only).
 *
 * With the WebKitGTK audio mixer on (the Tauri shell turns it on and sets
 * `window.__REKORD_SHARED_AUDIO_OUTPUT__`, see `linux_audio.rs`), every deck
 * and Web Audio feed one shared GStreamer mixer with one PipeWire/Pulse
 * stream. That mixer still pauses — corking the stream — whenever nothing is
 * playing, which is every pause and the instant between two tracks; each cork
 * wakes the sound server, WirePlumber, effects hosts and the shell's mixer
 * UI. An idle AudioContext is one more producer that never pauses: while it
 * runs the stream stays open and silent, like Chromium's output in the legacy
 * Electron app. After a while without playback it is suspended, so a long
 * pause still releases the device (Chromium also drops its stream after
 * about 10–15 s).
 *
 * Nothing is connected to the context: it renders silence and costs only its
 * render thread, at the "playback" latency (large buffers).
 */

/** The part of AudioContext this needs (a fake in tests). */
export type KeepAliveContext = {
  readonly state: string;
  resume(): Promise<void>;
  suspend(): Promise<void>;
};

export type OutputKeepAliveOptions = {
  /** Creates the context; null when Web Audio is unavailable. */
  create: () => KeepAliveContext | null;
  /** How long the output stays warm after playback stops. */
  idleMs: number;
};

export class OutputKeepAlive {
  private ctx: KeepAliveContext | null = null;
  private failed = false;
  private playing = false;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly opts: OutputKeepAliveOptions;

  constructor(opts: OutputKeepAliveOptions) {
    this.opts = opts;
  }

  /**
   * Before starting playback, from the user's gesture when there is one: the
   * first call creates the context (browsers only let it run after a
   * gesture), later calls wake it.
   */
  wake(): void {
    this.cancelIdle();
    const ctx = this.context();
    if (ctx && ctx.state === "suspended") void ctx.resume().catch(() => {});
  }

  /** Playback started or stopped: stay warm while playing, cool down after. */
  sync(playing: boolean): void {
    if (playing === this.playing) return;
    this.playing = playing;
    if (playing) {
      this.wake();
      return;
    }
    if (!this.ctx) return;
    this.cancelIdle();
    this.idleTimer = setTimeout(() => {
      this.idleTimer = null;
      if (this.playing) return;
      const ctx = this.ctx;
      if (ctx && ctx.state === "running") void ctx.suspend().catch(() => {});
    }, this.opts.idleMs);
  }

  /** The context's state, for diagnostics and tests. */
  get state(): string {
    return this.ctx?.state ?? "none";
  }

  private context(): KeepAliveContext | null {
    if (this.ctx || this.failed) return this.ctx;
    try {
      this.ctx = this.opts.create();
    } catch {
      this.ctx = null;
    }
    if (!this.ctx) this.failed = true;
    return this.ctx;
  }

  private cancelIdle() {
    if (this.idleTimer == null) return;
    clearTimeout(this.idleTimer);
    this.idleTimer = null;
  }
}

/** How long the shared output stays open after playback stops (like Chromium). */
export const OUTPUT_KEEPALIVE_IDLE_MS = 15_000;

/**
 * The keep-alive for this page: only when the shell reports the shared WebKit
 * audio mixer. Elsewhere (browsers, Android, macOS/Windows shells, WebKitGTK
 * without the mixer) an extra context would only add one more stream.
 */
export function sharedOutputKeepAlive(): OutputKeepAlive | null {
  if (typeof window === "undefined") return null;
  const w = window as unknown as {
    __REKORD_SHARED_AUDIO_OUTPUT__?: boolean;
    AudioContext?: typeof AudioContext;
    webkitAudioContext?: typeof AudioContext;
  };
  if (w.__REKORD_SHARED_AUDIO_OUTPUT__ !== true) return null;
  const Ctor = w.AudioContext ?? w.webkitAudioContext;
  if (!Ctor) return null;
  return new OutputKeepAlive({
    create: () => new Ctor({ latencyHint: "playback" }),
    idleMs: OUTPUT_KEEPALIVE_IDLE_MS,
  });
}
