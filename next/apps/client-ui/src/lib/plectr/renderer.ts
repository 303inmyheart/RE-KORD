/**
 * Note highway drawing: near-black stage (whatever the app theme), fixed lane
 * palette, gem notes, solid capped hold tails, bright hit line with
 * receptors, hit particles, combo milestone band, top fade and the accuracy
 * meter. Static parts (background, art, lane tints) are baked once into an
 * offscreen canvas and blitted each frame.
 */
import {
  HIT_LINE_BOTTOM_MAX_PX,
  HIT_LINE_BOTTOM_MIN_PX,
  HIT_WINDOWS,
  JUDGE_COLORS,
  LANES,
  STAGE_BG,
} from "./config";
import {
  activeFlash,
  lowerBoundNoteIndex,
  noteEndLane,
  upperBoundNoteIndex,
  type RunState,
} from "./engine";
import { clamp } from "./math";
import type { ChartNote } from "./types";

export type DrawContext = {
  cssWidth: number;
  cssHeight: number;
  hitY: number;
  laneWidth: number;
  /** Note speed, px per second (from the lead time). */
  speed: number;
  songTime: number;
  state: RunState;
  now: number;
  /** Light stage: no glow, no particles. */
  light: boolean;
  /** prefers-reduced-motion: no particles, no band pulse. */
  reducedMotion: boolean;
  /** Accuracy meter on the right edge (tablet / desktop). */
  showMeter: boolean;
  /** A visualizer was painted underneath: skip the baked background. */
  vizUnderlay: boolean;
};

/** Hit line: a little above the bottom edge of the highway (pads sit below). */
export function hitLineY(cssHeight: number): number {
  const gap = clamp(cssHeight * 0.06, HIT_LINE_BOTTOM_MIN_PX - 6, HIT_LINE_BOTTOM_MAX_PX - 12);
  return Math.max(1, cssHeight - gap);
}

