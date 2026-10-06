/**
 * Horizontal swipe on the player bar: right = previous track,
 * left = next, tap = opens Studio → Listen.
 *
 * The gesture math lives in `createSwipeGesture` without touching the DOM, so
 * the thresholds can be checked with node tests; the `playerSwipe` action
 * wires it to pointer events. Thresholds as in the 5.x dock (usePlayerBarSwipe).
 */

/** Distance beyond which the gesture becomes a swipe and not a scroll. */
export const SWIPE_ACTIVATE_PX = 12;
/** Distance that triggers the track change. */
export const SWIPE_THRESHOLD_PX = 48;
/** Beyond this vertical movement the gesture is a scroll, not a swipe. */
export const SWIPE_MAX_VERTICAL_PX = 40;
/** Within this movement the gesture counts as a tap. */
export const TAP_MAX_MOVE_PX = 10;

export type SwipeMove = "idle" | "capture" | "cancel" | "prev" | "next";
export type SwipeEnd = "tap" | "none";

export type SwipeGesture = {
  begin(x: number, y: number): void;
  /** `capture` asks the caller to take pointer capture. */
  move(x: number, y: number): SwipeMove;
  end(x: number, y: number): SwipeEnd;
  abort(): void;
  /** True if the gesture changed track: used to suppress the click that follows. */
  get fired(): boolean;
  get capturing(): boolean;
};

export function createSwipeGesture(): SwipeGesture {
  let start: { x: number; y: number } | null = null;
  let capturing = false;
  let fired = false;

  const reset = () => {
    start = null;
    capturing = false;
  };

  return {
    begin(x, y) {
      start = { x, y };
      capturing = false;
      fired = false;
    },
    move(x, y) {
      if (!start || fired) return "idle";
      const dx = x - start.x;
      const dy = y - start.y;
      if (Math.abs(dy) > SWIPE_MAX_VERTICAL_PX) {
        reset();
        return "cancel";
      }
      if (!capturing) {
        if (Math.abs(dx) < SWIPE_ACTIVATE_PX || Math.abs(dx) <= Math.abs(dy)) {
          return "idle";
        }
        capturing = true;
        return "capture";
      }
      if (Math.abs(dx) < SWIPE_THRESHOLD_PX) return "idle";
      fired = true;
      reset();
      return dx > 0 ? "prev" : "next";
    },
    end(x, y) {
      if (!start) {
        reset();
        return "none";
      }
      const dx = Math.abs(x - start.x);
      const dy = Math.abs(y - start.y);
      const tap = !fired && dx <= TAP_MAX_MOVE_PX && dy <= TAP_MAX_MOVE_PX;
      reset();
      return tap ? "tap" : "none";
    },
    abort() {
      reset();
      fired = false;
    },
    get fired() {
      return fired;
    },
    get capturing() {
      return capturing;
    },
  };
}

export type PlayerSwipeOptions = {
  enabled: boolean;
  onprev: () => void;
  onnext: () => void;
  /** Tap on the bar: typically opens Studio → Listen. */
  ontap?: () => void;
  /**
   * Elements that handle their own gesture: transport, position bar,
   * fields. There the gesture does not even start.
   */
  ignoreSelector?: string;
  /**
   * Elements that stay swipeable but do not respond to taps, because they
   * already have a destination of their own: artist and album in the track row.
   */
  tapIgnoreSelector?: string;
};

const DEFAULT_IGNORE = "input, [data-swipe-ignore]";

/** Svelte action: `use:playerSwipe={{ enabled, onprev, onnext, ontap }}`. */
export function playerSwipe(node: HTMLElement, options: PlayerSwipeOptions) {
  let opts = options;
  const gesture = createSwipeGesture();
  let pointerId: number | null = null;
  let suppressClick = false;

  const ignored = (target: EventTarget | null): boolean => {
    if (!(target instanceof Element)) return true;
    const selector = opts.ignoreSelector
      ? `${DEFAULT_IGNORE}, ${opts.ignoreSelector}`
      : DEFAULT_IGNORE;
    return target.closest(selector) !== null;
  };

  const capture = (id: number) => {
    try {
      node.setPointerCapture(id);
    } catch {
      /* the pointer may already have left the page */
    }
  };

  const release = () => {
    if (pointerId === null) return;
    try {
      node.releasePointerCapture(pointerId);
    } catch {
      /* the pointer may already have been released by the browser */
    }
    pointerId = null;
  };

  const onPointerDown = (e: PointerEvent) => {
    if (!opts.enabled || ignored(e.target)) return;
    pointerId = e.pointerId;
    suppressClick = false;
    gesture.begin(e.clientX, e.clientY);
  };

  const onPointerMove = (e: PointerEvent) => {
    if (pointerId !== e.pointerId) return;
    const move = gesture.move(e.clientX, e.clientY);
    if (move === "capture") {
      capture(e.pointerId);
      return;
    }
    if (move === "prev" || move === "next") {
      suppressClick = true;
      release();
      if (move === "prev") opts.onprev();
      else opts.onnext();
    }
  };

  const tapIgnored = (target: EventTarget | null): boolean => {
    if (ignored(target)) return true;
    if (!opts.tapIgnoreSelector) return false;
    return (
      target instanceof Element &&
      target.closest(opts.tapIgnoreSelector) !== null
    );
  };

  const onPointerUp = (e: PointerEvent) => {
    if (pointerId !== e.pointerId) return;
    const end = gesture.end(e.clientX, e.clientY);
    release();
    if (end === "tap" && !tapIgnored(e.target)) opts.ontap?.();
  };

  const onPointerCancel = (e: PointerEvent) => {
    if (pointerId !== e.pointerId) return;
    gesture.abort();
    release();
  };

  /** The swipe ends over a button: the click that follows must not fire. */
  const onClickCapture = (e: MouseEvent) => {
    if (!suppressClick) return;
    suppressClick = false;
    e.preventDefault();
    e.stopPropagation();
  };

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  node.addEventListener("pointerup", onPointerUp);
  node.addEventListener("pointercancel", onPointerCancel);
  node.addEventListener("click", onClickCapture, true);

  return {
    update(next: PlayerSwipeOptions) {
      opts = next;
      if (!next.enabled) {
        gesture.abort();
        release();
      }
    },
    destroy() {
      release();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerCancel);
      node.removeEventListener("click", onClickCapture, true);
    },
  };
}
