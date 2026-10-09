/**
 * Plectr records: per-track bests (overall and per difficulty), run
 * counters, settings and the stored blob shape (port of legacy
 * `plectrStorage.ts` + `sessionScores.ts`, extended). Pure: no storage
 * access here, see `persist.svelte.ts` for where the blob lives.
 *
 * Store v2: `byDifficulty` holds one best per difficulty; `bests` keeps the
 * overall best per track (what older clients, Statistics and Achievements
 * read). v1 stores migrate on read: a record tagged with its difficulty seeds
 * that slot, untagged legacy records stay overall-only.
 */
import { DIFFICULTY_IDS, KEY_PRESETS, LATENCY_LIMIT_MS, NOTE_SPEED_MAX, NOTE_SPEED_MIN } from "./config";
import { migratePlectrPlayMode } from "./difficulty";
import { buildGameResult } from "./runResult";
import type { DifficultyId, GameResult, PlectrBestScore } from "./types";

export type LightStageMode = "auto" | "on" | "off";
/**
 * What sits behind the lanes: the Listen visualizer (`bars`, off on the light
 * stage) or nothing. 5.0 also had `art` (the album cover): it migrates to the
 * default on read.
 */
export type StageBackdrop = "off" | "bars";

export type PlectrSettings = {
  /** Note speed multiplier (lead time = 1.6 s / speed). */
  speed: number;
  /** Audio/input latency calibration, ms (+ = heard late). */
  latencyMs: number;
  /** Lighter stage: auto = on for slow engines / measured < 50 fps. */
  lightStage: LightStageMode;
  backdrop: StageBackdrop;
  keyLetters: boolean;
  vibration: boolean;
  /** One KeyboardEvent.key (lower case) per lane. */
  keys: string[];
  /** "Sfida": the run fails below 30% accuracy. */
  challenge: boolean;
};

export type DifficultyBests = Partial<Record<DifficultyId, PlectrBestScore>>;

/** Stored under the synced account setting `"plectr"`. */
export type PlectrStore = {
  version: 2;
  difficulty: DifficultyId;
  /** Best result per track (any difficulty), keyed by rel_path (legacy `plectrBests`). */
  bests: Record<string, PlectrBestScore>;
  /** Best result per track and difficulty. */
  byDifficulty: Record<string, DifficultyBests>;
  /** Runs that counted (finished, or ≥ 60% of the notes judged). */
  runs: number;
  /** Runs played from the top to the end with nothing skipped. */
  fullRuns: number;
  /** Notes hit across every counted run. */
  notesHit: number;
  /** ISO time of the last counted run. */
  lastRunAt: string | null;
  /** v1 "light stage" flag, kept in sync with `settings.lightStage` for older clients. */
  lowEnd: boolean | null;
  settings: PlectrSettings;
  /** Records reset: anything older than this is dropped when stores merge. */
  resetAt: string | null;
};

export function defaultPlectrSettings(): PlectrSettings {
  return {
    speed: 1,
    latencyMs: 0,
    lightStage: "auto",
    backdrop: "bars",
    keyLetters: true,
    vibration: true,
    keys: [...KEY_PRESETS.dfjk],
    challenge: false,
  };
}

export function emptyPlectrStore(): PlectrStore {
  return {
    version: 2,
    difficulty: "easy",
    bests: {},
    byDifficulty: {},
    runs: 0,
    fullRuns: 0,
    notesHit: 0,
    lastRunAt: null,
    lowEnd: null,
    settings: defaultPlectrSettings(),
    resetAt: null,
  };
}

/** Loose-track folder was renamed Tracce → Tracks: records follow both names. */
export function relPathAliases(relPath: string): string[] {
  const migrated = relPath.replace("/Tracce/", "/Tracks/");
  const legacy = relPath.replace("/Tracks/", "/Tracce/");
  return [...new Set([relPath, migrated, legacy].filter(Boolean))];
}

