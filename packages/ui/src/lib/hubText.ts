/**
 * Hub-generated text (activity log lines, job titles and details) comes with
 * a stable code and params next to the hub's own wording. The panels look the
 * code up as `hub.<area>.<code>` and fall back to the hub text when the code
 * is unknown (older hub, newer event) or a placeholder stays unfilled.
 */

export type HubTextTranslator = {
  has: (key: string) => boolean;
  t: (key: string, vars?: Record<string, string | number>) => string;
  formatNumber: (n: number) => string;
};

export type HubTextArea = "activity" | "job";
export type HubParams = Record<string, unknown> | null | undefined;

/** Params as placeholder values: counts localized, known enum values translated. */
function hubVars(tr: HubTextTranslator, params: HubParams): Record<string, string | number> {
  const out: Record<string, string | number> = {};
  for (const [name, value] of Object.entries(params ?? {})) {
    if (typeof value === "number") {
      // Ids stay as the hub wrote them ("1234", not "1.234").
      out[name] = /id$/i.test(name) ? String(value) : tr.formatNumber(value);
    } else if (typeof value === "string") {
      const key = `hub.param.${name}.${value}`;
      out[name] = tr.has(key) ? tr.t(key) : value;
    } else if (typeof value === "boolean") {
      out[name] = String(value);
    }
  }
  return out;
}

export function hubText(
  tr: HubTextTranslator,
  area: HubTextArea,
  code: string | null | undefined,
  params: HubParams,
  fallback: string,
): string {
  const c = (code ?? "").trim();
  if (!c) return fallback;
  const key = `hub.${area}.${c}`;
  if (!tr.has(key)) return fallback;
  const text = tr.t(key, hubVars(tr, params));
  return /\{\{\w+\}\}/.test(text) ? fallback : text;
}

export type HubActivityEntry = {
  kind: string;
  message: string;
  code?: string | null;
  params?: Record<string, unknown> | null;
};

type LegacyRule = [RegExp, string, (m: RegExpExecArray) => Record<string, unknown>];

const num = (s: string | undefined) => Number(s ?? NaN);

/**
 * Lines written before the hub sent codes (plain English / Italian text):
 * the same code and params read back from the text, so older entries of the
 * log are translated too. Unknown lines stay as written.
 */
const LEGACY_ACTIVITY: LegacyRule[] = [
  [/^settings updated$/, "settings.updated", () => ({})],
  [/^library scan started \((\w+)\)$/, "scan.started", (m) => ({ mode: m[1] })],
  [
    /^(\d+) tracce non trovate ma mantenute: (.+)$/,
    "scan.pruneSkipped",
    (m) => ({ missing: num(m[1]), reason: m[2] }),
  ],
  [
    /^download (finished|cancelled|failed) \(([^)]*)\): (.*) — (\d+) ok, (\d+) skipped, (\d+) failed$/,
    "",
    (m) => ({
      verdict: m[1],
      kind: m[2],
      folder: m[3],
      ok: num(m[4]),
      skipped: num(m[5]),
      failed: num(m[6]),
    }),
  ],
  [/^download started \(([^)]*)\): (.*)$/, "download.started", (m) => ({ kind: m[1], folder: m[2] })],
  [/^library selection updated$/, "library.selection", () => ({})],
  [/^layout impostato: (.+)$/, "library.layout", (m) => ({ layout: m[1] })],
  [/^watcher libreria attivato$/, "library.watchOn", () => ({})],
  [/^watcher libreria disattivato$/, "library.watchOff", () => ({})],
  [/^operazioni di macchina abilitate da remoto$/, "system.remoteAdminOn", () => ({})],
  [/^operazioni di macchina limitate al computer dell'hub$/, "system.remoteAdminOff", () => ({})],
  [/^yt-dlp updated: (.+) → (.+)$/, "system.ytdlpUpdated", (m) => ({ from: m[1], to: m[2] })],
  [/^tunnel url: (.+)$/, "remote.tunnelUrl", (m) => ({ url: m[1] })],
  [/^public url set: (.+)$/, "remote.publicUrl", (m) => ({ url: m[1] })],
  [/^cloudflared tunnel start requested$/, "remote.startRequested", () => ({})],
  [/^tunnel stopped$/, "remote.stopped", () => ({})],
  [/^cloudflare logout$/, "remote.logout", () => ({})],
  [/^favorite added: (\d+)$/, "favorites.added", (m) => ({ trackId: num(m[1]) })],
  [/^favorite removed: (\d+)$/, "favorites.removed", (m) => ({ trackId: num(m[1]) })],
  [/^playlist created: (.+)$/, "playlist.created", (m) => ({ name: m[1] })],
  [/^playlist renamed: (.+?) → (.+)$/, "playlist.renamed", (m) => ({ id: m[1], name: m[2] })],
  [/^playlist deleted: (.+)$/, "playlist.deleted", (m) => ({ id: m[1] })],
  [/^account created: (.+)$/, "account.created", (m) => ({ name: m[1] })],
  [/^account renamed: (.+)$/, "account.renamed", (m) => ({ name: m[1] })],
  [/^account deleted: (.+)$/, "account.deleted", (m) => ({ id: m[1] })],
  [
    /^allow_remote_admin del backup ignorato \(resta disattivato\)$/,
    "restore.remoteAdminIgnored",
    () => ({}),
  ],
];

/** Code and params of an activity line: the hub's own, else read from older text. */
export function hubActivityCode(
  entry: HubActivityEntry,
): { code: string; params: Record<string, unknown> } | null {
  const code = (entry.code ?? "").trim();
  if (code) return { code, params: entry.params ?? {} };
  const text = (entry.message ?? "").trim();
  for (const [re, ruleCode, read] of LEGACY_ACTIVITY) {
    const m = re.exec(text);
    if (!m) continue;
    const params = read(m);
    if (!ruleCode) {
      const { verdict, ...rest } = params;
      return { code: `download.${String(verdict)}`, params: rest };
    }
    return { code: ruleCode, params };
  }
  return null;
}

/** An activity line in the UI language; the hub's text when it is not known. */
export function hubActivityText(tr: HubTextTranslator, entry: HubActivityEntry): string {
  const coded = hubActivityCode(entry);
  if (!coded) return entry.message;
  return hubText(tr, "activity", coded.code, coded.params, entry.message);
}
