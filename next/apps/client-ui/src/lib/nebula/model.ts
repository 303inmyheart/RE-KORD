/**
 * Sonic Nebula — galaxy layout of the library (port of legacy `lib/sonicNebula.ts`).
 *
 * Every track is a star:
 *  - angle  → tempo (BPM, percentile-ranked so the disk is evenly covered);
 *  - radius → energy (calm in the centre, energetic at the rim);
 *  - colour → first mood, else genre hue, else artist hue;
 *  - size   → play count, favourites a bit bigger.
 *
 * Data sources (same order as legacy):
 *  - BPM: `track.bpm` when known (the hub has no BPM field today, legacy read it
 *    from the Plectr chart cache, which next does not have) → otherwise the same
 *    deterministic heuristic legacy used: hash of the path + duration nudge;
 *  - energy: personal moods (user prefs) → genre keywords → hash.
 *
 * Pure module (no DOM, no Svelte): unit-tested in `nebula.test.mjs`.
 */

import { parseTrackGenres } from "../genres";
import { hashSeed, TRACK_MOOD_COLORS, type TrackMoodId } from "../trackMoods";

export const NEBULA_WORLD = 2200;
export const NEBULA_CENTER = NEBULA_WORLD / 2;
export const NEBULA_GALAXY_RADIUS = NEBULA_WORLD * 0.44;
/** Minimum zoom in the interactive view (furthest out). */
export const NEBULA_MIN_ZOOM = 0.32;
export const NEBULA_MAX_ZOOM = 2.8;
/**
 * Stars drawn at most. Legacy capped at 3000; next batches small stars per
 * colour so it can afford more. Beyond the cap the most played tracks win.
 */
export const NEBULA_MAX_STARS = 8000;
/** Tracks sampled for the dashboard / embedded preview model. */
export const NEBULA_PREVIEW_TRACKS = 400;

export type NebulaCamera = { x: number; y: number; zoom: number };

export function defaultNebulaCamera(zoom = 0.52): NebulaCamera {
  return { x: NEBULA_CENTER, y: NEBULA_CENTER, zoom };
}

/** Perceived energy 0–1 per canonical mood. */
export const MOOD_ENERGY: Record<TrackMoodId, number> = {
  aggressive_heavy: 0.96,
  party_dance: 0.9,
  energy_boost: 0.86,
  motivational_drive: 0.82,
  epic_cinematic: 0.76,
  fun_quirky: 0.64,
  soulful_groovy: 0.58,
  nostalgia_retro: 0.5,
  romantic_intimacy: 0.44,
  dreamy_ethereal: 0.38,
  focus_study: 0.34,
  chill_relax: 0.26,
  sad_melancholy: 0.2,
  dark_tense: 0.14,
};

/** What the layout needs from a track (decoupled from the API `Track`). */
export type NebulaTrackInput = {
  relPath: string;
  title: string;
  artist: string;
  album: string;
  durationMs?: number | null;
  genre?: string | null;
  moods: readonly TrackMoodId[];
  /** Real BPM when some analysis knows it (none today). */
  bpm?: number | null;
};

export type NebulaStar<T extends NebulaTrackInput = NebulaTrackInput> = {
  id: string;
  track: T;
  x: number;
  y: number;
  radius: number;
  color: string;
  bpm: number;
  energy: number;
  moods: readonly TrackMoodId[];
  favorite: boolean;
  playCount: number;
};

export type NebulaFog = {
  mood: TrackMoodId | null;
  x: number;
  y: number;
  radius: number;
  color: string;
  count: number;
};

export type NebulaModel<T extends NebulaTrackInput = NebulaTrackInput> = {
  stars: NebulaStar<T>[];
  fogs: NebulaFog[];
  bpmMin: number;
  bpmMax: number;
};

export type NebulaBuildOptions = {
  playCount: (relPath: string) => number;
  favorites: ReadonlySet<string>;
  maxStars?: number;
};

export function clamp(v: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(hi, v));
}

