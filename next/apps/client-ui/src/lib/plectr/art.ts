/**
 * Album art for the Plectr page, computed once per album: the dominant
 * colour (device glow, stage tint) and a tiny pre-blurred copy of the cover
 * (page backdrop, stage backdrop). Everything is static — no CSS
 * `backdrop-filter`, no per-frame filters (WebKitGTK composites in software).
 */
import { coverUrlFor, type CoverEntity } from "../api";

export type AlbumArt = {
  /** "#rrggbb", null for grey / dark covers. */
  color: string | null;
  /** 48 px blurred cover (canvas), stretched by the users. */
  image: HTMLCanvasElement | null;
  /** Same image as a data URL for CSS backgrounds. */
  dataUrl: string | null;
};

const EMPTY: AlbumArt = { color: null, image: null, dataUrl: null };
/** Keyed by cover URL (it carries `?v=`, so a changed cover is rebuilt). */
const cache = new Map<string, Promise<AlbumArt>>();

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.decoding = "async";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("cover"));
    img.src = src;
  });
}

/** Saturated, mid-light pixels weigh more: a cover's "colour", not its average grey. */
export function dominantColor(data: Uint8ClampedArray): string | null {
  let r = 0;
  let g = 0;
  let b = 0;
  let wsum = 0;
  for (let i = 0; i < data.length; i += 4) {
    const pr = data[i]!;
    const pg = data[i + 1]!;
    const pb = data[i + 2]!;
    const max = Math.max(pr, pg, pb);
    const min = Math.min(pr, pg, pb);
    const sat = max === 0 ? 0 : (max - min) / max;
    const light = max / 255;
    if (light < 0.18) continue;
    const w = sat * sat * (light > 0.92 ? 0.5 : 1);
    r += pr * w;
    g += pg * w;
    b += pb * w;
    wsum += w;
  }
  if (wsum < 0.5) return null;
  // Lift to a glow-friendly lightness.
  let cr = r / wsum;
  let cg = g / wsum;
  let cb = b / wsum;
  const max = Math.max(cr, cg, cb, 1);
  const lift = Math.min(2.2, 210 / max);
  cr = Math.min(255, cr * lift);
  cg = Math.min(255, cg * lift);
  cb = Math.min(255, cb * lift);
  const hex = (v: number) => Math.round(v).toString(16).padStart(2, "0");
  return `#${hex(cr)}${hex(cg)}${hex(cb)}`;
}

async function build(src: string): Promise<AlbumArt> {
  if (typeof document === "undefined") return EMPTY;
  const img = await loadImage(src);
  const small = document.createElement("canvas");
  small.width = small.height = 24;
  const sctx = small.getContext("2d", { willReadFrequently: true });
  if (!sctx) return EMPTY;
  sctx.drawImage(img, 0, 0, 24, 24);
  const color = dominantColor(sctx.getImageData(0, 0, 24, 24).data);
  // 24 px → 48 px with a blur where supported: upscaled by CSS/canvas it is a soft wash.
  const out = document.createElement("canvas");
  out.width = out.height = 48;
  const octx = out.getContext("2d");
  if (!octx) return { color, image: null, dataUrl: null };
  octx.imageSmoothingEnabled = true;
  if ("filter" in octx) octx.filter = "blur(3px)";
  octx.drawImage(small, -4, -4, 56, 56);
  if ("filter" in octx) octx.filter = "none";
  let dataUrl: string | null = null;
  try {
    dataUrl = out.toDataURL("image/png");
  } catch {
    dataUrl = null;
  }
  return { color, image: out, dataUrl };
}

/**
 * Art for a track's album (cached). Resolves to empty art when the hub says
 * there is no cover (no request at all) or the cover fails to load.
 */
export function albumArt(entity: CoverEntity | null | undefined): Promise<AlbumArt> {
  const src = coverUrlFor(entity, 128);
  if (!src) return Promise.resolve(EMPTY);
  let p = cache.get(src);
  if (!p) {
    p = build(src).catch(() => EMPTY);
    cache.set(src, p);
    if (cache.size > 40) cache.delete(cache.keys().next().value!);
  }
  return p;
}
