import { resolveTrackMoods, type TrackMoodId } from "./trackMoods";
import {
  getPlayCountsMap,
  getRecentRelPaths,
  getTrackMoodsMap,
  playCountIn,
  subscribeUserPrefsPatch,
} from "./userPrefs";

/**
 * Reactive revision counters for the user-pref fields views aggregate over.
 *
 * `session.tick` bumps on every player event (play, pause, seek, track change),
 * so derived values that only need play counts / moods / exclusions should
 * depend on these counters instead: they move only when those fields are
 * patched (`patchUserPrefs`, which is also what hub pulls go through), or when
 * the whole blob is replaced (account switch, another tab).
 *
 * The getters (`playCountsMap`, `trackMoodsMap`, `recentRelPaths`) read the
 * in-memory prefs cache AND register the matching counter, so inside a
 * `$derived` they are both cheap and reactive:
 *
 *   const counts = $derived(prefsRevision.playCountsMap);
 *   const plays = (t: Track) => playCountIn(counts, t);
 */
class PrefsRevision {
  playCounts = $state(0);
  moods = $state(0);
  exclusions = $state(0);
  recent = $state(0);
  /** Any prefs change at all (appearance included). */
  any = $state(0);

  /** Play counts of the bound account; reactive on `playCounts`. Read-only. */
  get playCountsMap(): Readonly<Record<string, number>> {
    void this.playCounts;
    return getPlayCountsMap();
  }

  /** Personal moods of the bound account; reactive on `moods`. Read-only. */
  get trackMoodsMap(): Readonly<Record<string, readonly string[]>> {
    void this.moods;
    return getTrackMoodsMap();
  }

  /** Recent paths, newest first; reactive on `recent`. Read-only. */
  get recentRelPaths(): readonly string[] {
    void this.recent;
    return getRecentRelPaths();
  }
}

export const prefsRevision = new PrefsRevision();

subscribeUserPrefsPatch((patch) => {
  prefsRevision.any += 1;
  if ("playCounts" in patch) prefsRevision.playCounts += 1;
  if ("trackMoods" in patch) prefsRevision.moods += 1;
  if ("recentRelPaths" in patch || "recentTrackIds" in patch) prefsRevision.recent += 1;
  if ("excludedRelPaths" in patch || "excludedAlbumIds" in patch || "excludedTrackIds" in patch) {
    prefsRevision.exclusions += 1;
  }
});

/** Play count of `track` from a counts map (see `prefsRevision.playCountsMap`). */
export { playCountIn };

/** Personal moods of `track` from a moods map (see `prefsRevision.trackMoodsMap`). */
export function moodsIn(
  moods: Readonly<Record<string, readonly string[]>>,
  track: { id: number; rel_path: string },
): TrackMoodId[] {
  return resolveTrackMoods(track.id, track.rel_path, moods as Record<string, string[]>);
}
