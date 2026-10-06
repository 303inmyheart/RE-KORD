/** Compact render target + viz loop cadence (ported from legacy renderQuality). */

import { BREAKPOINTS, mediaDown } from "../breakpoints";
import { platformCaps } from "../platformCaps";

export type LoopCadence = { minFrameIntervalMs: number };

let coarsePointerMq: MediaQueryList | null = null;
let compactLayoutMq: MediaQueryList | null = null;
let reducedMotionMq: MediaQueryList | null = null;

function coarsePointerMqRef(): MediaQueryList | null {
  if (typeof window === "undefined") return null;
  if (!coarsePointerMq) {
    coarsePointerMq = window.matchMedia("(pointer: coarse)");
  }
  return coarsePointerMq;
}

function compactLayoutMqRef(): MediaQueryList | null {
  if (typeof window === "undefined") return null;
  if (!compactLayoutMq) {
    compactLayoutMq = window.matchMedia(mediaDown("lg"));
  }
  return compactLayoutMq;
}

function reducedMotionMqRef(): MediaQueryList | null {
  if (typeof window === "undefined") return null;
  if (!reducedMotionMq) {
    reducedMotionMq = window.matchMedia("(prefers-reduced-motion: reduce)");
  }
  return reducedMotionMq;
}

export function prefersReducedMotion(): boolean {
  return reducedMotionMqRef()?.matches === true;
}

/** Touch or compact layout: same tier on mobile WebView and narrow browser. */
export function isCompactRenderTarget(): boolean {
  if (typeof window !== "undefined" && window.innerWidth < BREAKPOINTS.sm) {
    return true;
  }
  return (
    coarsePointerMqRef()?.matches === true ||
    compactLayoutMqRef()?.matches === true
  );
}

/**
 * Software-composited WebKitGTK (Tauri on Linux, Epiphany): every canvas frame
 * is presented through the CPU, so loops run at a lower cap there.
 */
export function isSlowCanvasEngine(): boolean {
  return platformCaps.webkitGtk;
}

/**
 * Backing-store DPR cap for the heavy canvases (DiscoWall, Nebula) on
 * WebKitGTK: every device pixel is filled and presented through a slow path
 * there, and 1.5 on a 2x screen is ~44% fewer pixels for a barely softer
 * image. Elsewhere `canvasDprCap` alone applies.
 */
export const SLOW_ENGINE_DPR_CAP = 1.5;

export function vizEngineDprCap(): number {
  return isSlowCanvasEngine() ? SLOW_ENGINE_DPR_CAP : Number.POSITIVE_INFINITY;
}

/** 30 fps cap everywhere, 24 fps on WebKitGTK. */
export function vizFrameCapMs(): number {
  return isSlowCanvasEngine() ? 42 : 33;
}

/**
 * Listen visualizer while playing (paused: the caller draws one frame and
 * stops). Capped at 30 fps, 24 fps on WebKitGTK, lower on compact targets.
 */
export function vizLoopCadence(opts: {
  expanded: boolean;
  isPlaying: boolean;
}): LoopCadence {
  if (prefersReducedMotion()) {
    return { minFrameIntervalMs: opts.isPlaying ? 100 : 250 };
  }
  const cap = vizFrameCapMs();
  if (opts.expanded) {
    return { minFrameIntervalMs: isCompactRenderTarget() ? Math.max(cap, 42) : cap };
  }
  return { minFrameIntervalMs: isCompactRenderTarget() ? Math.max(cap, 48) : cap };
}

/** Cap DPR canvas (viz panel / expanded). */
export function canvasDprCap(opts?: { lite?: boolean }): number {
  const base = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  if (prefersReducedMotion()) return Math.min(base, 1);
  if (opts?.lite && isCompactRenderTarget()) return Math.min(base, 1.35);
  if (isCompactRenderTarget()) return Math.min(base, 1.5);
  return Math.min(base, opts?.lite ? 1.75 : 2);
}

export function isDocumentHidden(): boolean {
  return typeof document !== "undefined" && document.hidden;
}

/** Nebula: ≤30 fps while the user interacts (24 on WebKitGTK). */
export const NEBULA_ACTIVE_FRAME_MS = 33;
/** Nebula full view while the playing track's star beats: 15 fps. */
export const NEBULA_IDLE_FRAME_MS = 66;
/** Nebula full view with a selection / hover glow breathing: 10 fps. */
export const NEBULA_FOCUS_FRAME_MS = 100;
/**
 * Nebula full view with only the slow twinkle (±16% radius over ~2.2 s) and
 * drift moving: 5 fps, in step with the backdrop refresh. A star moves by a
 * fraction of a pixel between two frames, and each star frame re-rasterises
 * thousands of circles (~7 ms on WebKitGTK).
 */
export const NEBULA_CALM_FRAME_MS = 200;
/** Dashboard preview: animates only on hover / focus, ≤10 fps. */
export const NEBULA_PREVIEW_FRAME_MS = 100;

/**
 * Sonic Nebula loop cadence. `active` = the view changed (camera, hover,
 * selection) and must follow the pointer; `pulsing` = the playing track's
 * star beats with the music; `focused` = a selected / hovered star breathes.
 * Otherwise only the slow twinkle moves.
 */
export function nebulaLoopCadence(opts: {
  active: boolean;
  preview?: boolean;
  pulsing?: boolean;
  /** A star is selected or hovered (its glow / vignette breathes). */
  focused?: boolean;
}): LoopCadence {
  if (prefersReducedMotion()) return { minFrameIntervalMs: 250 };
  if (isDocumentHidden()) return { minFrameIntervalMs: 250 };
  if (opts.active) {
    return {
      minFrameIntervalMs: isSlowCanvasEngine() || isCompactRenderTarget()
        ? Math.max(NEBULA_ACTIVE_FRAME_MS, vizFrameCapMs())
        : NEBULA_ACTIVE_FRAME_MS,
    };
  }
  if (opts.preview) return { minFrameIntervalMs: NEBULA_PREVIEW_FRAME_MS };
  if (opts.pulsing) return { minFrameIntervalMs: NEBULA_IDLE_FRAME_MS };
  return { minFrameIntervalMs: opts.focused ? NEBULA_FOCUS_FRAME_MS : NEBULA_CALM_FRAME_MS };
}

/** DiscoWall Listen: FPS cap panel vs expanded, active vs calm (legacy discowallLoopCadence). */
export function discowallLoopCadence(opts: { expanded: boolean; active: boolean }): LoopCadence {
  if (prefersReducedMotion()) return { minFrameIntervalMs: 120 };
  if (isDocumentHidden()) return { minFrameIntervalMs: 250 };
  const cap = vizFrameCapMs();
  if (opts.expanded && opts.active) return { minFrameIntervalMs: cap };
  return { minFrameIntervalMs: isCompactRenderTarget() ? Math.max(cap, 48) : Math.max(cap, 40) };
}
