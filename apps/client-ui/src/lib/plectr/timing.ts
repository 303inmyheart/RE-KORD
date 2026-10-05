/**
 * Plectr timing rules, pure (no DOM): countdown at the chart tempo, note
 * speed as a lead time, latency offset, record eligibility, tap-test
 * calibration and the daily track seed.
 */
import {
  BASE_LEAD_TIME,
  COUNTDOWN_LEAD_IN_SECONDS,
  LATENCY_LIMIT_MS,
  NOTE_SPEED_MAX,
  NOTE_SPEED_MIN,
  RECORD_MIN_JUDGED_RATIO,
} from "./config";
import { clamp, stableHash } from "./math";

/* ── Countdown ─────────────────────────────────────────────────────────── */

export type CountdownLabel = "3" | "2" | "1" | "go";

export type CountdownPlan = {
  /** Seconds per step (one beat of the chart, folded into 0.5-0.8 s). */
  stepSec: number;
  /** Seconds from the start of the countdown to the audio start. */
  total: number;
  steps: { label: CountdownLabel; at: number }[];
};

/** One beat at `bpm`, doubled / halved into the 0.5-0.8 s range. */
export function countdownStepSeconds(bpm: number): number {
  if (!Number.isFinite(bpm) || bpm <= 0) return 0.6;
  let beat = 60 / bpm;
  for (let i = 0; i < 6 && beat < 0.5; i += 1) beat *= 2;
  for (let i = 0; i < 6 && beat > 0.8 && beat / 2 >= 0.5; i += 1) beat /= 2;
  return clamp(beat, 0.5, 0.8);
}

/**
 * "3 · 2 · 1 · VIA!": one step per beat; the notes already scroll during a
 * lead-in of at least `leadIn` seconds, the audio starts on "VIA!".
 */
export function countdownPlan(bpm: number, leadIn = COUNTDOWN_LEAD_IN_SECONDS): CountdownPlan {
  const stepSec = countdownStepSeconds(bpm);
  const total = Math.max(leadIn, stepSec * 3);
  return {
    stepSec,
    total,
    steps: [
      { label: "3", at: total - stepSec * 3 },
      { label: "2", at: total - stepSec * 2 },
      { label: "1", at: total - stepSec },
      { label: "go", at: total },
    ],
  };
}

/** Label on screen `elapsed` seconds into the countdown (null = none). */
export function countdownLabelAt(plan: CountdownPlan, elapsed: number): CountdownLabel | null {
  if (elapsed >= plan.total + plan.stepSec) return null;
  let label: CountdownLabel | null = null;
  for (const step of plan.steps) {
    if (elapsed >= step.at) label = step.label;
  }
  return label;
}

/** Song time shown during the pre-roll: reaches `startAt` when the audio starts. */
export function preRollSongTime(plan: CountdownPlan, elapsed: number, startAt: number): number {
  return startAt - Math.max(0, plan.total - elapsed);
}

/* ── Note speed ────────────────────────────────────────────────────────── */

export function clampSpeedMultiplier(multiplier: number): number {
  if (!Number.isFinite(multiplier)) return 1;
  return clamp(Math.round(multiplier * 20) / 20, NOTE_SPEED_MIN, NOTE_SPEED_MAX);
}

/** Seconds from the top of the highway to the hit line. */
export function leadTimeFor(multiplier: number): number {
  return BASE_LEAD_TIME / clampSpeedMultiplier(multiplier);
}

/** Pixels per second so a note crosses `hitY - spawnY` px in `leadTime` s. */
export function pxPerSecond(hitY: number, leadTime: number, spawnY = 0): number {
  const travel = Math.max(1, hitY - spawnY);
  return travel / Math.max(0.2, leadTime);
}

/** Y of a note at `songTime` (hit line when the note is due). */
export function noteY(noteTime: number, songTime: number, hitY: number, speed: number): number {
  return hitY - (noteTime - songTime) * speed;
}

/* ── Latency ───────────────────────────────────────────────────────────── */

export function clampLatencyMs(ms: number): number {
  if (!Number.isFinite(ms)) return 0;
  return Math.round(clamp(ms, -LATENCY_LIMIT_MS, LATENCY_LIMIT_MS));
}

/** Game clock from the audio clock: audio heard `latencyMs` late → notes later. */
export function gameTimeFromAudio(audioTime: number, latencyMs: number): number {
  return audioTime - clampLatencyMs(latencyMs) / 1000;
}

/**
 * Tap test: tap offsets (ms, tap − click) → calibration. The first taps are
 * the player finding the beat: dropped. Median against stray taps.
 */
export function estimateLatencyMs(offsetsMs: number[]): number | null {
  const usable = offsetsMs.filter((v) => Number.isFinite(v) && Math.abs(v) < 400).slice(2);
  if (usable.length < 4) return null;
  const sorted = [...usable].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  const median = sorted.length % 2 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
  return clampLatencyMs(median);
}

/* ── Records ───────────────────────────────────────────────────────────── */

export type RunEligibilityInput = {
  /** "end" = the song reached its end (or the chart completed). */
  reason: "end" | "interrupted";
  failed?: boolean;
  /** The run started at the top of the song (countdown from 0:00). */
  fromStart: boolean;
  /** Notes skipped (joined late, seek forward, grace period). */
  skipped: number;
  /**
   * Notes jumped over by joining late or seeking (no resume grace). When set
   * it decides the full run instead of `skipped`: pausing never spoils one.
   */
  jumped?: number;
  /** Notes judged (hits + misses). */
  judged: number;
  totalNotes: number;
};

/**
 * Only real runs count: interrupted runs never set records nor count as
 * played. A full run (from the top, nothing jumped over) counts, as does a run
 * that judged at least 60% of the chart.
 */
export function isRecordEligible(r: RunEligibilityInput): boolean {
  if (r.reason !== "end" || r.failed) return false;
  if (r.totalNotes <= 0 || r.judged <= 0) return false;
  if (r.fromStart && (r.jumped ?? r.skipped) === 0) return true;
  return r.judged / r.totalNotes >= RECORD_MIN_JUDGED_RATIO;
}

/* ── Daily track ───────────────────────────────────────────────────────── */

/** Local calendar day, "YYYY-MM-DD". */
export function dayKey(at: Date = new Date()): string {
  const y = at.getFullYear();
  const m = String(at.getMonth() + 1).padStart(2, "0");
  const d = String(at.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Same pick for everyone on the same day and library size. */
export function dailyIndex(key: string, count: number): number {
  if (count <= 0) return -1;
  return stableHash(`plectr-daily:${key}`) % count;
}
