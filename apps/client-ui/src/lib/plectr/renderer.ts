/**
 * Note highway drawing, ported from the legacy Plectr dock (`GameCanvas.tsx`,
 * "lite" path): alternating lane tints with lane-coloured edges, flat
 * receptors on the hit line that light up when pressed / hit / missed,
 * rounded lane-coloured notes, dashed hold trails that fill while held.
 *
 * Built for WebKitGTK (Tauri on Linux, every canvas op on the CPU): the
 * static part (stage colour, album glow, visualizer, lane tints, edges, hit band)
 * is composed into one device-pixel canvas and blitted 1:1 each frame; notes
 * are pre-rendered sprites (glow baked once, never `shadowBlur` per frame);
 * no gradients, filters or allocations in the frame path.
 */
import { HIT_LINE_BOTTOM_MAX_PX, HIT_LINE_BOTTOM_MIN_PX, HIT_LINE_Y, HOLD_WIDTH, JUDGE_COLORS, LANES, STAGE_BG } from "./config";
import { activeFlash, lowerBoundNoteIndex, noteEndLane, upperBoundNoteIndex, type RunState } from "./engine";
import { clamp } from "./math";
import type { ChartNote } from "./types";

export type DrawContext = {
  cssWidth: number;
  cssHeight: number;
  dpr: number;
  hitY: number;
  laneWidth: number;
  /** Note speed, px per second (from the lead time). */
  speed: number;
  songTime: number;
  state: RunState;
  now: number;
  /** Light stage: flat notes, no baked glow. */
  light: boolean;
  /** A visualizer is composed under the lanes (lanes and receptors dimmer). */
  vizUnderlay: boolean;
};

/** Hit line: ~83.5% of the canvas, 28-52 px from the bottom edge (legacy dock). */
export function hitLineY(cssHeight: number): number {
  const y = cssHeight * HIT_LINE_Y;
  return Math.min(cssHeight - HIT_LINE_BOTTOM_MIN_PX, Math.max(cssHeight - HIT_LINE_BOTTOM_MAX_PX, y));
}

