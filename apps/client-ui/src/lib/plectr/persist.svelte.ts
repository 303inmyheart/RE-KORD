/**
 * Where Plectr records live.
 *
 * Per account, synced with the hub user-state as `settings.plectr` through the
 * session's synced-settings API when it exists (`session.syncedSetting` /
 * `session.setSyncedSetting`); otherwise — and always as an offline mirror —
 * in per-account localStorage. Both copies are merged with best-of semantics
 * (`mergePlectrStores`), so a run saved before the account state arrived, or
 * on a client without the synced API, is never lost.
 *
 * One-shot imports, per account and device: the legacy client's difficulty
 * (`rekord-plectr-difficulty`) and legacy `plectrBests` if the hub still
 * returns them in the user state.
 */
import { getSelectedAccountId } from "../account";
import { api } from "../api";
import { session } from "../session.svelte";
import { readLegacyPlectrPlayMode } from "./difficulty";
import {
  applyRunToStore,
  countPlectrTracksPlayed,
  emptyPlectrStore,
  mergePlectrBests,
  mergePlectrStores,
  normalizePlectrSettings,
  normalizePlectrStore,
  resetPlectrStore,
  type PlectrSettings,
  type PlectrStore,
  type RunApplyOptions,
} from "./records";
import type { DifficultyId, GameResult } from "./types";

export const PLECTR_SETTING_KEY = "plectr";
const LOCAL_PREFIX = "rekord.next.plectr.";
const META_PREFIX = "rekord.next.plectr.meta.";

type SyncedSettingsApi = {
  syncedSetting: (key: string) => unknown;
  setSyncedSetting: (key: string, value: unknown) => void;
};

type LocalMeta = { legacyImported?: boolean };

function syncedApi(): SyncedSettingsApi | null {
  const s = session as unknown as Partial<SyncedSettingsApi>;
  if (typeof s.syncedSetting === "function" && typeof s.setSyncedSetting === "function") {
    return s as SyncedSettingsApi;
  }
  return null;
}

function accountKey(): string {
  return (getSelectedAccountId() || "").trim() || "default";
}

function readJson(key: string): unknown {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* quota / private mode: the synced copy still has it */
  }
}

function readLocal(): PlectrStore {
  return normalizePlectrStore(readJson(LOCAL_PREFIX + accountKey()));
}

function readSyncedRaw(): unknown {
  return syncedApi()?.syncedSetting(PLECTR_SETTING_KEY) ?? null;
}

function readSynced(): PlectrStore | null {
  const raw = readSyncedRaw();
  return raw == null ? null : normalizePlectrStore(raw);
}

function lowEndOf(settings: PlectrSettings): boolean | null {
  return settings.lightStage === "on" ? true : settings.lightStage === "off" ? false : null;
}

class PlectrPersistence {
  /** Bumped on local writes / account switches: views re-read `store`. */
  revision = $state(0);
  private importing = new Set<string>();
  /** Parsed store, reused until the revision, account or synced value changes. */
  private cache: { rev: number; account: string; synced: unknown; store: PlectrStore } | null = null;

  constructor() {
    // Pull / 409 rebase: combine this device's records with the hub's instead
    // of letting one copy replace the other. Hub first, as in `store`.
    session.registerSyncedMerge(PLECTR_SETTING_KEY, (local, remote) => {
      const mine = normalizePlectrStore(local);
      if (remote == null) return mine;
      return mergePlectrStores(normalizePlectrStore(remote), mine);
    });
    if (typeof window !== "undefined") {
      window.addEventListener("rekord-account-session-changed", () => {
        this.revision += 1;
      });
      window.addEventListener("storage", (event) => {
        if (event.key?.startsWith(LOCAL_PREFIX)) this.revision += 1;
      });
    }
  }

  /** Current records for the active account (reactive). */
  get store(): PlectrStore {
    const rev = this.revision;
    const account = accountKey();
    const syncedRaw = readSyncedRaw();
    const c = this.cache;
    if (c && c.rev === rev && c.account === account && c.synced === syncedRaw) return c.store;
    const local = readLocal();
    const synced = syncedRaw == null ? null : normalizePlectrStore(syncedRaw);
    const store = synced ? mergePlectrStores(synced, local) : local;
    this.cache = { rev, account, synced: syncedRaw, store };
    return store;
  }