export function lookupByRelPathAliases<T>(
  record: Record<string, T> | undefined,
  relPath: string,
): T | undefined {
  if (!record) return undefined;
  for (const alias of relPathAliases(relPath)) {
    if (Object.prototype.hasOwnProperty.call(record, alias)) return record[alias];
  }
  return undefined;
}

export function hasPlectrPlayRecord(
  raw: PlectrBestScore | null | undefined,
): raw is PlectrBestScore {
  if (!raw || typeof raw.score !== "number" || !Number.isFinite(raw.score)) return false;
  return raw.score > 0 || (raw.hits ?? 0) > 0;
}

export function plectrBestToResult(raw: PlectrBestScore | null | undefined): GameResult | null {
  if (!hasPlectrPlayRecord(raw)) return null;
  return buildGameResult({
    score: Math.max(0, Math.round(raw.score)),
    maxCombo: raw.maxCombo ?? 0,
    hits: raw.hits ?? 0,
    misses: raw.misses ?? 0,
  });
}

export function plectrBestFromBests(
  bests: Record<string, PlectrBestScore> | undefined,
  relPath: string,
): GameResult | null {
  if (!bests) return null;
  return plectrBestToResult(lookupByRelPathAliases(bests, relPath));
}

export function gameResultToPlectrBest(
  result: GameResult,
  difficulty?: DifficultyId,
  at: Date = new Date(),
  flags?: { fc?: boolean; ap?: boolean },
): PlectrBestScore {
  const best: PlectrBestScore = {
    score: result.score,
    grade: result.grade,
    accuracy: result.accuracy,
    maxCombo: result.maxCombo,
    hits: result.hits,
    misses: result.misses,
    updatedAt: at.toISOString(),
  };
  if (difficulty) best.difficulty = difficulty;
  if (flags?.fc) best.fc = true;
  if (flags?.ap) best.ap = true;
  return best;
}

export function isBetterPlectrScore(next: GameResult, current: GameResult | null): boolean {
  if (!current) return next.score > 0 || next.hits > 0;
  if (next.score !== current.score) return next.score > current.score;
  return next.accuracy > current.accuracy;
}

export function pickBetterPlectrScore(a: GameResult | null, b: GameResult | null): GameResult | null {
  if (!a) return b;
  if (!b) return a;
  return isBetterPlectrScore(a, b) ? a : b;
}

/** Distinct tracks with at least one saved Plectr record. */
export function countPlectrTracksPlayed(
  bests: Record<string, PlectrBestScore> | undefined,
): number {
  if (!bests) return 0;
  let n = 0;
  for (const best of Object.values(bests)) {
    if (hasPlectrPlayRecord(best)) n += 1;
  }
  return n;
}

/** Better of two raw records; FC / AP flags stick to the slot either way. */
function pickBetterRaw(a: PlectrBestScore, b: PlectrBestScore): PlectrBestScore {
  const better =
    a.score !== b.score ? (a.score > b.score ? a : b) : (a.accuracy ?? 0) >= (b.accuracy ?? 0) ? a : b;
  const fc = !!(a.fc || b.fc);
  const ap = !!(a.ap || b.ap);
  if (fc === !!better.fc && ap === !!better.ap) return better;
  const out = { ...better };
  if (fc) out.fc = true;
  if (ap) out.ap = true;
  return out;
}

function isDifficulty(raw: unknown): raw is DifficultyId {
  return typeof raw === "string" && (DIFFICULTY_IDS as readonly string[]).includes(raw);
}

function normalizeBest(raw: unknown): PlectrBestScore | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);
  if (typeof r.score !== "number" || !Number.isFinite(r.score)) return null;
  const best: PlectrBestScore = {
    score: Math.max(0, Math.round(r.score)),
    grade: typeof r.grade === "string" ? r.grade : "",
    accuracy: Math.min(1, Math.max(0, num(r.accuracy))),
    maxCombo: Math.max(0, Math.round(num(r.maxCombo))),
    hits: Math.max(0, Math.round(num(r.hits))),
    misses: Math.max(0, Math.round(num(r.misses))),
  };
  if (typeof r.updatedAt === "string") best.updatedAt = r.updatedAt;
  if (isDifficulty(r.difficulty)) best.difficulty = r.difficulty;
  if (r.fc === true) best.fc = true;
  if (r.ap === true) best.ap = true;
  return hasPlectrPlayRecord(best) ? best : null;
}

