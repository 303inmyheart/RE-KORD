import type { FeedbackCode } from "./engine";

export type FeedbackTone = "neutral" | "perfect" | "good" | "timing" | "miss";

export type FeedbackView = {
  tone: FeedbackTone;
  /** i18n key of the single judgement word, null = nothing on screen. */
  labelKey: string | null;
};

const VIEWS: Record<FeedbackCode, FeedbackView> = {
  ready: { tone: "neutral", labelKey: null },
  paused: { tone: "neutral", labelKey: null },
  stayOnTrack: { tone: "neutral", labelKey: null },
  complete: { tone: "neutral", labelKey: null },
  perfect: { tone: "perfect", labelKey: "plectr.judge.perfect" },
  good: { tone: "good", labelKey: "plectr.judge.good" },
  early: { tone: "timing", labelKey: "plectr.judge.early" },
  late: { tone: "timing", labelKey: "plectr.judge.late" },
  miss: { tone: "miss", labelKey: "plectr.judge.miss" },
  slideMiss: { tone: "miss", labelKey: "plectr.judge.miss" },
  holdMiss: { tone: "miss", labelKey: "plectr.judge.miss" },
};

/** Label-only judgement (one word, coloured by tone). */
export function feedbackView(code: FeedbackCode): FeedbackView {
  return VIEWS[code] ?? VIEWS.ready;
}

/** "COMBO ×N" under the judgement from this combo on. */
export const COMBO_LABEL_FROM = 16;
