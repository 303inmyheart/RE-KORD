/**
 * Desktop width settings (Settings › Interface), saved per device.
 *
 * - content: how wide the page content (and the header row aligned with it)
 *   may grow before it is centred;
 * - dock: how wide the floating player bar may grow before it is centred.
 *
 * They live in this device's localStorage, not in the account prefs: the same
 * account on a 13" laptop and on a 34" monitor wants different widths. They
 * reach the page as two CSS custom properties on <html>, read only by the
 * desktop layout (min-width: 1000px), so the phone layout never sees them and
 * a change costs a style recalc, no script layout work.
 */

/** "default" = the built-in look (content 1360 px, player 72 rem). */
export type ContentWidthPref = "default" | "full" | number;
export type DockWidthPref = "default" | "full" | "content" | number;

export type LayoutWidthPrefs = {
  content: ContentWidthPref;
  dock: DockWidthPref;
};

export const LAYOUT_WIDTH_PRESETS = [1200, 1440, 1680, 1920] as const;
export const LAYOUT_WIDTH_MIN = 960;
export const LAYOUT_WIDTH_MAX = 2560;
export const LAYOUT_WIDTH_STEP = 20;
/** Built-in widths (packages/ui tokens.css `--rk-content-max`, PlayerDock `.bar`). */
export const DEFAULT_CONTENT_MAX_PX = 1360;
export const DEFAULT_DOCK_MAX_PX = 1152;

export const CONTENT_MAX_VAR = "--rk-user-content-max";
export const DOCK_MAX_VAR = "--rk-user-dock-max";

export const DEFAULT_LAYOUT_WIDTH: LayoutWidthPrefs = { content: "default", dock: "default" };

const STORAGE_KEY = "rekord.next.device.layoutWidth";

export function clampLayoutWidth(n: number): number {
  const v = Math.round(n / LAYOUT_WIDTH_STEP) * LAYOUT_WIDTH_STEP;
  return Math.min(LAYOUT_WIDTH_MAX, Math.max(LAYOUT_WIDTH_MIN, v));
}

function normalizeWidth(raw: unknown): number | null {
  const n = typeof raw === "number" ? raw : typeof raw === "string" ? Number(raw) : NaN;
  return Number.isFinite(n) && n > 0 ? clampLayoutWidth(n) : null;
}

export function normalizeContentWidth(raw: unknown): ContentWidthPref {
  if (raw === "full") return "full";
  return normalizeWidth(raw) ?? "default";
}

export function normalizeDockWidth(raw: unknown): DockWidthPref {
  if (raw === "full" || raw === "content") return raw;
  return normalizeWidth(raw) ?? "default";
}

export function normalizeLayoutWidth(raw: unknown): LayoutWidthPrefs {
  const o = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  return { content: normalizeContentWidth(o.content), dock: normalizeDockWidth(o.dock) };
}

/** CSS values for the two custom properties; `null` = leave the built-in width. */
export function layoutWidthCssVars(prefs: LayoutWidthPrefs): {
  content: string | null;
  dock: string | null;
} {
  const content =
    prefs.content === "default"
      ? null
      : prefs.content === "full"
        ? "none"
        : `${prefs.content}px`;
  let dock: string | null;
  if (prefs.dock === "default") dock = null;
  else if (prefs.dock === "full") dock = "none";
  else if (prefs.dock === "content") dock = content ?? `${DEFAULT_CONTENT_MAX_PX}px`;
  else dock = `${prefs.dock}px`;
  return { content, dock };
}

export function loadLayoutWidth(): LayoutWidthPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? normalizeLayoutWidth(JSON.parse(raw)) : { ...DEFAULT_LAYOUT_WIDTH };
  } catch {
    return { ...DEFAULT_LAYOUT_WIDTH };
  }
}

export function saveLayoutWidth(prefs: LayoutWidthPrefs): void {
  const clean = normalizeLayoutWidth(prefs);
  try {
    if (clean.content === "default" && clean.dock === "default") {
      localStorage.removeItem(STORAGE_KEY);
    } else {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(clean));
    }
  } catch {
    /* private mode / quota: the widths still apply to this page */
  }
}

export function applyLayoutWidth(
  prefs: LayoutWidthPrefs,
  root: HTMLElement | undefined = typeof document !== "undefined"
    ? document.documentElement
    : undefined,
): void {
  if (!root) return;
  const vars = layoutWidthCssVars(prefs);
  if (vars.content) root.style.setProperty(CONTENT_MAX_VAR, vars.content);
  else root.style.removeProperty(CONTENT_MAX_VAR);
  if (vars.dock) root.style.setProperty(DOCK_MAX_VAR, vars.dock);
  else root.style.removeProperty(DOCK_MAX_VAR);
}

let storageBound = false;

/** Boot: apply this device's widths and follow changes made in other tabs. */
export function initLayoutWidth(): void {
  applyLayoutWidth(loadLayoutWidth());
  if (storageBound || typeof window === "undefined") return;
  storageBound = true;
  window.addEventListener("storage", (event: StorageEvent) => {
    if (event.key == null || event.key === STORAGE_KEY) applyLayoutWidth(loadLayoutWidth());
  });
}
