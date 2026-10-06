<script lang="ts">
  /**
   * "Equalizer" icon (centred bars).
   *
   * - `animated`: something is playing. On its own it gives a static "listening" pose
   *   (bars at different heights), with no animation at all.
   * - `live`: this is the only instance allowed to move (the Studio entry
   *   of the sidebar / mobile nav, like legacy `RekordNavIcon`). While
   *   something plays the bars pulse around the centre line exactly like the
   *   legacy `UiGraphicEq` SMIL animation (same min heights, periods,
   *   delays and ease-in-out spline), on every engine. It stops when the
   *   window is hidden and with «reduce motion» (lib/platformCaps.ts).
   *
   * Cost: `transform` only, on five tiny bars inside a `contain: strict`
   * box promoted to its own layer, so the page around it is never repainted.
   * WebKitGTK does not run it on the compositor: every frame repainted and
   * re-sent the whole window (under Xwayland on a 2× panel: Xwayland 35% of a
   * core, WebKit 14%, while a track played). There the same motion is
   * stepped from a timer at 5 fps: one small repaint every 200 ms.
   * Rows and dashboard cards stay static: dozens of animated icons were what
   * kept WebKitGTK busy (perf report: 34% CPU with an icon in every row).
   */
  import { onMount } from "svelte";
  import { canAnimateLiveIndicators, platformCaps } from "../../lib/platformCaps";

  let {
    animated = false,
    live = false,
    class: className = "",
  }: {
    animated?: boolean;
    live?: boolean;
    class?: string;
  } = $props();

  // Left to right, in twenty-fourths of the side (like the old viewBox).
  // `min` / `dur` (s) / `delay` (s): legacy GRAPHIC_EQ_PULSE for the bar at
  // that x (4, 8, 12, 16, 20). `pose`: scale of the static "listening" pose
  // (heights 10/16/12/20/8 out of 24).
  const BARS = [
    { h: 4, min: 0.35, dur: 0.88, delay: 0.16, pose: 2.5 },
    { h: 12, min: 0.4, dur: 0.68, delay: 0, pose: 1.333 },
    { h: 20, min: 0.55, dur: 0.52, delay: 0.08, pose: 0.6 },
    { h: 12, min: 0.45, dur: 0.62, delay: 0.04, pose: 1.667 },
    { h: 4, min: 0.3, dur: 0.76, delay: 0.2, pose: 2 },
  ] as const;

  const allowed = canAnimateLiveIndicators();
  let hidden = $state(false);

  onMount(() => {
    if (!live || !allowed) return;
    const sync = () => {
      hidden = document.visibilityState === "hidden";
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => document.removeEventListener("visibilitychange", sync);
  });

  const moving = $derived(animated && live && allowed && !hidden);
  const posed = $derived(animated && !moving);

  /** WebKitGTK: the pulse sampled at 5 fps from a timer (see above). */
  const STEP_MS = 200;
  const stepped = platformCaps.webkitGtk;
  let scales = $state<number[] | null>(null);

  /** Legacy curve: min → 1 → min over `dur`, ease-in-out, after `delay`. */
  function pulseAt(bar: (typeof BARS)[number], t: number): number {
    if (t < bar.delay) return 1;
    const phase = ((t - bar.delay) % bar.dur) / bar.dur;
    const half = phase < 0.5 ? phase * 2 : (1 - phase) * 2;
    const eased = half * half * (3 - 2 * half);
    return bar.min + (1 - bar.min) * eased;
  }

  $effect(() => {
    if (!stepped || !moving) {
      scales = null;
      return;
    }
    const t0 = performance.now();
    const tick = () => {
      const t = (performance.now() - t0) / 1000;
      scales = BARS.map((bar) => Math.round(pulseAt(bar, t) * 20) / 20);
    };
    tick();
    const id = window.setInterval(tick, STEP_MS);
    return () => window.clearInterval(id);
  });
</script>

<span class="geq {className}" class:pulse={moving && !stepped} class:posed aria-hidden="true">
  {#each BARS as bar, i (i)}
    <span
      class="bar"
      style:height={`${(bar.h / 24) * 100}%`}
      style:--geq-min={bar.min}
      style:--geq-pose={bar.pose}
      style:animation-duration={moving && !stepped ? `${bar.dur}s` : undefined}
      style:animation-delay={moving && !stepped ? `${bar.delay}s` : undefined}
      style:transform={scales ? `scaleY(${scales[i]})` : undefined}
    ></span>
  {/each}
</span>

<style>
  .geq {
    width: 1.25rem;
    height: 1.25rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: calc(100% / 12);
    flex-shrink: 0;
    /* The icon never affects the layout or paint of its container. */
    contain: strict;
  }

  .bar {
    width: calc(100% / 12);
    border-radius: 1px;
    background: currentColor;
    transform-origin: center;
  }

  /* Static "listening" pose: irregular profile, recognizable when still. */
  .posed .bar {
    transform: scaleY(var(--geq-pose));
  }

  /* Its own layer while it moves: the rail around it is never repainted. */
  .geq.pulse {
    will-change: transform;
  }

  /* Legacy SMIL: values min;1;min over `dur`, keySplines 0.42 0 0.58 1 on
     both halves, begin after `delay`. */
  .pulse .bar {
    animation-name: geq-pulse;
    animation-iteration-count: infinite;
    animation-timing-function: cubic-bezier(0.42, 0, 0.58, 1);
  }

  @keyframes geq-pulse {
    0%,
    100% {
      transform: scaleY(var(--geq-min));
    }
    50% {
      transform: scaleY(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse .bar {
      animation: none;
    }
  }
</style>
