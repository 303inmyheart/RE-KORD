<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import { session } from "../../lib/session.svelte";
  import {
    loadUserPrefs,
    subscribeUserPrefs,
    type VisualizerMode,
  } from "../../lib/userPrefs";
  import { currentLrcLineIndex, parseLrcLyrics } from "../../lib/visualizer/lrc";
  import {
    canvasDprCap,
    discowallLoopCadence,
    vizLoopCadence,
  } from "../../lib/visualizer/renderQuality";
  import {
    VizCanvasEngine,
    type VizMode,
  } from "../../lib/visualizer/vizCanvasEngine";

  let {
    playing = false,
    mode,
    lyrics = "",
    currentTime = 0,
  }: {
    playing?: boolean;
    /** If omitted, follows Settings → Player visualizer prefs. */
    mode?: VisualizerMode;
    lyrics?: string;
    currentTime?: number;
  } = $props();

  let wrapEl: HTMLDivElement | null = $state(null);
  let canvas: HTMLCanvasElement | null = $state(null);
  let expanded = $state(false);
  let prefsMode = $state<VisualizerMode>(loadUserPrefs().visualizerMode);

  const engine = new VizCanvasEngine();
  /**
   * The player routes audio through Web Audio only while someone holds its
   * analyser (costly on some engines): held while a spectrum is drawn on
   * screen for a playing song. Until the graph is up `getAnalyser()` is null
   * and the engine draws its idle state.
   */
  const analyserLease = player.analyserLease();

  let raf = 0;
  let timerId = 0;
  let lastDraw = 0;
  /** Size + mode + expansion + theme of the last painted frame (a static frame stays valid while equal). */
  let lastDrawKey = "";
  function drawKey(): string {
    const c = canvas;
    return `${c?.width ?? 0}x${c?.height ?? 0}|${modeRef}|${expandedRef ? 1 : 0}|${document.documentElement.dataset.theme ?? ""}`;
  }
  let backingScale = 1;
  let visible = typeof document !== "undefined" ? !document.hidden : true;
  let inView = true;
  let unsubPrefs: (() => void) | null = null;
  // Non-reactive copies read by the draw loop, which runs outside Svelte: the
  // effect further down realigns them on every state change.
  let playingRef = false;
  let modeRef: VizMode = "bars";
  let expandedRef = false;
  /** Last known playback position + when it arrived: DiscoWall extrapolates between updates. */
  let timeRef = 0;
  let timeStamp = typeof performance !== "undefined" ? performance.now() : 0;
  let resize: (() => void) | null = null;

  const activeMode = $derived((mode ?? prefsMode) as VizMode);

  const parsedLrc = $derived(parseLrcLyrics(lyrics || ""));
  const lrcIdx = $derived(currentLrcLineIndex(parsedLrc, currentTime));
  const currentLrcText = $derived(
    lrcIdx >= 0 ? parsedLrc[lrcIdx]?.text?.trim() || "" : "",
  );
  const previousLrcText = $derived(
    lrcIdx > 0 ? parsedLrc[lrcIdx - 1]?.text?.trim() || "" : "",
  );
  const nextLrcText = $derived(
    lrcIdx >= 0 && lrcIdx + 1 < parsedLrc.length
      ? parsedLrc[lrcIdx + 1]?.text?.trim() || ""
      : "",
  );
  const plainLyrics = $derived(
    !parsedLrc.length && lyrics.trim()
      ? lyrics
          .split(/\r?\n/)
          .map((l) => l.trim())
          .filter(Boolean)
          .slice(0, 12)
          .join("\n")
      : "",
  );

  function clearLoop() {
    cancelAnimationFrame(raf);
    window.clearTimeout(timerId);
    raf = 0;
    timerId = 0;
  }

  function scheduleNext(delayMs: number) {
    if (delayMs <= 4) {
      raf = requestAnimationFrame(step);
      return;
    }
    timerId = window.setTimeout(() => {
      timerId = 0;
      raf = requestAnimationFrame(step);
    }, delayMs);
  }

  function syncAnalyserLease() {
    // Karaoke draws no spectrum; hidden / off-screen panels draw nothing.
    analyserLease.set(visible && inView && playingRef && activeMode !== "karaoke");
  }

  function syncLoop() {
    syncAnalyserLease();
    if (!visible || !inView) {
      clearLoop();
      return;
    }
    if (raf === 0 && timerId === 0) scheduleNext(0);
  }

  /**
   * Playing: throttled loop (≤30 fps, 24 on WebKitGTK). Paused / idle: one
   * calm frame, then the loop stops until something changes (DiscoWall may
   * keep going briefly while its pulse decays).
   */
  function step(t: number) {
    raf = 0;
    const c = canvas;
    if (!c || !visible || !inView) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const cadence =
      modeRef === "discowall"
        ? discowallLoopCadence({ expanded: expandedRef, active: engine.discoActive })
        : vizLoopCadence({
            expanded: expandedRef,
            isPlaying: playingRef,
          });
    const settling = !playingRef && modeRef === "discowall" && engine.discoActive;
    // Timer wakes a little early, rAF aligns to the frame (see NebulaCanvas).
    const interval = cadence.minFrameIntervalMs;
    const early = Math.min(10, interval * 0.15);
    const now = performance.now();
    // Playing but no analyser yet (graph not up / no audio): the idle frame is
    // static, so only poll for the analyser instead of repainting it.
    const waitingForData =
      playingRef && engine.lastFrameStatic && lastDrawKey === drawKey() && !player.getAnalyser();
    const due = !waitingForData && (!playingRef || now - lastDraw >= interval - early - 2);
    if (due) {
      lastDraw = now;
      lastDrawKey = drawKey();
      const w = c.width / backingScale;
      const h = c.height / backingScale;
      const liveTime = playingRef ? timeRef + Math.max(0, (t - timeStamp) / 1000) : timeRef;
      engine.drawFrame(ctx, {
        width: w,
        height: h,
        mode: modeRef,
        analyser: playingRef ? player.getAnalyser() : null,
        isPlaying: playingRef,
        expanded: expandedRef,
        currentTime: liveTime,
        trackKey: player.current?.rel_path ?? null,
      });
    }
    if (!playingRef && !settling) return;
    if (playingRef && engine.lastFrameStatic && !player.getAnalyser()) {
      scheduleNext(250);
      return;
    }
    scheduleNext(Math.max(1, interval - (performance.now() - lastDraw) - early));
  }

  function collapse() {
    expanded = false;
  }

  function toggleExpanded() {
    expanded = !expanded;
  }

  /**
   * When expanded, move the viz node to document.body (legacy createPortal)
   * while keeping `host` in-flow as size placeholder.
   */
  function portalExpanded(node: HTMLElement, active: boolean) {
    const host = node.parentElement;
    let portaled = false;

    const sync = (on: boolean) => {
      if (on && !portaled) {
        document.body.appendChild(node);
        portaled = true;
      } else if (!on && portaled && host) {
        host.appendChild(node);
        portaled = false;
      }
    };

    sync(active);
    return {
      update(on: boolean) {
        sync(on);
      },
      destroy() {
        sync(false);
      },
    };
  }

  $effect(() => {
    playingRef = playing;
    expandedRef = expanded;
    // Host stays in-flow while wrap is portaled; force visible when expanded.
    if (expanded) inView = true;
    syncLoop();
  });

  $effect(() => {
    timeRef = currentTime;
    timeStamp = performance.now();
  });

  $effect(() => {
    modeRef = activeMode;
    engine.resetForMode(activeMode);
    resize?.();
    syncLoop();
  });

  $effect(() => {
    if (!expanded) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") collapse();
    };
    window.addEventListener("keydown", onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prev;
    };
  });

  onMount(() => {
    unsubPrefs = subscribeUserPrefs((p) => {
      prefsMode = p.visualizerMode;
    });

    const c = canvas;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;

    const dpr = () =>
      typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;

    const size = () => {
      const p = c.parentElement;
      const lw = p ? p.clientWidth : 400;
      const lh = p ? Math.max(100, p.clientHeight || 200) : 200;
      let s = dpr();
      if (modeRef === "discowall") {
        // Per-pixel wall: legacy draws the panel at 1x, expanded at the lite cap.
        s = expandedRef ? canvasDprCap({ lite: true }) : 1;
      } else if (expandedRef) {
        s = Math.min(s, modeRef === "signals" ? 1.38 : 1.52);
      } else {
        s = canvasDprCap({ lite: true });
      }
      backingScale = s;
      c.width = Math.max(1, Math.floor(lw * s));
      c.height = Math.max(1, Math.floor(lh * s));
      c.style.width = `${lw}px`;
      c.style.height = `${lh}px`;
      ctx.setTransform(s, 0, 0, s, 0, 0);
      // Resizing clears the bitmap: paint again (paused loops are stopped).
      syncLoop();
    };
    size();
    resize = size;
    const ro = new ResizeObserver(size);
    if (c.parentElement) ro.observe(c.parentElement);

    const io =
      typeof IntersectionObserver !== "undefined" && wrapEl
        ? new IntersectionObserver(
            ([entry]) => {
              inView = Boolean(entry?.isIntersecting);
              syncLoop();
            },
            { rootMargin: "40px" },
          )
        : null;
    if (io && wrapEl) io.observe(wrapEl);

    const onVis = () => {
      visible = !document.hidden;
      syncLoop();
    };
    document.addEventListener("visibilitychange", onVis);

    // Theme switch while paused: the stopped loop must repaint with the new palette.
    const themeMo =
      typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => syncLoop())
        : null;
    themeMo?.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });

    visible = !document.hidden;
    syncLoop();

    return () => {
      clearLoop();
      resize = null;
      document.removeEventListener("visibilitychange", onVis);
      themeMo?.disconnect();
      io?.disconnect();
      ro.disconnect();
    };
  });

  onDestroy(() => {
    clearLoop();
    analyserLease.dispose();
    unsubPrefs?.();
  });