/** Union of two best maps, keeping the better entry per track. */
export function mergePlectrBests(
  a: Record<string, PlectrBestScore> | undefined,
  b: Record<string, unknown> | undefined,
): Record<string, PlectrBestScore> {
  const out: Record<string, PlectrBestScore> = { ...(a ?? {}) };
  for (const [relPath, raw] of Object.entries(b ?? {})) {
    const next = normalizeBest(raw);
    if (!next || !relPath) continue;
    const cur = out[relPath];
    out[relPath] = cur ? pickBetterRaw(cur, next) : next;
  }
  return out;
}

/** Union of two per-difficulty maps, best entry per track × difficulty. */
export function mergeDifficultyBests(
  a: Record<string, DifficultyBests> | undefined,
  b: Record<string, unknown> | undefined,
): Record<string, DifficultyBests> {
  const out: Record<string, DifficultyBests> = {};
  for (const [relPath, slots] of Object.entries(a ?? {})) out[relPath] = { ...slots };
  for (const [relPath, raw] of Object.entries(b ?? {})) {
    if (!relPath || !raw || typeof raw !== "object") continue;
    const slots = raw as Record<string, unknown>;
    for (const id of DIFFICULTY_IDS) {
      const next = normalizeBest(slots[id]);
      if (!next) continue;
      next.difficulty = id;
      const row = (out[relPath] ??= {});
      const cur = row[id];
      row[id] = cur ? pickBetterRaw(cur, next) : next;
    }
  }
  return out;
}

/** v1 → v2: records tagged with a difficulty seed that difficulty's slot. */
export function migrateBestsToDifficulties(
  bests: Record<string, PlectrBestScore>,
  byDifficulty: Record<string, DifficultyBests>,
): Record<string, DifficultyBests> {
  const seed: Record<string, DifficultyBests> = {};
  for (const [relPath, best] of Object.entries(bests)) {
    if (!best.difficulty) continue;
    (seed[relPath] ??= {})[best.difficulty] = best;
  }
  return mergeDifficultyBests(byDifficulty, seed);
}

function normalizeKeys(raw: unknown): string[] {
  const fallback = [...KEY_PRESETS.dfjk];
  if (!Array.isArray(raw) || raw.length !== fallback.length) return fallback;
  const keys = raw.map((k) => (typeof k === "string" ? k.trim().toLowerCase() : ""));
  if (keys.some((k) => !k || k === " " || k === "escape") || new Set(keys).size !== keys.length) {
    return fallback;
  }
  return keys;
}

export function normalizePlectrSettings(raw: unknown, legacyLowEnd: boolean | null = null): PlectrSettings {
  const base = defaultPlectrSettings();
  if (legacyLowEnd != null) base.lightStage = legacyLowEnd ? "on" : "off";
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Record<string, unknown>;
  const num = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? v : d);
  const speed = Math.min(NOTE_SPEED_MAX, Math.max(NOTE_SPEED_MIN, num(r.speed, base.speed)));
  const latency = Math.min(LATENCY_LIMIT_MS, Math.max(-LATENCY_LIMIT_MS, num(r.latencyMs, 0)));
  return {
    speed: Math.round(speed * 20) / 20,
    latencyMs: Math.round(latency),
    lightStage: r.lightStage === "on" || r.lightStage === "off" || r.lightStage === "auto" ? r.lightStage : base.lightStage,
    // 5.0 "art" (cover) and anything unknown → the default.
    backdrop: r.backdrop === "off" || r.backdrop === "bars" ? r.backdrop : base.backdrop,
    keyLetters: typeof r.keyLetters === "boolean" ? r.keyLetters : base.keyLetters,
    vibration: typeof r.vibration === "boolean" ? r.vibration : base.vibration,
    keys: normalizeKeys(r.keys),
    challenge: r.challenge === true,
  };
}

