import type { GameResult } from "./types";

export interface RunStats {
  score: number;
  maxCombo: number;
  hits: number;
  misses: number;
}

export function resultGrade(accuracy: number, failed: boolean): string {
  if (failed) return "F";
  if (accuracy >= 0.95) return "S";
  if (accuracy >= 0.9) return "A";
  if (accuracy >= 0.8) return "B";
  if (accuracy >= 0.7) return "C";
  return "D";
}

export function buildGameResult(stats: RunStats, failed = false): GameResult {
  const totalJudged = stats.hits + stats.misses;
  const accuracy = totalJudged ? stats.hits / totalJudged : 0;
  return {
    failed,
    score: stats.score,
    maxCombo: stats.maxCombo,
    hits: stats.hits,
    misses: stats.misses,
    accuracy,
    grade: resultGrade(accuracy, failed),
  };
}

/** End of run: prefer the real audio duration (crossfade / end of track in the player). */
export function resolveRunEndTime(chartDuration: number, audioDuration?: number): number {
  if (typeof audioDuration === "number" && Number.isFinite(audioDuration) && audioDuration > 0) {
    return audioDuration;
  }
  return chartDuration;
}