/** Note size for a lane width (legacy: 62% of the lane, 14 px tall). */
export function noteSize(laneWidth: number): { w: number; h: number } {
  return { w: laneWidth * 0.62, h: 14 };
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

function rgba(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

function makeCanvas(w: number, h: number): HTMLCanvasElement | null {
  if (typeof document === "undefined") return null;
  const c = document.createElement("canvas");
  c.width = Math.max(1, w);
  c.height = Math.max(1, h);
  return c;
}

function releaseCanvas(c: HTMLCanvasElement | null): void {
  if (!c) return;
  c.width = 0;
  c.height = 0;
}

/* ── Static layer ─────────────────────────────────────────────────────── */

/**
 * Everything that does not move with the notes, in device pixels: stage
 * colour (+ album glow), an optional visualizer frame under the
 * lanes, lane tints, lane edges and the hit band. Rebuilt on resize / look
 * change; the visualizer part is recomposed only when a new viz frame comes
 * (~25 fps), so a game frame starts with a single unscaled `drawImage`.
 */
export class StageLayer {
  private base: HTMLCanvasElement | null = null;
  private lanes: HTMLCanvasElement | null = null;
  private full: HTMLCanvasElement | null = null;
  private key = "";
  private vizStamp = -1;
  private width = 0;
  private height = 0;
  private dpr = 1;

  /** Re-bakes when the size, the album tint ("#rrggbb") or the viz mode changes. */
  configure(width: number, height: number, dpr: number, hitY: number, tint: string | null, viz: boolean): void {
    const key = `${width}x${height}@${dpr}:${hitY}:${tint ?? ""}:${viz ? 1 : 0}`;
    if (key === this.key && this.full) return;
    this.key = key;
    this.width = width;
    this.height = height;
    this.dpr = dpr;
    this.vizStamp = -1;
    const bw = Math.max(1, Math.round(width * dpr));
    const bh = Math.max(1, Math.round(height * dpr));
    for (const c of [this.base, this.lanes, this.full]) releaseCanvas(c);
    this.base = makeCanvas(bw, bh);
    this.lanes = makeCanvas(bw, bh);
    this.full = makeCanvas(bw, bh);
    const bctx = this.base?.getContext("2d");
    const lctx = this.lanes?.getContext("2d");
    if (!bctx || !lctx) return;

    // Base: stage colour, album glow at the bottom.
    bctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    bctx.fillStyle = STAGE_BG;
    bctx.fillRect(0, 0, width, height);
    if (tint && /^#[0-9a-f]{6}$/i.test(tint) && !viz) {
      const glow = bctx.createLinearGradient(0, height, 0, height * 0.55);
      glow.addColorStop(0, rgba(tint, 0.14));
      glow.addColorStop(1, rgba(tint, 0));
      bctx.fillStyle = glow;
      bctx.fillRect(0, height * 0.55, width, height * 0.45);
    }

    // Lanes (legacy lite): 2% / 3.5% white alternating, lane-coloured 2 px edges.
    lctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const laneWidth = width / LANES.length;
    for (let lane = 0; lane < LANES.length; lane += 1) {
      const x = lane * laneWidth;
      lctx.globalAlpha = viz ? 0.88 : 1;
      lctx.fillStyle = lane % 2 === 0 ? "rgba(255,255,255,0.02)" : "rgba(255,255,255,0.035)";
      lctx.fillRect(x, 0, laneWidth, height);
      lctx.fillStyle = LANES[lane]!.color;
      lctx.globalAlpha = viz ? 0.05 : 0.14;
      lctx.fillRect(x + 1, 0, 2, height);
      lctx.fillRect(x + laneWidth - 3, 0, 2, height);
    }
    lctx.globalAlpha = 1;
    // Hit band under the receptors.
    lctx.fillStyle = viz ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.06)";
    lctx.fillRect(0, hitY - 2, width, 4);
    this.compose(null, 0);
  }

  /** Composes base + viz frame + lanes into the blit canvas. */
  compose(viz: HTMLCanvasElement | null, stamp: number): void {
    const fctx = this.full?.getContext("2d");
    if (!fctx || !this.base || !this.lanes) return;
    if (stamp === this.vizStamp && stamp !== 0) return;
    this.vizStamp = stamp;
    fctx.setTransform(1, 0, 0, 1, 0, 0);
    fctx.globalAlpha = 1;
    fctx.drawImage(this.base, 0, 0);
    if (viz && viz.width > 0) fctx.drawImage(viz, 0, 0, this.full!.width, this.full!.height);
    fctx.drawImage(this.lanes, 0, 0);
  }

  /** Blits the static layer (device pixels, identity transform). */
  blit(ctx: CanvasRenderingContext2D): boolean {
    if (!this.full) return false;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.drawImage(this.full, 0, 0);
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    return true;
  }

  get size(): { width: number; height: number } {
    return { width: this.width, height: this.height };
  }

  dispose(): void {
    for (const c of [this.base, this.lanes, this.full]) releaseCanvas(c);
    this.base = this.lanes = this.full = null;
    this.key = "";
  }
}

/* ── Note sprites ─────────────────────────────────────────────────────── */

/**
 * One pre-rendered note per lane at device resolution. Full stage: the
 * legacy gem (white top → lane colour → darker base) with its glow baked in;
 * light stage: the legacy dock's flat rounded note.
 */
export class NoteSprites {
  private sprites: (HTMLCanvasElement | null)[] = [];
  private key = "";
  /** Sprite padding (css px) around the note body (room for the glow). */
  pad = 0;
  w = 0;
  h = 0;
  dpr = 1;

  configure(laneWidth: number, dpr: number, light: boolean): void {
    const { w, h } = noteSize(laneWidth);
    const key = `${w.toFixed(2)}:${dpr}:${light ? 1 : 0}`;
    if (key === this.key) return;
    this.key = key;
    this.dispose(false);
    this.w = w;
    this.h = h;
    this.dpr = dpr;
    this.pad = light ? 1 : 8;
    const cw = Math.ceil((w + this.pad * 2) * dpr);
    const ch = Math.ceil((h + this.pad * 2) * dpr);
    for (const lane of LANES) {
      const c = makeCanvas(cw, ch);
      const ctx = c?.getContext("2d");
      if (!c || !ctx) {
        this.sprites.push(null);
        continue;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const x = this.pad;
      const y = this.pad;
      if (light) {
        ctx.fillStyle = lane.color;
        roundedRect(ctx, x, y, w, h, 2);
        ctx.fill();
      } else {
        const g = ctx.createLinearGradient(0, y, 0, y + h);
        g.addColorStop(0, "#ffffff");
        g.addColorStop(0.18, lane.color);
        g.addColorStop(0.55, lane.color);
        g.addColorStop(1, rgba(lane.color, 0.78));
        ctx.shadowColor = lane.shadow;
        ctx.shadowBlur = 10 * dpr;
        ctx.fillStyle = g;
        roundedRect(ctx, x, y, w, h, 3);
        ctx.fill();
        ctx.shadowBlur = 0;
        ctx.fillStyle = "rgba(0,0,0,0.18)";
        ctx.fillRect(x + 2, y + h - 3, w - 4, 2);
      }
      this.sprites.push(c);
    }
  }

  get(lane: number): HTMLCanvasElement | null {
    return this.sprites[lane] ?? null;
  }

  dispose(resetKey = true): void {
    for (const c of this.sprites) releaseCanvas(c);
    this.sprites = [];
    if (resetKey) this.key = "";
  }
}

/* ── Frame ────────────────────────────────────────────────────────────── */

/** Lanes carrying a hold right now (reused, no allocation per frame). */
const holdLanes: boolean[] = LANES.map(() => false);
const DASH: number[] = [5, 4];
const NO_DASH: number[] = [];

function laneCenterX(laneIndex: number, laneWidth: number): number {
  return laneIndex * laneWidth + laneWidth / 2;
}

function markHoldLanes(state: RunState): void {
  for (let i = 0; i < holdLanes.length; i += 1) holdLanes[i] = false;
  for (const note of state.activeHolds) {
    if (!note.holding || note.completed) continue;
    holdLanes[note.lane] = true;
    holdLanes[noteEndLane(note)] = true;
  }
}

/** Pressed / flashing lanes and the receptors (legacy lite `drawStage` + `drawReceptors`). */
export function drawLanes(ctx: CanvasRenderingContext2D, d: DrawContext): void {
  const { cssHeight, hitY, laneWidth, state, now, vizUnderlay } = d;
  markHoldLanes(state);
  for (let lane = 0; lane < LANES.length; lane += 1) {
    const x = lane * laneWidth;
    const color = LANES[lane]!.color;
    const pressed = state.pressedLanes[lane];
    const flash = activeFlash(state, lane, now);
    if (pressed) {
      ctx.fillStyle = color;
      ctx.globalAlpha = vizUnderlay ? 0.14 : 0.2;
      ctx.fillRect(x, 0, laneWidth, cssHeight);
    }
    if (pressed || flash) {
      // Brighter lane edges over the baked ones.
      ctx.fillStyle = color;
      ctx.globalAlpha = pressed
        ? vizUnderlay ? 0.2 : 0.42
        : flash === "hit"
          ? vizUnderlay ? 0.12 : 0.24
          : vizUnderlay ? 0.1 : 0.18;
      ctx.fillRect(x + 1, 0, 2, cssHeight);
      ctx.fillRect(x + laneWidth - 3, 0, 2, cssHeight);
    }
    // Receptor: a flat bar across 84% of the lane.
    const holding = holdLanes[lane];
    ctx.fillStyle =
      flash === "miss" ? JUDGE_COLORS.miss : flash === "hit" || holding || pressed ? color : "rgba(255,255,255,0.22)";
    ctx.globalAlpha = vizUnderlay && !pressed && !holding && !flash ? 0.42 : 0.88;
    ctx.fillRect(x + laneWidth * 0.08, hitY - 11, laneWidth * 0.84, 22);
  }
  ctx.globalAlpha = 1;
}

function visibleTimeRange(d: DrawContext): { min: number; max: number } {
  return {
    min: d.songTime - (d.cssHeight - d.hitY + 56) / d.speed,
    max: d.songTime + (d.hitY + 56) / d.speed,
  };
}

export function drawNotes(ctx: CanvasRenderingContext2D, d: DrawContext, sprites: NoteSprites): void {
  const { cssHeight, hitY, laneWidth, speed, songTime, state, dpr } = d;
  const { min, max } = visibleTimeRange(d);
  // Holds start well before their tail: widen the scan to the longest hold.
  let start = lowerBoundNoteIndex(state.notes, min - 3);
  let end = upperBoundNoteIndex(state.notes, max);
  for (const holdNote of state.activeHolds) {
    if (!holdNote.holding || holdNote.completed) continue;
    start = Math.min(start, holdNote.id);
    end = Math.max(end, holdNote.id + 1);
  }
  const noteW = sprites.w;
  const noteH = sprites.h;
  const yMin = -noteH - 40;
  const yMax = cssHeight + noteH + 40;
  // Holds first (tails under the heads).
  for (let i = start; i < end; i += 1) {
    const note = state.notes[i];
    if (!note || note.duration <= 0 || note.completed || note.missed) continue;
    const y = hitY - (note.time - songTime) * speed;
    const endY = hitY - (note.time + note.duration - songTime) * speed;
    if (!note.holding && (Math.min(y, endY) > yMax || Math.max(y, endY) < yMin)) continue;
    drawHoldTrail(ctx, d, note, y, endY, noteH);
  }
  // Heads: sprite blits at device pixels.
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const pad = sprites.pad;
  for (let i = start; i < end; i += 1) {
    const note = state.notes[i];
    if (!note || note.completed || note.missed || (note.duration > 0 && note.holding)) continue;
    const y = hitY - (note.time - songTime) * speed;
    if (y < yMin || y > yMax) continue;
    const sprite = sprites.get(note.lane);
    if (!sprite) continue;
    const x = laneCenterX(note.lane, laneWidth) - noteW / 2 - pad;
    ctx.drawImage(sprite, Math.round(x * dpr), Math.round((y - noteH / 2 - pad) * dpr));
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

/** Legacy hold trail: dashed outline while falling, filling up while held. */
function drawHoldTrail(
  ctx: CanvasRenderingContext2D,
  d: DrawContext,
  note: ChartNote,
  headY: number,
  holdEndY: number,
  noteHeight: number,
): void {
  const { hitY, laneWidth, songTime } = d;
  const lane = LANES[note.lane]!;
  const centerX = laneCenterX(note.lane, laneWidth);
  const endLane = noteEndLane(note);
  const trailW = HOLD_WIDTH + (note.holding ? 4 : 0);

  if (endLane !== note.lane) {
    // Cross-lane slide (the generator does not emit them today; kept for parity).
    const endX = laneCenterX(endLane, laneWidth);
    const anchorY = note.holding ? hitY : headY;
    const switchY = anchorY + (holdEndY - anchorY) * 0.54;
    ctx.globalAlpha = note.holding ? 0.72 : 1;
    ctx.strokeStyle = note.holding ? lane.color : lane.shadow;
    ctx.lineWidth = HOLD_WIDTH;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(centerX, anchorY);
    ctx.lineTo(centerX, switchY);
    ctx.lineTo(endX, switchY);
    ctx.lineTo(endX, holdEndY);
    ctx.stroke();
    ctx.fillStyle = "rgba(255,255,255,0.55)";
    ctx.beginPath();
    ctx.arc(endX, holdEndY, 8, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    return;
  }

  if (note.holding) {
    const topY = Math.min(holdEndY, hitY - noteHeight * 0.5);
    const height = Math.max(6, hitY - topY);
    const progress = clamp((songTime - note.time) / Math.max(0.001, note.duration), 0, 1);
    ctx.fillStyle = "rgba(255,255,255,0.12)";
    roundedRect(ctx, centerX - trailW / 2, topY, trailW, height, 6);
    ctx.fill();
    const filledH = height * progress;
    if (filledH > 2) {
      ctx.fillStyle = lane.color;
      ctx.globalAlpha = 0.58;
      roundedRect(ctx, centerX - trailW / 2, hitY - filledH, trailW, filledH, 6);
      ctx.fill();
      ctx.globalAlpha = 1;
    }
    ctx.fillStyle = lane.color;
    ctx.fillRect(centerX - trailW / 2, hitY - noteHeight / 2, trailW, noteHeight);
    return;
  }

  const topY = Math.min(headY, holdEndY);
  const barHeight = Math.max(headY, holdEndY) - topY + noteHeight;
  roundedRect(ctx, centerX - trailW / 2, topY, trailW, barHeight, 8);
  ctx.fillStyle = "rgba(255,255,255,0.08)";
  ctx.fill();
  ctx.strokeStyle = lane.color;
  ctx.lineWidth = 2;
  ctx.setLineDash(DASH);
  ctx.stroke();
  ctx.setLineDash(NO_DASH);
}
