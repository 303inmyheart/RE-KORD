/**
 * Cross-component signal to open Sonic Nebula already expanded (legacy
 * `nebulaFullscreen.ts`): an event for a mounted view, plus a short time window
 * for the view that mounts right after navigation (e.g. Dashboard card → Library).
 */

const NEBULA_FULLSCREEN_EVENT = "rekord-nebula-fullscreen";
const PENDING_WINDOW_MS = 2500;

let pendingUntil = 0;

export function requestNebulaFullscreen(): void {
  pendingUntil = Date.now() + PENDING_WINDOW_MS;
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(NEBULA_FULLSCREEN_EVENT));
  }
}

export function consumeNebulaFullscreenRequest(): boolean {
  const pending = Date.now() < pendingUntil;
  pendingUntil = 0;
  return pending;
}

export function onNebulaFullscreenRequest(cb: () => void): () => void {
  const handler = () => {
    if (consumeNebulaFullscreenRequest()) cb();
  };
  window.addEventListener(NEBULA_FULLSCREEN_EVENT, handler);
  return () => window.removeEventListener(NEBULA_FULLSCREEN_EVENT, handler);
}