/** Accepts anything (synced JSON, old local blob, v1) and returns a valid v2 store. */
export function normalizePlectrStore(raw: unknown): PlectrStore {
  const base = emptyPlectrStore();
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Record<string, unknown>;
  const bests =
    r.bests && typeof r.bests === "object"
      ? mergePlectrBests({}, r.bests as Record<string, unknown>)
      : {};
  const byDifficulty = migrateBestsToDifficulties(
    bests,
    r.byDifficulty && typeof r.byDifficulty === "object"
      ? mergeDifficultyBests({}, r.byDifficulty as Record<string, unknown>)
      : {},
  );
  const count = (v: unknown) =>
    typeof v === "number" && Number.isFinite(v) && v > 0 ? Math.floor(v) : 0;
  const lowEnd = typeof r.lowEnd === "boolean" ? r.lowEnd : null;
  return {
    version: 2,
    difficulty: migratePlectrPlayMode(r.difficulty),
    bests,
    byDifficulty,
    runs: count(r.runs),
    fullRuns: count(r.fullRuns),
    notesHit: count(r.notesHit),
    lastRunAt: typeof r.lastRunAt === "string" ? r.lastRunAt : null,
    lowEnd,
    // v1 had only the light-stage flag: it seeds the setting.
    settings: normalizePlectrSettings(r.settings, r.settings ? null : lowEnd),
    resetAt: typeof r.resetAt === "string" ? r.resetAt : null,
  };
}

function latestIso(...values: (string | null | undefined)[]): string | null {
  return values.filter((v): v is string => !!v).sort().pop() ?? null;
}

/**
 * Merge two stores (the synced one and this device's copy). Records are
 * monotonic — the better entry per track wins — so a run saved offline or
 * before the account state arrived is never lost. A reset on either side
 * (`resetAt`) drops whatever is older than it. Settings and difficulty come
 * from `primary`.
 */
export function mergePlectrStores(primary: PlectrStore, other: PlectrStore): PlectrStore {
  const resetAt = latestIso(primary.resetAt, other.resetAt);
  const fresh = (s: PlectrStore) => !resetAt || s.resetAt === resetAt;
  const keep = (best: PlectrBestScore) =>
    !resetAt || (typeof best.updatedAt === "string" && best.updatedAt > resetAt);
  const bests: Record<string, PlectrBestScore> = {};
  for (const [relPath, best] of Object.entries(mergePlectrBests(primary.bests, other.bests))) {
    if (keep(best)) bests[relPath] = best;
  }
  const byDifficulty: Record<string, DifficultyBests> = {};
  for (const [relPath, slots] of Object.entries(
    mergeDifficultyBests(primary.byDifficulty, other.byDifficulty),
  )) {
    const row: DifficultyBests = {};
    for (const id of DIFFICULTY_IDS) {
      const best = slots[id];
      if (best && keep(best)) row[id] = best;
    }
    if (Object.keys(row).length) byDifficulty[relPath] = row;
  }
  const counted = [primary, other].filter(fresh);
  return {
    ...primary,
    bests,
    byDifficulty,
    runs: Math.max(0, ...counted.map((s) => s.runs)),
    fullRuns: Math.max(0, ...counted.map((s) => s.fullRuns)),
    notesHit: Math.max(0, ...counted.map((s) => s.notesHit)),
    lastRunAt: latestIso(primary.lastRunAt, other.lastRunAt),
    lowEnd: primary.lowEnd ?? other.lowEnd,
    resetAt,
  };
}

