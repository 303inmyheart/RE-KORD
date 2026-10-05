import type { Track } from "../api";
import { resolveTrackMoods } from "../trackMoods";
import type { NebulaTrackInput } from "./model";

/** A nebula input that remembers the API track it came from (to play it). */
export type NebulaSourceTrack = NebulaTrackInput & { source: Track };

/** BPM from the file tags (hub `Track.bpm`, TBPM / BPM comment) when the hub sends it. */
function tagBpm(t: Track): number | null {
  const raw = (t as Track & { bpm?: number | string | null }).bpm;
  const n = typeof raw === "string" ? Number.parseFloat(raw) : raw;
  return typeof n === "number" && Number.isFinite(n) && n > 40 && n < 220 ? n : null;
}

/**
 * Catalog tracks → layout inputs. Moods are the personal moods saved in user
 * prefs (keyed by rel_path), the next equivalent of legacy `meta.moods`.
 *
 * BPM priority: file tags (`track.bpm`) → `chartBpm` (Plectr chart cache, as
 * legacy) → none, in which case the model uses legacy's hash + duration guess.
 */
export function nebulaInputsFromTracks(
  tracks: readonly Track[],
  savedMoods: Record<string, string[]>,
  chartBpm?: (relPath: string) => number | null,
): NebulaSourceTrack[] {
  return tracks.map((t) => ({
    relPath: t.rel_path,
    title: t.title,
    artist: t.artist_name,
    album: t.album_name,
    durationMs: t.duration_ms,
    genre: t.genre ?? null,
    moods: resolveTrackMoods(t.id, t.rel_path, savedMoods),
    bpm: tagBpm(t) ?? chartBpm?.(t.rel_path) ?? null,
    source: t,
  }));
}