  get settings(): PlectrSettings {
    return this.store.settings;
  }

  get difficulty(): DifficultyId {
    return this.store.difficulty;
  }

  get tracksPlayed(): number {
    return countPlectrTracksPlayed(this.store.bests);
  }

  private write(next: PlectrStore) {
    writeJson(LOCAL_PREFIX + accountKey(), next);
    syncedApi()?.setSyncedSetting(PLECTR_SETTING_KEY, next);
    this.revision += 1;
  }

  update(fn: (store: PlectrStore) => PlectrStore): PlectrStore {
    const next = fn(this.store);
    this.write(next);
    return next;
  }

  setDifficulty(id: DifficultyId) {
    if (this.store.difficulty === id) return;
    this.update((s) => ({ ...s, difficulty: id }));
  }

  setLowEnd(on: boolean) {
    this.setSettings({ lightStage: on ? "on" : "off" });
  }

  setSettings(patch: Partial<PlectrSettings>) {
    this.update((s) => {
      const settings = normalizePlectrSettings({ ...s.settings, ...patch });
      return { ...s, settings, lowEnd: lowEndOf(settings) };
    });
  }

  /**
   * Saves a run; returns whether it set a new record for its difficulty.
   * Runs that do not count (`opts.eligible === false`) change nothing.
   */
  recordRun(
    relPath: string,
    result: GameResult,
    difficulty: DifficultyId,
    opts: RunApplyOptions = {},
  ): { newRecord: boolean; previous: GameResult | null } {
    const current = this.store;
    const out = applyRunToStore(current, relPath, result, difficulty, new Date(), opts);
    if (out.store !== current) this.write(out.store);
    return { newRecord: out.newRecord, previous: out.previous };
  }

  resetRecords() {
    this.update((s) => resetPlectrStore(s));
  }

  /**
   * Merges legacy records (legacy `plectrBests` map, e.g. from a legacy
   * user-state.json import) into the account's store.
   */
  importLegacyBests(bests: Record<string, unknown> | null | undefined): number {
    if (!bests || typeof bests !== "object") return 0;
    const before = countPlectrTracksPlayed(this.store.bests);
    const next = this.update((s) => ({ ...s, bests: mergePlectrBests(s.bests, bests) }));
    return countPlectrTracksPlayed(next.bests) - before;
  }

  /**
   * Pushes this device's copy into the synced setting when it holds records
   * the account does not have yet, and runs the one-shot legacy imports.
   * Call from views on mount; cheap after the first time.
   */
  async ensureReady(): Promise<void> {
    const account = accountKey();
    const synced = readSynced();
    const local = readLocal();
    if (synced) {
      const merged = mergePlectrStores(synced, local);
      if (JSON.stringify(merged) !== JSON.stringify(synced)) {
        syncedApi()?.setSyncedSetting(PLECTR_SETTING_KEY, merged);
      }
    }

    const metaKey = META_PREFIX + account;
    const meta = (readJson(metaKey) ?? {}) as LocalMeta;
    if (meta.legacyImported || this.importing.has(account)) return;
    this.importing.add(account);
    try {
      const legacyDifficulty = readLegacyPlectrPlayMode();
      if (legacyDifficulty && this.store.difficulty === emptyPlectrStore().difficulty) {
        this.update((s) => ({ ...s, difficulty: legacyDifficulty }));
      }
      try {
        const remote = (await api.getUserState()) as unknown as Record<string, unknown>;
        const settings = (remote?.settings ?? {}) as Record<string, unknown>;
        const legacyBests = (remote?.plectrBests ?? settings.plectrBests) as
          | Record<string, unknown>
          | undefined;
        if (accountKey() === account && legacyBests) this.importLegacyBests(legacyBests);
      } catch {
        // Hub offline: try again next time.
        return;
      }
      writeJson(metaKey, { ...meta, legacyImported: true } satisfies LocalMeta);
    } finally {
      this.importing.delete(account);
    }
  }
}

export const plectrRecords = new PlectrPersistence();
