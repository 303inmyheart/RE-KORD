<script lang="ts">
  /**
   * Sonic Nebula — interactive galaxy of the library (legacy SonicNebulaView).
   *
   * Pointer: drag to pan, wheel / pinch to zoom, click a star to select it
   * (callout with Play / Radio), double-click to play it, Shift+click to start a
   * local radio from its neighbourhood. Keyboard: arrows pan, +/- zoom, Esc
   * leaves fullscreen.
   */
  import { onMount } from "svelte";
  import type { Track } from "../../lib/api";
  import { t } from "../../lib/i18n.svelte";
  import {
    NEBULA_MAX_ZOOM,
    NEBULA_MIN_ZOOM,
    buildNebulaModel,
    buildNebulaSpatialGrid,
    clamp,
    defaultNebulaCamera,
    filterNebulaStars,
    nebulaStarsNear,
    pickNebulaStarAt,
    screenToWorld,
    starCalloutLayout,
    worldToScreen,
    zoomCameraAt,
    type NebulaCamera,
    type NebulaStar,
  } from "../../lib/nebula/model";
  import { consumeNebulaFullscreenRequest, onNebulaFullscreenRequest } from "../../lib/nebula/fullscreen";
  import { chartBpm, chartBpmSource, ensureChartBpmSource } from "../../lib/nebula/chartBpm.svelte";
  import { nebulaInputsFromTracks, type NebulaSourceTrack } from "../../lib/nebula/source";
  import { prefsRevision } from "../../lib/prefsRevision.svelte";
  import { session } from "../../lib/session.svelte";
  import { loadUserPrefs } from "../../lib/userPrefs";
  import UiIcon from "../icons/UiIcon.svelte";
  import NebulaCanvas from "./NebulaCanvas.svelte";

  type Star = NebulaStar<NebulaSourceTrack>;

  let {
    tracks,
    loading = false,
  }: {
    tracks: readonly Track[];
    loading?: boolean;
  } = $props();

  let query = $state("");
  let camera = $state.raw<NebulaCamera>(defaultNebulaCamera());
  let hoveredId = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let expanded = $state(false);
  let hintDismissed = $state(false);
  let stageEl: HTMLDivElement | null = $state(null);
  let stageW = $state(0);
  let stageH = $state(0);

  const coarsePointer =
    typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;

  const favoriteRelPaths = $derived(new Set(session.favorites.map((f) => f.rel_path)));

  /** Rebuilt only when tracks, moods, play counts or favourites change — not on player ticks. */
  const model = $derived.by(() => {
    void prefsRevision.playCounts;
    void prefsRevision.moods;
    void session.moodPrefsTick;
    void chartBpmSource.revision;
    const prefs = loadUserPrefs();
    const counts = prefs.playCounts;
    return buildNebulaModel(nebulaInputsFromTracks(tracks, prefs.trackMoods, chartBpm), {
      playCount: (rel) => counts[rel] ?? 0,
      favorites: favoriteRelPaths,
    });
  });

  const visibleStars = $derived(filterNebulaStars(model.stars, query));
  const sortedStars = $derived([...visibleStars].sort((a, b) => a.radius - b.radius));
  const spatial = $derived(buildNebulaSpatialGrid(visibleStars));
  const starById = $derived(new Map(model.stars.map((s) => [s.id, s])));

  const selected = $derived<Star | null>(selectedId ? (starById.get(selectedId) ?? null) : null);
  const currentId = $derived(session.current?.rel_path ?? null);
  const currentBpm = $derived(currentId ? (starById.get(currentId)?.bpm ?? 0) : 0);

  const callout = $derived.by(() => {
    if (!selected || stageW <= 0 || stageH <= 0) return null;
    const { sx, sy } = worldToScreen(selected.x, selected.y, stageW, stageH, camera);
    return { anchorX: sx, anchorY: sy, ...starCalloutLayout(sx, sy, stageW, stageH) };
  });
  const selectedIsPlaying = $derived(
    Boolean(selected && session.playing && selected.id === currentId),
  );

  // ── Actions ──────────────────────────────────────────────────────────────

  function selectStar(star: Star) {
    selectedId = star.id;
    hoveredId = star.id;
    camera = {
      x: star.x,
      y: star.y,
      zoom: clamp(Math.max(camera.zoom, 0.95), 0.35, 2.6),
    };
  }

  function playStar(star: Star) {
    selectedId = star.id;
    session.playSequence([star.track.source], 0);
  }

  /** "Radio from here": the star + its neighbours (similar tempo / energy by construction). */
  function playStarRadio(star: Star) {
    selectedId = star.id;
    const neighbors = nebulaStarsNear(visibleStars, star, 320, 24);
    session.playSequence([star, ...neighbors].map((s) => s.track.source), 0);
  }

  function surpriseMe() {
    const pool = visibleStars.length ? visibleStars : model.stars;
    if (!pool.length) return;
    selectStar(pool[Math.floor(Math.random() * pool.length)]!);
  }

  function resetView() {
    camera = defaultNebulaCamera();
    selectedId = null;
    hoveredId = null;
    query = "";
  }

  function toggleExpanded() {
    expanded = !expanded;
  }

  // ── Pointer / touch ──────────────────────────────────────────────────────

  const pointers = new Map<number, { x: number; y: number }>();
  let drag = { active: false, pointerId: -1, lastX: 0, lastY: 0, moved: false };
  let pinch = { active: false, startDist: 0, startZoom: 1, anchorX: 0, anchorY: 0, camera: defaultNebulaCamera() };

  function isChrome(target: EventTarget | null) {
    return target instanceof Element && Boolean(target.closest("[data-nebula-chrome]"));
  }

  function pinchMetrics() {
    const pts = [...pointers.values()];
    if (pts.length < 2) return null;
    const [a, b] = pts as [{ x: number; y: number }, { x: number; y: number }];
    return { dist: Math.hypot(b.x - a.x, b.y - a.y), cx: (a.x + b.x) / 2, cy: (a.y + b.y) / 2 };
  }

  function stageRect() {
    return stageEl?.getBoundingClientRect() ?? null;
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || isChrome(event.target)) return;
    hintDismissed = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    const pm = pinchMetrics();
    if (pm && pm.dist > 0) {
      pinch = { active: true, startDist: pm.dist, startZoom: camera.zoom, anchorX: pm.cx, anchorY: pm.cy, camera };
      drag.active = false;
      return;
    }
    drag = { active: true, pointerId: event.pointerId, lastX: event.clientX, lastY: event.clientY, moved: false };
  }

  function onPointerMove(event: PointerEvent) {
    if (isChrome(event.target)) return;
    const rect = stageRect();
    if (!rect) return;
    if (pointers.has(event.pointerId)) {
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    }
    const pm = pinchMetrics();
    if (pinch.active && pm && pm.dist > 0) {
      camera = zoomCameraAt(rect, pinch.anchorX, pinch.anchorY, pinch.camera, pinch.startZoom * (pm.dist / pinch.startDist));
      return;
    }
    if (drag.active && event.pointerId === drag.pointerId && pointers.size === 1) {
      const dx = event.clientX - drag.lastX;
      const dy = event.clientY - drag.lastY;
      const slop = event.pointerType === "touch" ? 12 : 3;
      if (Math.abs(dx) + Math.abs(dy) > slop) drag.moved = true;
      if (!drag.moved) return;
      drag.lastX = event.clientX;
      drag.lastY = event.clientY;
      camera = { ...camera, x: camera.x - dx / camera.zoom, y: camera.y - dy / camera.zoom };
      return;
    }
    if (pointers.size === 0 && event.pointerType !== "touch") {
      const { wx, wy } = screenToWorld(event.clientX, event.clientY, rect, camera);
      const hit = pickNebulaStarAt(spatial, wx, wy, camera.zoom);
      const next = hit?.id ?? (selectedId && hoveredId === selectedId ? selectedId : null);
      if (next !== hoveredId) hoveredId = next;
    }
  }

  function clearTransientHover() {
    if (hoveredId && hoveredId !== selectedId) hoveredId = selectedId;
  }

  function endPointer(event: PointerEvent) {
    pointers.delete(event.pointerId);
    if (pointers.size < 2) pinch.active = false;
    if (pointers.size === 0) {
      drag.active = false;
      drag.pointerId = -1;
    }
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* already released */
    }
  }

  function onPointerUp(event: PointerEvent) {
    if (isChrome(event.target)) return;
    const rect = stageRect();
    const wasPinch = pinch.active;
    const tapPointer = drag.pointerId;
    const moved = drag.moved;
    endPointer(event);
    if (!rect || wasPinch || pointers.size > 0 || moved || event.pointerId !== tapPointer) {
      if (pointers.size === 0) clearTransientHover();
      return;
    }
    const { wx, wy } = screenToWorld(event.clientX, event.clientY, rect, camera);
    const hit = pickNebulaStarAt(spatial, wx, wy, camera.zoom);
    if (!hit) {
      selectedId = null;
      hoveredId = null;
      return;
    }
    if (event.shiftKey) {
      playStarRadio(hit);
      return;
    }
    selectStar(hit);
  }

  function onDoubleClick(event: MouseEvent) {
    if (isChrome(event.target)) return;
    const rect = stageRect();
    if (!rect) return;
    const { wx, wy } = screenToWorld(event.clientX, event.clientY, rect, camera);
    const hit = pickNebulaStarAt(spatial, wx, wy, camera.zoom);
    if (hit) playStar(hit);
  }

  function onPointerCancel(event: PointerEvent) {
    endPointer(event);
    if (pointers.size === 0) clearTransientHover();
  }

  function onPointerLeave(event: PointerEvent) {
    // On touch, pointerleave fires on lift: it would wipe the selection.
    if (event.pointerType === "touch" || drag.active || pointers.size > 0) return;
    clearTransientHover();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (isChrome(event.target)) return;
    const step = 80 / camera.zoom;
    const rect = stageRect();
    switch (event.key) {
      case "ArrowLeft":
        camera = { ...camera, x: camera.x - step };
        break;
      case "ArrowRight":
        camera = { ...camera, x: camera.x + step };
        break;
      case "ArrowUp":
        camera = { ...camera, y: camera.y - step };
        break;
      case "ArrowDown":
        camera = { ...camera, y: camera.y + step };
        break;
      case "+":
      case "=":
        if (rect) camera = zoomCameraAt(rect, rect.left + rect.width / 2, rect.top + rect.height / 2, camera, camera.zoom * 1.15);
        break;
      case "-":
        if (rect) camera = zoomCameraAt(rect, rect.left + rect.width / 2, rect.top + rect.height / 2, camera, camera.zoom / 1.15);
        break;
      case "Enter":
        if (selected) {
          if (event.shiftKey) playStarRadio(selected);
          else playStar(selected);
        }
        break;
      default:
        return;
    }
    hintDismissed = true;
    event.preventDefault();
  }

  // Wheel must be non-passive to stop the page from scrolling.
  $effect(() => {
    const el = stageEl;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      if (isChrome(event.target)) return;
      event.preventDefault();
      hintDismissed = true;
      const rect = el.getBoundingClientRect();
      const factor = event.deltaY < 0 ? 1.07 : 0.93;
      camera = zoomCameraAt(rect, event.clientX, event.clientY, camera, camera.zoom * factor);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  $effect(() => {
    const el = stageEl;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      if (!entry) return;
      stageW = entry.contentRect.width;
      stageH = entry.contentRect.height;
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  $effect(() => {
    if (!expanded) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") expanded = false;
    };
    window.addEventListener("keydown", onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prev;
    };
  });

  /** Fullscreen: move the stage to <body> (legacy createPortal); the host keeps its place. */
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
      update: sync,
      destroy() {
        sync(false);
      },
    };
  }

  onMount(() => {
    void ensureChartBpmSource();
    if (consumeNebulaFullscreenRequest()) expanded = true;
    return onNebulaFullscreenRequest(() => {
      expanded = true;
    });
  });

  // Keep the selection meaningful when the filter hides it.
  $effect(() => {
    if (selectedId && !visibleStars.some((s) => s.id === selectedId)) {
      selectedId = null;
      hoveredId = null;
    }
  });

</script>

<div class="nebula-root">
  {#if !model.stars.length}
    <div class="nebula-empty">
      {loading ? t("nebula.loading") : t("nebula.empty")}
    </div>
  {:else}
    <!-- Canvas map: pointer + keyboard navigation on a focusable application region. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="nebula-stage"
      class:is-expanded={expanded}
      bind:this={stageEl}
      use:portalExpanded={expanded}
      role="application"
      tabindex="0"
      aria-label={t("nebula.stageAria")}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerCancel}
      onpointerleave={onPointerLeave}
      ondblclick={onDoubleClick}
      onkeydown={onKeyDown}
    >
      <NebulaCanvas
        fogs={model.fogs}
        {sortedStars}
        {camera}
        {hoveredId}
        {selectedId}
        {currentId}
        playing={session.playing}
        {currentBpm}
      />

      <header class="nebula-topbar" data-nebula-chrome>
        <div
          class="nebula-brand"
          title={`${t("nebula.legendCenter")}\n${t("nebula.legendEdge")}\n${t("nebula.legendAngle")}`}
        >
          <UiIcon name="sparkle" class="nebula-brand__ic" />
          <span class="nebula-brand__text">
            {t("nebula.title")}
            <span class="nebula-brand__count">· {visibleStars.length}{query ? ` / ${model.stars.length}` : ""}</span>
          </span>
        </div>
        <div class="nebula-tools">
          <input
            class="nebula-search"
            type="search"
            bind:value={query}
            placeholder={t("nebula.searchPlaceholder")}
            aria-label={t("nebula.searchPlaceholder")}
          />
          <button type="button" class="nebula-tool" onclick={surpriseMe} title={t("nebula.surprise")} aria-label={t("nebula.surprise")}>
            <UiIcon name="shuffle" class="nebula-tool__ic" />
          </button>
          <button type="button" class="nebula-tool" onclick={resetView} title={t("nebula.resetView")} aria-label={t("nebula.resetView")}>
            <svg class="nebula-tool__ic" viewBox="0 0 24 24" aria-hidden="true">
              <path fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" d="M4 12a8 8 0 1 0 2.4-5.7M4 4v4.5h4.5" />
            </svg>
          </button>
          <button
            type="button"
            class="nebula-tool"
            data-active={expanded}
            onclick={toggleExpanded}
            title={expanded ? t("nebula.collapse") : t("nebula.expand")}
            aria-label={expanded ? t("nebula.collapse") : t("nebula.expand")}
          >
            <svg class="nebula-tool__ic" viewBox="0 0 24 24" aria-hidden="true">
              {#if expanded}
                <path fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />
              {:else}
                <path fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
              {/if}
            </svg>
          </button>
        </div>
      </header>

      {#if !hintDismissed && !selected}
        <div class="nebula-hint" aria-hidden="true">
          {coarsePointer ? t("nebula.hintShort") : t("nebula.hint")}
        </div>
      {/if}

      {#if selected && callout}
        <div
          class="nebula-vignette"
          style:left="{callout.anchorX}px"
          style:top="{callout.anchorY}px"
          style:color={selected.color}
          aria-hidden="true"
        ></div>
        <div
          class="nebula-callout"
          style:left="{callout.left}px"
          style:top="{callout.top}px"
          style:--star-c={selected.color}
          data-nebula-chrome
        >
          <div class="nebula-callout__text">
            <p class="nebula-callout__title">{selected.track.title}</p>
            <p class="nebula-callout__meta">{selected.track.artist} · {selected.track.album}</p>
          </div>
          <button
            type="button"
            class="nebula-callout__btn"
            onclick={() => playStarRadio(selected!)}
            title={t("nebula.radioHere")}
            aria-label={t("nebula.radioHere")}
          >
            <UiIcon name="radio" />
          </button>
          <button
            type="button"
            class="nebula-callout__btn nebula-callout__btn--play"
            data-playing={selectedIsPlaying}
            onclick={() => playStar(selected!)}
            title={t("nebula.play")}
            aria-label={t("nebula.play")}
          >
            <UiIcon name="play" />
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .nebula-root {
    position: relative;
    width: 100%;
    height: clamp(22rem, calc(100dvh - 17rem), 52rem);
    min-height: 0;
  }

  .nebula-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 1rem;
    text-align: center;
    color: var(--rk-muted);
    border-radius: var(--rk-radius-lg);
    border: 1px solid var(--rk-line);
  }

  .nebula-stage {
    position: absolute;
    inset: 0;
    overflow: hidden;
    touch-action: none;
    cursor: grab;
    border-radius: var(--rk-radius-lg);
    border: 1px solid var(--rk-line);
    background: #03040a;
    user-select: none;
    -webkit-user-select: none;
    outline: none;
  }

  .nebula-stage:active {
    cursor: grabbing;
  }

  .nebula-stage:focus-visible {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--rk-accent-2) 70%, transparent);
  }

  /* One rung below the player so the dock stays usable (like the Listen viz). */
  .nebula-stage.is-expanded {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    left: var(--rk-side-w, 0px);
    z-index: calc(var(--rk-z-player) - 1);
    border: none;
    border-radius: 0;
    padding: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px)
      max(env(safe-area-inset-bottom, 0px), var(--rk-dock-h, 0px)) env(safe-area-inset-left, 0px);
  }

  .nebula-topbar {
    position: absolute;
    top: 10px;
    left: 10px;
    right: 10px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    pointer-events: none;
  }

  .nebula-stage.is-expanded .nebula-topbar {
    top: max(10px, env(safe-area-inset-top, 0px));
  }

  .nebula-brand,
  .nebula-search,
  .nebula-tool,
  .nebula-hint,
  .nebula-callout {
    /* Opaque chrome: no backdrop-filter over the live canvas (WebKitGTK re-blurs every frame). */
    border: 1px solid color-mix(in srgb, var(--rk-line) 70%, transparent);
    background: color-mix(in srgb, var(--rk-surface) 94%, #03040a);
  }

  .nebula-brand {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 999px;
    pointer-events: auto;
  }

  .nebula-brand :global(.nebula-brand__ic) {
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    color: #c4b5fd;
  }

  .nebula-brand__text {
    min-width: 0;
    font-size: var(--rk-fs-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .nebula-brand__count {
    color: var(--rk-muted);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .nebula-tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 1;
    min-width: 0;
    pointer-events: auto;
  }

  .nebula-search {
    width: min(280px, 48vw);
    flex: 1 1 8rem;
    min-width: 0;
    border-radius: 999px;
    color: var(--rk-ink);
    padding: 0.38rem 0.72rem;
    font: inherit;
    font-size: var(--rk-fs-sm);
  }

  .nebula-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2rem;
    min-width: 2rem;
    padding: 0 0.55rem;
    border-radius: 999px;
    color: var(--rk-ink);
    cursor: pointer;
  }

  .nebula-tool[data-active="true"] {
    border-color: color-mix(in srgb, var(--rk-accent) 65%, white);
  }

  .nebula-tool :global(.nebula-tool__ic),
  .nebula-tool__ic {
    width: 1rem;
    height: 1rem;
    display: block;
  }

  .nebula-hint {
    position: absolute;
    bottom: 14px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 3;
    max-width: calc(100% - 28px);
    padding: 6px 14px;
    border-radius: 999px;
    color: var(--rk-muted);
    font-size: var(--rk-fs-2xs, 0.72rem);
    text-align: center;
    pointer-events: none;
    animation: nebula-hint-in 0.6s ease both;
  }

  .nebula-stage.is-expanded .nebula-hint {
    bottom: calc(max(env(safe-area-inset-bottom, 0px), var(--rk-dock-h, 0px)) + 14px);
  }

  @keyframes nebula-hint-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }

  .nebula-vignette {
    position: absolute;
    z-index: 2;
    width: 240px;
    height: 240px;
    margin-left: -120px;
    margin-top: -120px;
    pointer-events: none;
    border-radius: 50%;
    background: radial-gradient(
      circle,
      color-mix(in srgb, currentColor 48%, transparent) 0%,
      color-mix(in srgb, currentColor 18%, transparent) 34%,
      transparent 72%
    );
  }

  .nebula-callout {
    position: absolute;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(252px, calc(100% - 24px));
    min-width: 0;
    padding: 8px 10px;
    border-radius: 12px;
    border-color: color-mix(in srgb, var(--star-c) 55%, var(--rk-line));
    box-shadow: 0 10px 36px rgba(0, 0, 0, 0.42);
    cursor: default;
  }

  .nebula-callout__text {
    min-width: 0;
    flex: 1;
  }

  .nebula-callout__title,
  .nebula-callout__meta {
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .nebula-callout__title {
    font-size: var(--rk-fs-sm);
    font-weight: 600;
    line-height: 1.25;
  }

  .nebula-callout__meta {
    margin-top: 0.14rem;
    font-size: var(--rk-fs-2xs, 0.7rem);
    color: var(--rk-muted);
  }

  .nebula-callout__btn {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 999px;
    border: 1px solid var(--rk-line);
    background: var(--rk-surface-2);
    color: var(--rk-ink);
    cursor: pointer;
  }

  .nebula-callout__btn :global(svg) {
    width: 1.1rem;
    height: 1.1rem;
  }

  .nebula-callout__btn--play {
    border: none;
    background: linear-gradient(135deg, var(--star-c, var(--rk-accent)), var(--rk-accent-2));
    color: #07101b;
  }

  .nebula-callout__btn--play[data-playing="true"] {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--rk-accent-2) 35%, transparent);
  }

  @media (max-width: 720px) {
    .nebula-topbar {
      top: 6px;
      left: 6px;
      right: 6px;
      flex-wrap: wrap;
      gap: 6px;
    }

    .nebula-tools {
      margin-left: 0;
      flex: 1 1 100%;
      justify-content: flex-end;
    }

    .nebula-search {
      width: auto;
      flex: 1 1 6rem;
    }

    .nebula-callout {
      width: min(228px, calc(100% - 20px));
      padding: 7px 8px;
    }
  }
</style>
