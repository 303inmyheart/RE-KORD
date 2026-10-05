/**
 * Locale choice for the hub panel, kept free of Svelte and DOM so the node
 * tests can import it directly.
 */

export type AdminLocale = "it" | "en" | "de";

export const ADMIN_LOCALES: readonly AdminLocale[] = ["it", "en", "de"];

/** localStorage key of the language picked in the panel header. */
export const LOCALE_STORAGE_KEY = "rekord-admin-locale";

/** `it`, `it-IT`, `en_US`, `de-AT`… → a supported locale, or null. */
export function normalizeLocale(value: unknown): AdminLocale | null {
  if (typeof value !== "string") return null;
  const base = value.trim().toLowerCase().split(/[-_]/)[0];
  return (ADMIN_LOCALES as readonly string[]).includes(base) ? (base as AdminLocale) : null;
}

/** Saved choice first, then the browser languages, then Italian. */
export function pickLocale(
  stored: unknown,
  languages: readonly (string | null | undefined)[],
): AdminLocale {
  const saved = normalizeLocale(stored);
  if (saved) return saved;
  for (const lang of languages) {
    const found = normalizeLocale(lang);
    if (found) return found;
  }
  return "it";
}

const DEFAULT_INTL_TAGS: Record<AdminLocale, string> = {
  it: "it-IT",
  en: "en-GB",
  de: "de-DE",
};

/**
 * BCP-47 tag for `Intl`: the browser's own regional variant when it speaks the
 * chosen language (en-US keeps US dates), otherwise a neutral default.
 */
export function intlTag(
  locale: AdminLocale,
  languages: readonly (string | null | undefined)[],
): string {
  for (const lang of languages) {
    if (typeof lang === "string" && normalizeLocale(lang) === locale) return lang;
  }
  return DEFAULT_INTL_TAGS[locale] ?? "it-IT";
}

/** Replace `{{name}}` placeholders. */
export function interpolate(
  template: string,
  vars?: Record<string, string | number>,
): string {
  if (!vars) return template;
  let out = template;
  for (const [k, v] of Object.entries(vars)) {
    out = out.replaceAll(`{{${k}}}`, String(v));
  }
  return out;
}
