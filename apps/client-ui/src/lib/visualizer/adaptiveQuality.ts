/**
 * Frame pacing + adaptive quality for the canvas loops (DiscoWall, Sonic
 * Nebula). Pure helpers, no DOM: unit-tested in `adaptiveQuality.test.mjs`.
 */

/**
 * A throttled loop wakes on a timer a little early and lets rAF align the draw
 * to the next frame; the slack keeps that from costing an extra rAF round trip.
 */
export function frameSlackMs(intervalMs: number): number {
  return Math.min(10, intervalMs * 0.15);
}

/** True when a throttled loop should draw now (see `frameSlackMs`). */
export function isFrameDue(now: number, lastDraw: number, intervalMs: number): boolean {
  return now - lastDraw >= intervalMs - frameSlackMs(intervalMs) - 2;
}

/** Delay before the next wake-up of a throttled loop (≥ 1 ms). */
export function nextFrameDelay(now: number, lastDraw: number, intervalMs: number): number {
  return Math.max(1, intervalMs - (now - lastDraw) - frameSlackMs(intervalMs));
}

/** Device-pixel ratio after the engine cap and the adaptive step (never below 1 unless the base is). */
export function scaledDpr(base: number, engineCap: number, step: number): number {
  const capped = Math.min(base, engineCap);
  if (capped <= 1) return capped;
  return Math.max(1, Math.round(capped * step * 100) / 100);
}

export type AdaptiveQualityOptions = {
  /** Number of quality levels (0 = full quality … levels - 1 = cheapest). */
  levels: number;
  /** Draw time (ms) above which a frame counts as slow. */
  budgetMs: number;
  /** Consecutive slow frames (smoothed) before stepping down. Default 8. */
  slowFrames?: number;
  /** Consecutive fast frames before stepping back up. Default 120. */
  fastFrames?: number;
  /** A frame is "fast" below `budgetMs * recoverRatio`. Default 0.45. */
  recoverRatio?: number;
};

/**
 * Watches the draw time of each frame and steps the quality level down when
 * the loop runs over budget, back up when it has plenty of headroom. The time
 * is smoothed (EMA) and both directions need a run of frames, with a wide
 * gap between the two thresholds, so the level does not oscillate.
 */
export class AdaptiveQuality {
  level = 0;
  private ema = -1;
  private slow = 0;
  private fast = 0;
  private readonly levels: number;
  private readonly budgetMs: number;
  private readonly slowFrames: number;
  private readonly fastFrames: number;
  private readonly recoverRatio: number;

  constructor(opts: AdaptiveQualityOptions) {
    this.levels = Math.max(1, Math.floor(opts.levels));
    this.budgetMs = opts.budgetMs;
    this.slowFrames = opts.slowFrames ?? 8;
    this.fastFrames = opts.fastFrames ?? 120;
    this.recoverRatio = opts.recoverRatio ?? 0.45;
  }

  /** Smoothed draw time (ms), or -1 before the first sample. */
  get averageMs(): number {
    return this.ema;
  }

  /** Records one frame's draw time. Returns true when the level changed. */
  push(drawMs: number): boolean {
    if (!Number.isFinite(drawMs) || drawMs < 0) return false;
    this.ema = this.ema < 0 ? drawMs : this.ema + (drawMs - this.ema) * 0.25;
    if (this.ema > this.budgetMs) {
      this.fast = 0;
      this.slow += 1;
      if (this.slow >= this.slowFrames && this.level < this.levels - 1) {
        this.level += 1;
        this.restart();
        return true;
      }
    } else if (this.ema < this.budgetMs * this.recoverRatio) {
      this.slow = 0;
      this.fast += 1;
      if (this.fast >= this.fastFrames && this.level > 0) {
        this.level -= 1;
        this.restart();
        return true;
      }
    } else {
      this.slow = 0;
      this.fast = 0;
    }
    return false;
  }

  /** Back to full quality (new content, resize…). */
  reset(): void {
    this.level = 0;
    this.restart();
  }

  private restart() {
    this.ema = -1;
    this.slow = 0;
    this.fast = 0;
  }
}
