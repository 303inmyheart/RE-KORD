/**
 * Plectr run state + judging (port of the pure parts of legacy
 * `GameCanvas.tsx`): taps, holds (incl. cross-lane slides), misses, combo,
 * score multiplier, run completion. No DOM, no timers: the stage component
 * owns rendering and passes `now` (performance.now) in.
 */
import { COMBO_MILESTONES, HIT_WINDOWS, LANES } from "./config";
import { buildGameResult } from "./runResult";
import { createSongClockState, type SongClockState } from "./smoothSongClock";
import type { ChartNote, GameResult } from "./types";

export type FeedbackCode =
  | "ready"
  | "paused"
  | "stayOnTrack"
  | "perfect"
  | "good"
  | "early"
  | "late"
  | "miss"
  | "slideMiss"
  | "holdMiss"
  | "complete";

export type LaneFlash = { kind: "hit" | "miss"; until: number } | null;

export type RunState = {
  notes: ChartNote[];
  activeHolds: ChartNote[];
  score: number;
  combo: number;
  maxCombo: number;
  hits: number;
  misses: number;
  /** Judged as perfect / good / ok (early-late) — shown on the results screen. */
  perfects: number;
  goods: number;
  oks: number;
  /** The "ok" hits split by side. */
  earlies: number;
  lates: number;
  /** Notes passed without judging (joined late, seek forward, grace period). */
  skipped: number;
  /**
   * The part of `skipped` jumped over by joining late or seeking forward. The
   * grace after a resume is not in it: a paused run is still a full run.
   */
  jumped: number;
  /** Holds started (a hit) then dropped (a miss): one note, two judgements. */
  holdFails: number;
  /** Song time until which an unplayed note is skipped, not missed. */
  graceUntil: number;
  /** Last hit offsets (ms, + = late), newest last — accuracy meter. */
  offsets: number[];
  /** Last combo milestone reached and a counter that bumps with each. */
  milestone: number;
  milestonePulse: number;
  /** performance.now() of the last milestone (flash band). */
  milestoneAt: number;
  /** Recent hits for particles: lane, kind, performance.now(). */
  bursts: { lane: number; perfect: boolean; at: number }[];
  feedback: FeedbackCode;
  feedbackPulse: number;
  songTime: number;
  started: boolean;
  pressedLanes: boolean[];
  laneFlash: LaneFlash[];
  missScanIndex: number;
  finished: boolean;
  /** Notes exhausted, waiting for the track to end in the player. */
  awaitingTrackEnd: boolean;
  /** End of the last note (head + hold), computed once. */
  lastEnd: number;
} & SongClockState;

/** Side effects the engine asks the host for (vibration on miss). */
export type JudgeEnv = {
  now: number;
  onMiss?: () => void;
  onMilestone?: (combo: number) => void;
};

const OFFSET_HISTORY = 24;
const BURST_HISTORY = 16;

const FLASH_MS = 420;

export function initialRunState(notes: ChartNote[]): RunState {
  let lastEnd = 0;
  for (const note of notes) lastEnd = Math.max(lastEnd, note.time + note.duration);
  return {
    lastEnd,
    notes: notes.map((note) => ({ ...note })),
    activeHolds: [],
    score: 0,
    combo: 0,
    maxCombo: 0,
    hits: 0,
    misses: 0,
    perfects: 0,
    goods: 0,
    oks: 0,
    earlies: 0,
    lates: 0,
    skipped: 0,
    jumped: 0,
    holdFails: 0,
    graceUntil: -Infinity,
    offsets: [],
    milestone: 0,
    milestonePulse: 0,
    milestoneAt: 0,
    bursts: [],
    feedback: "ready",
    feedbackPulse: 0,
    songTime: 0,
    started: false,
    pressedLanes: LANES.map(() => false),
    laneFlash: LANES.map(() => null),
    missScanIndex: 0,
    finished: false,
    awaitingTrackEnd: false,
    ...createSongClockState(),
  };
}

