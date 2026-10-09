/**
 * "Playlist al volo" (dashboard): genre + mood picker with live counts.
 *
 * Pure helpers, no Svelte: the card builds one `MixEntry` per track when the
 * catalog (or the saved moods / exclusions) change, then every click runs one
 * cheap pass (`computeMixFacets`) that yields the matches, the live count of
 * every chip and the total duration together. Nothing is computed per chip.
 *
 * Rules (what the card says in words):
 * - genres: a track matches if it has **any** of the chosen genres;
 * - moods: **any** chosen mood ("Almeno uno") or **all** of them ("Tutti insieme");
 * - genres and moods together: the track must pass both.
 */
import {
  canonicalGenreLabel,
  genreLabelKey,
  trackGenres,
  type GenreSource,
} from "./genres";
import { NO_GENRE_KEY, TRACK_MOOD_IDS, type TrackMoodId } from "./trackMoods";

export type MixEntry = {
  /** Genre chip keys (`genreLabelKey`), or `[NO_GENRE_KEY]`. */
  genres: readonly string[];
  moods: readonly TrackMoodId[];
  durationMs: number;
};

export type MixSelection = {
  genres: readonly string[];
  moods: readonly TrackMoodId[];
  matchAll: boolean;
};

export const EMPTY_MIX: MixSelection = { genres: [], moods: [], matchAll: false };

export function emptyMoodCounts(): Record<TrackMoodId, number> {
  return Object.fromEntries(TRACK_MOOD_IDS.map((id) => [id, 0])) as Record<TrackMoodId, number>;
}

export function moodsPass(
  moods: readonly TrackMoodId[],
  want: readonly TrackMoodId[],
  matchAll: boolean,
): boolean {
  if (!want.length) return true;
  if (matchAll) return want.every((m) => moods.includes(m));
  return want.some((m) => moods.includes(m));
}

export type MixTotals = {
  /** Library-wide tracks per genre key. */
  genres: Map<string, number>;
  /** Library-wide tracks per mood. */
  moods: Record<TrackMoodId, number>;
};

/** Library-wide counts: chip order and "hidden because empty" (once per catalog). */
export function computeMixTotals(entries: readonly MixEntry[]): MixTotals {
  const genres = new Map<string, number>();
  const moods = emptyMoodCounts();
  for (const e of entries) {
    for (const g of e.genres) genres.set(g, (genres.get(g) ?? 0) + 1);
    for (const m of e.moods) moods[m] += 1;
  }
  return { genres, moods };
}

export type MixFacets = {
  /** Indexes into `entries` of the matching tracks (empty with no selection). */
  matches: number[];
  totalMs: number;
  /** Tracks of each genre that pass the mood choice (what picking it would add). */
  genreCounts: Map<string, number>;
  /** Tracks of each mood within the chosen genres. */
  moodCounts: Record<TrackMoodId, number>;
};

/** One pass: matches + live chip counts for the current selection. */
export function computeMixFacets(entries: readonly MixEntry[], sel: MixSelection): MixFacets {
  const wantGenres = new Set(sel.genres);
  const hasGenres = wantGenres.size > 0;
  const active = hasGenres || sel.moods.length > 0;
  const genreCounts = new Map<string, number>();
  const moodCounts = emptyMoodCounts();
  const matches: number[] = [];
  let totalMs = 0;
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i]!;
    const genreOk = !hasGenres || e.genres.some((g) => wantGenres.has(g));
    const moodOk = moodsPass(e.moods, sel.moods, sel.matchAll);
    if (moodOk) for (const g of e.genres) genreCounts.set(g, (genreCounts.get(g) ?? 0) + 1);
    if (genreOk) for (const m of e.moods) moodCounts[m] += 1;
    if (active && genreOk && moodOk) {
      matches.push(i);
      if (e.durationMs > 0) totalMs += e.durationMs;
    }
  }
  return { matches, totalMs, genreCounts, moodCounts };
}

/**
 * Library genres in chip order: most tracks first, ties by name, "no genre"
 * always last (it is the leftover bucket, not a genre).
 */
export function sortGenreKeys(
  totals: ReadonlyMap<string, number>,
  label: (key: string) => string,
): string[] {
  return [...totals.keys()].sort((a, b) => {
    if (a === NO_GENRE_KEY) return 1;
    if (b === NO_GENRE_KEY) return -1;
    return (totals.get(b) ?? 0) - (totals.get(a) ?? 0) || label(a).localeCompare(label(b));
  });
}

