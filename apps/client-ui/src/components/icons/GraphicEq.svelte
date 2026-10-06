<script lang="ts">
  /**
   * "Equalizer" icon (centred bars).
   *
   * - `animated`: something is playing. On its own it gives a static "listening" pose
   *   (bars at different heights), with no animation at all.
   * - `live`: this is the only instance allowed to move (the Studio entry
   *   of the sidebar). It animates in steps (`steps()`, ~7 fps), stops
   *   when the window is hidden, and never on WebKitGTK / Tauri-Linux nor with
   *   «reduce motion» (lib/platformCaps.ts).
   *
   * Reason: an infinite animation, even of just an icon, keeps WebKitGTK
   * recompositing the page at 60 fps (perf report: queue playing 34% CPU,
   * 0.1% with the icon still). Rows, dashboard and mobile nav stay static.
   */
  import { onMount } from "svelte";
  import { canAnimateLiveIndicators } from "../../lib/platformCaps";

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
  // `pose`: scale of the static "listening" pose (heights 10/16/12/20/8 out of 24).
  const BARS = [
    { h: 4, min: 0.35, dur: 0.9, delay: -0.2, pose: 2.5 },
    { h: 12, min: 0.4, dur: 0.75, delay: 0, pose: 1.333 },
    { h: 20, min: 0.5, dur: 0.6, delay: -0.3, pose: 0.6 },
    { h: 12, min: 0.45, dur: 0.7, delay: -0.1, pose: 1.667 },
    { h: 4, min: 0.3, dur: 0.85, delay: -0.4, pose: 2 },
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
</script>

<span class="geq {className}" class:pulse={moving} class:posed aria-hidden="true">
  {#each BARS as bar, i (i)}
    <span
      class="bar"
      style:height={`${(bar.h / 24) * 100}%`}
      style:--geq-min={bar.min}
      style:--geq-pose={bar.pose}
      style:animation-duration={moving ? `${bar.dur}s` : undefined}
      style:animation-delay={moving ? `${bar.delay}s` : undefined}
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

  .pulse .bar {
    animation-name: geq-pulse;
    animation-iteration-count: infinite;
    animation-direction: alternate;
    /* 4 steps per half cycle: ≤ 7 frames per second. The effect remains,
       the compositor rests. */
    animation-timing-function: steps(4, jump-none);
  }

  @keyframes geq-pulse {
    from {
      transform: scaleY(var(--geq-min));
    }
    to {
      transform: scaleY(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse .bar {
      animation: none;
    }
  }
</style>
