/**
 * Canvas painter for Sonic Nebula (port of legacy `NebulaCanvas.tsx` drawing).
 *
 * Performance notes for 10k+ libraries:
 *  - stars are pre-sorted by radius once (not per frame);
 *  - off-screen stars are culled against the camera bounds;
 *  - small "simple" stars are batched into one path per colour (one fill per
 *    colour instead of one per star);
 *  - glow uses pre-rendered sprites per colour (drawImage, no per-star gradients);
 *  - the backdrop (background, drifting haze, guides, genre fog) is painted
 *    on its own canvas under a transparent star canvas (`NebulaCanvas`): it
 *    is repainted only when the camera or size changes, or a few times per
 *    second while it slowly drifts, so a star frame never re-blits or
 *    re-fills the full-screen gradients. On software-composited engines
 *    (WebKitGTK) this removes most of the per-frame pixel work.
 */

import {
  NEBULA_CENTER,
  NEBULA_GALAXY_RADIUS,
  clamp,
  isStarInBounds,
  nebulaWorldBounds,
  type NebulaCamera,
  type NebulaFog,
  type NebulaStar,
  type NebulaTrackInput,
} from "./model";

type Star = NebulaStar<NebulaTrackInput>;

export type NebulaPaintProps = {
  fogs: readonly NebulaFog[];
  /** Visible stars sorted by radius (small first). */
  sortedStars: readonly Star[];
  camera: NebulaCamera;
  hoveredId: string | null;
  selectedId: string | null;
  currentId: string | null;
  playing: boolean;
  currentBpm: number;
  beatEpoch: number;
  preview: boolean;
  reducedMotion: boolean;
};

const rgbCache = new Map<string, [number, number, number]>();

export function parseHex(hex: string): [number, number, number] {
  const cached = rgbCache.get(hex);
  if (cached) return cached;
  const raw = hex.replace("#", "");
  const n = parseInt(raw.length === 3 ? raw.replace(/./g, "$&$&") : raw, 16);
  const rgb: [number, number, number] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  if (rgbCache.size > 1024) rgbCache.clear();
  rgbCache.set(hex, rgb);
  return rgb;
}

const GLOW_SPRITE_SIZE = 64;
const glowSpriteCache = new Map<string, HTMLCanvasElement>();

function glowSpriteFor(color: string): HTMLCanvasElement | null {
  const cached = glowSpriteCache.get(color);
  if (cached) return cached;
  if (typeof document === "undefined") return null;
  const sprite = document.createElement("canvas");
  sprite.width = GLOW_SPRITE_SIZE;
  sprite.height = GLOW_SPRITE_SIZE;
  const sctx = sprite.getContext("2d");
  if (!sctx) return null;
  const [r, g, b] = parseHex(color);
  const half = GLOW_SPRITE_SIZE / 2;
  const grad = sctx.createRadialGradient(half, half, 0, half, half, half);
  grad.addColorStop(0, `rgba(${r},${g},${b},0.95)`);
  grad.addColorStop(0.4, `rgba(${r},${g},${b},0.35)`);
  grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
  sctx.fillStyle = grad;
  sctx.fillRect(0, 0, GLOW_SPRITE_SIZE, GLOW_SPRITE_SIZE);
  // Colours come from 14 moods, genres and artists: defensive cap.
  if (glowSpriteCache.size > 512) glowSpriteCache.clear();
  glowSpriteCache.set(color, sprite);
  return sprite;
}

function drawFog(ctx: CanvasRenderingContext2D, fog: NebulaFog, t: number, alpha: number) {
  const [r, g, b] = parseHex(fog.color);
  const rad = fog.radius * (1 + Math.sin(t * 0.00045 + fog.x * 0.01) * 0.08);
  const grad = ctx.createRadialGradient(fog.x, fog.y, 0, fog.x, fog.y, rad);
  grad.addColorStop(0, `rgba(${r},${g},${b},${0.22 * alpha})`);
  grad.addColorStop(0.45, `rgba(${r},${g},${b},${0.08 * alpha})`);
  grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
  ctx.fillStyle = grad;
  ctx.beginPath();
  ctx.arc(fog.x, fog.y, rad, 0, Math.PI * 2);
  ctx.fill();
}

