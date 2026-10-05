/**
 * One owner for `history` and `popstate`.
 *
 * On Android the Tauri shell answers the hardware Back key with
 * `webView.canGoBack() ? goBack() : exit`. For Back to close an open sheet and
 * then step through the app's sections instead of quitting, both overlays and
 * in-app navigation have to leave history entries — and they have to agree on
 * who handles each `popstate`, or one Back closes a dialog *and* changes view.
 *
 * - Overlays (Modal, sheets) call `pushBackLayer(onBack)`: Back closes the
 *   top-most one; closing it any other way removes its entry silently.
 * - Navigation calls `pushHistoryEntry` / `historyBackSilently` and handles the
 *   pops no overlay claimed through `onHistoryPop`.
 *
 * Disabled until the app calls `enableBackStack()` (server-ui never does), so a
 * plain page using the shared Modal keeps its history untouched.
 */

type Layer = { id: number; onBack: () => void };
type PopHandler = (state: Record<string, unknown> | null) => void;

const LAYER_KEY = "rkLayer";
/** A `history.back()` we issued whose `popstate` never came (first entry). */
const SILENT_BACK_TIMEOUT_MS = 1000;

let enabled = false;
let nextLayerId = 1;
const layers: Layer[] = [];
const popHandlers = new Set<PopHandler>();
/** Pops caused by our own `history.back()`, to be swallowed. */
let silentPops = 0;
let silentTimer: ReturnType<typeof setTimeout> | null = null;
/** Pushes requested while a silent back was in flight (it would undo them). */
let queued: Array<() => void> = [];

function currentState(): Record<string, unknown> {
  const s = history.state;
  return s && typeof s === "object" ? { ...(s as Record<string, unknown>) } : {};
}

function flushQueued() {
  const run = queued;
  queued = [];
  for (const fn of run) fn();
}

function settleSilent() {
  if (silentTimer) clearTimeout(silentTimer);
  silentTimer = null;
  silentPops = Math.max(0, silentPops - 1);
  if (silentPops === 0) flushQueued();
}

function silentBack() {
  silentPops += 1;
  if (silentTimer) clearTimeout(silentTimer);
  silentTimer = setTimeout(() => {
    silentPops = 0;
    silentTimer = null;
    flushQueued();
  }, SILENT_BACK_TIMEOUT_MS);
  history.back();
}

function whenIdle(fn: () => void) {
  if (silentPops > 0) queued.push(fn);
  else fn();
}

function onPopState(e: PopStateEvent) {
  const state =
    e.state && typeof e.state === "object" ? (e.state as Record<string, unknown>) : null;
  const target = typeof state?.[LAYER_KEY] === "number" ? (state[LAYER_KEY] as number) : 0;
  if (silentPops > 0) {
    if (target > 0 && !layers.some((l) => l.id === target)) {
      // Our back landed on the entry of an overlay closed while not on top
      // (two dialogs closed in one go): step over it too, or the user's next
      // Back would only pop this dead entry and seem to do nothing.
      silentPops -= 1;
      silentBack();
      return;
    }
    settleSilent();
    return;
  }

  // Back from an overlay: every layer above the entry we landed on closes.
  let closed = false;
  while (layers.length && layers[layers.length - 1]!.id > target) {
    const top = layers.pop()!;
    closed = true;
    try {
      top.onBack();
    } catch (err) {
      console.error("[backStack] layer close failed", err);
    }
  }
  if (closed) {
    // Landed on an orphan overlay entry: this Back already closed something,
    // so step over the dead entry without a second visible effect.
    if (target > 0 && !layers.some((l) => l.id === target)) silentBack();
    return;
  }

  if (target > 0 && !layers.some((l) => l.id === target)) {
    // Entry of an overlay that was closed while not on top: skip over it.
    history.back();
    return;
  }
  if (target > 0) return;
  for (const fn of popHandlers) fn(state);
}

/** Install the `popstate` owner. Returns the teardown. */
export function enableBackStack(): () => void {
  if (enabled || typeof window === "undefined") return () => {};
  enabled = true;
  window.addEventListener("popstate", onPopState);
  return () => {
    enabled = false;
    window.removeEventListener("popstate", onPopState);
    layers.length = 0;
    popHandlers.clear();
    queued = [];
    silentPops = 0;
  };
}

export function isBackStackEnabled(): boolean {
  return enabled;
}

/**
 * Register an overlay that Back should close. Returns `release`, to call when
 * the overlay closes for any other reason (it drops the history entry).
 */
export function pushBackLayer(onBack: () => void): () => void {
  if (!enabled) return () => {};
  const layer: Layer = { id: nextLayerId++, onBack };
  layers.push(layer);
  whenIdle(() => {
    if (!layers.includes(layer)) return;
    history.pushState({ ...currentState(), [LAYER_KEY]: layer.id }, "");
  });
  return () => {
    const i = layers.indexOf(layer);
    if (i < 0) return; // already closed by Back
    layers.splice(i, 1);
    const st = history.state as Record<string, unknown> | null;
    // Only pop our own entry when it is the one on top; otherwise it becomes an
    // orphan that `onPopState` skips over later.
    if (st && st[LAYER_KEY] === layer.id) silentBack();
  };
}

/** Number of overlays currently registered with Back. */
export function backLayerCount(): number {
  return layers.length;
}

/** Push a navigation entry (deferred while one of our own backs is in flight). */
export function pushHistoryEntry(state: Record<string, unknown>) {
  if (!enabled) return;
  whenIdle(() => history.pushState(state, ""));
}

/** Replace the current entry's state (no new entry). */
export function replaceHistoryEntry(state: Record<string, unknown>) {
  if (!enabled) return;
  whenIdle(() => history.replaceState(state, ""));
}

/** Step back one entry without notifying `onHistoryPop` handlers. */
export function historyBackSilently() {
  if (!enabled) return;
  whenIdle(silentBack);
}

/** Handle Back/Forward pops that did not belong to an overlay. */
export function onHistoryPop(fn: PopHandler): () => void {
  popHandlers.add(fn);
  return () => popHandlers.delete(fn);
}
