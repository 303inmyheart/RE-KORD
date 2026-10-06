<script lang="ts">
  /**
   * Canvas surface for Sonic Nebula (legacy `NebulaCanvas.tsx`).
   *
   * Layers, bottom to top:
   * - the screen-space background gradient, as a static CSS background;
   * - backdrop canvas (haze, guides, fog, world space, transparent): painted
   *   at ≤1x CSS resolution (soft content) only when the size / fog change, a
   *   few times per second while it drifts, and at most every
   *   BACKDROP_FOLLOW_MS while the camera moves; between those repaints a CSS
   *   transform keeps it aligned with the camera (compositor only);
   * - star canvas (transparent): repainted per frame.
   * So a frame never re-fills or re-blits the full-screen gradients.
   *
   * Frames are drawn on demand: a change of the inputs (camera, hover,
   * selection…) asks for one frame on the next animation frame (≤30 fps, 24
   * on WebKitGTK); otherwise the slow twinkle runs at 5 fps, 10 while a star
   * is selected / hovered, 15 while the playing track's star beats; nothing runs when the page is hidden, the canvas is off-screen
   * or `animated` is false. The star layer's DPR is capped (1.5 on WebKitGTK)
   * and lowered further if frames run over budget (`AdaptiveQuality`).
   */
  import { onMount } from "svelte";
  import {
    isStarInBounds,
    nebulaWorldBounds,
    type NebulaCamera,
    type NebulaFog,
    type NebulaStar,
    type NebulaTrackInput,
  } from "../../lib/nebula/model";
  import {
    NEBULA_BACKDROP_REFRESH_MS,
    nebulaBgCss,
    paintNebulaBackdrop,
    paintNebulaStars,
    type NebulaPaintProps,
  } from "../../lib/nebula/render";
  import {
    AdaptiveQuality,
    isFrameDue,
    nextFrameDelay,
    scaledDpr,
  } from "../../lib/visualizer/adaptiveQuality";
  import {
    canvasDprCap,
    isDocumentHidden,
    nebulaLoopCadence,
    prefersReducedMotion,
    vizEngineDprCap,
  } from "../../lib/visualizer/renderQuality";

  let {
    fogs,
    sortedStars,
    camera,
    hoveredId = null,
    selectedId = null,
    currentId = null,
    playing = false,
    currentBpm = 0,
    preview = false,
    animated = true,
  }: {
    fogs: readonly NebulaFog[];
    /** Visible stars sorted by radius (small first). */
    sortedStars: readonly NebulaStar<NebulaTrackInput>[];
    camera: NebulaCamera;
    hoveredId?: string | null;
    selectedId?: string | null;
    currentId?: string | null;
    playing?: boolean;
    currentBpm?: number;
    preview?: boolean;
    animated?: boolean;
  } = $props();

  let frameEl: HTMLDivElement | null = $state(null);
  let backdropEl: HTMLCanvasElement | null = $state(null);
  let canvasEl: HTMLCanvasElement | null = $state(null);

  // Plain (non-reactive) copies read by the draw loop, which runs outside Svelte.
  let paint: NebulaPaintProps = {
    fogs: [],
    sortedStars: [],
    camera: { x: 0, y: 0, zoom: 1 },
    hoveredId: null,
    selectedId: null,
    currentId: null,
    playing: false,
    currentBpm: 0,
    beatEpoch: 0,
    preview: false,
    reducedMotion: false,
  };
  let animatedRef = true;
  let beatEpoch = 0;
  /** The playing track's star (null when it is not on the map). */
  let currentStar: NebulaStar<NebulaTrackInput> | null = null;
  /** Asks the loop for a frame on the next animation frame (inputs changed). */
  let invalidate: (() => void) | null = null;
  let kick: (() => void) | null = null;

  /** While the camera moves, the backdrop is repainted at most this often (CSS transform in between). */
  const BACKDROP_FOLLOW_MS = 120;
  /** Star-layer draw budget (ms) before the adaptive DPR steps down. */
  const STAR_BUDGET_MS = 9;
  /** Adaptive DPR multipliers of the star layer. */
  const DPR_STEPS = [1, 0.84, 0.67];

  // Beat phase is anchored to the track: reset only on track/BPM change.
  $effect(() => {
    void currentId;
    void currentBpm;
    beatEpoch = performance.now();
  });

  $effect(() => {
    paint = {
      fogs,
      sortedStars,
      camera,
      hoveredId,
      selectedId,
      currentId,
      playing,
      currentBpm,
      beatEpoch,
      preview,
      reducedMotion: prefersReducedMotion(),
    };
    animatedRef = animated;
    currentStar = currentId ? (sortedStars.find((s) => s.id === currentId) ?? null) : null;
    invalidate?.();
    kick?.();
  });

  onMount(() => {
    const frame = frameEl;
    const canvas = canvasEl;
    const bdCanvas = backdropEl;
    if (!frame || !canvas || !bdCanvas) return;
    const ctx = canvas.getContext("2d");
    const bctx = bdCanvas.getContext("2d");
    if (!ctx || !bctx) return;

    let measuredW = frame.clientWidth;
    let measuredH = frame.clientHeight;
    let size = { w: 0, h: 0, dpr: 1 };
    let bdSize = { w: 0, h: 0, scale: 0 };
    let raf = 0;
    let timer = 0;
    let lastDraw = -Infinity;
    let inView = true;
    let dirty = true;
    const quality = new AdaptiveQuality({ levels: DPR_STEPS.length, budgetMs: STAR_BUDGET_MS });

    // Backdrop state: what it was painted with, and when.
    let bd = {
      camera: null as NebulaCamera | null,
      fogs: null as readonly NebulaFog[] | null,
      preview: false,
      dense: false,
      at: -Infinity,
      transformed: false,
    };

    const baseDpr = () => canvasDprCap({ lite: size.w < 520 || paint.preview });

    const syncBuffers = () => {
      const w = Math.max(1, Math.floor(measuredW));
      const h = Math.max(1, Math.floor(measuredH));
      const base = baseDpr();
      const dpr = scaledDpr(base, vizEngineDprCap(), DPR_STEPS[quality.level] ?? 1);
      if (size.w !== w || size.h !== h || size.dpr !== dpr) {
        size = { w, h, dpr };
        canvas.width = Math.floor(w * dpr);
        canvas.height = Math.floor(h * dpr);
      }
      // Soft gradients: ≤1x CSS resolution is indistinguishable once upscaled.
      const scale = Math.min(1, base);
      if (bdSize.w !== w || bdSize.h !== h || bdSize.scale !== scale) {
        bdSize = { w, h, scale };
        bdCanvas.width = Math.max(1, Math.floor(w * scale));
        bdCanvas.height = Math.max(1, Math.floor(h * scale));
        bd.at = -Infinity;
        bd.camera = null;
        frame.style.background = nebulaBgCss(w, h);
      }
    };

    const setBackdropTransform = (cam: NebulaCamera) => {
      const from = bd.camera;
      if (!from) return;
      const k = cam.zoom / from.zoom;
      const tx = (from.x - cam.x) * cam.zoom;
      const ty = (from.y - cam.y) * cam.zoom;
      bdCanvas.style.transform = `translate(${tx.toFixed(2)}px, ${ty.toFixed(2)}px) scale(${k.toFixed(5)})`;
      bd.transformed = true;
    };

    const paintBackdrop = (t: number) => {
      const p = paint;
      bctx.setTransform(bdCanvas.width / size.w, 0, 0, bdCanvas.height / size.h, 0, 0);
      paintNebulaBackdrop(bctx, size.w, size.h, t, p, false);
      bd = {
        camera: p.camera,
        fogs: p.fogs,
        preview: p.preview,
        dense: p.sortedStars.length > 900,
        at: t,
        transformed: false,
      };
      bdCanvas.style.transform = "";
    };

    /** Repaints the backdrop when stale; follows a moving camera with a CSS transform in between. */
    const syncBackdrop = (t: number) => {
      const p = paint;
      const cam = p.camera;
      const structural =
        bd.camera == null ||
        bd.fogs !== p.fogs ||
        bd.preview !== p.preview ||
        bd.dense !== p.sortedStars.length > 900;
      if (structural) {
        paintBackdrop(t);
        return;
      }
      const prev = bd.camera!;
      const moved = prev.x !== cam.x || prev.y !== cam.y || prev.zoom !== cam.zoom;
      const age = t - bd.at;
      if (moved) {
        const k = cam.zoom / prev.zoom;
        if (age >= BACKDROP_FOLLOW_MS || k < 0.7 || k > 1.4) paintBackdrop(t);
        else setBackdropTransform(cam);
        return;
      }
      if (bd.transformed || (!p.reducedMotion && age >= NEBULA_BACKDROP_REFRESH_MS)) paintBackdrop(t);
    };

    const drawNow = (t = performance.now()) => {
      syncBuffers();
      if (size.w <= 1 || size.h <= 1) return;
      syncBackdrop(t);
      ctx.setTransform(size.dpr, 0, 0, size.dpr, 0, 0);
      const t0 = performance.now();
      paintNebulaStars(ctx, size.w, size.h, t, paint);
      quality.push(performance.now() - t0);
      dirty = false;
      lastDraw = performance.now();
    };

    const stop = () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(timer);
      raf = 0;
      timer = 0;
    };

    const visibleNow = () => inView && !isDocumentHidden();
    const animating = () => animatedRef && visibleNow();
    const interactiveMs = () => nebulaLoopCadence({ active: true }).minFrameIntervalMs;
    /** The playing track's star beats on screen (off-screen it needs no frames). */
    const currentPulsing = () =>
      paint.playing &&
      paint.currentBpm > 0 &&
      currentStar != null &&
      isStarInBounds(currentStar, nebulaWorldBounds(size.w, size.h, paint.camera));
    const idleMs = () =>
      nebulaLoopCadence({
        active: false,
        preview: paint.preview,
        pulsing: currentPulsing(),
        focused: Boolean(paint.selectedId || paint.hoveredId),
      }).minFrameIntervalMs;
    /** Backdrop drift / CSS-transform settle still pending (paint even when not animated). */
    const backdropPending = () => bd.transformed;

    const loop = (t: number) => {
      raf = 0;
      if (!visibleNow()) return;
      const now = performance.now();
      const interval = dirty || backdropPending() ? interactiveMs() : idleMs();
      if (isFrameDue(now, lastDraw, interval)) drawNow(t);
      if (dirty || animating()) schedule(nextFrameDelay(performance.now(), lastDraw, dirty ? interactiveMs() : idleMs()));
      else if (backdropPending()) schedule(BACKDROP_FOLLOW_MS);
    };

    const schedule = (delay: number) => {
      if (raf || timer) return;
      if (delay <= 4) {
        raf = requestAnimationFrame(loop);
        return;
      }
      timer = window.setTimeout(() => {
        timer = 0;
        raf = requestAnimationFrame(loop);
      }, delay);
    };

    invalidate = () => {
      dirty = true;
    };

    kick = () => {
      if (!visibleNow()) {
        stop();
        return;
      }
      if (dirty) {
        // Inputs changed: draw on the next frame (if the throttle allows it),
        // coalescing every pointer event of that frame into one paint.
        if (timer) {
          window.clearTimeout(timer);
          timer = 0;
        }
        schedule(nextFrameDelay(performance.now(), lastDraw, interactiveMs()));
      } else if (animating()) schedule(0);
      else stop();
    };

    const ro = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      if (entry) {
        measuredW = entry.contentRect.width;
        measuredH = entry.contentRect.height;
      }
      quality.reset();
      // Resizing clears both bitmaps: repaint now, in the same frame.
      drawNow();
    });
    ro.observe(frame);

    const io =
      typeof IntersectionObserver !== "undefined"
        ? new IntersectionObserver(
            ([entry]) => {
              inView = Boolean(entry?.isIntersecting);
              kick?.();
            },
            { rootMargin: "80px" },
          )
        : null;
    io?.observe(frame);

    const onVisibility = () => kick?.();
    document.addEventListener("visibilitychange", onVisibility);

    drawNow();
    kick();

    return () => {
      stop();
      invalidate = null;
      kick = null;
      ro.disconnect();
      io?.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  });
</script>

<div class="nebula-canvas-frame" bind:this={frameEl}>
  <canvas class="nebula-canvas-backdrop" bind:this={backdropEl} aria-hidden="true"></canvas>
  <canvas class="nebula-canvas-surface" bind:this={canvasEl} aria-hidden="true"></canvas>
</div>

<style>
  .nebula-canvas-frame {
    position: absolute;
    inset: 0;
    overflow: hidden;
    /* Replaced by the view-sized gradient (nebulaBgCss) once measured. */
    background: #03040a;
  }

  .nebula-canvas-backdrop,
  .nebula-canvas-surface {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
  }

  .nebula-canvas-backdrop {
    transform-origin: 50% 50%;
    will-change: transform;
  }
</style>