/** Clears records and counters (difficulty and settings stay). */
export function resetPlectrStore(store: PlectrStore, at: Date = new Date()): PlectrStore {
  return {
    ...store,
    bests: {},
    byDifficulty: {},
    runs: 0,
    fullRuns: 0,
    notesHit: 0,
    lastRunAt: null,
    resetAt: at.toISOString(),
  };
}

/** The best for a track on one difficulty (alias aware). */
export function difficultyBest(
  store: Pick<PlectrStore, "byDifficulty">,
  relPath: string,
  difficulty: DifficultyId,
): PlectrBestScore | null {
  const best = lookupByRelPathAliases(store.byDifficulty, relPath)?.[difficulty];
  return hasPlectrPlayRecord(best) ? best : null;
}

export type RunApplyOptions = {
  /** Counted run (see `isRecordEligible`); default true. */
  eligible?: boolean;
  /** From the top to the end with nothing skipped. */
  fullRun?: boolean;
  /** Full combo / all perfect on a full chart. */
  fc?: boolean;
  ap?: boolean;
};

/**
 * Applies a run: when it counts, bumps counters and replaces the track
 * best for its difficulty (and the overall best) when the run beats them.
 * Returns the new store, whether the run set a new record for its
 * difficulty and the previous best there. Runs that do not count (or have
 * nothing judged) change nothing.
 */
export function applyRunToStore(
  store: PlectrStore,
  relPath: string,
  result: GameResult,
  difficulty: DifficultyId,
  at: Date = new Date(),
  opts: RunApplyOptions = {},
): { store: PlectrStore; newRecord: boolean; previous: GameResult | null } {
  const prevRaw = difficultyBest(store, relPath, difficulty);
  const previous = plectrBestToResult(prevRaw);
  const eligible = opts.eligible ?? true;
  if (!eligible || (result.score <= 0 && result.hits + result.misses <= 0)) {
    return { store, newRecord: false, previous };
  }
  const flags = { fc: !!opts.fc, ap: !!opts.ap };
  const newRecord = isBetterPlectrScore(result, previous);
  const nextRaw = gameResultToPlectrBest(result, difficulty, at, flags);

  const byDifficulty = { ...store.byDifficulty };
  const row: DifficultyBests = { ...(lookupByRelPathAliases(store.byDifficulty, relPath) ?? {}) };
  for (const alias of relPathAliases(relPath)) {
    if (alias !== relPath) delete byDifficulty[alias];
  }
  row[difficulty] = prevRaw ? pickBetterRaw(prevRaw, nextRaw) : nextRaw;
  byDifficulty[relPath] = row;

  const bests = { ...store.bests };
  const overallRaw = lookupByRelPathAliases(store.bests, relPath);
  if (isBetterPlectrScore(result, plectrBestToResult(overallRaw)) || (overallRaw && (flags.fc || flags.ap))) {
    for (const alias of relPathAliases(relPath)) {
      if (alias !== relPath) delete bests[alias];
    }
    bests[relPath] = overallRaw && hasPlectrPlayRecord(overallRaw) ? pickBetterRaw(overallRaw, nextRaw) : nextRaw;
  }
  return {
    store: {
      ...store,
      bests,
      byDifficulty,
      runs: store.runs + 1,
      fullRuns: store.fullRuns + (opts.fullRun ? 1 : 0),
      notesHit: store.notesHit + Math.max(0, result.hits),
      lastRunAt: at.toISOString(),
    },
    newRecord,
    previous,
  };
}

/* ── Read-only selectors (Statistics, records view, career card) ─────── */

export type PlectrTrackRecord = {
  relPath: string;
  /** Overall best (any difficulty, legacy records included). */
  best: PlectrBestScore | null;
  byDifficulty: DifficultyBests;
  /** Latest update across the track's records. */
  updatedAt: string | null;
  fc: boolean;
  ap: boolean;
};

export const GRADES = ["S", "A", "B", "C", "D"] as const;

