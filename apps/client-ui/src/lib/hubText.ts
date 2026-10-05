import { hubActivityText, type HubActivityEntry, type HubTextTranslator } from "@rekord/ui";
import { i18n } from "./i18n.svelte";

/**
 * Hub-generated text (activity log) in the UI language, from the hub's
 * stable codes and the `locales/hub` table; the hub's own words otherwise.
 */
const translator: HubTextTranslator = {
  has: (key) => i18n.has(key),
  t: (key, vars) => i18n.t(key, vars),
  formatNumber: (n) => i18n.formatNumber(n),
};

/** An activity-log line in the UI language (older uncoded lines included). */
export function hubActivityMsg(entry: HubActivityEntry): string {
  return hubActivityText(translator, entry);
}

/** Activity kind label ("scan" → "Scansione"); the raw kind when unknown. */
export function hubKind(kind: string): string {
  const key = `hub.kind.${kind.trim()}`;
  return i18n.has(key) ? i18n.t(key) : kind;
}
