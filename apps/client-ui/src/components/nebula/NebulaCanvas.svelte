<script lang="ts">
  /**
   * Canvas surface for Sonic Nebula (legacy `NebulaCanvas.tsx`).
   * Draws in a throttled rAF loop that stops when the page is hidden, the canvas
   * is scrolled out of view or `animated` is false (then it repaints only when
   * its inputs change). Full view: ≤30 fps for a moment after each interaction
   * (camera / hover / selection), 15 fps otherwise; preview: ≤10 fps. Playback
   * alone never forces full speed. The backing store is sized by a
   * ResizeObserver only (no per-frame layout reads) with a capped DPR.
   */
  import { onMount } from "svelte";
  import type { NebulaCamera, NebulaFog, NebulaStar, NebulaTrackInput } from "../../lib/nebula/model";
  import { paintNebulaFrame, type NebulaPaintProps } from "../../lib/nebula/render";
  import {
    canvasDprCap,
    isDocumentHidden,
    nebulaLoopCadence,
    prefersReducedMotion,
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
  /** Last camera / hover / selection change: keeps the loop at the active cadence briefly. */
  let lastInteraction = 0;
  const ACTIVE_WINDOW_MS = 1500;
  let redraw: (() => void) | null = null;
  let kick: (() => void) | null = null;

  // Beat phase is anchored to the track: reset only on track/BPM change.
  $effect(() => {
    void currentId;
    void currentBpm;
    beatEpoch = performance.now();
  });

  $effect(() => {
    void camera;
    void hoveredId;
    void selectedId;
    lastInteraction = performance.now();
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
    redraw?.();
    kick?.();
  });

  onMount(() => {
    const frame = frameEl;
    const canvas = canvasEl;
    if (!frame || !canvas) return;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;

    let measuredW = frame.clientWidth;
    let measuredH = frame.clientHeight;
    let size = { w: 0, h: 0, dpr: 1 };
    let raf = 0;
    let timer = 0;
    let lastDraw = 0;
    let inView = true;

    const syncBuffer = () => {
      const w = Math.max(1, Math.floor(measuredW));
      const h = Math.max(1, Math.floor(measuredH));
      const dpr = canvasDprCap({ lite: w < 520 || paint.preview });
      if (size.w === w && size.h === h && size.dpr === dpr) return;
      size = { w, h, dpr };
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const drawNow = (t = performance.now()) => {
      syncBuffer();
      if (size.w <= 1 || size.h <= 1) return;
      paintNebulaFrame(ctx, size.w, size.h, t, paint);
    };
    redraw = () => drawNow();

    const stop = () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(timer);
      raf = 0;
      timer = 0;
    };

    const shouldRun = () => animatedRef && inView && !isDocumentHidden();

    const loop = (t: number) => {
      raf = 0;
      if (!shouldRun()) return;
      const active = t - lastInteraction < ACTIVE_WINDOW_MS;
      const cadence = nebulaLoopCadence({ active, preview: paint.preview });
      // The timer wakes up a little early and rAF aligns the draw to the next
      // frame; the slack keeps that from costing an extra rAF round-trip.
      const interval = cadence.minFrameIntervalMs;
      const early = Math.min(10, interval * 0.15);
      const now = performance.now();
      if (now - lastDraw >= interval - early - 2) {
        drawNow(t);
        lastDraw = now;
      }
      schedule(Math.max(1, interval - (performance.now() - lastDraw) - early));
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

    kick = () => {
      if (shouldRun()) schedule(0);
      else stop();
    };

    const ro = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      if (entry) {
        measuredW = entry.contentRect.width;
        measuredH = entry.contentRect.height;
      }
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
      redraw = null;
      kick = null;
      ro.disconnect();
      io?.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  });
</script>

<div class="nebula-canvas-frame" bind:this={frameEl}>
  <canvas class="nebula-canvas-surface" bind:this={canvasEl} aria-hidden="true"></canvas>
</div>

<style>
  .nebula-canvas-frame {
    position: absolute;
    inset: 0;
  }

  .nebula-canvas-surface {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