export function hslToHex(h: number, s: number, l: number): string {
  const hue = ((h % 360) + 360) % 360;
  const sat = clamp(s / 100, 0, 1);
  const lit = clamp(l / 100, 0, 1);
  const c = (1 - Math.abs(2 * lit - 1)) * sat;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = lit - c / 2;
  let rp = 0;
  let gp = 0;
  let bp = 0;
  if (hue < 60) {
    rp = c;
    gp = x;
  } else if (hue < 120) {
    rp = x;
    gp = c;
  } else if (hue < 180) {
    gp = c;
    bp = x;
  } else if (hue < 240) {
    gp = x;
    bp = c;
  } else if (hue < 300) {
    rp = x;
    bp = c;
  } else {
    rp = c;
    bp = x;
  }
  const toByte = (v: number) =>
    Math.round(clamp((v + m) * 255, 0, 255))
      .toString(16)
      .padStart(2, "0");
  return `#${toByte(rp)}${toByte(gp)}${toByte(bp)}`;
}

/** Legacy `estimateBpm`: real BPM if known, else hash + duration heuristic. */
export function estimateBpm(track: NebulaTrackInput): number {
  const real = track.bpm;
  if (real != null && real > 40 && real < 220) return real;
  const h = hashSeed(track.relPath);
  let bpm = 68 + (h % 104);
  const dur = track.durationMs;
  if (dur) {
    if (dur < 150_000) bpm += 18;
    else if (dur < 210_000) bpm += 8;
    else if (dur > 420_000) bpm -= 14;
    else if (dur > 300_000) bpm -= 6;
  }
  return clamp(bpm, 62, 178);
}

/** Legacy `estimateEnergy`: moods → genre keywords → hash. */
export function estimateEnergy(track: NebulaTrackInput): number {
  if (track.moods.length) {
    const sum = track.moods.reduce((acc, m) => acc + MOOD_ENERGY[m], 0);
    return sum / track.moods.length;
  }
  const g = parseTrackGenres(track.genre).join(" ").toLowerCase();
  if (/metal|hardcore|punk|drum|techno|dnb|trap|grime/.test(g)) return 0.88;
  if (/dance|edm|house|disco|funk|hip.?hop|rap/.test(g)) return 0.78;
  if (/rock|pop|indie|electro/.test(g)) return 0.62;
  if (/jazz|soul|r&b|blues/.test(g)) return 0.52;
  if (/ambient|chill|lofi|classical|piano/.test(g)) return 0.28;
  if (/soundtrack|score|cinema/.test(g)) return 0.45;
  const h = hashSeed(`${track.relPath}:energy`);
  return 0.22 + (h % 700) / 1000;
}

export function starColor(track: NebulaTrackInput): string {
  const mood = track.moods[0];
  if (mood) return TRACK_MOOD_COLORS[mood];
  const genres = parseTrackGenres(track.genre);
  if (genres[0]) return hslToHex(hashSeed(genres[0]) % 360, 58, 56);
  return hslToHex(hashSeed(track.artist) % 360, 42, 62);
}

export function starRadius(playCount: number, favorite: boolean): number {
  const base = favorite ? 5.2 : 3.4;
  if (playCount <= 0) return base;
  return clamp(base + Math.log2(playCount + 1) * 1.35, base, 11);
}

function jitter(relPath: string, spread: number): [number, number] {
  const h = hashSeed(`${relPath}:pos`);
  const a = (h % 10_000) / 10_000;
  const b = ((h / 10_000) % 10_000) / 10_000;
  return [(a - 0.5) * spread, (b - 0.5) * spread];
}

function targetGalaxyRadius(energy: number, theta: number, hash: number): number {
  const ring = 0.05 + Math.pow(energy, 1.15) * 0.93;
  const ripple = Math.sin(theta * 3 + hash * 0.003) * 14;
  return clamp(ring * NEBULA_GALAXY_RADIUS + ripple, 12, NEBULA_GALAXY_RADIUS * 0.98);
}

function galaxyPosition(relPath: string, bpmNorm: number, energy: number): [number, number] {
  const hash = hashSeed(relPath);
  const theta = bpmNorm * Math.PI * 2 - Math.PI / 2;
  const arm = ((hash % 5) * Math.PI * 2) / 5;
  const wobble = ((hash % 1000) / 1000 - 0.5) * 0.4;
  const angle = theta + arm * 0.11 + wobble;
  const r = targetGalaxyRadius(energy, theta, hash);
  const [jx, jy] = jitter(relPath, 14);
  return [NEBULA_CENTER + Math.cos(angle) * r + jx, NEBULA_CENTER + Math.sin(angle) * r + jy];
}

