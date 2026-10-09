/**
 * Stall watch for the active deck: decides when a track that should be playing
 * needs a nudge (`play()` again), a reconnect (reload the stream at the current
 * position) or a push to the next track (an `ended` that led nowhere).
 *
 * Why it exists: with the phone's screen off the page is hidden, its timers are
 * throttled, and the stream can die in ways the `<audio>` element never reports as
 * an error: Wi-Fi dozing, a switch from Wi-Fi to mobile data, a tunnel that drops
 * the connection. The element then sits in `waiting` forever.
 *
 * It holds no timers of its own: the player calls `check()` from media events,
 * from a coarse interval, from `online`, and from the Android shell's heartbeat
 * (which keeps arriving when the page's own timers are frozen). Every decision
 * comes from wall-clock differences, so a check that runs late still decides right.
 *
 * Pure logic with an injected clock: `playbackWatch.test.mjs` covers it.
 */

export type WatchSample = {
  /** The player means to play (playing, or retrying a play that failed). */
  wantsPlay: boolean;
  /**
   * A load, crossfade, seek or outage the player is already handling. Its own
   * timeouts decide; past `busyCapMs` the watch stops trusting it.
   */
  busy: boolean;
  paused: boolean;
  ended: boolean;
  currentTime: number;
  /** The element reports an error. */
  error: boolean;
};

export type WatchAction =
  | { kind: "none" }
  | { kind: "play"; attempt: number; stuckMs: number }
  | { kind: "reload"; attempt: number; stuckMs: number }
  | { kind: "advance"; stuckMs: number }
  | { kind: "giveUp"; stuckMs: number };

export type WatchOptions = {
  /** No progress for this long while not paused: reload. */
  stallMs: number;
  /** A shorter wait after a network change / `online`. */
  urgentStallMs: number;
  /** Paused while it should play: `play()` after this. */
  pausedMs: number;
  /** `ended` with no advance after this: move on. */
  endedMs: number;
  /** Waits between two recoveries, in order; the last one repeats. */
  backoffMs: number[];
  /** Progress for this long after a recovery: the next problem starts from scratch. */
  healthyMs: number;
  /** A busy player for longer than this is considered stuck too. */
  busyCapMs: number;
  /** No progress for this long despite every recovery: stop trying. */
  giveUpMs: number;
};

export const DEFAULT_WATCH_OPTIONS: WatchOptions = {
  stallMs: 12_000,
  urgentStallMs: 2_000,
  pausedMs: 3_000,
  endedMs: 4_000,
  backoffMs: [2_000, 4_000, 8_000, 16_000, 30_000],
  healthyMs: 20_000,
  busyCapMs: 25_000,
  giveUpMs: 10 * 60_000,
};

export class PlaybackWatch {
  private readonly opts: WatchOptions;
  private lastTime = Number.NaN;
  private lastProgressAt = 0;
  private busySince = 0;
  private attempts = 0;
  private nextAttemptAt = 0;
  private lastRecoveryAt = 0;
  private stuckSince = 0;

  constructor(opts: Partial<WatchOptions> = {}) {
    this.opts = { ...DEFAULT_WATCH_OPTIONS, ...opts };
  }

  /** Forget everything (new track, user action): the next check starts fresh. */
  reset(now: number) {
    this.lastTime = Number.NaN;
    this.lastProgressAt = now;
    this.busySince = 0;
    this.attempts = 0;
    this.nextAttemptAt = 0;
    this.lastRecoveryAt = 0;
    this.stuckSince = 0;
  }

  /** Recoveries tried since playback was last healthy. */
  get attemptCount(): number {
    return this.attempts;
  }

  check(now: number, s: WatchSample, urgent = false): WatchAction {
    if (!s.wantsPlay) {
      this.reset(now);
      return { kind: "none" };
    }
    if (s.busy) {
      if (!this.busySince) this.busySince = now;
      if (now - this.busySince < this.opts.busyCapMs) {
        // Whatever it is doing counts as activity: no stall is measured meanwhile.
        this.lastProgressAt = now;
        this.lastTime = s.currentTime;
        return { kind: "none" };
      }
    } else {
      this.busySince = 0;
    }

    if (Number.isNaN(this.lastTime)) {
      // First look since the reset: the stall clock already runs from the reset.
      this.lastTime = s.currentTime;
    } else if (this.observe(now, s.currentTime, s.paused)) {
      return { kind: "none" };
    }

    const stuckMs = now - this.lastProgressAt;
    if (!this.stuckSince) this.stuckSince = this.lastProgressAt;
    if (now < this.nextAttemptAt) return { kind: "none" };

    if (s.ended) {
      if (stuckMs < this.opts.endedMs) return { kind: "none" };
      this.note(now);
      return { kind: "advance", stuckMs };
    }

    if (now - this.stuckSince >= this.opts.giveUpMs) {
      this.reset(now);
      return { kind: "giveUp", stuckMs };
    }

    if (s.paused && !s.error) {
      if (stuckMs < this.opts.pausedMs) return { kind: "none" };
      // A `play()` first; twice refused or ignored, reload the stream.
      const attempt = this.note(now);
      return attempt <= 2
        ? { kind: "play", attempt, stuckMs }
        : { kind: "reload", attempt, stuckMs };
    }

    const limit = s.error ? this.opts.pausedMs : urgent ? this.opts.urgentStallMs : this.opts.stallMs;
    if (stuckMs < limit) return { kind: "none" };
    const attempt = this.note(now);
    return { kind: "reload", attempt, stuckMs };
  }

  /**
   * A position seen outside `check` (every `timeupdate`): cheap, keeps the
   * stall clock right between checks. True when the position moved.
   */
  observe(now: number, currentTime: number, paused = false): boolean {
    if (Number.isNaN(this.lastTime)) {
      this.lastTime = currentTime;
      this.lastProgressAt = now;
      return true;
    }
    if (Math.abs(currentTime - this.lastTime) < 0.05) return false;
    this.lastTime = currentTime;
    this.lastProgressAt = now;
    // A jump while paused is a seek or a new track, not playback.
    if (!paused) {
      this.stuckSince = 0;
      if (this.attempts && now - this.lastRecoveryAt >= this.opts.healthyMs) {
        this.attempts = 0;
        this.nextAttemptAt = 0;
      }
    }
    return true;
  }

  private note(now: number): number {
    this.attempts += 1;
    this.lastRecoveryAt = now;
    const waits = this.opts.backoffMs;
    const wait = waits[Math.min(this.attempts - 1, waits.length - 1)] ?? 0;
    this.nextAttemptAt = now + wait;
    return this.attempts;
  }
}
