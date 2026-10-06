import type { Album, Artist } from "./api";
import { hashSeed } from "./trackMoods";

/** Maps artist → cover album id (stable pseudo-random pick, like old density). */
export function buildArtistCoverAlbumMap(
  artists: Artist[],
  albums: Album[],
  /** If true, random reshuffle (e.g. on Sync); otherwise stable hash. */
  reshuffle = false,
): Map<number, number> {
  const byArtist = new Map<number, number[]>();
  for (const a of albums) {
    if (!a.has_cover || a.artist_id == null) continue;
    const list = byArtist.get(a.artist_id) ?? [];
    list.push(a.id);
    byArtist.set(a.artist_id, list);
  }

  const map = new Map<number, number>();
  for (const artist of artists) {
    const ids = byArtist.get(artist.id);
    if (!ids?.length) continue;
    const ix = reshuffle
      ? Math.floor(Math.random() * ids.length)
      : hashSeed(`artist-cover:${artist.id}`) % ids.length;
    map.set(artist.id, ids[ix]!);
  }
  return map;
}