export function runResultOf(state: RunState, failed = false): GameResult {
  return buildGameResult(
    {
      score: state.score,
      maxCombo: state.maxCombo,
      hits: state.hits,
      misses: state.misses,
    },
    failed,
  );
}

/** Score multiplier for the current combo: 1x, +1 every 12 up to 4x. */
export function comboMultiplier(combo: number): number {
  return 1 + Math.min(3, Math.floor(Math.max(0, combo) / 12));
}

/** Combo values that trigger a milestone (25, 50, 100, then every 100). */
export function isComboMilestone(combo: number): boolean {
  if (combo <= 0) return false;
  return (COMBO_MILESTONES as readonly number[]).includes(combo) || combo % 100 === 0;
}

/**
 * Grace period: unplayed notes due before `songTime + seconds` are skipped
 * instead of missed (after joining mid-song, a difficulty change, a resume).
 */
export function startGrace(state: RunState, songTime: number, seconds: number): void {
  state.graceUntil = Math.max(state.graceUntil, songTime + seconds);
}

/** Notes judged so far (skipped notes excluded; a dropped hold is one note). */
export function judgedNotes(state: RunState): number {
  return state.hits + state.misses - state.holdFails;
}

export function noteEndLane(note: ChartNote): number {
  return note.endLane ?? note.lane;
}

export function holdReleaseAt(note: ChartNote): number {
  return note.time + note.duration;
}

export function setFeedback(state: RunState, feedback: FeedbackCode): void {
  state.feedback = feedback;
  state.feedbackPulse += 1;
}

export function flashLane(state: RunState, lane: number, kind: "hit" | "miss", now: number): void {
  state.laneFlash[lane] = { kind, until: now + FLASH_MS };
}

export function activeFlash(state: RunState, lane: number, now: number): "hit" | "miss" | null {
  const flash = state.laneFlash[lane];
  return flash && flash.until > now ? flash.kind : null;
}

export function isHoldingLane(state: RunState, lane: number): boolean {
  return state.activeHolds.some(
    (n) => n.holding && !n.completed && (n.lane === lane || noteEndLane(n) === lane),
  );
}

