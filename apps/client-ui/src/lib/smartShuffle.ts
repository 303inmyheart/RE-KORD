import type { Track } from "./api";
import { trackGenreKeys } from "./genres";
import { resolveTrackMoods, type TrackMoodId } from "./trackMoods";
import { getTrackMoodsMap } from "./userPrefs";

export const CARD_QUEUE_CAP = 500;

export function fisherYatesShuffle<T>(items: readonly T[]): T[] {
  const a = [...items];
  for (let i = a.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j] as T, a[i] as T];
  }
  return a;
}

/** Keeps [0..currentIdx] and shuffles only the following queue (legacy shuffleTailFromCurrent). */
export function shuffleTailFromCurrent<T>(items: readonly T[], currentIdx: number): T[] {
  if (items.length <= 1) return [...items];
  const i = Math.min(Math.max(0, currentIdx), items.length - 1);
  const prefix = items.slice(0, i + 1);
  const tail = items.slice(i + 1);
  if (tail.length < 2) return [...prefix, ...tail];
  return [...prefix, ...fisherYatesShuffle(tail)];
}

function spreadConsecutiveArtists(tracks: Track[]): void {
  const n = tracks.length;
  if (n < 2) return;
  let guard = 0;
  const maxGuard = n * n;
  while (guard < maxGuard) {
    guard += 1;
    let swapped = false;
    for (let i = 0; i < n - 1; i += 1) {
      if (tracks[i].artist_name !== tracks[i + 1].artist_name) continue;
      let j = i + 2;
      while (j < n && tracks[j].artist_name === tracks[i].artist_name) j += 1;
      if (j >= n) continue;
      [tracks[i + 1], tracks[j]] = [tracks[j], tracks[i + 1]];
      swapped = true;
    }
    if (!swapped) break;
  }
}

type MoodMap = Readonly<Record<string, readonly string[]>>;

function moodsFor(track: Track, moods: MoodMap = getTrackMoodsMap()): TrackMoodId[] {
  return resolveTrackMoods(track.id, track.rel_path, moods as Record<string, string[]>);
}

/** Normalized genre keys: "Hip Hop; Pop Rap" counts as two, "Hip-Hop" = "hip hop". */
function genresFor(track: Track): string[] {
  return trackGenreKeys(track);
}

export type SmartShuffleOpts = {
  currentRelPath?: string;
  currentArtist?: string;
  recentRelPaths?: ReadonlySet<string>;
};

export type ShuffleExclusionOpts = {
  respectExclusions?: boolean;
  isExcluded?: (track: Track) => boolean;
};

/**
 * Blocked rule: excluded tracks do not enter generated queues, unless
 * we explicitly start from a blocked track (then they stay allowed).
 */
export function filterPoolForExclusions(
  pool: readonly Track[],
  seed: Track | null,
  opts?: ShuffleExclusionOpts,
): Track[] {
  if (!opts?.respectExclusions || !opts.isExcluded) return [...pool];
  if (seed && opts.isExcluded(seed)) return [...pool];
  return pool.filter(
    (t) => (seed != null && t.rel_path === seed.rel_path) || !opts.isExcluded!(t),
  );
}

export function seedSimilarityScore(
  seed: Track,
  candidate: Track,
  moods: MoodMap = getTrackMoodsMap(),
): number {
  const seedMoods = moodsFor(seed, moods);
  let moodScore = 0;
  if (seedMoods.length > 0) {
    const set = new Set(seedMoods);
    const overlap = moodsFor(candidate, moods).filter((m) => set.has(m)).length;
    moodScore = overlap / seedMoods.length;
  }
  const seedGenres = genresFor(seed);
  let genreScore = 0;
  if (seedGenres.length > 0) {
    const set = new Set(seedGenres);
    const overlap = genresFor(candidate).filter((g) => set.has(g)).length;
    genreScore = overlap / seedGenres.length;
  }
  const artistBonus =
    seed.artist_name.trim().toLowerCase() === candidate.artist_name.trim().toLowerCase()
      ? 0.05
      : 0;
  return moodScore + genreScore + artistBonus;
}

function sortPoolBySeedSimilarity(
  seed: Track,
  pool: readonly Track[],
  recentRelPaths?: ReadonlySet<string>,
): Track[] {
  // One read of the moods for the whole pool, not one per candidate.
  const moods = getTrackMoodsMap();
  const scored = pool.map((track) => ({
    track,
    score: seedSimilarityScore(seed, track, moods),
    jitter: Math.random(),
  }));
  scored.sort((a, b) => (b.score !== a.score ? b.score - a.score : a.jitter - b.jitter));
  if (recentRelPaths && recentRelPaths.size > 0) {
    const fresh = scored.filter((s) => !recentRelPaths.has(s.track.rel_path));
    const stale = scored.filter((s) => recentRelPaths.has(s.track.rel_path));
    if (fresh.length > 0) return [...fresh, ...stale].map((s) => s.track);
  }
  return scored.map((s) => s.track);
}

export function buildSmartRandomQueue(
  tracks: readonly Track[],
  opts: SmartShuffleOpts = {},
): Track[] {
  if (!tracks.length) return [];
  let a = fisherYatesShuffle(tracks);
  const recent = opts.recentRelPaths;
  if (recent && recent.size > 0) {
    const fresh = a.filter((t) => !recent.has(t.rel_path));
    const stale = a.filter((t) => recent.has(t.rel_path));
    if (fresh.length > 0) a = [...fresh, ...stale];
  }
  spreadConsecutiveArtists(a);
  if (opts.currentRelPath && a[0]?.rel_path === opts.currentRelPath) {
    const k = a.findIndex((t) => t.rel_path !== opts.currentRelPath);
    if (k > 0) [a[0], a[k]] = [a[k], a[0]];
  }
  if (opts.currentArtist && a.length > 1 && a[0].artist_name === opts.currentArtist) {
    const k = a.findIndex((t) => t.artist_name !== opts.currentArtist);
    if (k > 0) {
      [a[0], a[k]] = [a[k], a[0]];
      spreadConsecutiveArtists(a);
    }
  }
  return a;
}

/** Fixed seed + smart shuffle of the rest of the pool (favourites / genre / mood card). */
export function buildShuffleQueueFromSeed(
  seed: Track,
  pool: readonly Track[],
  opts: SmartShuffleOpts & ShuffleExclusionOpts = {},
): Track[] {
  const filtered = filterPoolForExclusions(pool, seed, opts);
  const rest = filtered.filter((t) => t.rel_path !== seed.rel_path);
  if (!rest.length) return [seed];
  const shuffled = buildSmartRandomQueue(rest, {
    ...opts,
    currentRelPath: seed.rel_path,
    currentArtist: seed.artist_name,
  });
  return [seed, ...shuffled].slice(0, CARD_QUEUE_CAP);
}

/** Smart radio: seed + library ordered by mood/genre similarity. */
export function buildRadioFromSeed(
  seed: Track,
  library: readonly Track[],
  opts?: {
    maxLength?: number;
    recentRelPaths?: ReadonlySet<string>;
  } & ShuffleExclusionOpts,
): Track[] {
  const cap = Math.max(1, Math.min(opts?.maxLength ?? CARD_QUEUE_CAP, CARD_QUEUE_CAP));
  const pool = filterPoolForExclusions(library, seed, opts).filter(
    (t) => t.rel_path !== seed.rel_path,
  );
  const ordered = sortPoolBySeedSimilarity(seed, pool, opts?.recentRelPaths);
  const full = [seed, ...ordered];
  spreadConsecutiveArtists(full);
  return full.slice(0, cap);
}
