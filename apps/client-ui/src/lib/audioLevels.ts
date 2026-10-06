/**
 * Pure helpers behind the player's levels and its Web Audio graph.
 *
 * Levels (crossfade per deck, sleep-timer master) are planned as linear ramps
 * on a millisecond clock. The player either schedules them on GainNodes (when
 * the analyser graph is engaged) or samples them into `HTMLMediaElement.volume`
 * on a short timer: both read the same plan, so switching between the two at
 * any moment — mid-fade included — keeps the level where it was.
 *
 * The lease counter decides when the graph is needed at all: only while some
 * consumer (visualizer, Plectr backdrop) holds the analyser.
 */

/** A linear level change from `from` at `startMs` to `to` at `endMs`. */
export type LevelRamp = {
  readonly from: number;
  readonly to: number;
  readonly startMs: number;
  readonly endMs: number;
};

export function clampLevel(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return v <= 0 ? 0 : v >= 1 ? 1 : v;
}

/** A level that does not move. */
export function constantLevel(v: number): LevelRamp {
  const l = clampLevel(v);
  return { from: l, to: l, startMs: 0, endMs: 0 };
}

/** The planned level at `nowMs`. */
export function levelAt(r: LevelRamp, nowMs: number): number {
  if (nowMs >= r.endMs || r.endMs <= r.startMs) return r.to;
  if (nowMs <= r.startMs) return r.from;
  const p = (nowMs - r.startMs) / (r.endMs - r.startMs);
  return clampLevel(r.from + (r.to - r.from) * p);
}

/** Still moving at `nowMs` (someone has to keep sampling it). */
export function rampActive(r: LevelRamp, nowMs: number): boolean {
  return r.from !== r.to && nowMs < r.endMs && r.endMs > r.startMs;
}

/**
 * New plan towards `to`, starting from wherever `r` is at `nowMs` — so a fade
 * interrupted halfway continues from its current level, never from its start.
 * `durationMs <= 0` jumps at once.
 */
export function retargetLevel(
  r: LevelRamp,
  to: number,
  nowMs: number,
  durationMs: number,
): LevelRamp {
  const target = clampLevel(to);
  if (!(durationMs > 0)) return constantLevel(target);
  const from = levelAt(r, nowMs);
  if (from === target) return constantLevel(target);
  return { from, to: target, startMs: nowMs, endMs: nowMs + durationMs };
}

/**
 * What an element plays at when no GainNode carries the level: its deck level
 * times the master (sleep fade) level. Specks are snapped so a finished fade
 * lands on exact silence / exact full volume.
 */
export function elementVolume(deckLevel: number, masterLevel: number): number {
  const v = clampLevel(deckLevel) * clampLevel(masterLevel);
  if (v < 1e-4) return 0;
  if (v > 1 - 1e-4) return 1;
  return v;
}

/**
 * An element volume snapped to a coarse grid (`step`), ends exact. Fades on
 * `HTMLMediaElement.volume` then change the level a few times per second
 * instead of on every tick: on WebKitGTK each write becomes a PulseAudio /
 * PipeWire stream-volume change that the sound server, the session's mixer
 * applets and the desktop shell all react to.
 */
export function quantizeVolume(v: number, step: number): number {
  if (!Number.isFinite(v) || v <= 0) return 0;
  if (v >= 1) return 1;
  if (!(step > 0)) return v;
  const q = Math.round(v / step) * step;
  return Math.min(1, Math.max(0, Math.round(q * 10_000) / 10_000));
}

export type LeaseCounter = {
  /** Take a lease: the returned release is idempotent. */
  acquire(): () => void;
  readonly count: number;
};

/**
 * Ref-count with edge callbacks: `onFirst` when the count leaves 0, `onLast`
 * when it returns to 0. Releasing the same lease twice counts once.
 */
export function createLeaseCounter(onFirst: () => void, onLast: () => void): LeaseCounter {
  let count = 0;
  return {
    acquire() {
      count += 1;
      if (count === 1) onFirst();
      let released = false;
      return () => {
        if (released) return;
        released = true;
        count -= 1;
        if (count === 0) onLast();
      };
    },
    get count() {
      return count;
    },
  };
}

export type LeaseSwitch = {
  /** Hold a lease while `on`, drop it otherwise. Cheap to call every frame. */
  set(on: boolean): void;
  /** Drop the lease for good (component destroyed). */
  dispose(): void;
  readonly held: boolean;
};

/** A component-side on/off handle over an `acquire()` that returns its release. */
export function createLeaseSwitch(acquire: () => () => void): LeaseSwitch {
  let release: (() => void) | null = null;
  const set = (on: boolean) => {
    if (on && !release) release = acquire();
    else if (!on && release) {
      const r = release;
      release = null;
      r();
    }
  };
  return {
    set,
    dispose() {
      set(false);
    },
    get held() {
      return release != null;
    },
  };
}