type StarOpts = {
  hovered: boolean;
  current: boolean;
  playing: boolean;
  beatPhase: number;
  dimmed: boolean;
  preview: boolean;
};

function twinkleAt(star: Star, t: number) {
  return 0.84 + Math.sin(t * 0.0028 + star.x * 0.04 + star.y * 0.03) * 0.16;
}

function drawRichStar(ctx: CanvasRenderingContext2D, star: Star, t: number, o: StarOpts) {
  const [r, g, b] = parseHex(star.color);
  let radius = star.radius * twinkleAt(star, t) * (o.preview ? 1.65 : 1);
  let alpha = o.dimmed ? 0.52 : o.preview ? 0.96 : 0.9;
  if (o.current && o.playing) {
    const beat = 0.5 + 0.5 * Math.sin(o.beatPhase * Math.PI * 2);
    radius *= 1 + beat * 0.4;
    alpha = 1;
  }
  if (o.hovered) {
    radius *= 1.5;
    alpha = 1;
  }
  if (star.favorite && !o.dimmed) alpha = Math.min(1, alpha + 0.1);

  const glowR = radius * (o.current ? 4.2 : 3.4);
  const sprite = glowSpriteFor(star.color);
  if (sprite) {
    const prev = ctx.globalAlpha;
    ctx.globalAlpha = alpha;
    ctx.drawImage(sprite, star.x - glowR, star.y - glowR, glowR * 2, glowR * 2);
    ctx.globalAlpha = prev;
  }

  if (star.favorite || o.current) {
    ctx.strokeStyle = `rgba(${r},${g},${b},${alpha * 0.75})`;
    ctx.lineWidth = o.current ? 1.6 : 1.1;
    const spike = radius * 1.65;
    ctx.beginPath();
    ctx.moveTo(star.x - spike, star.y);
    ctx.lineTo(star.x + spike, star.y);
    ctx.moveTo(star.x, star.y - spike);
    ctx.lineTo(star.x, star.y + spike);
    ctx.stroke();
  }

  ctx.fillStyle = `rgba(255,255,255,${clamp(alpha, 0.25, 1)})`;
  ctx.beginPath();
  ctx.arc(star.x, star.y, radius * 0.5, 0, Math.PI * 2);
  ctx.fill();

  if (o.hovered || o.current) {
    ctx.strokeStyle = `rgba(255,255,255,${o.current ? 0.9 : 0.5})`;
    ctx.lineWidth = o.current ? 2 : 1.2;
    ctx.beginPath();
    ctx.arc(star.x, star.y, radius * 2, 0, Math.PI * 2);
    ctx.stroke();
  }
}

function drawSelectionVignette(ctx: CanvasRenderingContext2D, star: Star, t: number) {
  const [r, g, b] = parseHex(star.color);
  const rad = (108 + star.radius * 10) * (1 + Math.sin(t * 0.0022 + star.x * 0.03) * 0.07);
  const glow = ctx.createRadialGradient(star.x, star.y, 0, star.x, star.y, rad);
  glow.addColorStop(0, `rgba(${r},${g},${b},0.5)`);
  glow.addColorStop(0.38, `rgba(${r},${g},${b},0.16)`);
  glow.addColorStop(1, `rgba(${r},${g},${b},0)`);
  ctx.fillStyle = glow;
  ctx.beginPath();
  ctx.arc(star.x, star.y, rad, 0, Math.PI * 2);
  ctx.fill();
}