/**
 * Collapsed genre row: the first `limit`, plus any selected one beyond them
 * (a choice never disappears), in the original order.
 */
export function visibleGenreKeys(
  ordered: readonly string[],
  selected: readonly string[],
  limit: number,
  expanded: boolean,
): { shown: string[]; hidden: number } {
  if (expanded || ordered.length <= limit) return { shown: [...ordered], hidden: 0 };
  const pick = new Set(selected);
  const shown = ordered.filter((k, i) => i < limit || pick.has(k));
  return { shown, hidden: ordered.length - shown.length };
}

/**
 * Mood row: moods nobody tagged in the library are tucked away ("mostra tutti"),
 * selected ones always stay. Canonical order, so chips never jump.
 */
export function visibleMoodIds(
  totals: Readonly<Record<TrackMoodId, number>>,
  selected: readonly TrackMoodId[],
  showAll: boolean,
): { shown: TrackMoodId[]; hidden: number } {
  if (showAll) return { shown: [...TRACK_MOOD_IDS], hidden: 0 };
  const shown = TRACK_MOOD_IDS.filter((id) => (totals[id] ?? 0) > 0 || selected.includes(id));
  return { shown, hidden: TRACK_MOOD_IDS.length - shown.length };
}

/**
 * Up to `max` tracks from different albums that have artwork, for the cover
 * strip (placeholder discs would only add noise: no art, no strip).
 */
export function pickCoverTracks<T extends { album_id: number | null }>(
  tracks: readonly T[],
  hasCover: (t: T) => boolean,
  max = 5,
): T[] {
  const out: T[] = [];
  const seen = new Set<number>();
  for (const t of tracks) {
    if (out.length >= max) break;
    if (t.album_id == null || seen.has(t.album_id) || !hasCover(t)) continue;
    seen.add(t.album_id);
    out.push(t);
  }
  return out;
}

/**
 * Genre chips of one track (album genre as fallback): one per label *as
 * shown*, keyed by `genreLabelKey` like the album page, so aliases the hub
 * keeps apart ("Drum & Bass" / "Drum and Bass", "Rhythm & Blues" / "R&B")
 * are one chip and their counts merge. Empty when the track has no genre.
 */
export function mixGenresOf(
  track: GenreSource | null | undefined,
  album?: GenreSource | null,
): Array<{ key: string; label: string }> {
  const out: Array<{ key: string; label: string }> = [];
  for (const g of trackGenres(track, album)) {
    const key = genreLabelKey(g);
    if (!key || out.some((c) => c.key === key)) continue;
    out.push({ key, label: canonicalGenreLabel(g) });
  }
  return out;
}

/** Keep only choices that still exist (renamed genre, mood list change). */
export function pruneMixSelection(
  sel: MixSelection,
  genreExists: (key: string) => boolean,
): MixSelection {
  const genres = sel.genres.filter(genreExists);
  if (genres.length === sel.genres.length) return sel;
  return { ...sel, genres };
}

/** Saved selection (localStorage JSON) → a valid selection, never throws. */
export function parseMixSelection(raw: string | null | undefined): MixSelection {
  if (!raw) return EMPTY_MIX;
  try {
    const v = JSON.parse(raw) as Partial<Record<keyof MixSelection, unknown>>;
    // Selections saved before chips were keyed by label used
    // `normalizeGenreKey`: map them onto the label key ("drum&bass" →
    // "drumandbass"), which is the same for every other genre.
    const genres = Array.isArray(v.genres)
      ? [
          ...new Set(
            v.genres
              .filter((g): g is string => typeof g === "string" && g !== "")
              .map((g) => (g === NO_GENRE_KEY ? g : genreLabelKey(g)))
              .filter((g) => g !== ""),
          ),
        ]
      : [];
    const moods = Array.isArray(v.moods)
      ? [
          ...new Set(
            v.moods.filter((m): m is TrackMoodId =>
              (TRACK_MOOD_IDS as readonly unknown[]).includes(m),
            ),
          ),
        ]
      : [];
    return { genres, moods, matchAll: v.matchAll === true };
  } catch {
    return EMPTY_MIX;
  }
}
