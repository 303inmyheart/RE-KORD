<script lang="ts">
  /**
   * Icona "equalizzatore" (barre centrate).
   *
   * - `animated`: si sta ascoltando. Da solo dà una posa statica "in ascolto"
   *   (barre ad altezze diverse), senza nessuna animazione.
   * - `live`: questa è l'unica istanza autorizzata a muoversi (la voce Studio
   *   della barra laterale). Si anima a scatti (`steps()`, ~7 fps), si ferma
   *   quando la finestra è nascosta, e mai su WebKitGTK / Tauri-Linux né con
   *   «riduci movimento» (lib/platformCaps.ts).
   *
   * Motivo: un'animazione infinita, anche solo di un'icona, tiene WebKitGTK a
   * ricomporre la pagina a 60 fps (report perf: coda in riproduzione 34% CPU,
   * 0.1% con l'icona ferma). Righe, dashboard e nav mobile restano statiche.
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

  // Da sinistra a destra, in ventiquattresimi del lato (come il vecchio viewBox).
  // `pose`: scala della posa statica "in ascolto" (altezze 10/16/12/20/8 su 24).
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
    /* L'icona non influenza mai il layout o il paint di chi la contiene. */
    contain: strict;
  }

  .bar {
    width: calc(100% / 12);
    border-radius: 1px;
    background: currentColor;
    transform-origin: center;
  }

  /* Posa statica "in ascolto": profilo irregolare, riconoscibile da fermo. */
  .posed .bar {
    transform: scaleY(var(--geq-pose));
  }

  .pulse .bar {
    animation-name: geq-pulse;
    animation-iteration-count: infinite;
    animation-direction: alternate;
    /* 4 scatti per mezzo ciclo: ≤ 7 fotogrammi al secondo. L'effetto resta,
       il compositore riposa. */
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
