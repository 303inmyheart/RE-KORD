/**
 * DiscoWall — pixel wall visualizer (port of legacy `DiscoWallVisualizer.tsx`).
 *
 * Legacy lit the wall from the Plectr rhythm chart of the current track (notes,
 * BPM). Next has no chart cache, so `LiveOnsetNotes` derives equivalent notes
 * in real time from the analyser: spectral flux on four bands (bass → lane 0 …
 * highs → lane 3) with an adaptive threshold; sustained bands become "hold"
 * notes. A tempo estimate comes from the spacing of bass onsets. The field
 * math (motifs, triads, hues, burst points) is the legacy code unchanged.
 */

import {
  buildPlectrFieldGrid,
  buildTriads,
  burstPoint,
  collectMotifs,
  createSceneStyleWeights,
  frameHues,
  mathPixelHue,
  pixelMathPhase,
  prepareConstellationTaps,
  samplePlectrFieldGrid,
  seededNoise,
  writeSceneStyleWeights,
  type ChartNote,
  type DiscoTriad,
} from "./discowallField";

const MAX_CELLS_PANEL = 1800;
const MAX_CELLS_EXPANDED = 4000;
const MIN_CELL = 9;
const MAX_CELL = 18;

function clamp(v: number, lo = 0, hi = 1) {
  return Math.max(lo, Math.min(hi, v));
}

export function hashText(text: string) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