</script>

<div
  class="viz-host"
  class:is-expanded-host={expanded}
  bind:this={wrapEl}
>
  <div
    class="viz-wrap"
    class:is-expanded={expanded}
    class:is-karaoke={activeMode === "karaoke"}
    use:portalExpanded={expanded}
    role="button"
    tabindex="0"
    aria-label={expanded ? t("player.vizCollapseAria") : t("player.vizExpandAria")}
    onclick={() => toggleExpanded()}
    onkeydown={(event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        toggleExpanded();
      }
    }}
  >
    <canvas class="viz-canvas listen-viz" bind:this={canvas} aria-hidden="true"></canvas>

    {#if activeMode === "karaoke"}
      <div class="viz-karaoke-overlay" aria-live="polite">
        {#if parsedLrc.length}
          {#if expanded && previousLrcText}
            <p class="viz-karaoke-overlay__line viz-karaoke-overlay__line--prev">
              {previousLrcText}
            </p>
          {/if}
          <p class="viz-karaoke-overlay__line viz-karaoke-overlay__line--current">
            {currentLrcText || "…"}
          </p>
          {#if expanded && nextLrcText}
            <p class="viz-karaoke-overlay__line viz-karaoke-overlay__line--next">
              {nextLrcText}
            </p>
          {/if}
        {:else if plainLyrics}
          <pre class="viz-karaoke-overlay__plain">{plainLyrics}</pre>
        {:else}
          <p class="viz-karaoke-overlay__line viz-karaoke-overlay__line--current">
            {t("listen.karaokeEmpty")}
          </p>
        {/if}
      </div>
    {:else if activeMode === "discowall" && !session.current}
      <div class="viz-discowall-status" role="status">
        <p>{t("viz.discowallIdle")}</p>
      </div>
    {:else if expanded && currentLrcText}
      <div class="viz-lyrics-overlay" aria-live="polite">
        {#if previousLrcText}
          <p class="viz-lyrics-overlay__prev">{previousLrcText}</p>
        {/if}
        <p class="viz-lyrics-overlay__current">{currentLrcText}</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .viz-host {
    flex: 1;
    min-height: 0;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
    align-self: stretch;
    border-radius: inherit;
  }

  .viz-host.is-expanded-host {
    background: color-mix(in srgb, var(--rk-surface-3) 55%, transparent);
  }

  .viz-wrap {
    flex: 1;
    min-height: 0;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
    align-self: stretch;
    cursor: zoom-in;
    outline: none;
    border-radius: inherit;
    overflow: hidden;
  }

  .viz-wrap:focus-visible {
    box-shadow:
      inset 0 0 0 2px color-mix(in srgb, var(--rk-accent-2) 76%, transparent),
      0 0 0 3px color-mix(in srgb, var(--rk-accent-2) 18%, transparent);
  }

  /* One rung below the player so the dock stays visible and clickable. */
  .viz-wrap.is-expanded {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    left: var(--rk-side-w, 0px);
    z-index: calc(var(--rk-z-player) - 1);
    box-sizing: border-box;
    border-radius: 0;
    cursor: zoom-out;
    min-height: 100vh;
    min-height: 100dvh;
    padding: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px)
      max(
        env(safe-area-inset-bottom, 0px),
        var(--viz-expanded-bottom-clear, 0px)
      )
      env(safe-area-inset-left, 0px);
    background:
      radial-gradient(
        circle at 50% 50%,
        color-mix(in srgb, var(--rk-page-glow-2, var(--rk-accent-2)) 12%, transparent),
        transparent 55%
      ),
      linear-gradient(
        180deg,
        var(--rk-page-lg-1, var(--rk-bg-deep, var(--rk-bg))) 0%,
        var(--rk-page-lg-2, var(--rk-bg)) 100%
      );
  }

  .viz-canvas,
  .listen-viz {
    width: 100%;
    height: 100%;
    min-height: 0;
    flex: 1;
    display: block;
    border-radius: inherit;
  }

  .viz-wrap.is-expanded .viz-canvas {
    border-radius: 0;
  }

  .viz-discowall-status {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 1rem;
    pointer-events: none;
    text-align: center;
    z-index: 2;
  }

  .viz-discowall-status p {
    margin: 0;
    color: color-mix(in srgb, var(--rk-ink, #fff) 72%, transparent);
    font-size: var(--rk-fs-sm);
    font-weight: 600;
    text-shadow: 0 2px 12px rgba(0, 0, 0, 0.6);
  }

  .viz-karaoke-overlay,
  .viz-lyrics-overlay {
    position: absolute;
    left: max(1rem, env(safe-area-inset-left, 0px) + 0.6rem);
    right: max(1rem, env(safe-area-inset-right, 0px) + 0.6rem);
    bottom: calc(
      max(
          env(safe-area-inset-bottom, 0px),
          var(--viz-expanded-bottom-clear, 0px)
        ) + 1.5rem
    );
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    pointer-events: none;
    text-align: center;
    z-index: 2;
  }

  .viz-karaoke-overlay {
    top: 20%;
    bottom: auto;
    justify-content: center;
  }

  .viz-karaoke-overlay__line,
  .viz-lyrics-overlay__current,
  .viz-lyrics-overlay__prev {
    margin: 0;
    font-weight: 700;
    letter-spacing: 0.01em;
    text-shadow: 0 2px 16px rgba(0, 0, 0, 0.55);
  }

  .viz-karaoke-overlay__line--current,
  .viz-lyrics-overlay__current {
    color: var(--rk-ink, #fff);
    font-size: clamp(1.05rem, 2.4vw, 1.65rem);
  }

  .viz-karaoke-overlay__line--prev,
  .viz-karaoke-overlay__line--next,
  .viz-lyrics-overlay__prev {
    color: color-mix(in srgb, var(--rk-ink, #fff) 55%, transparent);
    font-size: clamp(0.85rem, 1.6vw, 1.1rem);
  }

  .viz-karaoke-overlay__plain {
    margin: 0;
    white-space: pre-wrap;
    text-align: center;
    color: var(--rk-ink, #fff);
    font: inherit;
    font-weight: 600;
    font-size: clamp(0.9rem, 1.8vw, 1.2rem);
    line-height: var(--rk-lh);
    text-shadow: 0 2px 16px rgba(0, 0, 0, 0.55);
    max-height: 50vh;
    overflow: hidden;
  }
</style>
