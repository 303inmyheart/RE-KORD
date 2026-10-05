/**
 * Three-way merge for the per-account user state.
 *
 * `base` is the last state this client and the hub agreed on (pull or
 * successful push), `local` what this device has now, `server` what the hub
 * holds after someone else wrote in between (the 409 answer). The result is
 * what this device should push next:
 *
 * - play counts are additive: plays made here since `base` land on top of the
 *   server's numbers, so two devices listening at once never lose a play;
 * - per-track moods merge key by key, the side that changed a key wins;
 * - exclusion lists merge as sets (local adds/removes replayed on the server);
 * - recent history keeps this device's new plays first, then the server's.
 *
 * Settings are not here: the hub merges `settings` key by key on PATCH, so the
 * client only resends the keys it changed locally.
 */

export type SyncedUserFields = {
  playCounts: Record<string, number>;
  recentRelPaths: string[];
  trackMoods: Record<string, string[]>;
  excludedRelPaths: string[];
  excludedAlbumIds: number[];
};

export const SYNCED_FIELDS = [
  "playCounts",
  "recentRelPaths",
  "trackMoods",
  "excludedRelPaths",
  "excludedAlbumIds",
] as const satisfies readonly (keyof SyncedUserFields)[];

export type SyncedField = (typeof SYNCED_FIELDS)[number];

/** Cap shared with userPrefs (`recentRelPaths` keeps 100). */
export const RECENT_CAP = 100;

/** Key order does not count: the hub hands maps back sorted (`sameJsonValue`). */
function sameJson(a: unknown, b: unknown): boolean {
  return sameJsonValue(a, b);
}