export function lastNoteIndexAt(notes: readonly ChartNote[], time: number, slack = 0.035): number {
  if (!notes.length) return -1;
  const target = time + slack;
  let lo = 0;
  let hi = notes.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (notes[mid]!.time <= target) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

function hslToRgb(h: number, sPct: number, lPct: number): [number, number, number] {
  const hue = ((h % 360) + 360) % 360;
  const s = clamp(sPct / 100);
  const l = clamp(lPct / 100);
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = l - c / 2;
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
  return [((rp + m) * 255 + 0.5) | 0, ((gp + m) * 255 + 0.5) | 0, ((bp + m) * 255 + 0.5) | 0];
}

function fillCellRgb(
  data: Uint8ClampedArray,
  bufW: number,
  bufH: number,
  x0: number,
  y0: number,
  size: number,
  r: number,
  g: number,
  b: number,
  a: number,
) {
  const x1 = Math.min(bufW, x0 + size);
  const y1 = Math.min(bufH, y0 + size);
  const xStart = Math.max(0, x0);
  const yStart = Math.max(0, y0);
  const inv = 1 - a;
  const ra = r * a + 0.5;
  const ga = g * a + 0.5;
  const ba = b * a + 0.5;
  for (let py = yStart; py < y1; py += 1) {
    let idx = (py * bufW + xStart) * 4;
    for (let px = xStart; px < x1; px += 1) {
      data[idx] = (ra + data[idx]! * inv) | 0;
      data[idx + 1] = (ga + data[idx + 1]! * inv) | 0;
      data[idx + 2] = (ba + data[idx + 2]! * inv) | 0;
      idx += 4;
    }
  }
}

// ── Live note detection ────────────────────────────────────────────────────

/** Band edges as fractions of the analysed bins (log-ish: bass, low-mid, high-mid, highs). */
const BAND_EDGES = [0, 0.03, 0.12, 0.35, 1] as const;
const MIN_ONSET_GAP_S = 0.13;
const HOLD_MIN_S = 0.35;

export type BandFrame = readonly number[];

/**
 * Turns per-frame band energies into rhythm notes. Pure state machine (no Web
 * Audio): `push(time, bands)` with energies 0–1, read `notes` / `bpm`.
 */
export class LiveOnsetNotes {
  notes: ChartNote[] = [];
  bpm: number | null = null;
  private prev: number[] = [0, 0, 0, 0];
  private mean: number[] = [0, 0, 0, 0];
  private dev: number[] = [0.02, 0.02, 0.02, 0.02];
  private lastOnset: number[] = [-9, -9, -9, -9];
  private holdStart: (number | null)[] = [null, null, null, null];
  private nextId = 1;
  private lastTime = -1;
  private bassTimes: number[] = [];

  reset() {
    this.notes = [];
    this.bpm = null;
    this.prev = [0, 0, 0, 0];
    this.mean = [0, 0, 0, 0];
    this.dev = [0.02, 0.02, 0.02, 0.02];
    this.lastOnset = [-9, -9, -9, -9];
    this.holdStart = [null, null, null, null];
    this.lastTime = -1;
    this.bassTimes = [];
  }

  push(time: number, bands: BandFrame): void {
    // Seek backwards / new track: old notes would replay at the wrong time.
    if (time < this.lastTime - 0.25) this.reset();
    this.lastTime = time;
    for (let lane = 0; lane < 4; lane += 1) {
      const e = bands[lane] ?? 0;
      const flux = Math.max(0, e - this.prev[lane]!);
      this.prev[lane] = e;
      const m = this.mean[lane]!;
      const d = this.dev[lane]!;
      const threshold = m + d * 1.6 + 0.012;
      const isOnset = flux > threshold && time - this.lastOnset[lane]! >= MIN_ONSET_GAP_S && e > 0.08;
      // Adaptive statistics (slow EMA) of the flux.
      this.mean[lane] = m + (flux - m) * 0.06;
      this.dev[lane] = d + (Math.abs(flux - m) - d) * 0.06;
      if (isOnset) {
        this.lastOnset[lane] = time;
        this.notes.push({ id: this.nextId++, type: "tap", time, lane, endLane: null, duration: 0 });
        if (lane === 0) this.trackTempo(time);
      }
      // Sustained loud band → hold note spanning it.
      const loud = e > 0.55;
      const start = this.holdStart[lane]!;
      if (loud && start == null) this.holdStart[lane] = time;
      else if (!loud && start != null) {
        const dur = time - start;
        if (dur >= HOLD_MIN_S) {
          this.notes.push({
            id: this.nextId++,
            type: "hold",
            time: start,
            lane,
            endLane: (lane + 1 + (this.nextId % 3)) % 4,
            duration: dur,
          });
          this.notes.sort((a, b) => a.time - b.time);
        }
        this.holdStart[lane] = null;
      }
    }
    // Keep only what the field can still show (≈1.5 s window).
    const cutoff = time - 3;
    if (this.notes.length && this.notes[0]!.time < cutoff) {
      this.notes = this.notes.filter((n) => n.time + n.duration >= cutoff);
    }
  }

  private trackTempo(time: number) {
    this.bassTimes.push(time);
    if (this.bassTimes.length > 12) this.bassTimes.shift();
    const gaps: number[] = [];
    for (let i = 1; i < this.bassTimes.length; i += 1) {
      const g = this.bassTimes[i]! - this.bassTimes[i - 1]!;
      if (g >= 0.3 && g <= 1.5) gaps.push(g);
    }
    if (gaps.length < 4) return;
    gaps.sort((a, b) => a - b);
    const median = gaps[gaps.length >> 1]!;
    let bpm = 60 / median;
    while (bpm < 70) bpm *= 2;
    while (bpm > 180) bpm /= 2;
    this.bpm = Math.round(bpm);
  }
}

/** Average 0–1 energy of the analyser bins in each band. */
export function bandEnergies(freq: Uint8Array, len: number, out: number[]): number[] {
  for (let b = 0; b < 4; b += 1) {
    const lo = Math.floor(BAND_EDGES[b]! * len);
    const hi = Math.max(lo + 1, Math.floor(BAND_EDGES[b + 1]! * len));
    let sum = 0;
    for (let i = lo; i < hi; i += 1) sum += freq[i] ?? 0;
    out[b] = sum / ((hi - lo) * 255);
  }
  return out;
}

// ── Renderer ───────────────────────────────────────────────────────────────

export type DiscoWallFrame = {
  /** CSS pixels. */
  width: number;
  height: number;
  analyser: AnalyserNode | null;
  isPlaying: boolean;
  expanded: boolean;
  /** Playback position (s) — drives beat phase and note timing. */
  currentTime: number;
  /** Stable key of the current track (rel_path); null when idle. */
  trackKey: string | null;
};

export class DiscoWallRenderer {
  private seed = hashText("rekord-discowall");
  private trackKey: string | null = null;
  private triads: DiscoTriad[] = buildTriads(this.seed, null);
  private live = new LiveOnsetNotes();
  private styleW = createSceneStyleWeights();
  private fft: Uint8Array = new Uint8Array(1024);
  private bands: number[] = [0, 0, 0, 0];
  private pulse = 0;
  private flash = 0;
  private colorNudge = 0;
  private burst = { x: 0.5, y: 0.5 };
  private lastNote = -1;
  private liveEnergy = 0;
  private bassEnergy = 0;
  private frameIx = 0;
  private image: ImageData | null = null;
  private noise: Float32Array | null = null;
  private grain: Float32Array | null = null;
  private xn: Float32Array | null = null;
  private yn: Float32Array | null = null;
  private layoutKey = "";
  private cols = 1;
  private rows = 1;
  private cell = 12;
  private pad = 1;

  /** Pulse/flash still decaying: the caller may draw at a higher cadence. */
  get active(): boolean {
    return this.pulse > 0.12 || this.flash > 0.08;
  }

  reset() {
    this.live.reset();
    this.lastNote = -1;
    this.pulse = 0;
    this.flash = 0;
    this.colorNudge = 0;
  }

  private setTrack(key: string | null) {
    if (key === this.trackKey) return;
    this.trackKey = key;
    this.seed = hashText(key ?? "rekord-discowall");
    this.triads = buildTriads(this.seed, null);
    this.layoutKey = "";
    this.reset();
  }

  private layout(width: number, height: number, expanded: boolean) {
    const key = `${width}x${height}:${expanded ? 1 : 0}:${this.seed}`;
    if (key === this.layoutKey) return;
    this.layoutKey = key;
    const maxCells = expanded ? MAX_CELLS_EXPANDED : MAX_CELLS_PANEL;
    let cell = clamp(Math.round(width / 92), MIN_CELL, MAX_CELL);
    let cols = Math.max(18, Math.floor(width / cell));
    let rows = Math.max(12, Math.floor(height / cell));
    while (cols * rows > maxCells) {
      cell += 1;
      cols = Math.max(18, Math.floor(width / cell));
      rows = Math.max(12, Math.floor(height / cell));
    }
    this.cell = cell;
    this.cols = cols;
    this.rows = rows;
    this.pad = Math.max(1, cell * 0.11);
    const n = cols * rows;
    this.noise = new Float32Array(n);
    this.grain = new Float32Array(n);
    for (let i = 0; i < n; i += 1) {
      this.noise[i] = seededNoise(this.seed, i);
      this.grain[i] = seededNoise(this.seed + 31, i);
    }
    this.xn = new Float32Array(cols);
    this.yn = new Float32Array(rows);
    for (let x = 0; x < cols; x += 1) this.xn[x] = cols <= 1 ? 0 : x / (cols - 1);
    for (let y = 0; y < rows; y += 1) this.yn[y] = rows <= 1 ? 0 : y / (rows - 1);
  }

  private notePulseAt(time: number): number {
    const notes = this.live.notes;
    if (!notes.length) return 0;
    const idx = lastNoteIndexAt(notes, time);
    if (idx > this.lastNote) {
      for (let i = this.lastNote + 1; i <= idx; i += 1) {
        const note = notes[i]!;
        this.flash = Math.max(this.flash, 1);
        this.colorNudge = Math.min(1.2, this.colorNudge + 0.22 + seededNoise(this.seed, note.id) * 0.18);
        this.burst = burstPoint(note, this.seed);
      }
    }
    this.lastNote = idx;
    let p = 0;
    const k0 = Math.max(0, idx - 3);
    const k1 = Math.min(notes.length, idx + 5);
    for (let k = k0; k < k1; k += 1) {
      const note = notes[k]!;
      const dist = Math.abs(note.time - time);
      if (dist > 0.42) continue;
      const laneBoost = 0.95 + seededNoise(this.seed, note.id) * 0.18;
      p = Math.max(p, Math.pow(1 - dist / 0.42, 2.4) * laneBoost);
    }
    return clamp(p);
  }

  draw(ctx: CanvasRenderingContext2D, f: DiscoWallFrame): void {
    this.setTrack(f.trackKey);
    const width = Math.max(1, f.width);
    const height = Math.max(1, f.height);
    const bufW = ctx.canvas.width;
    const bufH = ctx.canvas.height;
    if (bufW <= 0 || bufH <= 0) return;
    const dpr = bufW / width;
    this.layout(width, height, f.expanded);
    this.frameIx += 1;

    const an = f.isPlaying ? f.analyser : null;
    const liveTime = f.currentTime;
    if (an) {
      const len = Math.min(an.frequencyBinCount, 512);
      if (this.fft.length < an.frequencyBinCount) this.fft = new Uint8Array(an.frequencyBinCount);
      an.getByteFrequencyData(this.fft.subarray(0, an.frequencyBinCount) as never);
      bandEnergies(this.fft, len, this.bands);
      this.live.push(liveTime, this.bands);
      if (this.frameIx % 2 === 0) {
        const n = Math.min(96, len);
        let sum = 0;
        let bass = 0;
        const bassN = Math.min(12, n);
        for (let i = 0; i < n; i += 1) {
          const v = this.fft[i] ?? 0;
          sum += v;
          if (i < bassN) bass += v;
        }
        this.liveEnergy = sum / (n * 255);
        this.bassEnergy = bass / (bassN * 255);
      }
    } else {
      this.liveEnergy *= 0.9;
      this.bassEnergy *= 0.9;
    }

    const mapped = this.notePulseAt(liveTime);
    const pulseTarget = Math.max(this.liveEnergy * 0.5 + this.bassEnergy * 0.26, mapped * 1.02);
    this.pulse += (pulseTarget - this.pulse) * 0.16;
    this.pulse *= 0.992;
    this.flash *= 0.91;
    this.colorNudge *= 0.965;

    const pulse = this.pulse;
    const flash = this.flash;
    const notes = this.live.notes;
    const bpm = this.live.bpm;
    const beatHz = bpm ? bpm / 60 : 1.25;
    const beatIndex = liveTime * beatHz;
    const hasChart = notes.length > 0;
    const hues = frameHues(this.triads, beatIndex, liveTime, hasChart ? this.colorNudge : 0);
    const motifs = hasChart ? collectMotifs(notes, liveTime, this.seed) : [];
    const taps = hasChart ? prepareConstellationTaps(motifs, liveTime) : [];
    if (hasChart) writeSceneStyleWeights(this.styleW, beatIndex, this.seed);
    const grid = hasChart ? buildPlectrFieldGrid(motifs, this.styleW, liveTime, taps) : null;

    if (!this.image || this.image.width !== bufW || this.image.height !== bufH) {
      this.image = ctx.createImageData(bufW, bufH);
    }
    const pixels = this.image.data;
    const [bgR, bgG, bgB] = hslToRgb(228 + (this.seed % 20), 22, 4 + pulse * 1.1);
    for (let i = 0; i < pixels.length; i += 4) {
      pixels[i] = bgR;
      pixels[i + 1] = bgG;
      pixels[i + 2] = bgB;
      pixels[i + 3] = 255;
    }

    const { cols, rows, cell, pad } = this;
    const noise = this.noise!;
    const grain = this.grain!;
    const xnRow = this.xn!;
    const ynCol = this.yn!;
    const burst = this.burst;
    const colorW: [number, number, number] = [0.2, 0.2, 0.2];

    for (let y = 0; y < rows; y += 1) {
      const yn = ynCol[y]!;
      for (let x = 0; x < cols; x += 1) {
        const xn = xnRow[x]!;
        const i = y * cols + x;
        let field: number;
        let accent = 0;
        colorW[0] = 0.2;
        colorW[1] = 0.2;
        colorW[2] = 0.2;
        if (grid) {
          const s = samplePlectrFieldGrid(grid, xn, yn);
          field = s.field;
          accent = s.accent;
          colorW[0] = s.colorW[0];
          colorW[1] = s.colorW[1];
          colorW[2] = s.colorW[2];
        } else {
          // Dark wall (silence / no onsets yet): a few pixels, audio energy only.
          field = 0.05 + pulse * 0.14 + this.bassEnergy * 0.06;
        }

        const bdx = xn - burst.x;
        const bdy = yn - burst.y;
        const burstDist = Math.sqrt(bdx * bdx + bdy * bdy);
        const burstHit =
          hasChart && flash > 0.08 ? Math.max(0, 1 - burstDist * (3 - flash * 0.7)) ** 1.7 : 0;

        const phase = pixelMathPhase(this.seed, i, beatIndex, field, accent);
        field += hasChart
          ? 0.08 * (0.5 + 0.5 * Math.sin(phase * 1.25))
          : 0.03 * (0.5 + 0.5 * Math.sin(phase * 1.1));

        const gateThreshold = hasChart
          ? 0.4 - field * 0.18 + Math.sin(phase * 2.1) * 0.05
          : 0.5 - field * 0.12 + Math.sin(phase * 2.1) * 0.04;
        const pixelGate = grain[i]! > gateThreshold;
        const core = clamp(
          field * (hasChart ? 1.08 : 0.92) +
            pulse * (hasChart ? 0.16 : 0.1) +
            this.bassEnergy * 0.08 +
            burstHit * (0.48 + flash * 0.45) -
            (pixelGate ? 0.1 : 0),
        );
        const coreMin = hasChart ? 0.14 : 0.2;
        const coreGate = hasChart ? 0.34 : 0.4;
        if (core < coreMin || (pixelGate && core < coreGate)) continue;

        const hue =
          mathPixelHue(hues, colorW, phase, accent * 0.2) + (noise[i]! - 0.5) * 2.5 + burstHit * 4;
        const sat = 44 + clamp(accent + pulse * 0.5) * 26;
        const light = 16 + core * 28 + flash * burstHit * 9;
        const sizeBoost = clamp(core * 0.9 + flash * burstHit * 0.34);
        const s = Math.max(1.2, cell - pad * 2) * (0.18 + sizeBoost * 0.9);
        const px = (x * cell + pad * 0.5 + (cell - s) * 0.5) * dpr;
        const py = (y * cell + pad * 0.5 + (cell - s) * 0.5) * dpr;
        const sD = Math.max(1, Math.round(s * dpr));
        const [r, g, b] = hslToRgb(hue, sat, light);
        fillCellRgb(pixels, bufW, bufH, px | 0, py | 0, sD, r, g, b, clamp(0.11 + core * 0.76, 0.1, 0.88));
      }
    }

    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.putImageData(this.image, 0, 0);
    ctx.restore();
    const scrim = f.expanded ? 0.28 - Math.min(0.08, flash * 0.06) : 0.38 - Math.min(0.1, flash * 0.08);
    ctx.fillStyle = `rgba(0, 0, 0, ${scrim})`;
    ctx.fillRect(0, 0, width, height);
  }
}