/** Percentile rank 0–1 with deterministic tie spreading (legacy). */
export function percentileRanks(
  values: readonly number[],
  tieKey: (index: number) => string = (i) => String(i),
): number[] {
  const order = values
    .map((v, i) => ({ v, i, tie: hashSeed(tieKey(i)) % 1_000_000 }))
    .sort((a, b) => a.v - b.v || a.tie - b.tie);
  const ranks = new Array<number>(values.length).fill(0);
  let i = 0;
  while (i < order.length) {
    let j = i;
    while (j + 1 < order.length && order[j + 1]!.v === order[i]!.v) j += 1;
    const center = order.length <= 1 ? 0.5 : (i + j) / 2 / (order.length - 1);
    const groupN = j - i + 1;
    const band = Math.min(0.12, (groupN / order.length) * 0.34);
    for (let k = i; k <= j; k += 1) {
      const local = groupN > 1 ? (k - i) / (groupN - 1) - 0.5 : 0;
      ranks[order[k]!.i] = clamp(center + local * band * 2, 0, 1);
    }
    i = j + 1;
  }
  return ranks;
}

/** Spatial hash: integer cell key without string allocation. */
function cellKey(cx: number, cy: number): number {
  return (cx + 1024) * 4096 + (cy + 1024);
}

export type NebulaSpatialGrid<T extends NebulaTrackInput = NebulaTrackInput> = {
  cell: number;
  buckets: Map<number, NebulaStar<T>[]>;
};

export function buildNebulaSpatialGrid<T extends NebulaTrackInput>(
  stars: readonly NebulaStar<T>[],
  cell = 72,
): NebulaSpatialGrid<T> {
  const buckets = new Map<number, NebulaStar<T>[]>();
  for (const star of stars) {
    const key = cellKey(Math.floor(star.x / cell), Math.floor(star.y / cell));
    const list = buckets.get(key);
    if (list) list.push(star);
    else buckets.set(key, [star]);
  }
  return { cell, buckets };
}

function relaxGalaxyStars(stars: NebulaStar[], layoutEnergies: readonly number[]) {
  if (stars.length < 2) return;
  const cell = 52;
  const maxR = NEBULA_GALAXY_RADIUS + 24;
  const passes = stars.length > 900 ? 2 : 4;
  const pushScale = stars.length > 900 ? 0.22 : stars.length > 400 ? 0.32 : 0.42;
  const pullScale = stars.length > 900 ? 0.045 : 0.1;
  for (let pass = 0; pass < passes; pass += 1) {
    const grid = buildNebulaSpatialGrid(stars, cell);
    for (let si = 0; si < stars.length; si += 1) {
      const star = stars[si]!;
      const layoutEnergy = layoutEnergies[si] ?? star.energy;
      const gx = Math.floor(star.x / cell);
      const gy = Math.floor(star.y / cell);
      for (let ox = -1; ox <= 1; ox += 1) {
        for (let oy = -1; oy <= 1; oy += 1) {
          const bucket = grid.buckets.get(cellKey(gx + ox, gy + oy));
          if (!bucket) continue;
          for (const other of bucket) {
            if (other === star) continue;
            const dx = other.x - star.x;
            const dy = other.y - star.y;
            const d2 = dx * dx + dy * dy;
            const min = star.radius + other.radius + 7;
            if (d2 >= min * min || d2 < 0.25) continue;
            const d = Math.sqrt(d2);
            const push = ((min - d) / d) * pushScale;
            star.x -= dx * push;
            star.y -= dy * push;
          }
        }
      }
      const rdx = star.x - NEBULA_CENTER;
      const rdy = star.y - NEBULA_CENTER;
      const rd = Math.hypot(rdx, rdy);
      if (rd > 0.5) {
        const theta = Math.atan2(rdy, rdx);
        const targetR = targetGalaxyRadius(layoutEnergy, theta, hashSeed(star.id));
        const pull = (targetR - rd) * pullScale;
        star.x += (rdx / rd) * pull;
        star.y += (rdy / rd) * pull;
      }
      const rdx2 = star.x - NEBULA_CENTER;
      const rdy2 = star.y - NEBULA_CENTER;
      const rd2 = Math.hypot(rdx2, rdy2);
      if (rd2 > maxR) {
        star.x = NEBULA_CENTER + (rdx2 / rd2) * maxR;
        star.y = NEBULA_CENTER + (rdy2 / rd2) * maxR;
      }
    }
  }
}

