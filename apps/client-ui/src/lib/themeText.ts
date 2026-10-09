/**
 * Text colours of the custom theme (pure helpers, no DOM).
 *
 * The custom theme has one optional `text` colour. Unset means "automatic":
 * the ink is derived from the section colour as it always was. When the user
 * picks one, the main ink is that colour and the two muted inks are blends of
 * it towards the section colour, so secondary text keeps the same hue.
 */

export type Rgb = { r: number; g: number; b: number };

/** WCAG AA for normal-size text. */
export const WCAG_AA_TEXT = 4.5;

export function parseHex(hex: string): Rgb | null {
  const s = String(hex ?? "").trim();
  let h: string;
  if (/^#[0-9a-f]{6}$/i.test(s)) h = s.slice(1);
  else if (/^#[0-9a-f]{3}$/i.test(s)) h = `${s[1]}${s[1]}${s[2]}${s[2]}${s[3]}${s[3]}`;
  else return null;
  return {
    r: parseInt(h.slice(0, 2), 16),
    g: parseInt(h.slice(2, 4), 16),
    b: parseInt(h.slice(4, 6), 16),
  };
}

function rgbOf(hex: string): Rgb {
  return parseHex(hex) ?? { r: 0, g: 0, b: 0 };
}

export function rgbToHex({ r, g, b }: Rgb): string {
  const ch = (n: number) =>
    Math.max(0, Math.min(255, Math.round(n)))
      .toString(16)
      .padStart(2, "0");
  return `#${ch(r)}${ch(g)}${ch(b)}`;
}

/** Linear blend a → b (t = 0 gives a, 1 gives b). */
export function mixHex(a: string, b: string, t: number): string {
  const ca = rgbOf(a);
  const cb = rgbOf(b);
  return rgbToHex({
    r: ca.r * (1 - t) + cb.r * t,
    g: ca.g * (1 - t) + cb.g * t,
    b: ca.b * (1 - t) + cb.b * t,
  });
}

export function luminance({ r, g, b }: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrastRgb(a: Rgb, b: Rgb): number {
  const l1 = luminance(a);
  const l2 = luminance(b);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

export function contrastHex(a: string, b: string): number {
  return contrastRgb(rgbOf(a), rgbOf(b));
}

/** Same rule as the theme tokens: a light section means a light theme. */
export function isLightSection(section: string): boolean {
  return luminance(rgbOf(section)) > 0.45;
}

/** The ink the theme had before the text picker existed. */
export function autoInk(section: string): string {
  return isLightSection(section)
    ? mixHex(section, "#0f172a", 0.78)
    : mixHex(section, "#f8fafc", 0.92);
}

export type TextTokens = { ink: string; muted: string; mutedStrong: string };

/** `--rk-ink`, `--rk-muted`, `--rk-muted-strong` for a custom theme. */
export function customTextTokens(theme: {
  section: string;
  text?: string | null;
}): TextTokens {
  const section = theme.section;
  const chosen = theme.text ? parseHex(theme.text) : null;
  if (!chosen) {
    if (isLightSection(section)) {
      return {
        ink: autoInk(section),
        muted: mixHex(section, "#475569", 0.52),
        mutedStrong: mixHex(section, "#0f172a", 0.68),
      };
    }
    return {
      ink: autoInk(section),
      muted: mixHex(section, "#94a3b8", 0.55),
      mutedStrong: mixHex(section, "#e2e8f0", 0.72),
    };
  }
  const ink = rgbToHex(chosen);
  return {
    ink,
    muted: mixHex(section, ink, 0.62),
    mutedStrong: mixHex(section, ink, 0.8),
  };
}

/**
 * Surfaces text sits on: the section colour and the panel surface (a blend of
 * page background and section, as in the theme tokens).
 */
export function textSurfaces(theme: { bg: string; section: string }): string[] {
  return [theme.section, mixHex(theme.bg, theme.section, 0.55)];
}

/** Lowest contrast of the main ink over the theme surfaces. */
export function customTextContrast(theme: {
  bg: string;
  section: string;
  text?: string | null;
}): number {
  const { ink } = customTextTokens(theme);
  return Math.min(...textSurfaces(theme).map((s) => contrastHex(ink, s)));
}

/**
 * A readable ink for these surfaces: starts from the automatic ink (tinted
 * with the section hue) and moves towards white or black until every surface
 * reaches `minRatio`. Falls back to whichever of white/black reads better.
 */
export function pickReadableText(
  section: string,
  bg: string = section,
  minRatio: number = WCAG_AA_TEXT,
): string {
  const surfaces = textSurfaces({ bg, section });
  const worst = (hex: string) => Math.min(...surfaces.map((s) => contrastHex(hex, s)));
  const start = autoInk(section);
  if (worst(start) >= minRatio) return start;
  const avgLum =
    surfaces.reduce((sum, s) => sum + luminance(rgbOf(s)), 0) / surfaces.length;
  const target = avgLum > 0.18 ? "#000000" : "#ffffff";
  for (let step = 1; step <= 10; step += 1) {
    const candidate = mixHex(start, target, step / 10);
    if (worst(candidate) >= minRatio) return candidate;
  }
  return worst("#ffffff") >= worst("#000000") ? "#ffffff" : "#000000";
}
