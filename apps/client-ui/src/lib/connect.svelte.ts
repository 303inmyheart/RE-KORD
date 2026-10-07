/**
 * First launch: which hub to connect to and with which account.
 *
 * In the browser served by the hub none of this is visible, because the hub
 * is the page's origin and answers right away. It is needed in the APK and in the
 * desktop client, where the UI is inside the app and the hub is an address only
 * the person installing it knows.
 */

import { hubModules } from "./hubModules.svelte";
import { getSelectedAccountId, type Account, type AccountsResponse } from "./account";
import { getServerBaseUrl, setServerBaseUrl } from "./config";
import { i18n } from "./i18n.svelte";
import { checkHubCompat, compat } from "./platform/compatState.svelte";
import { isTauri } from "./platform/env";

export type ConnectPhase =
  /** Silent probe at startup: show the logo, not a question yet. */
  | "probing"
  | "connect"
  | "app";

export type ProbeFailure =
  | { reason: "unreachable" }
  | { reason: "timeout" }
  | { reason: "http"; status: number }
  | { reason: "not-hub" }
  | { reason: "no-accounts" };

export type HubProbe =
  | {
      ok: true;
      accounts: Account[];
      defaultAccountId: string;
      version: string;
      /** `/api/v1` API version declared by the hub (absent on hubs < 5.1). */
      apiVersion: number | null;
      /** Oldest client the hub accepts (see `platform/compat.ts`). */
      minClientVersion: string;
    }
  | ({ ok: false } & ProbeFailure);

/**
 * Six seconds: a hub on the local network answers in milliseconds, and past this
 * threshold whoever is in front of the screen has already realised the address is wrong.
 */
const PROBE_TIMEOUT_MS = 6_000;

/** Shorter probe at startup: here we only decide whether to show the flow. */
const STARTUP_PROBE_TIMEOUT_MS = 3_500;

/**
 * Desktop client and hub on the same machine: the normal case on a computer, and
 * the only address that can be guessed. On a phone the port is closed and
 * the attempt ends immediately, without making anyone wait.
 */
const LOCAL_HUB = "http://127.0.0.1:7420";
const LOCAL_PROBE_TIMEOUT_MS = 1_500;

async function getJson(
  url: string,
  timeoutMs: number,
): Promise<{ status: number; body: unknown } | { status: 0; body: null }> {
  const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
  const text = await res.text();
  let body: unknown = null;
  try {
    body = text.trim() ? JSON.parse(text) : null;
  } catch {
    body = null;
  }
  return { status: res.status, body };
}

/**
 * Asks the hub «are you there?» and «what accounts do you have?». The two questions go
 * together because the useful answer is a single one: can we get in, and with which profile.
 */
export async function probeHub(
  base: string,
  timeoutMs = PROBE_TIMEOUT_MS,
): Promise<HubProbe> {
  const root = base.replace(/\/+$/, "");
  let health: { status: number; body: unknown };
  try {
    health = await getJson(`${root}/api/v1/health`, timeoutMs);
  } catch (e) {
    const name = e instanceof Error ? e.name : "";
    return { ok: false, reason: name === "TimeoutError" ? "timeout" : "unreachable" };
  }
  if (health.status !== 200) return { ok: false, reason: "http", status: health.status };
  const info = (health.body ?? {}) as {
    service?: string;
    version?: string;
    apiVersion?: unknown;
    minClientVersion?: unknown;
  };
  // A wifi portal or another server on the same port answers 200 to everything:
  // without this check the flow would close on an address that is not
  // a hub, and the error would show up ten screens later.
  if (info.service !== "RE-KORD") return { ok: false, reason: "not-hub" };
  // The version notice is decided right here: a client that is too old must
  // know before getting in, not ten screens later.
  compat.apply(info);
  hubModules.apply(info);

  let accounts: { status: number; body: unknown };
  try {
    accounts = await getJson(`${root}/api/v1/accounts`, timeoutMs);
  } catch (e) {
    const name = e instanceof Error ? e.name : "";
    return { ok: false, reason: name === "TimeoutError" ? "timeout" : "unreachable" };
  }
  if (accounts.status !== 200) {
    return { ok: false, reason: "http", status: accounts.status };
  }
  const envelope = (accounts.body ?? {}) as {
    data?: { accounts?: Account[]; defaultAccountId?: string };
  };
  const list = envelope.data?.accounts ?? [];
  if (!list.length) return { ok: false, reason: "no-accounts" };
  return {
    ok: true,
    accounts: list,
    defaultAccountId: envelope.data?.defaultAccountId || list[0].id,
    version: info.version || "",
    apiVersion: typeof info.apiVersion === "number" ? info.apiVersion : null,
    minClientVersion: typeof info.minClientVersion === "string" ? info.minClientVersion : "",
  };
}