export function buildNebulaModel<T extends NebulaTrackInput>(
  tracks: readonly T[],
  opts: NebulaBuildOptions,
): NebulaModel<T> {
  const maxStars = Math.max(1, opts.maxStars ?? NEBULA_MAX_STARS);
  let source: readonly T[] = tracks;
  if (tracks.length > maxStars) {
    source = [...tracks]
      .map((t) => ({ t, p: opts.playCount(t.relPath) }))
      .sort((a, b) => b.p - a.p || a.t.relPath.localeCompare(b.t.relPath))
      .slice(0, maxStars)
      .map((x) => x.t);
  }
  if (!source.length) return { stars: [], fogs: [], bpmMin: 60, bpmMax: 180 };

  const bpms = source.map(estimateBpm);
  const energies = source.map(estimateEnergy);
  const bpmRanks = percentileRanks(bpms, (i) => `${source[i]?.relPath}:bpm-rank`);
  const energyRanks = percentileRanks(energies, (i) => `${source[i]?.relPath}:energy-rank`);
  const layoutRank = (raw: number, rank: number) => clamp(raw * 0.62 + rank * 0.38, 0, 1);
  let bpmMin = Infinity;
  let bpmMax = -Infinity;
  for (const b of bpms) {
    if (b < bpmMin) bpmMin = b;
    if (b > bpmMax) bpmMax = b;
  }
  const bpmSpan = Math.max(12, bpmMax - bpmMin);

  const stars: NebulaStar<T>[] = source.map((track, i) => {
    const energy = energies[i]!;
    const [x, y] = galaxyPosition(
      track.relPath,
      layoutRank((bpms[i]! - bpmMin) / bpmSpan, bpmRanks[i]!),
      layoutRank(energy, energyRanks[i]!),
    );
    const playCount = opts.playCount(track.relPath);
    const favorite = opts.favorites.has(track.relPath);
    return {
      id: track.relPath,
      track,
      x,
      y,
      radius: starRadius(playCount, favorite),
      color: starColor(track),
      bpm: bpms[i]!,
      energy,
      moods: track.moods,
      favorite,
      playCount,
    };
  });

  relaxGalaxyStars(
    stars as NebulaStar[],
    stars.map((_, i) => layoutRank(energies[i]!, energyRanks[i]!)),
  );

  return { stars, fogs: buildNebulaFogs(stars), bpmMin, bpmMax };
}

/** Mood-coloured nebulae at each mood's centroid (+ one grey cloud for unlabeled). */
export function buildNebulaFogs(stars: readonly NebulaStar<NebulaTrackInput>[]): NebulaFog[] {
  const moodBuckets = new Map<TrackMoodId, { sx: number; sy: number; n: number }>();
  const unlabeled = { sx: 0, sy: 0, n: 0 };
  for (const star of stars) {
    const mood = star.moods[0];
    const bucket = mood
      ? (moodBuckets.get(mood) ?? moodBuckets.set(mood, { sx: 0, sy: 0, n: 0 }).get(mood)!)
      : unlabeled;
    bucket.sx += star.x;
    bucket.sy += star.y;
    bucket.n += 1;
  }
  const fogs: NebulaFog[] = [];
  for (const [mood, b] of moodBuckets) {
    if (b.n < 2) continue;
    fogs.push({
      mood,
      x: b.sx / b.n,
      y: b.sy / b.n,
      radius: clamp(90 + Math.sqrt(b.n) * 34, 120, 420),
      color: TRACK_MOOD_COLORS[mood],
      count: b.n,
    });
  }
  if (unlabeled.n >= 4) {
    fogs.push({
      mood: null,
      x: unlabeled.sx / unlabeled.n,
      y: unlabeled.sy / unlabeled.n,
      radius: clamp(80 + Math.sqrt(unlabeled.n) * 28, 100, 360),
      color: "#94a3b8",
      count: unlabeled.n,
    });
  }
  return fogs;
}