export function lowerBoundNoteIndex(notes: ChartNote[], time: number): number {
  let lo = 0;
  let hi = notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (notes[mid].time < time) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export function upperBoundNoteIndex(notes: ChartNote[], time: number): number {
  let lo = 0;
  let hi = notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (notes[mid].time <= time) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export function isChartRunComplete(state: RunState, songTime: number): boolean {
  if (!state.notes.length) return false;
  // Cheap test first: this runs every frame.
  if (songTime < state.lastEnd + HIT_WINDOWS.ok) return false;
  return state.notes.every((note) => note.hit || note.missed);
}

/**
 * Live sync: a run joined mid-track, or a seek forward, would otherwise turn
 * every note already behind the hit line into a miss. Those notes are marked
 * done without counting as hits or misses.
 */
export function skipNotesBefore(state: RunState, songTime: number): void {
  const cut = lowerBoundNoteIndex(state.notes, songTime - HIT_WINDOWS.ok);
  for (let i = state.missScanIndex; i < cut; i += 1) {
    const note = state.notes[i];
    if (!note.hit && !note.missed) {
      skipNote(state, note);
      state.jumped += 1;
    }
  }
  state.missScanIndex = Math.max(state.missScanIndex, cut);
}

function skipNote(state: RunState, note: ChartNote): void {
  note.completed = true;
  note.hit = true;
  note.missed = false;
  note.holding = false;
  state.skipped += 1;
}

function registerHit(
  state: RunState,
  delta: number,
  noteTime: number,
  lane: number,
  env: JudgeEnv,
): FeedbackCode {
  state.hits += 1;
  state.combo += 1;
  state.maxCombo = Math.max(state.maxCombo, state.combo);
  const perfect = delta <= HIT_WINDOWS.perfect;
  const good = delta <= HIT_WINDOWS.good;
  const late = state.songTime > noteTime;
  const points = perfect ? 300 : good ? 180 : 90;
  state.score += Math.round(points * comboMultiplier(state.combo));
  if (perfect) state.perfects += 1;
  else if (good) state.goods += 1;
  else {
    state.oks += 1;
    if (late) state.lates += 1;
    else state.earlies += 1;
  }
  state.offsets.push(Math.round((state.songTime - noteTime) * 1000));
  if (state.offsets.length > OFFSET_HISTORY) state.offsets.shift();
  state.bursts.push({ lane, perfect, at: env.now });
  if (state.bursts.length > BURST_HISTORY) state.bursts.shift();
  if (isComboMilestone(state.combo)) {
    state.milestone = state.combo;
    state.milestonePulse += 1;
    state.milestoneAt = env.now;
    env.onMilestone?.(state.combo);
  }
  return perfect ? "perfect" : good ? "good" : late ? "late" : "early";
}

function failHoldRelease(state: RunState, note: ChartNote, message: FeedbackCode, env: JudgeEnv): void {
  note.holding = false;
  note.missed = true;
  state.combo = 0;
  state.misses += 1;
  state.holdFails += 1;
  flashLane(state, note.lane, "miss", env.now);
  const endLane = noteEndLane(note);
  if (endLane !== note.lane) flashLane(state, endLane, "miss", env.now);
  setFeedback(state, message);
}

function isHoldLanePressed(state: RunState, note: ChartNote): boolean {
  return state.pressedLanes[note.lane] || state.pressedLanes[noteEndLane(note)];
}

function completeHeldNote(note: ChartNote): void {
  note.holding = false;
  note.completed = true;
}

/** Notes that scrolled past the hit window without being played. */
export function applyMisses(state: RunState, songTime: number, env: JudgeEnv): void {
  const missLine = songTime - HIT_WINDOWS.ok;
  const notes = state.notes;
  let i = state.missScanIndex;
  while (i < notes.length && notes[i].time < missLine) {
    const note = notes[i];
    i += 1;
    if (note.hit || note.missed) continue;
    if (note.time < state.graceUntil) {
      skipNote(state, note);
      continue;
    }
    note.missed = true;
    state.combo = 0;
    state.misses += 1;
    flashLane(state, note.lane, "miss", env.now);
    setFeedback(state, "miss");
    env.onMiss?.();
  }
  state.missScanIndex = i;
}

/** Ends holds that reached their tail, fails the ones let go too early. */
export function completeHeldNotes(state: RunState, songTime: number, env: JudgeEnv): void {
  if (!state.activeHolds.length) return;
  for (const note of state.activeHolds) {
    if (!note.holding || note.completed || note.missed) continue;
    const holdEnd = holdReleaseAt(note);

    if (songTime >= holdEnd - HIT_WINDOWS.holdSlack) {
      const requiredLane = noteEndLane(note);
      if (requiredLane !== note.lane && !state.pressedLanes[requiredLane]) {
        note.holding = false;
        note.missed = true;
        state.combo = 0;
        state.misses += 1;
        state.holdFails += 1;
        flashLane(state, note.lane, "miss", env.now);
        flashLane(state, requiredLane, "miss", env.now);
        setFeedback(state, "slideMiss");
        env.onMiss?.();
        continue;
      }
    }

    if (!isHoldLanePressed(state, note) && songTime < holdEnd) {
      failHoldRelease(state, note, "holdMiss", env);
      env.onMiss?.();
      continue;
    }

    if (songTime >= holdEnd) completeHeldNote(note);
  }
  state.activeHolds = state.activeHolds.filter((note) => note.holding && !note.completed);
}

/** Press on a lane: starts the closest hold head in the window, if any. */
export function judgeHoldStart(state: RunState, laneIndex: number, env: JudgeEnv): boolean {
  const songTime = state.songTime;
  let best: ChartNote | null = null;
  let bestDelta = Infinity;
  const start = lowerBoundNoteIndex(state.notes, songTime - HIT_WINDOWS.ok);
  const end = upperBoundNoteIndex(state.notes, songTime + HIT_WINDOWS.ok);
  for (let i = start; i < end; i += 1) {
    const note = state.notes[i];
    if (note.type !== "hold" || note.lane !== laneIndex || note.hit || note.missed || note.holding) {
      continue;
    }
    const delta = Math.abs(note.time - songTime);
    if (delta < bestDelta) {
      best = note;
      bestDelta = delta;
    }
  }
  if (!best) return false;
  best.hit = true;
  best.holding = true;
  state.activeHolds.push(best);
  setFeedback(state, registerHit(state, bestDelta, best.time, laneIndex, env));
  flashLane(state, laneIndex, "hit", env.now);
  return true;
}

/** Release of a lane: completes or drops the hold it was carrying. */
export function judgeHoldRelease(state: RunState, laneIndex: number, env: JudgeEnv): void {
  const songTime = state.songTime;
  let best: ChartNote | null = null;
  let bestEnd = Infinity;
  for (const note of state.activeHolds) {
    if (!note.holding || note.completed || note.missed) continue;
    const endLane = noteEndLane(note);
    if (note.lane !== laneIndex && endLane !== laneIndex) continue;
    const holdEnd = holdReleaseAt(note);
    if (holdEnd < bestEnd) {
      best = note;
      bestEnd = holdEnd;
    }
  }
  if (best) {
    // Legacy: letting go before the tail is a hold miss (the slack only
    // applies to the slide lane check in `completeHeldNotes`).
    if (songTime < bestEnd) {
      failHoldRelease(state, best, "holdMiss", env);
      env.onMiss?.();
    } else {
      completeHeldNote(best);
    }
  }
  state.activeHolds = state.activeHolds.filter((n) => n.holding && !n.completed);
}

/** Press on a lane that did not start a hold: closest tap in the window. */
export function judgeTap(state: RunState, laneIndex: number, env: JudgeEnv): boolean {
  const songTime = state.songTime;
  let best: ChartNote | null = null;
  let bestDelta = Infinity;
  const start = lowerBoundNoteIndex(state.notes, songTime - HIT_WINDOWS.ok);
  const end = upperBoundNoteIndex(state.notes, songTime + HIT_WINDOWS.ok);
  for (let i = start; i < end; i += 1) {
    const note = state.notes[i];
    if (note.type !== "tap" || note.lane !== laneIndex || note.hit || note.missed) continue;
    const delta = Math.abs(note.time - songTime);
    if (delta < bestDelta) {
      best = note;
      bestDelta = delta;
    }
  }
  if (!best || bestDelta > HIT_WINDOWS.ok) {
    flashLane(state, laneIndex, "miss", env.now);
    return false;
  }
  best.hit = true;
  best.completed = true;
  flashLane(state, laneIndex, "hit", env.now);
  setFeedback(state, registerHit(state, bestDelta, best.time, laneIndex, env));
  return true;
}

/** Full press: hold head first, then tap (legacy order). */
export function pressLane(state: RunState, laneIndex: number, env: JudgeEnv): void {
  state.pressedLanes[laneIndex] = true;
  if (!judgeHoldStart(state, laneIndex, env)) judgeTap(state, laneIndex, env);
}

/** Full release: clears the lane and judges a hold being carried on it. */
export function releaseLane(state: RunState, laneIndex: number, env: JudgeEnv): void {
  const hadActiveHold = state.activeHolds.some(
    (note) =>
      note.holding &&
      !note.completed &&
      !note.missed &&
      (note.lane === laneIndex || noteEndLane(note) === laneIndex),
  );
  state.pressedLanes[laneIndex] = false;
  if (hadActiveHold) judgeHoldRelease(state, laneIndex, env);
}
