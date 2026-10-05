/** Plectr rhythm game — shared types (port of legacy `src/game/types.ts`). */

export type DifficultyId = "easy" | "normal" | "hard";
export type SwipeDirection = "left" | "up" | "right";
export type NoteType = "tap" | "hold" | "swipe";

export interface Difficulty {
  id: DifficultyId;
  label: string;
  tag: string;
  level: number;
  onsetAdjust: number;
  cooldownBase: number;
  cooldownDrop: number;
  cooldownMin: number;
  pulseModulo: number;
  holdEvery: number;
  holdIntensity: number;
  swipeEvery: number;
  swipeIntensity: number;
}

export interface Lane {
  name: string;
  key: string;
  color: string;
  shadow: string;
}

export interface ChartNote {
  id: number;
  type: NoteType;
  direction: SwipeDirection | null;
  time: number;
  lane: number;
  endLane: number | null;
  duration: number;
  hit: boolean;
  missed: boolean;
  holding: boolean;
  completed: boolean;
}

export interface ChartStats {
  bpm: number;
  rmsAvg: number;
  density: number;
}

export interface Chart {
  songId: string;
  baseSongId: string;
  difficulty: Difficulty;
  title: string;
  duration: number;
  notes: ChartNote[];
  stats: ChartStats;
}

export type ChartMap = Record<DifficultyId, Chart>;

export interface ChartSet {
  baseSongId: string;
  title: string;
  duration: number;
  charts: ChartMap;
}

export interface GameResult {
  failed: boolean;
  score: number;
  maxCombo: number;
  hits: number;
  misses: number;
  accuracy: number;
  grade: string;
}

/** Per-track record as stored for the account (legacy `PlectrBestScore`). */
export type PlectrBestScore = {
  score: number;
  grade: string;
  accuracy: number;
  maxCombo: number;
  hits?: number;
  misses?: number;
  updatedAt?: string;
  /** Difficulty the record was set on (not present on legacy records). */
  difficulty?: DifficultyId;
  /** Full combo (no miss) / all perfect, ever reached on this record slot. */
  fc?: boolean;
  ap?: boolean;
};