function drawGalaxyGuides(ctx: CanvasRenderingContext2D, zoom: number) {
  const c = NEBULA_CENTER;
  ctx.lineWidth = 1 / zoom;
  for (let ring = 1; ring <= 4; ring += 1) {
    ctx.strokeStyle = `rgba(148,163,184,${0.05 + ring * 0.012})`;
    ctx.beginPath();
    ctx.arc(c, c, (NEBULA_GALAXY_RADIUS * ring) / 4, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.strokeStyle = "rgba(99,102,241,0.07)";
  ctx.beginPath();
  for (let i = 0; i < 8; i += 1) {
    const a = (i / 8) * Math.PI * 2 - Math.PI / 2;
    ctx.moveTo(c, c);
    ctx.lineTo(c + Math.cos(a) * NEBULA_GALAXY_RADIUS, c + Math.sin(a) * NEBULA_GALAXY_RADIUS);
  }
  ctx.stroke();
  const halo = ctx.createRadialGradient(c, c, NEBULA_GALAXY_RADIUS * 0.2, c, c, NEBULA_GALAXY_RADIUS * 1.08);
  halo.addColorStop(0, "rgba(99,102,241,0)");
  halo.addColorStop(0.85, "rgba(99,102,241,0)");
  halo.addColorStop(1, "rgba(129,140,248,0.14)");
  ctx.fillStyle = halo;
  ctx.beginPath();
  ctx.arc(c, c, NEBULA_GALAXY_RADIUS * 1.08, 0, Math.PI * 2);
  ctx.fill();
}

/** Faint threads between tracks of the same album (zoomed in, few stars only). */
function drawAlbumThreads(ctx: CanvasRenderingContext2D, stars: readonly Star[], zoom: number) {
  if (zoom < 1.05 || stars.length > 600) return;
  const byAlbum = new Map<string, Star[]>();
  for (const star of stars) {
    const key = `${star.track.artist}::${star.track.album}`;
    const list = byAlbum.get(key);
    if (list) list.push(star);
    else byAlbum.set(key, [star]);
  }
  ctx.lineWidth = 0.9 / zoom;
  for (const group of byAlbum.values()) {
    if (group.length < 2 || group.length > 5) continue;
    const [r, g, b] = parseHex(group[0]!.color);
    ctx.strokeStyle = `rgba(${r},${g},${b},0.14)`;
    const sorted = [...group].sort((a, b) => a.bpm - b.bpm);
    ctx.beginPath();
    ctx.moveTo(sorted[0]!.x, sorted[0]!.y);
    for (let i = 1; i < sorted.length; i += 1) ctx.lineTo(sorted[i]!.x, sorted[i]!.y);
    ctx.stroke();
  }
}

/** Backdrop drift is slow (fog breath ~14 s period): 5 repaints/s are plenty. */
export const NEBULA_BACKDROP_REFRESH_MS = 200;

/** Screen-space background: radial gradient centred on the view (stops shared with the CSS fallback). */
export const NEBULA_BG_STOPS = [
  [0, "#0c1024"],
  [0.45, "#070a16"],
  [1, "#03040a"],
] as const;

/** Radius of the background gradient for a w×h view. */
export function nebulaBgRadius(w: number, h: number): number {
  return Math.max(w, h) * 0.72;
}

/** The same background as a CSS `background` value (static layer under the canvases). */
export function nebulaBgCss(w: number, h: number): string {
  const r = nebulaBgRadius(w, h);
  const stops = NEBULA_BG_STOPS.map(([at, c]) => `${c} ${(at * r).toFixed(1)}px`).join(", ");
  return `radial-gradient(circle ${r.toFixed(1)}px at 50% 50%, ${stops})`;
}

/**
 * Backdrop: background, drifting haze, galaxy guides and genre fog, in
 * CSS-pixel space (the caller sets the backing-store scale on `ctx`).
 * `background: false` leaves the screen-space gradient out (transparent
 * world-space layer over a CSS background, see `nebulaBgCss`).
 */
export function paintNebulaBackdrop(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  now: number,
  p: NebulaPaintProps,
  background = true,
) {
  const t = p.reducedMotion ? 0 : now;
  const zoom = p.camera.zoom;
  if (background) {
    const r = nebulaBgRadius(w, h);
    const bg = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, r);
    for (const [at, c] of NEBULA_BG_STOPS) bg.addColorStop(at, c);
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, w, h);
  } else {
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    ctx.restore();
  }

  ctx.save();
  ctx.translate(w / 2, h / 2);
  ctx.scale(zoom, zoom);
  ctx.translate(-p.camera.x, -p.camera.y);

  const parallax = (t * 0.00002) % 1;
  const layers = p.preview || w < 520 || p.sortedStars.length > 900 ? 3 : 4;
  for (let i = 0; i < layers; i += 1) {
    const gx = NEBULA_CENTER + Math.cos(parallax * Math.PI * 2 + i) * 180;
    const gy = NEBULA_CENTER + Math.sin(parallax * Math.PI * 2 + i * 1.4) * 140;
    const rad = 240 + i * 90;
    const g = ctx.createRadialGradient(gx, gy, 0, gx, gy, rad);
    g.addColorStop(0, `rgba(99,102,241,${0.055 - i * 0.01})`);
    g.addColorStop(1, "rgba(99,102,241,0)");
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(gx, gy, rad, 0, Math.PI * 2);
    ctx.fill();
  }

  drawGalaxyGuides(ctx, zoom);

  const fogAlpha = clamp(zoom, 0.35, 1.5) * (p.preview ? 1.35 : 1);
  for (const fog of p.fogs) drawFog(ctx, fog, t, fogAlpha);
  ctx.restore();
}

