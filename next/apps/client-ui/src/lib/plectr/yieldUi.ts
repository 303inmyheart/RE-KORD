/**
 * Yields the thread between heavy phases (decode / chart analysis).
 * In a Worker (no rIC/rAF) it is a setTimeout(0): analysis must not slow
 * down there, it does not block the UI anyway.
 */
export function yieldUi(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestIdleCallback === "function") {
      requestIdleCallback(() => resolve(), { timeout: 48 });
    } else if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => resolve());
    } else {
      setTimeout(resolve, 0);
    }
  });
}
