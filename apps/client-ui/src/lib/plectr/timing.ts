/**
 * Plectr timing rules, pure (no DOM): note speed (legacy fixed px/s), latency
 * offset, record eligibility and tap-test calibration.
 */
import {
  LATENCY_LIMIT_MS,
  NOTE_SPEED,
  NOTE_SPEED_MAX,
  NOTE_SPEED_MIN,
  RECORD_MIN_JUDGED_RATIO,
} from "./config";
import { clamp } from "./math";

/* ── Note speed ────────────────────────────────────────────────────────── */

export function clampSpeedMultiplier(multiplier: number): number {
  if (!Number.isFinite(multiplier)) return 1;
  return clamp(Math.round(multiplier * 20) / 20, NOTE_SPEED_MIN, NOTE_SPEED_MAX);
}

/** Note speed in px/s for the setting's multiplier (1.0x = legacy 280 px/s). */
export function noteSpeedFor(multiplier: number): number {
  return NOTE_SPEED * clampSpeedMultiplier(multiplier);
}

/** Seconds a note is visible above the hit line at `speed` px/s. */
export function leadTimeOn(hitY: number, speed: number): number {
  return Math.max(0, hitY) / Math.max(1, speed);
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
  /** The run started at the top of the song. */
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
