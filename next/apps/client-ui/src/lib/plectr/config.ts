import type { Difficulty, DifficultyId, Lane } from "./types";

/** Four lanes — RE-KORD palette (hex values for the canvas). */
export const LANES: Lane[] = [
  { name: "Lane 1", key: "D", color: "#35d26f", shadow: "rgba(53, 210, 111, 0.55)" },
  { name: "Lane 2", key: "F", color: "#f97316", shadow: "rgba(249, 115, 22, 0.55)" },
  { name: "Lane 3", key: "J", color: "#eab308", shadow: "rgba(234, 179, 8, 0.5)" },
  { name: "Lane 4", key: "K", color: "#38bdf8", shadow: "rgba(56, 189, 248, 0.55)" },
];

/** Keyboard → lane (legacy mapping D F J K). */
export const LANE_KEYS: ReadonlyMap<string, number> = new Map([
  ["d", 0],
  ["f", 1],
  ["j", 2],
  ["k", 3],
]);

/** Key layouts offered in settings (KeyboardEvent.key, lower case). */
export const KEY_PRESETS = {
  dfjk: ["d", "f", "j", "k"],
  sdkl: ["s", "d", "k", "l"],
  arrows: ["arrowleft", "arrowdown", "arrowup", "arrowright"],
} as const satisfies Record<string, readonly string[]>;
export type KeyPresetId = keyof typeof KEY_PRESETS;

/** Three RE-KORD levels: Easy / Normal / Hard, Hard on the highest density. */
export const DIFFICULTIES: Difficulty[] = [
  {
    id: "easy",
    label: "Easy",
    tag: "4B Lite",
    level: 7,
    onsetAdjust: -0.06,
    cooldownBase: 0.42,
    cooldownDrop: 0.21,
    cooldownMin: 0.21,
    pulseModulo: 10,
    holdEvery: 3,
    holdIntensity: 0.38,
    swipeEvery: 0,
    swipeIntensity: 1,
  },
  {
    id: "normal",
    label: "Normal",
    tag: "4B Standard",
    level: 10,
    onsetAdjust: -0.22,
    cooldownBase: 0.29,
    cooldownDrop: 0.21,
    cooldownMin: 0.145,
    pulseModulo: 6,
    holdEvery: 2,
    holdIntensity: 0.3,
    swipeEvery: 0,
    swipeIntensity: 1,
  },
  {
    id: "hard",
    label: "Hard",
    tag: "4B Maximum",
    level: 14,
    onsetAdjust: -0.45,
    cooldownBase: 0.205,
    cooldownDrop: 0.14,
    cooldownMin: 0.085,
    pulseModulo: 3,
    holdEvery: 3,
    holdIntensity: 0.21,
    swipeEvery: 0,
    swipeIntensity: 1,
  },
];

export const DIFFICULTY_IDS: readonly DifficultyId[] = ["easy", "normal", "hard"];

export const HIT_WINDOWS = {
  perfect: 0.06,
  good: 0.105,
  ok: 0.15,
  holdSlack: 0.19,
};

/** Note speed (px/s) on the live-synced highway (legacy, fixed). */
export const NOTE_SPEED = 280;
/**
 * Note speed is a lead time: seconds a note takes from the top of the
 * highway to the hit line, whatever the stage height (1.0x = 1.6 s).
 */
export const BASE_LEAD_TIME = 1.6;
export const NOTE_SPEED_MIN = 0.8;
export const NOTE_SPEED_MAX = 1.6;
/** Latency calibration range (ms, positive = audio heard late). */
export const LATENCY_LIMIT_MS = 150;
/** No miss in the first seconds after joining mid-song / resuming. */
export const GRACE_SECONDS = 1.5;
/** A run that did not start from the top needs this share of notes judged. */
export const RECORD_MIN_JUDGED_RATIO = 0.6;
/** Visual lead-in before the audio starts (countdown pre-roll). */
export const COUNTDOWN_LEAD_IN_SECONDS = 2;
/** Optional "challenge" mode: the run fails below this accuracy. */
export const CHALLENGE_FAIL_ACCURACY = 0.3;
export const CHALLENGE_MIN_JUDGED = 20;
/** Combo milestones (flash band + haptics). */
export const COMBO_MILESTONES = [25, 50, 100] as const;
/** Fixed near-black stage, whatever the app theme. */
export const STAGE_BG = "#06080f";
/** Judgement colours. */
export const JUDGE_COLORS = {
  perfect: "#46e7ff",
  good: "#4ade80",
  timing: "#fbbf24",
  miss: "#f87171",
} as const;
/** Hit line: ~83.5% of the canvas, clamped in px from the bottom edge. */
export const HIT_LINE_Y = 0.835;
export const HIT_LINE_BOTTOM_MIN_PX = 28;
export const HIT_LINE_BOTTOM_MAX_PX = 52;
export const HOLD_WIDTH = 18;
export const CHART_LEAD_IN_SECONDS = 4;
/** A difficulty with fewer notes than this is not offered. */
export const MIN_NOTES_PER_DIFFICULTY = 12;