export function filterNebulaStars<T extends NebulaTrackInput>(
  stars: readonly NebulaStar<T>[],
  query: string,
): NebulaStar<T>[] {
  const q = query.trim().toLowerCase();
  if (!q) return stars as NebulaStar<T>[];
  return stars.filter((star) =>
    `${star.track.title} ${star.track.artist} ${star.track.album}`.toLowerCase().includes(q),
  );
}

/** Deterministic sample to build a light preview model. */
export function sampleTracksForNebulaBuild<T>(tracks: readonly T[], limit = NEBULA_PREVIEW_TRACKS): T[] {
  if (tracks.length <= limit) return [...tracks];
  const out: T[] = [];
  const step = tracks.length / limit;
  for (let i = 0; i < limit; i += 1) {
    out.push(tracks[Math.min(tracks.length - 1, Math.floor(i * step))]!);
  }
  return out;
}

/** Balanced sample for the dashboard preview (uniform density over the disk). */
export function sampleNebulaStarsForPreview<T extends NebulaTrackInput>(
  stars: readonly NebulaStar<T>[],
  limit = 300,
): NebulaStar<T>[] {
  if (stars.length <= limit) return [...stars];
  const sectors = 16;
  const buckets: NebulaStar<T>[][] = Array.from({ length: sectors }, () => []);
  for (const star of stars) {
    let angle = Math.atan2(star.y - NEBULA_CENTER, star.x - NEBULA_CENTER);
    angle = (Math.PI / 2 - angle + Math.PI * 8) % (Math.PI * 2);
    const sector = Math.min(sectors - 1, Math.floor((angle / (Math.PI * 2)) * sectors));
    buckets[sector]!.push(star);
  }
  const perSector = Math.max(1, Math.ceil(limit / sectors));
  const picked: NebulaStar<T>[] = [];
  const seen = new Set<string>();
  for (const bucket of buckets) {
    if (!bucket.length || picked.length >= limit) continue;
    const take = Math.min(perSector, bucket.length, limit - picked.length);
    for (let k = 0; k < take; k += 1) {
      const star = bucket[Math.min(bucket.length - 1, Math.floor(((k + 0.5) * bucket.length) / take))]!;
      if (seen.has(star.id)) continue;
      seen.add(star.id);
      picked.push(star);
    }
  }
  if (picked.length < limit) {
    const rings: NebulaStar<T>[][] = [[], [], []];
    for (const star of stars) {
      if (seen.has(star.id)) continue;
      const norm = Math.hypot(star.x - NEBULA_CENTER, star.y - NEBULA_CENTER) / NEBULA_GALAXY_RADIUS;
      rings[norm < 0.38 ? 0 : norm < 0.72 ? 1 : 2]!.push(star);
    }
    for (const bucket of rings) {
      if (!bucket.length || picked.length >= limit) continue;
      const step = Math.max(1, Math.floor(bucket.length / 12));
      for (let i = 0; i < bucket.length && picked.length < limit; i += step) {
        const star = bucket[i]!;
        if (seen.has(star.id)) continue;
        seen.add(star.id);
        picked.push(star);
      }
    }
  }
  return picked;
}

/** "Radio from here": nearest neighbours (similar tempo/energy by construction). */
export function nebulaStarsNear<T extends NebulaTrackInput>(
  stars: readonly NebulaStar<T>[],
  center: NebulaStar<T>,
  radius: number,
  limit = 48,
): NebulaStar<T>[] {
  const r2 = radius * radius;
  const hits: { star: NebulaStar<T>; d2: number }[] = [];
  for (const star of stars) {
    if (star.id === center.id) continue;
    const dx = star.x - center.x;
    const dy = star.y - center.y;
    const d2 = dx * dx + dy * dy;
    if (d2 <= r2) hits.push({ star, d2 });
  }
  hits.sort((a, b) => a.d2 - b.d2);
  return hits.slice(0, limit).map((h) => h.star);
}

