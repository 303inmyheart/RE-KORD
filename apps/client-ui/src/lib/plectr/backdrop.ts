/**
 * Visualizer behind the note highway (port of legacy `plectrVizBackdrop.ts`).
 *
 * It sits under a dark veil, so rendering it at full resolution every frame
 * would only steal budget from the notes (legacy measured half the frames lost
 * on mobile): it renders to a reduced offscreen canvas at 15-25 fps, composed into the
 * stage's static layer only when a new frame comes.
 */
import { VizCanvasEngine, type VizMode } from "../visualizer/vizCanvasEngine";
import { platformCaps } from "../platformCaps";
import { isCompactRenderTarget, prefersReducedMotion } from "../visualizer/renderQuality";

export type BackdropInput = {
  mode: VizMode;
  analyser: AnalyserNode | null;
  isPlaying: boolean;
};

/**
 * The user's Listen visualizer, as drawn behind the highway. DiscoWall is
 * far too heavy for a backdrop under a dark veil: it falls back to bars.
 */
export function plectrBackdropMode(mode: string | null | undefined): VizMode {
  switch (mode) {
    case "mirror":
    case "osc":
    case "oscSoft":
    case "hmb":
    case "signals":
    case "karaoke":
      return mode;
    default:
      return "bars";
  }
}

export function backdropCadence(): { scale: number; intervalMs: number } {
  if (prefersReducedMotion()) return { scale: 0.35, intervalMs: 120 };
  if (isCompactRenderTarget() || platformCaps.webkitGtk) return { scale: 0.4, intervalMs: 66 };
  return { scale: 0.5, intervalMs: 40 };
}

export class PlectrBackdrop {
  private readonly viz = new VizCanvasEngine();
  private lastMode: VizMode | null = null;
  private off: HTMLCanvasElement | null = null;
  private offCtx: CanvasRenderingContext2D | null = null;
  private lastRenderAt = 0;
  private veil: CanvasGradient | null = null;
  private veilHeight = 0;
  private veilBg = "";

  private renderOffscreen(width: number, height: number, input: BackdropInput, scale: number, bg: string) {
    const targetW = Math.max(1, Math.round(width * scale));
    const targetH = Math.max(1, Math.round(height * scale));
    if (!this.off) {
      this.off = document.createElement("canvas");
      this.offCtx = this.off.getContext("2d");
    }
    const octx = this.offCtx;
    if (!octx || !this.off) return;
    if (this.off.width !== targetW || this.off.height !== targetH) {
      this.off.width = targetW;
      this.off.height = targetH;
    }
    // Same geometry as full-res: scale the context, not the layout.
    octx.setTransform(targetW / width, 0, 0, targetH / height, 0, 0);
    octx.globalAlpha = 1;
    octx.fillStyle = bg;
    octx.fillRect(0, 0, width, height);
    octx.save();
    octx.globalAlpha = 0.45;
    this.viz.drawFrame(octx, {
      width,
      height,
      mode: input.mode,
      analyser: input.analyser,
      isPlaying: input.isPlaying,
      expanded: false,
    });
    octx.restore();
    // Dim it so lanes and notes stay readable (veil in the stage colour).
    if (!this.veil || this.veilHeight !== height || this.veilBg !== bg) {
      const g = octx.createLinearGradient(0, 0, 0, height);
      g.addColorStop(0, withAlpha(bg, 0.48));
      g.addColorStop(0.55, withAlpha(bg, 0.64));
      g.addColorStop(1, withAlpha(bg, 0.86));
      this.veil = g;
      this.veilHeight = height;
      this.veilBg = bg;
    }
    octx.fillStyle = this.veil;
    octx.fillRect(0, 0, width, height);
  }

  private stamp = 0;

  /**
   * The backdrop frame for this moment (reduced-size canvas: bg colour, viz,
   * veil) and a stamp that changes when it was re-rendered — the caller
   * composes it into its static layer only then, not every game frame.
   */
  frame(width: number, height: number, rawInput: BackdropInput, bg: string): { canvas: HTMLCanvasElement; stamp: number } | null {
    const input = { ...rawInput, mode: plectrBackdropMode(rawInput.mode) };
    if (input.mode !== this.lastMode) {
      this.viz.resetForMode(input.mode);
      this.lastMode = input.mode;
      this.lastRenderAt = 0;
    }
    const { scale, intervalMs } = backdropCadence();
    const now = performance.now();
    const targetW = Math.max(1, Math.round(width * scale));
    const targetH = Math.max(1, Math.round(height * scale));
    const sizeChanged = !this.off || this.off.width !== targetW || this.off.height !== targetH;
    // Paused: one frame, then the stage keeps the last one.
    const due = input.isPlaying ? now - this.lastRenderAt >= intervalMs : this.lastRenderAt === 0;
    if (sizeChanged || due) {
      this.renderOffscreen(width, height, input, scale, bg);
      this.lastRenderAt = now;
      this.stamp += 1;
    }
    return this.off ? { canvas: this.off, stamp: this.stamp } : null;
  }

  /** Paints the backdrop over the whole stage (`bg` = stage colour, "#rrggbb"). */
  draw(ctx: CanvasRenderingContext2D, width: number, height: number, rawInput: BackdropInput, bg: string) {
    const f = this.frame(width, height, rawInput, bg);
    if (f) ctx.drawImage(f.canvas, 0, 0, width, height);
  }

  dispose() {
    if (this.off) {
      this.off.width = 0;
      this.off.height = 0;
    }
    this.off = null;
    this.offCtx = null;
    this.veil = null;
  }
}

function withAlpha(color: string, alpha: number): string {
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
  }
  const rgb = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)/i.exec(color);
  if (rgb) return `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${alpha})`;
  return `rgba(4, 8, 14, ${alpha})`;
}

/** Heuristic for the default "low-end" stage when the user never chose. */
export function detectLowEndDevice(): boolean {
  if (prefersReducedMotion()) return true;
  const nav = typeof navigator !== "undefined" ? (navigator as Navigator & { deviceMemory?: number }) : null;
  if (!nav) return false;
  const cores = nav.hardwareConcurrency ?? 8;
  const memory = nav.deviceMemory ?? 8;
  return cores <= 4 || memory <= 3;
}
