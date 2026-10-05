/**
 * Dashboard counters for the active account.
 *
 * Hub ≥ 5.2 answers `library/stats` for the requesting account's library
 * selection and adds the quality counters plus `catalog_track_count` (whole
 * index). Older hubs answer with global totals: then the counters come from the
 * account's own catalog (`tracks-page`, already selection-filtered) once it is
 * loaded, never from the global stats (an empty account would show the hub's
 * totals).
 */
import type { Album, LibraryStats, Track } from "../../../lib/api";
import { albumHasAlbumMeta, trackHasFileMeta } from "../../../lib/trackMoods";

/** `library/stats` (the hub 5.2 fields are on `LibraryStats` itself, optional). */
export type AccountStats = LibraryStats;

export type LibraryCounts = { artists: number; albums: number; tracks: number };

export type QualityCounts = {
  albumsWithoutCover: number;
  albumsWithoutMeta: number;
  tracksWithoutMeta: number;
  looseAlbums: number;
};

/** Hub stats follow the account selection (hub ≥ 5.2). */
export function statsArePerAccount(stats: AccountStats | null | undefined): boolean {
  return typeof stats?.catalog_track_count === "number";
}

export function libraryCounts(
  stats: AccountStats | null | undefined,
  catalog: readonly Track[],
  catalogReady: boolean,
): LibraryCounts | null {
  if (stats && statsArePerAccount(stats)) {
    return { artists: stats.artist_count, albums: stats.album_count, tracks: stats.track_count };
  }
  if (!catalogReady) return null;
  const albums = new Set<number>();
  const artists = new Set<number | string>();
  for (const tr of catalog) {
    if (tr.album_id != null) albums.add(tr.album_id);
    artists.add(tr.artist_id ?? tr.artist_name);
  }
  return { artists: artists.size, albums: albums.size, tracks: catalog.length };
}

export function qualityCounts(
  stats: AccountStats | null | undefined,
  albums: readonly Album[],
  catalog: readonly Track[],
  ready: boolean,
): QualityCounts | null {
  if (
    stats &&
    statsArePerAccount(stats) &&
    typeof stats.albums_without_cover === "number" &&
    typeof stats.tracks_without_meta === "number"
  ) {
    return {
      albumsWithoutCover: stats.albums_without_cover,
      albumsWithoutMeta: stats.albums_without_meta ?? 0,
      tracksWithoutMeta: stats.tracks_without_meta,
      looseAlbums: stats.loose_album_count ?? 0,
    };
  }
  if (!ready) return null;
  // Legacy formula over the account's own albums (albums seen in its catalog).
  const mine = new Set<number>();
  for (const tr of catalog) if (tr.album_id != null) mine.add(tr.album_id);
  const own = albums.filter((a) => mine.has(a.id));
  return {
    albumsWithoutCover: own.filter((a) => !a.has_cover && !a.loose).length,
    albumsWithoutMeta: own.filter((a) => !a.loose && !albumHasAlbumMeta(a)).length,
    tracksWithoutMeta: catalog.filter((tr) => !trackHasFileMeta(tr)).length,
    looseAlbums: own.filter((a) => a.loose).length,
  };
}

/** "Album aggiornati": most recently updated first (hub `updated_at`), else newest id. */
export function recentlyUpdatedAlbums(albums: readonly Album[], limit: number): Album[] {
  const stamp = (a: Album) => {
    const raw = (a as Album & { updated_at?: string | null; added_at?: string | null }).updated_at
      ?? (a as Album & { added_at?: string | null }).added_at;
    const ms = raw ? Date.parse(raw) : NaN;
    return Number.isFinite(ms) ? ms : null;
  };
  return [...albums]
    .sort((a, b) => {
      const sa = stamp(a);
      const sb = stamp(b);
      if (sa != null || sb != null) return (sb ?? -Infinity) - (sa ?? -Infinity) || b.id - a.id;
      return b.id - a.id || a.name.localeCompare(b.name);
    })
    .slice(0, limit);
}
