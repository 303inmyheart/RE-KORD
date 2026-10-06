<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { formatTime } from "../lib/player";

  let {
    currentTime = 0,
    duration = 0,
    onseek,
  }: {
    currentTime?: number;
    duration?: number;
    onseek: (seconds: number) => void;
  } = $props();

  /** Arrow keys step by this much; Page Up / Down by `BIG_STEP_S`. */
  const STEP_S = 5;
  const BIG_STEP_S = 30;

  let railEl: HTMLDivElement | null = $state(null);
  /** Position under the finger while dragging; the seek happens on release. */
  let dragTime = $state<number | null>(null);
  let dragPointer: number | null = null;

  const shownTime = $derived(dragTime ?? currentTime);
  /** 0..1 — drives transforms only, so a tick never triggers layout. */
  const ratio = $derived(duration > 0 ? Math.min(1, Math.max(0, shownTime / duration)) : 0);

  function timeFromClientX(clientX: number): number | null {
    if (!railEl || duration <= 0) return null;
    const rect = railEl.getBoundingClientRect();
    if (rect.width <= 0) return null;
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    return ratio * duration;
  }

  function onPointerDown(e: PointerEvent) {
    if (dragPointer != null || (e.pointerType === "mouse" && e.button !== 0)) return;
    const at = timeFromClientX(e.clientX);
    if (at == null) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragPointer = e.pointerId;
    dragTime = at;
  }

  function onPointerMove(e: PointerEvent) {
    if (e.pointerId !== dragPointer) return;
    const at = timeFromClientX(e.clientX);
    if (at != null) dragTime = at;
  }

  function endDrag(e: PointerEvent, commit: boolean) {
    if (e.pointerId !== dragPointer) return;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    const at = commit ? (timeFromClientX(e.clientX) ?? dragTime) : null;
    dragPointer = null;
    dragTime = null;
    if (at != null) onseek(at);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (duration <= 0 || e.ctrlKey || e.metaKey || e.altKey) return;
    let next: number | null = null;
    switch (e.key) {
      case "ArrowRight":
      case "ArrowUp":
        next = currentTime + STEP_S;
        break;
      case "ArrowLeft":
      case "ArrowDown":
        next = currentTime - STEP_S;
        break;
      case "PageUp":
        next = currentTime + BIG_STEP_S;
        break;
      case "PageDown":
        next = currentTime - BIG_STEP_S;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = Math.max(0, duration - 0.5);
        break;
      default:
        return;
    }
    // Handled here: the global ←/→ shortcut must not seek a second time.
    e.preventDefault();
    e.stopPropagation();
    onseek(Math.min(duration, Math.max(0, next)));
  }
</script>

<div class="timeline">
  <div
    class="progress2"
    class:is-dragging={dragTime != null}
    role="slider"
    tabindex="0"
    aria-valuemin={0}
    aria-valuemax={Math.floor(duration)}
    aria-valuenow={Math.floor(shownTime)}
    aria-valuetext={t("ui.timeline.valueText", {
      at: formatTime(shownTime),
      total: formatTime(duration),
    })}
    aria-label={t("ui.timeline.label")}
    aria-disabled={duration <= 0 ? "true" : undefined}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={(e) => endDrag(e, true)}
    onpointercancel={(e) => endDrag(e, false)}
    onlostpointercapture={(e) => endDrag(e, false)}
    onkeydown={onKeyDown}
  >
    <div class="slot" bind:this={railEl}>
      <div class="rail">
        <div class="fill" style:transform={`scaleX(${ratio})`}></div>
      </div>
      <!-- Full-width carrier: translateX(%) of its own width = % of the rail. -->
      <div class="thumb-track" style:transform={`translateX(${ratio * 100}%)`}>
        <div class="thumb"></div>
      </div>
    </div>
  </div>
  <div class="times rk-num">
    <span>{formatTime(shownTime)}</span>
    <span>{formatTime(duration)}</span>
  </div>
</div>

<style>
  .timeline {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .progress2 {
    position: relative;
    width: 100%;
    padding-block: 0.2rem;
    cursor: pointer;
    touch-action: none;
  }

  .progress2:focus-visible {
    outline: 2px solid var(--rk-focus);
    outline-offset: 2px;
  }

  .slot {
    position: relative;
    width: 100%;
    height: 14px;
  }

  .rail {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 10px;
    transform: translateY(-50%);
    border-radius: var(--rk-radius-sm);
    background: rgba(255, 255, 255, 0.07);
    overflow: hidden;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.2);
  }

  .fill {
    width: 100%;
    height: 100%;
    pointer-events: none;
    background: linear-gradient(90deg, var(--rk-accent), var(--rk-accent-2));
    transform-origin: left center;
  }

  .thumb-track {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .thumb {
    position: absolute;
    left: 0;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    z-index: 1;
    background: linear-gradient(
      145deg,
      color-mix(in srgb, var(--rk-accent-2) 90%, white 8%),
      color-mix(in srgb, var(--rk-accent) 88%, black 10%)
    );
    border: 1px solid color-mix(in srgb, var(--rk-ink) 22%, transparent);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.32);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .progress2:hover .thumb,
  .progress2:focus-visible .thumb,
  .progress2.is-dragging .thumb {
    transform: translate(-50%, -50%) scale(1.2);
  }

  .times {
    display: flex;
    justify-content: space-between;
    font-size: var(--rk-fs-1);
    font-weight: 550;
    color: var(--rk-muted);
    font-variant-numeric: tabular-nums;
  }

  /* Phone: thinner bar and bigger knob, because it is the only way to
     seek to a point in the track. */
  @media (max-width: 999.98px) {
    .slot {
      height: 12px;
    }

    .rail {
      height: 7px;
    }

    .thumb {
      width: 14px;
      height: 14px;
    }
  }

  /* With a finger the hit strip grows taller: the bar stays thin but grabbing it does not
     take precision. It deliberately stops short of the full 44px — a seek that
     tall would eat the dock and steal the page's scrolling. After the
     phone block, because on a phone both apply. */
  @media (pointer: coarse) {
    .progress2 {
      padding-block: var(--rk-space-md);
    }
  }
</style>