function sameStringList(a: readonly unknown[] | undefined, b: readonly unknown[] | undefined) {
  if (!a || !b) return !a === !b;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

function sameCounts(a: Record<string, number>, b: Record<string, number>): boolean {
  const ak = Object.keys(a);
  if (ak.length !== Object.keys(b).length) return false;
  for (const k of ak) if (a[k] !== b[k]) return false;
  return true;
}

/** Fields that differ from `base` (all of them when nothing was agreed yet). */
export function changedUserFields(
  base: SyncedUserFields | null,
  local: SyncedUserFields,
): SyncedField[] {
  if (!base) return [...SYNCED_FIELDS];
  const out: SyncedField[] = [];
  if (!sameCounts(base.playCounts, local.playCounts)) out.push("playCounts");
  if (!sameStringList(base.recentRelPaths, local.recentRelPaths)) out.push("recentRelPaths");
  if (!sameJson(base.trackMoods, local.trackMoods)) out.push("trackMoods");
  if (!sameStringList(base.excludedRelPaths, local.excludedRelPaths)) {
    out.push("excludedRelPaths");
  }
  if (!sameStringList(base.excludedAlbumIds, local.excludedAlbumIds)) {
    out.push("excludedAlbumIds");
  }
  return out;
}

/**
 * What to rebase on. `inflight` is the state the last push would leave on the
 * hub, journaled before the request. When its answer never came (timeout, a
 * keepalive push from a closing page) it may have landed all the same: if the
 * hub holds exactly that state, that is the agreed one. Rebasing on the older
 * base would add those plays a second time.
 */
export function agreedBase(
  base: SyncedUserFields,
  inflight: SyncedUserFields | null | undefined,
  server: SyncedUserFields,
): SyncedUserFields {
  if (inflight && changedUserFields(inflight, server).length === 0) return inflight;
  return base;
}

export function rebasePlayCounts(
  base: Record<string, number>,
  local: Record<string, number>,
  server: Record<string, number>,
): Record<string, number> {
  const out: Record<string, number> = { ...server };
  for (const [key, value] of Object.entries(local)) {
    const n = Number(value) || 0;
    const delta = n - (Number(base[key]) || 0);
    if (delta > 0) out[key] = (Number(server[key]) || 0) + delta;
  }
  // Keys dropped here (id → rel_path migration, deleted files) go away too,
  // unless another device counted new plays on them meanwhile.
  for (const key of Object.keys(base)) {
    if (key in local) continue;
    if (!(key in out)) continue;
    if ((Number(out[key]) || 0) <= (Number(base[key]) || 0)) delete out[key];
  }
  return out;
}

export function rebaseRecent(
  base: string[],
  local: string[],
  server: string[],
  cap = RECENT_CAP,
): string[] {
  if (sameStringList(base, local)) return server.slice(0, cap);
  if (sameStringList(base, server)) return local.slice(0, cap);
  // Plays started here since `base` are the newest this device knows about.
  const baseSet = new Set(base);
  const fresh = local.filter((p) => !baseSet.has(p) || local.indexOf(p) < base.indexOf(p));
  const out: string[] = [];
  const seen = new Set<string>();
  for (const p of [...fresh, ...server, ...local]) {
    if (seen.has(p)) continue;
    seen.add(p);
    out.push(p);
    if (out.length >= cap) break;
  }
  return out;
}

export function rebaseMoods(
  base: Record<string, string[]>,
  local: Record<string, string[]>,
  server: Record<string, string[]>,
): Record<string, string[]> {
  const out: Record<string, string[]> = { ...server };
  const keys = new Set([...Object.keys(base), ...Object.keys(local)]);
  for (const key of keys) {
    const mine = local[key];
    if (sameJson(mine, base[key])) continue;
    if (mine === undefined) delete out[key];
    else out[key] = [...mine];
  }
  return out;
}

export function rebaseSet<T extends string | number>(base: T[], local: T[], server: T[]): T[] {
  const baseSet = new Set(base);
  const localSet = new Set(local);
  const out = new Set(server);
  for (const v of local) if (!baseSet.has(v)) out.add(v);
  for (const v of base) if (!localSet.has(v)) out.delete(v);
  return [...out];
}

export function rebaseUserState(
  base: SyncedUserFields,
  local: SyncedUserFields,
  server: SyncedUserFields,
): SyncedUserFields {
  return {
    playCounts: rebasePlayCounts(base.playCounts, local.playCounts, server.playCounts),
    recentRelPaths: rebaseRecent(base.recentRelPaths, local.recentRelPaths, server.recentRelPaths),
    trackMoods: rebaseMoods(base.trackMoods, local.trackMoods, server.trackMoods),
    excludedRelPaths: rebaseSet(
      base.excludedRelPaths,
      local.excludedRelPaths,
      server.excludedRelPaths,
    ),
    excludedAlbumIds: rebaseSet(
      base.excludedAlbumIds,
      local.excludedAlbumIds,
      server.excludedAlbumIds,
    ),
  };
}

/** Normalise a hub payload (fields may be missing on older hubs). */
export function syncedFieldsOf(raw: Partial<Record<SyncedField, unknown>> | null | undefined): SyncedUserFields {
  const r = raw ?? {};
  const counts: Record<string, number> = {};
  if (r.playCounts && typeof r.playCounts === "object") {
    for (const [k, v] of Object.entries(r.playCounts as Record<string, unknown>)) {
      const n = Number(v);
      if (Number.isFinite(n)) counts[k] = n;
    }
  }
  const moods: Record<string, string[]> = {};
  if (r.trackMoods && typeof r.trackMoods === "object") {
    for (const [k, v] of Object.entries(r.trackMoods as Record<string, unknown>)) {
      if (Array.isArray(v)) moods[k] = v.filter((x): x is string => typeof x === "string");
    }
  }
  const strings = (v: unknown) =>
    Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  const numbers = (v: unknown) =>
    Array.isArray(v)
      ? v.filter((x): x is number => typeof x === "number" && Number.isFinite(x))
      : [];
  return {
    playCounts: counts,
    recentRelPaths: strings(r.recentRelPaths),
    trackMoods: moods,
    excludedRelPaths: strings(r.excludedRelPaths),
    excludedAlbumIds: numbers(r.excludedAlbumIds),
  };
}

/**
 * Deep equality for JSON values that ignores object key order. The hub keeps
 * `settings` in a sorted map, so a value this client built (`{version, bests…}`)
 * comes back with its keys reordered: comparing `JSON.stringify` outputs would
 * call them different on every pull.
 */
export function sameJsonValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") {
    // `undefined` keys vanish from JSON: treat them as absent, like the hub does.
    return a === b;
  }
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    const bb = b as unknown[];
    if (a.length !== bb.length) return false;
    for (let i = 0; i < a.length; i++) if (!sameJsonValue(a[i], bb[i])) return false;
    return true;
  }
  const ao = a as Record<string, unknown>;
  const bo = b as Record<string, unknown>;
  const ak = Object.keys(ao).filter((k) => ao[k] !== undefined);
  const bk = Object.keys(bo).filter((k) => bo[k] !== undefined);
  if (ak.length !== bk.length) return false;
  for (const k of ak) {
    if (!Object.prototype.hasOwnProperty.call(bo, k)) return false;
    if (!sameJsonValue(ao[k], bo[k])) return false;
  }
  return true;
}

/** Retry schedule for a failed push: 2s, 4s, 8s … capped at one minute. */
export function pushRetryDelay(attempt: number): number {
  const n = Math.max(0, Math.floor(attempt));
  return Math.min(60_000, 2000 * 2 ** Math.min(n, 5));
}

/**
 * Pull the server's copy out of a 409 answer. New hubs send
 * `{ error: "revision_conflict", current: {...} }`; older ones only a message,
 * in which case the caller has to GET the state.
 */
export function conflictState(body: unknown): (Partial<SyncedUserFields> & {
  revision: number;
  settings?: Record<string, unknown>;
}) | null {
  if (!body || typeof body !== "object") return null;
  const env = body as { current?: unknown; data?: { current?: unknown } };
  const cur = (env.current ?? env.data?.current) as { revision?: unknown } | undefined;
  if (!cur || typeof cur !== "object") return null;
  const rev = Number(cur.revision);
  if (!Number.isFinite(rev)) return null;
  return cur as Partial<SyncedUserFields> & {
    revision: number;
    settings?: Record<string, unknown>;
  };
}