export type PlectrCareer = {
  tracksPlayed: number;
  runs: number;
  fullRuns: number;
  notesHit: number;
  /** Best grade per track × difficulty (legacy untagged records count once). */
  grades: Record<(typeof GRADES)[number], number>;
  fullCombos: number;
  allPerfects: number;
  /** Plectr XP: notes, full runs, grades, FC/AP. */
  xp: number;
  lastRunAt: string | null;
};

function trackRecordOf(
  relPath: string,
  best: PlectrBestScore | undefined,
  slots: DifficultyBests | undefined,
): PlectrTrackRecord | null {
  const byDifficulty: DifficultyBests = {};
  for (const id of DIFFICULTY_IDS) {
    const b = slots?.[id];
    if (hasPlectrPlayRecord(b)) byDifficulty[id] = b;
  }
  const overall = hasPlectrPlayRecord(best) ? best : null;
  const all = [overall, ...Object.values(byDifficulty)].filter((b): b is PlectrBestScore => !!b);
  if (!all.length) return null;
  return {
    relPath,
    best: overall ?? all.reduce((a, b) => pickBetterRaw(a, b)),
    byDifficulty,
    updatedAt: latestIso(...all.map((b) => b.updatedAt)),
    fc: all.some((b) => b.fc),
    ap: all.some((b) => b.ap),
  };
}

/** One row per track with any record, newest first. */
export function selectPlectrTrackRecords(
  store: Pick<PlectrStore, "bests" | "byDifficulty">,
): PlectrTrackRecord[] {
  const paths = new Set([...Object.keys(store.bests), ...Object.keys(store.byDifficulty)]);
  const rows: PlectrTrackRecord[] = [];
  for (const relPath of paths) {
    const row = trackRecordOf(relPath, store.bests[relPath], store.byDifficulty[relPath]);
    if (row) rows.push(row);
  }
  rows.sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""));
  return rows;
}

/** Records of one track (alias aware), null when never played. */
export function selectPlectrTrackRecord(
  store: Pick<PlectrStore, "bests" | "byDifficulty">,
  relPath: string,
): PlectrTrackRecord | null {
  return trackRecordOf(
    relPath,
    lookupByRelPathAliases(store.bests, relPath),
    lookupByRelPathAliases(store.byDifficulty, relPath),
  );
}

const GRADE_XP: Record<string, number> = { S: 40, A: 25, B: 15, C: 8, D: 3 };

export function selectPlectrCareer(store: PlectrStore): PlectrCareer {
  const grades = { S: 0, A: 0, B: 0, C: 0, D: 0 };
  let fullCombos = 0;
  let allPerfects = 0;
  const rows = selectPlectrTrackRecords(store);
  for (const row of rows) {
    const slots = Object.values(row.byDifficulty);
    const counted = slots.length ? slots : row.best ? [row.best] : [];
    for (const b of counted) {
      if (b.grade in grades) grades[b.grade as keyof typeof grades] += 1;
      if (b.fc) fullCombos += 1;
      if (b.ap) allPerfects += 1;
    }
  }
  let xp = Math.floor(store.notesHit / 25) + store.fullRuns * 10 + fullCombos * 20 + allPerfects * 50;
  for (const [g, n] of Object.entries(grades)) xp += (GRADE_XP[g] ?? 0) * n;
  return {
    tracksPlayed: rows.length,
    runs: store.runs,
    fullRuns: store.fullRuns,
    notesHit: store.notesHit,
    grades,
    fullCombos,
    allPerfects,
    xp,
    lastRunAt: store.lastRunAt,
  };
}

/** Best result per track for the current page session (legacy `sessionScores`). */
export class SessionBests {
  private byTrack = new Map<string, GameResult>();

  get(relPath: string): GameResult | null {
    return this.byTrack.get(relPath) ?? null;
  }

  save(relPath: string, result: GameResult): GameResult {
    const next = pickBetterPlectrScore(result, this.get(relPath)) ?? result;
    this.byTrack.set(relPath, next);
    return next;
  }

  clear(): void {
    this.byTrack.clear();
  }
}
