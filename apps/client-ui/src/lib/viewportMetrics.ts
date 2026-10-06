/**
 * Keeps `--rk-app-vh` aligned with the window's actually visible height.
 *
 * With `interactive-widget=overlays-content` the virtual keyboard covers the page
 * without shrinking the viewport: `dvh` still spans the whole screen and a
 * 90dvh-tall dialog ends up under the keys. The visual viewport does shrink instead,
 * so we read it from here and pass it to the CSS.
 */

/** Above this zoom level the visual viewport is the lens, not the window. */
const PINCH_ZOOM_LIMIT = 1.01;

export function trackViewportMetrics(): () => void {
  if (typeof window === "undefined") return () => {};

  const root = document.documentElement;
  const vv = window.visualViewport;

  const apply = () => {
    // During a pinch-zoom the height is that of the zoomed portion: if we
    // used it, dialogs would shrink just because the user zoomed in.
    if (vv && vv.scale > PINCH_ZOOM_LIMIT) return;
    const h = Math.round(vv?.height ?? window.innerHeight);
    if (h > 0) root.style.setProperty("--rk-app-vh", `${h}px`);
  };

  apply();
  vv?.addEventListener("resize", apply);
  vv?.addEventListener("scroll", apply);
  window.addEventListener("resize", apply);
  window.addEventListener("orientationchange", apply);

  return () => {
    vv?.removeEventListener("resize", apply);
    vv?.removeEventListener("scroll", apply);
    window.removeEventListener("resize", apply);
    window.removeEventListener("orientationchange", apply);
    root.style.removeProperty("--rk-app-vh");
  };
}