/**
 * The local hub is guessed only from the native shell: from a web page served by
 * another origin the attempt would be a cross-origin request (CORS error in the
 * console) to a port that is almost never the right one.
 */
function mayProbeLocalHub(): boolean {
  if (isTauri()) return true;
  if (typeof location === "undefined") return false;
  return !/^https?:$/.test(location.protocol);
}

class ConnectGate {
  phase = $state<ConnectPhase>("probing");
  /** Address to prefill in the fields: the last saved one, if any. */
  savedBase = $state("");
  /** Account the client is bound to now (the screen marks it in the list). */
  currentAccountId = $state<string | null>(getSelectedAccountId());

  /** Last successful startup probe: the session reuses it instead of redoing it. */
  private lastProbe: { base: string; at: number; data: AccountsResponse } | null = null;

  private rememberProbe(base: string, probe: HubProbe) {
    if (!probe.ok) return;
    this.lastProbe = {
      base,
      at: Date.now(),
      data: { accounts: probe.accounts, defaultAccountId: probe.defaultAccountId },
    };
  }

  /**
   * «accounts» answer of the startup probe, if it is recent and concerns the hub in
   * use: the session startup does not redo the same two requests. Only once.
   */
  takeRecentProbe(maxAgeMs: number): AccountsResponse | null {
    const p = this.lastProbe;
    this.lastProbe = null;
    if (!p || Date.now() - p.at > maxAgeMs) return null;
    const base = (getServerBaseUrl() || (typeof location === "undefined" ? "" : location.origin))
      .replace(/\/+$/, "");
    if (p.base.replace(/\/+$/, "") !== base) return null;
    return p.data;
  }

  /**
   * At startup: with an address already saved we just go in — if the hub does not
   * answer, the session's reconnect loop takes care of it, with the
   * UI up.
   *
   * Without an address we first try the page's origin, which in the browser is
   * the hub itself, and then the local hub. Only if nobody answers do we ask
   * where it is: the first-launch flow must not appear to someone who opened
   * the UI served by the hub.
   */
  async decideOnStart(): Promise<boolean> {
    // The boot splash has no text: wait there for the user's language, so
    // nothing is ever painted in the fallback language first.
    const ready = await this.decide();
    await i18n.ready;
    this.phase = ready ? "app" : "connect";
    return ready;
  }

  private async decide(): Promise<boolean> {
    this.savedBase = getServerBaseUrl();
    if (this.savedBase) {
      // Go in right away; compatibility with the hub is checked in the background.
      void checkHubCompat(this.savedBase);
      return true;
    }
    const origin = typeof location === "undefined" ? "" : location.origin;
    if (origin.startsWith("http")) {
      const probe = await probeHub(origin, STARTUP_PROBE_TIMEOUT_MS);
      if (probe.ok) {
        this.rememberProbe(origin, probe);
        return true;
      }
    }
    if (!mayProbeLocalHub()) return false;
    const local = await probeHub(LOCAL_HUB, LOCAL_PROBE_TIMEOUT_MS);
    if (local.ok) {
      this.rememberProbe(LOCAL_HUB, local);
      // It must be saved: in the native shell the origin is the app, and without a base the
      // calls would end up on tauri://localhost.
      setServerBaseUrl(LOCAL_HUB);
      this.savedBase = LOCAL_HUB;
      return true;
    }
    return false;
  }

  /** Reopens the flow: from settings, or when the hub has changed address. */
  open() {
    this.savedBase = getServerBaseUrl();
    this.currentAccountId = getSelectedAccountId();
    this.phase = "connect";
  }

  close() {
    this.phase = "app";
  }
}

export const connectGate = new ConnectGate();