export function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  const r = Math.max(0, Math.min(radius, width / 2, height / 2));
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + width - r, y);
  ctx.quadraticCurveTo(x + width, y, x + width, y + r);
  ctx.lineTo(x + width, y + height - r);
  ctx.quadraticCurveTo(x + width, y + height, x + width - r, y + height);
  ctx.lineTo(x + r, y + height);
  ctx.quadraticCurveTo(x, y + height, x, y + height - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

function laneCenterX(laneIndex: number, laneWidth: number): number {
  return laneIndex * laneWidth + laneWidth / 2;
}

function rgba(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/* ── Baked background ─────────────────────────────────────────────────── */

export type StageArt = {
  /** Small (already blurred) cover image, drawn stretched and dimmed. */
  image: CanvasImageSource | null;
  /** Album dominant colour "#rrggbb" for a faint glow at the bottom. */
  tint: string | null;
};

/**
 * Background canvas: stage colour, optional blurred cover, lane tints
 * (2% / 3.5% white alternating) and lane separators. Rebuilt on resize or
 * when the art changes, blitted once per frame.
 */
export class StageBackground {
  private canvas: HTMLCanvasElement | null = null;
  private key = "";

  get(width: number, height: number, dpr: number, art: StageArt, withBase: boolean): HTMLCanvasElement | null {
    if (typeof document === "undefined") return null;
    const key = `${width}x${height}@${dpr}:${art.image ? 1 : 0}:${art.tint ?? ""}:${withBase ? 1 : 0}`;
    if (this.canvas && key === this.key) return this.canvas;
    const c = this.canvas ?? document.createElement("canvas");
    c.width = Math.max(1, Math.round(width * dpr));
    c.height = Math.max(1, Math.round(height * dpr));
    const ctx = c.getContext("2d");
    if (!ctx) return null;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (withBase) {
      ctx.fillStyle = STAGE_BG;
      ctx.fillRect(0, 0, width, height);
      if (art.image) {
        ctx.globalAlpha = 0.22;
        // Cover the stage (portrait): crop the square art to the height.
        const side = Math.max(width, height);
        ctx.drawImage(art.image, (width - side) / 2, (height - side) / 2, side, side);
        ctx.globalAlpha = 1;
        const veil = ctx.createLinearGradient(0, 0, 0, height);
        veil.addColorStop(0, rgba(STAGE_BG, 0.55));
        veil.addColorStop(0.6, rgba(STAGE_BG, 0.7));
        veil.addColorStop(1, rgba(STAGE_BG, 0.9));
        ctx.fillStyle = veil;
        ctx.fillRect(0, 0, width, height);
      }
      if (art.tint && /^#[0-9a-f]{6}$/i.test(art.tint)) {
        const glow = ctx.createLinearGradient(0, height, 0, height * 0.55);
        glow.addColorStop(0, rgba(art.tint, 0.16));
        glow.addColorStop(1, rgba(art.tint, 0));
        ctx.fillStyle = glow;
        ctx.fillRect(0, height * 0.55, width, height * 0.45);
      }
    }
    const laneWidth = width / LANES.length;
    for (let lane = 0; lane < LANES.length; lane += 1) {
      ctx.fillStyle = `rgba(255, 255, 255, ${lane % 2 === 0 ? 0.02 : 0.035})`;
      ctx.fillRect(lane * laneWidth, 0, laneWidth, height);
      if (lane > 0) {
        ctx.fillStyle = "rgba(255, 255, 255, 0.06)";
        ctx.fillRect(Math.round(lane * laneWidth) - 0.5, 0, 1, height);
      }
    }
    this.canvas = c;
    this.key = key;
    return c;
  }

  dispose(): void {
    if (this.canvas) {
      this.canvas.width = 0;
      this.canvas.height = 0;
    }
    this.canvas = null;
    this.key = "";
  }
}

/* ── Frame ────────────────────────────────────────────────────────────── */

export function drawStage(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  const { cssWidth, cssHeight, laneWidth, state, now } = d;
  // Pressed lanes: a lane-coloured column rising from the hit line.
  for (let lane = 0; lane < LANES.length; lane += 1) {
    const flash = activeFlash(state, lane, now);
    const pressed = state.pressedLanes[lane];
    if (!pressed && flash !== "hit") continue;
    const x = lane * laneWidth;
    const g = ctx.createLinearGradient(0, d.hitY, 0, d.hitY - cssHeight * 0.55);
    g.addColorStop(0, rgba(LANES[lane]!.color, pressed ? 0.22 : 0.12));
    g.addColorStop(1, rgba(LANES[lane]!.color, 0));
    ctx.fillStyle = g;
    ctx.fillRect(x, 0, laneWidth, d.hitY);
  }
  drawMilestoneBand(ctx, d);
  drawHitLine(ctx, d);
  void cssWidth;
}

function drawMilestoneBand(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  const { state, now, cssWidth, cssHeight } = d;
  if (!state.milestoneAt) return;
  const age = now - state.milestoneAt;
  if (age < 0 || age > 700) return;
  const t = age / 700;
  const alpha = (1 - t) * (d.reducedMotion ? 0.12 : 0.22);
  const bandH = cssHeight * 0.16;
  const y = d.hitY - cssHeight * 0.42 - (d.reducedMotion ? 0 : t * cssHeight * 0.08);
  const g = ctx.createLinearGradient(0, y, 0, y + bandH);
  g.addColorStop(0, "rgba(70, 231, 255, 0)");
  g.addColorStop(0.5, `rgba(70, 231, 255, ${alpha})`);
  g.addColorStop(1, "rgba(70, 231, 255, 0)");
  ctx.fillStyle = g;
  ctx.fillRect(0, y, cssWidth, bandH);
}

function drawHitLine(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  const { cssWidth, hitY, laneWidth, state, now, light } = d;
  const holdLanes = new Set<number>();
  for (const note of state.activeHolds) {
    if (!note.holding || note.completed) continue;
    holdLanes.add(note.lane);
    holdLanes.add(noteEndLane(note));
  }
  ctx.fillStyle = "rgba(255, 255, 255, 0.85)";
  ctx.fillRect(0, hitY - 1, cssWidth, 2);
  if (!light) {
    ctx.fillStyle = "rgba(255, 255, 255, 0.08)";
    ctx.fillRect(0, hitY - 5, cssWidth, 10);
  }
  const rw = laneWidth * 0.74;
  const rh = 18;
  for (let lane = 0; lane < LANES.length; lane += 1) {
    const cx = laneCenterX(lane, laneWidth);
    const pressed = state.pressedLanes[lane] || holdLanes.has(lane);
    const flash = activeFlash(state, lane, now);
    const color = flash === "miss" ? JUDGE_COLORS.miss : LANES[lane]!.color;
    roundedRect(ctx, cx - rw / 2, hitY - rh / 2, rw, rh, 9);
    ctx.lineWidth = 2;
    ctx.strokeStyle = rgba(color, pressed || flash ? 1 : 0.35);
    ctx.stroke();
    if (pressed || flash === "hit") {
      ctx.fillStyle = rgba(color, pressed ? 0.35 : 0.2);
      ctx.fill();
    }
  }
}

function visibleTimeRange(d: DrawContext): { min: number; max: number } {
  const margin = 40;
  return {
    min: d.songTime - (d.cssHeight - d.hitY + margin) / d.speed,
    max: d.songTime + (d.hitY + margin) / d.speed,
  };
}

export function drawNotes(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  const { cssHeight, hitY, laneWidth, speed, songTime, state, light } = d;
  const { min, max } = visibleTimeRange(d);
  // Holds start well before their tail: widen the scan to the longest hold.
  let start = lowerBoundNoteIndex(state.notes, min - 3);
  let end = upperBoundNoteIndex(state.notes, max);
  for (const holdNote of state.activeHolds) {
    if (!holdNote.holding || holdNote.completed) continue;
    start = Math.min(start, holdNote.id);
    end = Math.max(end, holdNote.id + 1);
  }
  const gemW = Math.min(laneWidth * 0.7, 96);
  const gemH = clamp(laneWidth * 0.16, 12, 18);

  for (let i = start; i < end; i += 1) {
    const note = state.notes[i];
    if (!note || note.completed || note.missed) continue;
    const y = hitY - (note.time - songTime) * speed;
    const holding = note.duration > 0 && note.holding;
    if (note.duration > 0) {
      const endY = hitY - (note.time + note.duration - songTime) * speed;
      if (Math.min(y, endY) <= cssHeight + gemH && Math.max(y, endY) >= -gemH) {
        drawHoldTail(ctx, d, note, holding ? hitY : y, endY);
      }
    }
    if (holding) continue;
    if (y < -gemH || y > cssHeight + gemH) continue;
    drawGem(ctx, laneCenterX(note.lane, laneWidth), y, gemW, gemH, LANES[note.lane]!.color, light);
  }
}

function drawGem(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  w: number,
  h: number,
  color: string,
  light: boolean,
): void {
  const x = cx - w / 2;
  const y = cy - h / 2;
  if (!light) {
    // Soft glow: a wider translucent body (no shadowBlur — too slow per note).
    ctx.fillStyle = rgba(color, 0.18);
    roundedRect(ctx, x - 4, y - 4, w + 8, h + 8, h / 2 + 4);
    ctx.fill();
  }
  ctx.fillStyle = color;
  roundedRect(ctx, x, y, w, h, h / 2);
  ctx.fill();
  // Specular glint on the left (the centre column stays the pure lane colour).
  ctx.fillStyle = "rgba(255, 255, 255, 0.42)";
  roundedRect(ctx, x + h * 0.35, y + 2.5, w * 0.26, Math.max(2, h * 0.26), h * 0.13);
  ctx.fill();
}

function drawHoldTail(
  ctx: CanvasRenderingContext2D,
  d: DrawContext,
  note: ChartNote,
  fromY: number,
  endY: number,
): void {
  const { laneWidth, hitY } = d;
  const lane = LANES[note.lane]!;
  const cx = laneCenterX(note.lane, laneWidth);
  const endLane = noteEndLane(note);
  const w = clamp(laneWidth * 0.34, 12, 34);
  const alpha = note.holding ? 0.8 : 0.55;
  if (endLane !== note.lane) {
    // Cross-lane slide (the generator does not emit them today; kept for parity).
    const ex = laneCenterX(endLane, laneWidth);
    const mid = fromY + (endY - fromY) * 0.54;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.lineWidth = w * 0.7;
    ctx.strokeStyle = rgba(lane.color, alpha);
    ctx.beginPath();
    ctx.moveTo(cx, fromY);
    ctx.lineTo(cx, mid);
    ctx.lineTo(ex, mid);
    ctx.lineTo(ex, endY);
    ctx.stroke();
    return;
  }
  const top = Math.min(fromY, endY);
  const bottom = Math.min(Math.max(fromY, endY), note.holding ? hitY : Infinity);
  const height = Math.max(w, bottom - top);
  // Solid tail with a rounded cap at the far end.
  ctx.fillStyle = rgba(lane.color, alpha);
  roundedRect(ctx, cx - w / 2, top, w, height, w / 2);
  ctx.fill();
  // Bright core line: holds read as "keep pressing".
  ctx.fillStyle = lane.color;
  ctx.fillRect(cx - 1.5, top + w / 2, 3, Math.max(0, height - w / 2));
  // End cap ring.
  ctx.strokeStyle = "rgba(255, 255, 255, 0.55)";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(cx, top + w / 2, w / 2 - 1, Math.PI, 0);
  ctx.stroke();
}

/** Hit sparks: 6-8 per hit (10 on perfect), ~320 ms. */
export function drawParticles(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  if (d.light || d.reducedMotion) return;
  const { state, now, hitY, laneWidth } = d;
  for (const burst of state.bursts) {
    const age = now - burst.at;
    if (age < 0 || age > 320) continue;
    const t = age / 320;
    const count = burst.perfect ? 10 : 7;
    const cx = laneCenterX(burst.lane, laneWidth);
    const color = burst.perfect ? JUDGE_COLORS.perfect : LANES[burst.lane]!.color;
    ctx.fillStyle = rgba(color, 1 - t);
    const reach = laneWidth * (0.35 + 0.45 * t);
    for (let i = 0; i < count; i += 1) {
      // Upper half-circle fan, deterministic angles (no per-frame allocation).
      const a = Math.PI + (Math.PI * (i + 0.5)) / count;
      const r = reach * (0.6 + 0.4 * ((i * 37) % 10) / 10);
      const size = 3.5 * (1 - t) + 1;
      ctx.fillRect(cx + Math.cos(a) * r - size / 2, hitY + Math.sin(a) * r - size / 2, size, size);
    }
  }
}

/** Notes fade in under the top 8% of the highway. */
export function drawTopFade(ctx: CanvasRenderingContext2D, d: DrawContext, fade: CanvasGradient | null): void {
  if (!fade) return;
  ctx.fillStyle = fade;
  ctx.fillRect(0, 0, d.cssWidth, d.cssHeight * 0.08);
}

export function topFadeGradient(ctx: CanvasRenderingContext2D, cssHeight: number): CanvasGradient {
  const g = ctx.createLinearGradient(0, 0, 0, cssHeight * 0.08);
  g.addColorStop(0, rgba(STAGE_BG, 1));
  g.addColorStop(1, rgba(STAGE_BG, 0));
  return g;
}

/** Recent hit offsets on a 4 px bar at the right edge: centre = perfect. */
export function drawAccuracyMeter(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  if (!d.showMeter) return;
  const { cssWidth, cssHeight, state } = d;
  const x = cssWidth - 7;
  const top = cssHeight * 0.3;
  const h = cssHeight * 0.4;
  const mid = top + h / 2;
  ctx.fillStyle = "rgba(255, 255, 255, 0.1)";
  ctx.fillRect(x, top, 4, h);
  const okMs = HIT_WINDOWS.ok * 1000;
  const perfH = (HIT_WINDOWS.perfect * 1000 * h) / (2 * okMs);
  ctx.fillStyle = rgba(JUDGE_COLORS.perfect, 0.25);
  ctx.fillRect(x, mid - perfH, 4, perfH * 2);
  const n = state.offsets.length;
  for (let i = 0; i < n; i += 1) {
    const off = state.offsets[i]!;
    const y = mid + clamp(off / okMs, -1, 1) * (h / 2);
    const abs = Math.abs(off) / 1000;
    const color =
      abs <= HIT_WINDOWS.perfect ? JUDGE_COLORS.perfect : abs <= HIT_WINDOWS.good ? JUDGE_COLORS.good : JUDGE_COLORS.timing;
    ctx.fillStyle = rgba(color, 0.25 + (0.75 * (i + 1)) / n);
    ctx.fillRect(x - 3, y - 1, 10, 2);
  }
  ctx.fillStyle = "rgba(255, 255, 255, 0.7)";
  ctx.fillRect(x - 2, mid - 0.5, 8, 1);
}