// Scratch reused across frames: colour → flat [x, y, r, …] for batched simple stars.
const simpleBatches = new Map<string, number[]>();

/** One full frame on a single canvas: backdrop, then stars. */
export function paintNebulaFrame(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  now: number,
  p: NebulaPaintProps,
): void {
  paintNebulaBackdrop(ctx, w, h, now, p);
  paintNebulaStars(ctx, w, h, now, p, false);
}

/**
 * Stars (album threads, selection vignette, batched small stars, glowing
 * ones) in CSS-pixel space. `clear`: wipe the canvas first (star layer over a
 * separate backdrop canvas).
 */
export function paintNebulaStars(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  now: number,
  p: NebulaPaintProps,
  clear = true,
): void {
  const zoom = p.camera.zoom;
  const t = p.reducedMotion ? 0 : now;

  if (clear) {
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    ctx.restore();
  }

  ctx.save();
  ctx.translate(w / 2, h / 2);
  ctx.scale(zoom, zoom);
  ctx.translate(-p.camera.x, -p.camera.y);

  const bounds = nebulaWorldBounds(w, h, p.camera);
  const visible: Star[] = [];
  let selectedStar: Star | null = null;
  for (const star of p.sortedStars) {
    if (star.id === p.selectedId) selectedStar = star;
    if (star.id === p.currentId || isStarInBounds(star, bounds)) visible.push(star);
  }

  if (!p.preview) drawAlbumThreads(ctx, visible, zoom);
  if (selectedStar) drawSelectionVignette(ctx, selectedStar, t);

  const hasFocus = Boolean(p.selectedId);
  const beatPhase =
    p.playing && p.currentId && p.currentBpm > 0
      ? (((t - p.beatEpoch) / 1000) * (p.currentBpm / 60)) % 1
      : 0;

  // Pass 1: batch the many small plain stars by colour.
  for (const list of simpleBatches.values()) list.length = 0;
  const rich: Star[] = [];
  for (const star of visible) {
    const focused = star.id === p.hoveredId || star.id === p.selectedId;
    const isCurrent = star.id === p.currentId;
    const simple =
      !p.preview && !focused && !isCurrent && !star.favorite && star.radius * zoom < 5.5;
    if (!simple) {
      rich.push(star);
      continue;
    }
    const dimmed = hasFocus;
    const key = dimmed ? `${star.color}|d` : star.color;
    let list = simpleBatches.get(key);
    if (!list) {
      list = [];
      simpleBatches.set(key, list);
    }
    list.push(star.x, star.y, star.radius * twinkleAt(star, t));
  }
  for (const [key, list] of simpleBatches) {
    if (!list.length) continue;
    const dimmed = key.endsWith("|d");
    const [r, g, b] = parseHex(dimmed ? key.slice(0, -2) : key);
    ctx.fillStyle = `rgba(${r},${g},${b},${(dimmed ? 0.52 : 0.9) * 0.88})`;
    ctx.beginPath();
    for (let i = 0; i < list.length; i += 3) {
      const x = list[i]!;
      const y = list[i + 1]!;
      const rad = list[i + 2]!;
      ctx.moveTo(x + rad, y);
      ctx.arc(x, y, rad, 0, Math.PI * 2);
    }
    ctx.fill();
  }
  if (simpleBatches.size > 2048) simpleBatches.clear();

  // Pass 2: glowing / focused / current stars on top.
  for (const star of rich) {
    const focused = star.id === p.hoveredId || star.id === p.selectedId;
    const isCurrent = star.id === p.currentId;
    drawRichStar(ctx, star, t, {
      hovered: focused,
      current: isCurrent,
      playing: p.playing,
      beatPhase: isCurrent ? beatPhase : 0,
      dimmed: hasFocus && !focused && !isCurrent,
      preview: p.preview,
    });
  }

  ctx.restore();
}
