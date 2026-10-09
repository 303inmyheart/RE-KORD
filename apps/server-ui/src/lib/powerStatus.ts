/**
 * The live status line of Rete › Energia, as plain text. No Svelte / DOM
 * here: the node tests import this file directly.
 */
import type { PowerState } from "../api";

export type Translate = (key: string, vars?: Record<string, string | number>) => string;

export type PowerLine = {
  /** `on`: sleep is blocked now; `error`: it should be but the platform said no. */
  tone: "on" | "off" | "error";
  text: string;
};

/** What keeps the hub busy, for "in uso: …". */
export function powerActivity(p: PowerState): "stream" | "job" | "remote" | "recent" {
  const s = p.status;
  if (s.activeStreams > 0) return "stream";
  if (s.activeJobs > 0) return "job";
  if (s.lastActivityKind === "remote") return "remote";
  return "recent";
}

/** Error text for a hub code; the hub's English detail when the code is unknown. */
function codeText(tr: Translate, code: string, detail: string | null): string {
  const key = `errors.code.${code}`;
  const text = tr(key);
  return text === key ? detail || code : text;
}

/**
 * @param fmt formats an RFC 3339 time for the line (a clock time today)
 */
export function powerStatusLine(p: PowerState, tr: Translate, fmt: (iso: string) => string): PowerLine {
  const s = p.status;
  if (s.inhibiting) {
    const time = s.since ? fmt(s.since) : "—";
    if (s.reason === "always") return { tone: "on", text: tr("power.status.always", { time }) };
    let text = tr("power.status.active", { what: tr(`power.what.${powerActivity(p)}`), time });
    if (s.releaseAt) text += ` ${tr("power.status.until", { time: fmt(s.releaseAt) })}`;
    return { tone: "on", text };
  }
  if (p.preventSleep !== "off" && s.errorCode) {
    return {
      tone: "error",
      text: tr("power.status.error", { reason: codeText(tr, s.errorCode, s.error) }),
    };
  }
  if (p.preventSleep === "whenActive") {
    let text = tr("power.status.idle");
    if (s.lastActivity) text += ` ${tr("power.status.last", { time: fmt(s.lastActivity) })}`;
    return { tone: "off", text };
  }
  return { tone: "off", text: tr("power.status.off") };
}

/** The lid part was asked for but refused. */
export function powerLidLine(p: PowerState, tr: Translate): string {
  const s = p.status;
  if (!p.keepAwakeLidClosed || !s.inhibiting || !s.lidErrorCode) return "";
  return tr("power.status.lidError", { reason: codeText(tr, s.lidErrorCode, s.lidError) });
}
