/**
 * Client-side library search (legacy `useLibrarySearch`): what a person can
 * see — title, artist, album, genres — never the file path, and never the
 * "01 - " number a file name or a tag put in front of the title.
 *
 * Pure (no stores): the session keeps one index per catalog.
 */
import { normalizeGenreKey, parseTrackGenres, type GenreSource } from "./genres";

/** Searchable track: what the catalog rows carry. */
export type SearchableTrack = GenreSource & {
  title: string;
  artist_name: string;
  album_name: string;
  album_id?: number | null;
};

/**
 * Leading track numbers: "01 - Title", "01. Title", "3) Title", "1-03 Title",
 * "05_Title". A bare "21 Guns" or "1999" is a real title and stays.
 */
const NUMBER_PREFIX_RE = /^\s*(?:\d{1,2}-\d{1,3}\s+|\d{1,3}(?:[-.]\d{1,3})?\s*[-–—._)]\s*)/;

export function stripTrackNumberPrefix(title: string): string {
  const stripped = title.replace(NUMBER_PREFIX_RE, "");
  return stripped.trim() ? stripped : title;
}

/** Lowercase, accents off, spacing collapsed: "Café  Tacvba" → "cafe tacvba". */
export function foldText(s: string | null | undefined): string {
  return String(s ?? "")
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();
}

function queryTokens(q: string): string[] {
  return foldText(q).split(" ").filter(Boolean);
}

type Entry<T> = {
  track: T;
  title: string;
  artist: string;
  album: string;
  /** Genre tokens folded, plus their normalized keys ("hip hop" + "hiphop"). */
  genres: string;
};

function entryOf<T extends SearchableTrack>(track: T): Entry<T> {
  const genreTokens = parseTrackGenres(
    Array.isArray(track.genres) && track.genres.length ? track.genres.join("; ") : track.genre,
  );
  return {
    track,
    title: foldText(stripTrackNumberPrefix(track.title)),
    artist: foldText(track.artist_name),
    album: foldText(track.album_name),
    genres: genreTokens.map((g) => `${foldText(g)} ${normalizeGenreKey(g)}`).join(" | "),
  };
}

/** Every query word appears in one of the visible fields. */
function entryMatches(entry: Entry<unknown>, tokens: string[]): boolean {
  if (!tokens.length) return false;
  return tokens.every(
    (tok) =>
      entry.title.includes(tok) ||
      entry.artist.includes(tok) ||
      entry.album.includes(tok) ||
      entry.genres.includes(tok),
  );
}

/** Relevance: title hits first (prefix best), then artist, album, genre. */
function score(entry: Entry<unknown>, folded: string, tokens: string[]): number {
  let s = 0;
  if (entry.title === folded) s += 100;
  else if (entry.title.startsWith(folded)) s += 60;
  else if (entry.title.includes(folded)) s += 40;
  if (entry.artist === folded) s += 30;
  else if (entry.artist.includes(folded)) s += 20;
  if (entry.album.includes(folded)) s += 10;
  for (const tok of tokens) if (entry.title.includes(tok)) s += 2;
  return s;
}

/** Whether one track matches a query (used to filter hub search results). */
export function trackMatchesQuery(track: SearchableTrack, q: string): boolean {
  return entryMatches(entryOf(track), queryTokens(q));
}

export type SearchIndex<T extends SearchableTrack = SearchableTrack> = {
  entries: Entry<T>[];
  /** Any track of this artist (by name) has a genre matching the query. */
  artistGenreMatches: (artistName: string, q: string) => boolean;
  /** The album's own genre, or one of its tracks', matches the query. */
  albumGenreMatches: (
    album: { id: number; name: string; artist_name: string; genre?: string | null },
    q: string,
  ) => boolean;
};

export function buildSearchIndex<T extends SearchableTrack>(tracks: readonly T[]): SearchIndex<T> {
  const entries = tracks.map(entryOf);
  const artistGenres = new Map<string, Set<string>>();
  const albumGenres = new Map<string, Set<string>>();
  const add = (map: Map<string, Set<string>>, key: string, genres: string) => {
    if (!genres) return;
    let set = map.get(key);
    if (!set) map.set(key, (set = new Set()));
    set.add(genres);
  };
  for (const e of entries) {
    add(artistGenres, e.artist, e.genres);
    const albumKey = e.track.album_id != null ? `#${e.track.album_id}` : `${e.artist}/${e.album}`;
    add(albumGenres, albumKey, e.genres);
  }
  const hit = (set: Set<string> | undefined, tokens: string[]) =>
    !!set && tokens.length > 0 && [...set].some((g) => tokens.every((tok) => g.includes(tok)));
  return {
    entries,
    artistGenreMatches: (artistName, q) => hit(artistGenres.get(foldText(artistName)), queryTokens(q)),
    albumGenreMatches: (album, q) => {
      const tokens = queryTokens(q);
      if (!tokens.length) return false;
      const own = parseTrackGenres(album.genre)
        .map((g) => `${foldText(g)} ${normalizeGenreKey(g)}`)
        .join(" | ");
      if (own && tokens.every((tok) => own.includes(tok))) return true;
      return hit(albumGenres.get(`#${album.id}`), tokens);
    },
  };
}

/** Tracks matching `q`, best first, at most `limit`. */
export function searchTracks<T extends SearchableTrack>(
  index: SearchIndex<T>,
  q: string,
  limit = 500,
): T[] {
  const tokens = queryTokens(q);
  if (!tokens.length) return [];
  const folded = tokens.join(" ");
  const hits: { entry: Entry<T>; score: number; i: number }[] = [];
  index.entries.forEach((entry, i) => {
    if (entryMatches(entry, tokens)) hits.push({ entry, score: score(entry, folded, tokens), i });
  });
  hits.sort((a, b) => b.score - a.score || a.i - b.i);
  return hits.slice(0, limit).map((h) => h.entry.track);
}
