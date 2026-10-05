/** Local multi-account session (parity with old `rekord-session-account-id`). */

const SESSION_KEY = "rekord.next.sessionAccountId";
const LEGACY_KEYS = [
  "rekord-session-account-id",
  "rekord-active-account-id",
  "kord-session-account-id",
  "kord-active-account-id",
];

export type Account = { id: string; name: string };

export type AccountsResponse = {
  defaultAccountId: string;
  accounts: Account[];
  createdAccountId?: string;
};

/**
 * In-memory copy of the binding: every URL and every request asks for it, and
 * localStorage reads are not free. `undefined` = not read yet. Kept in step by
 * the setters below and by the `storage` event (another tab rebinds).
 */
let cachedId: string | null | undefined;

function readStoredAccountId(): string | null {
  try {
    const cur = localStorage.getItem(SESSION_KEY);
    if (cur) return cur;
    for (const k of LEGACY_KEYS) {
      const v = localStorage.getItem(k);
      if (v) {
        // Legacy binding (same origin as the old client): adopt the id only.
        // Its prefs are never copied from another account.
        localStorage.setItem(SESSION_KEY, v);
        return v;
      }
    }
    return null;
  } catch {
    return null;
  }
}

export function getSelectedAccountId(): string | null {
  if (cachedId === undefined) cachedId = readStoredAccountId();
  return cachedId;
}

export function setSelectedAccountId(id: string) {
  const next = id.trim();
  if (!next) return;
  cachedId = next;
  try {
    localStorage.setItem(SESSION_KEY, next);
    for (const k of LEGACY_KEYS) localStorage.removeItem(k);
  } catch {
    /* private mode: the in-memory binding still holds for this page */
  }
  try {
    window.dispatchEvent(
      new CustomEvent("rekord-account-session-changed", {
        detail: { accountId: next },
      }),
    );
  } catch {
    /* no window (tests) */
  }
}

/**
 * Forget the bound account (and the legacy keys that would resurrect it):
 * the hub said it does not exist. `rememberAvailableAccount` picks the next.
 */
export function clearSelectedAccountId() {
  cachedId = null;
  try {
    localStorage.removeItem(SESSION_KEY);
    for (const k of LEGACY_KEYS) localStorage.removeItem(k);
  } catch {
    /* ignore */
  }
}

/**
 * The binding changed in another tab (`storage` event): refresh the copy.
 * Called by the tab watcher before it tells the session.
 */
export function reloadSelectedAccountId(): string | null {
  cachedId = readStoredAccountId();
  return cachedId;
}

export function rememberAvailableAccount(data: AccountsResponse) {
  const current = getSelectedAccountId();
  if (current && data.accounts.some((a) => a.id === current)) return;
  const fallback = data.defaultAccountId || data.accounts[0]?.id;
  if (fallback) setSelectedAccountId(fallback);
}

/**
 * `?accountId=<id>` deep link (legacy `src/main.tsx`): bind that account
 * before anything is loaded, then drop the parameter from the address bar so a
 * reload or a shared link does not keep forcing it. Returns the id, if any.
 * The hub validates it: an unknown id falls back like a stale binding.
 */
export function applyAccountDeepLink(): string | null {
  if (typeof location === "undefined") return null;
  let id = "";
  try {
    const url = new URL(location.href);
    id = (url.searchParams.get("accountId") || "").trim();
    if (!id) return null;
    url.searchParams.delete("accountId");
    const rest = url.searchParams.toString();
    history.replaceState(history.state, "", `${url.pathname}${rest ? `?${rest}` : ""}${url.hash}`);
  } catch {
    return null;
  }
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(id)) return null;
  setSelectedAccountId(id);
  return id;
}

if (typeof window !== "undefined" && typeof window.addEventListener === "function") {
  window.addEventListener("storage", (event: StorageEvent) => {
    if (event.key == null || event.key === SESSION_KEY) reloadSelectedAccountId();
  });
}

export function accountHeaders(base: HeadersInit = {}): HeadersInit {
  const id = getSelectedAccountId();
  if (!id) return base;
  if (base instanceof Headers) {
    if (!base.has("X-KORD-Account-Id")) base.set("X-KORD-Account-Id", id);
    return base;
  }
  if (Array.isArray(base)) {
    return [...base, ["X-KORD-Account-Id", id]];
  }
  return { ...base, "X-KORD-Account-Id": id };
}

/** Append accountId query when missing (compat with old clients / proxies). */
export function withAccountQuery(path: string): string {
  const id = getSelectedAccountId();
  if (!id) return path;
  const qIndex = path.indexOf("?");
  const base = qIndex >= 0 ? path.slice(0, qIndex) : path;
  const params = new URLSearchParams(qIndex >= 0 ? path.slice(qIndex + 1) : "");
  if (!params.has("accountId") && !base.includes("/accounts")) {
    params.set("accountId", id);
  }
  const q = params.toString();
  return q ? `${base}?${q}` : base;
}
