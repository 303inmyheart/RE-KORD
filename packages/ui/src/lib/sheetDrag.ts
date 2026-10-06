/**
 * Drag down to close a sheet.
 *
 * On the phone the dialog comes from the bottom edge and the × sits at the top right,
 * far from the thumb: the natural gesture to dismiss it is pushing it down. Here
 * is the pure geometry (`createSheetDragGesture`, testable without a DOM) and the
 * Svelte action that attaches it to the panel.
 */

/** Below this distance it is still a stationary tap, not a drag. */
export const SHEET_ACTIVATE_PX = 8;
/** From here down the sheet closes even if the finger has stopped. */
export const SHEET_DISMISS_PX = 96;
/** A quick flick closes before reaching the threshold. */
export const SHEET_FLICK_PX_PER_MS = 0.5;
/** But a mere brush is not enough: a nudge of a few pixels stays a tap. */
export const SHEET_FLICK_MIN_PX = 24;

/** Same threshold as the mobile chrome: see styles/tokens.css and sheet.css. */
export const SHEET_MEDIA_QUERY = "(max-width: 999.98px)";

export interface SheetDragGesture {
  /** Starts following the finger. */
  start(y: number, time: number): void;
  /** Returns how far the sheet must move, never above its resting position. */
  move(y: number, time: number): number;
  /** Ends the grab and tells whether the sheet must go away. */
  end(y: number, time: number): { dismiss: boolean };
  /** True when the movement has passed the activation threshold. */
  isDragging(): boolean;
  cancel(): void;
}

export function createSheetDragGesture(): SheetDragGesture {
  let startY = 0;
  let startTime = 0;
  let tracking = false;
  let dragging = false;

  const offsetFor = (y: number) => Math.max(0, y - startY);

  return {
    start(y, time) {
      startY = y;
      startTime = time;
      tracking = true;
      dragging = false;
    },
    move(y, time) {
      if (!tracking) return 0;
      const offset = offsetFor(y);
      if (!dragging && offset > SHEET_ACTIVATE_PX) dragging = true;
      void time;
      return dragging ? offset : 0;
    },
    end(y, time) {
      if (!tracking) return { dismiss: false };
      const offset = offsetFor(y);
      const elapsed = Math.max(1, time - startTime);
      const speed = offset / elapsed;
      tracking = false;
      dragging = false;
      const dismiss =
        offset >= SHEET_DISMISS_PX ||
        (offset >= SHEET_FLICK_MIN_PX && speed >= SHEET_FLICK_PX_PER_MS);
      return { dismiss };
    },
    isDragging() {
      return dragging;
    },
    cancel() {
      tracking = false;
      dragging = false;
    },
  };
}

export interface SheetDragOptions {
  /** Active only when the dialog really is a sheet (phone). */
  enabled: boolean;
  /** Where the sheet can be grabbed: the handle and its header. */
  gripSelector: string;
  onclose: () => void;
}

/** How long the snap-back lasts when the gesture was not enough. */
const SNAP_BACK_MS = 160;

/**
 * Svelte action: applied to the sheet's panel, which is also the element that
 * moves. The grab however only counts inside `gripSelector`, otherwise scrolling
 * the content or pressing a button would start the gesture.
 */
export function sheetDrag(node: HTMLElement, options: SheetDragOptions) {
  let opts = options;
  const gesture = createSheetDragGesture();
  let activePointer: number | null = null;

  const setOffset = (px: number) => {
    node.style.transform = px > 0 ? `translateY(${px}px)` : "";
  };

  const snapBack = () => {
    node.style.transition = `transform ${SNAP_BACK_MS}ms ease`;
    setOffset(0);
    window.setTimeout(() => {
      node.style.transition = "";
    }, SNAP_BACK_MS + 40);
  };

  const stop = () => {
    if (activePointer != null && node.hasPointerCapture(activePointer)) {
      node.releasePointerCapture(activePointer);
    }
    activePointer = null;
  };

  const onPointerDown = (e: PointerEvent) => {
    if (!opts.enabled || activePointer != null || !e.isPrimary) return;
    if (!(e.target instanceof Element)) return;
    if (!e.target.closest(opts.gripSelector)) return;
    // A control inside the header (the × first of all) stays a control.
    if (e.target.closest("button, a, input, select, textarea")) return;

    activePointer = e.pointerId;
    node.style.transition = "";
    gesture.start(e.clientY, e.timeStamp);
    node.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: PointerEvent) => {
    if (activePointer !== e.pointerId) return;
    const offset = gesture.move(e.clientY, e.timeStamp);
    if (gesture.isDragging()) {
      e.preventDefault();
      setOffset(offset);
    }
  };

  const onPointerUp = (e: PointerEvent) => {
    if (activePointer !== e.pointerId) return;
    const { dismiss } = gesture.end(e.clientY, e.timeStamp);
    stop();
    if (dismiss) {
      opts.onclose();
      // The sheet is being unmounted: the position must be reset for the next round.
      setOffset(0);
      return;
    }
    snapBack();
  };

  const onPointerCancel = (e: PointerEvent) => {
    if (activePointer !== e.pointerId) return;
    gesture.cancel();
    stop();
    snapBack();
  };

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  node.addEventListener("pointerup", onPointerUp);
  node.addEventListener("pointercancel", onPointerCancel);

  return {
    update(next: SheetDragOptions) {
      opts = next;
      if (!next.enabled) {
        gesture.cancel();
        stop();
        node.style.transition = "";
        setOffset(0);
      }
    },
    destroy() {
      stop();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerCancel);
    },
  };
}