export function pickNebulaStarAt<T extends NebulaTrackInput>(
  grid: NebulaSpatialGrid<T>,
  wx: number,
  wy: number,
  zoom: number,
): NebulaStar<T> | null {
  const hitR = clamp(14 / Math.max(zoom, 0.35), 8, 28);
  const cell = grid.cell;
  const cx = Math.floor(wx / cell);
  const cy = Math.floor(wy / cell);
  let best: NebulaStar<T> | null = null;
  let bestD2 = hitR * hitR;
  for (let ox = -1; ox <= 1; ox += 1) {
    for (let oy = -1; oy <= 1; oy += 1) {
      const list = grid.buckets.get(cellKey(cx + ox, cy + oy));
      if (!list) continue;
      for (const star of list) {
        const dx = star.x - wx;
        const dy = star.y - wy;
        const d2 = dx * dx + dy * dy;
        const threshold = Math.max(star.radius + 4, hitR);
        if (d2 <= threshold * threshold && d2 < bestD2) {
          best = star;
          bestD2 = d2;
        }
      }
    }
  }
  return best;
}

// ── Camera math ────────────────────────────────────────────────────────────

export type ViewportRect = { left: number; top: number; width: number; height: number };

export function screenToWorld(
  clientX: number,
  clientY: number,
  rect: ViewportRect,
  camera: NebulaCamera,
): { wx: number; wy: number } {
  const sx = clientX - rect.left;
  const sy = clientY - rect.top;
  return {
    wx: (sx - rect.width / 2) / camera.zoom + camera.x,
    wy: (sy - rect.height / 2) / camera.zoom + camera.y,
  };
}

export function worldToScreen(
  wx: number,
  wy: number,
  width: number,
  height: number,
  camera: NebulaCamera,
): { sx: number; sy: number } {
  return {
    sx: (wx - camera.x) * camera.zoom + width / 2,
    sy: (wy - camera.y) * camera.zoom + height / 2,
  };
}

/** Zoom keeping the world point under (clientX, clientY) fixed on screen. */
export function zoomCameraAt(
  rect: ViewportRect,
  clientX: number,
  clientY: number,
  prev: NebulaCamera,
  nextZoom: number,
): NebulaCamera {
  const zoom = clamp(nextZoom, NEBULA_MIN_ZOOM, NEBULA_MAX_ZOOM);
  const { wx, wy } = screenToWorld(clientX, clientY, rect, prev);
  const sx = clientX - rect.left;
  const sy = clientY - rect.top;
  return {
    x: wx - (sx - rect.width / 2) / zoom,
    y: wy - (sy - rect.height / 2) / zoom,
    zoom,
  };
}

/** World-space bounds of the viewport (+ padding) for culling. */
export function nebulaWorldBounds(w: number, h: number, camera: NebulaCamera, pad = 48) {
  const halfW = w / 2 / camera.zoom;
  const halfH = h / 2 / camera.zoom;
  return {
    minX: camera.x - halfW - pad,
    maxX: camera.x + halfW + pad,
    minY: camera.y - halfH - pad,
    maxY: camera.y + halfH + pad,
  };
}

export function isStarInBounds(
  star: { x: number; y: number; radius: number },
  b: ReturnType<typeof nebulaWorldBounds>,
): boolean {
  const hit = star.radius + 12;
  return star.x + hit >= b.minX && star.x - hit <= b.maxX && star.y + hit >= b.minY && star.y - hit <= b.maxY;
}

/** Callout card position next to the selected star, kept inside the stage. */
export function starCalloutLayout(
  anchorX: number,
  anchorY: number,
  stageW: number,
  stageH: number,
  cardW = 252,
  cardH = 74,
): { left: number; top: number } {
  let left = anchorX + 24;
  let top = anchorY - cardH / 2;
  if (left + cardW > stageW - 10) left = anchorX - cardW - 24;
  if (top < 10) top = 10;
  if (top + cardH > stageH - 10) top = stageH - cardH - 10;
  left = clamp(left, 10, Math.max(10, stageW - cardW - 10));
  return { left, top };
}
