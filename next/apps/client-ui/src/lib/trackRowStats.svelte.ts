/**
 * Per-track numbers a track row shows (play count, moods, queue / shuffle
 * state), read from the in-memory prefs cache through `prefsRevision`.
 *
 * Before: each `TrackRow` re-parsed the whole prefs blob from localStorage on
 * mount and on every `session.tick` (QA perf: 330 KB prefs → ~200 ms of script
 * per track change in a long list). Now lists read the maps once
 * (`prefsRevision.playCountsMap` / `trackMoodsMap`, reactive only on their own
 * revision counters) and hand each row its values as props (`TrackList`).
 * A row rendered on its own (QueueView) falls back to the same helpers.
 */
import type { Track } from "./api";
import { player } from "./player";
import { moodsIn, playCountIn, prefsRevision } from "./prefsRevision.svelte";
import { session } from "./session.svelte";
import type { TrackMoodId } from "./trackMoods";

type TrackKey = Pick<Track, "id" | "rel_path">;
type MoodsMap = Readonly<Record<string, readonly string[]>>;
type CountsMap = Readonly<Record<string, number>>;

class TrackRowStats {
  /** Ids in the play queue; rebuilt once per player change, not per row. */
  readonly queueIds: ReadonlySet<number> = $derived.by(() => {
    void session.tick;
    return new Set(player.queue.map((t) => t.id));
  });

  /** Moves when shuffle exclusions may have changed (cheap Set lookups follow). */
  readonly exclusionRevision: number = $derived.by(() => prefsRevision.any + session.tick);

  /** Same array for the same track until the moods map changes: rows skip re-render. */
  #moodMemo = new WeakMap<MoodsMap, Map<string, TrackMoodId[]>>();

  playsIn(counts: CountsMap, track: TrackKey): number {
    return playCountIn(counts, track);
  }

  moodsFrom(map: MoodsMap, track: TrackKey): TrackMoodId[] {
    let memo = this.#moodMemo.get(map);
    if (!memo) {
      memo = new Map();
      this.#moodMemo.set(map, memo);
    }
    const key = `${track.id}\u0000${track.rel_path}`;
    let out = memo.get(key);
    if (!out) {
      out = moodsIn(map, track);
      memo.set(key, out);
    }
    return out;
  }

  /** Single-row fallbacks (reactive on the matching prefs revision). */
  plays(track: TrackKey): number {
    return this.playsIn(prefsRevision.playCountsMap, track);
  }

  moods(track: TrackKey): TrackMoodId[] {
    return this.moodsFrom(prefsRevision.trackMoodsMap, track);
  }

  inQueue(trackId: number): boolean {
    return this.queueIds.has(trackId);
  }

  excluded(track: Track): boolean {
    void this.exclusionRevision;
    return player.isTrackExcluded(track);
  }

  albumLocked(track: Track): boolean {
    void this.exclusionRevision;
    return track.album_id != null && player.isAlbumExcluded(track.album_id);
  }
}

export const trackRowStats = new TrackRowStats();
