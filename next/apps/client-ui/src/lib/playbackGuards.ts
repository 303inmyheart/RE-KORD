/**
 * Temporary rules a feature can lay over the player (Plectr while a run is
 * up): hold (no auto-advance at track end), a crossfade override (not saved
 * in prefs) and a seek lock (timeline, media keys and shortcuts refused; code
 * passing `{ force: true }` still seeks).
 *
 * Pure state, kept apart from the player so it can be tested in node.
 */

export type GuardReason = string | null;

export class PlaybackGuards {
  /** Why the player must not advance on its own (null = free). */
  hold: GuardReason = null;
  /** Why user seeks are refused (null = free). */
  seekLock: GuardReason = null;
  /** Crossfade seconds replacing the user's setting (null = use the setting). */
  crossfadeOverride: number | null = null;

  /** Normalizes an empty reason to null. */
  static reason(raw: string | null | undefined): GuardReason {
    const r = (raw ?? "").trim();
    return r ? r : null;
  }

  setHold(reason: string | null | undefined): boolean {
    const next = PlaybackGuards.reason(reason);
    const changed = next !== this.hold;
    this.hold = next;
    return changed;
  }

  setSeekLock(reason: string | null | undefined): boolean {
    const next = PlaybackGuards.reason(reason);
    const changed = next !== this.seekLock;
    this.seekLock = next;
    return changed;
  }

  /** Non-negative finite seconds, or null to drop the override. */
  setCrossfadeOverride(sec: number | null | undefined): boolean {
    const next =
      sec == null || !Number.isFinite(Number(sec)) ? null : Math.max(0, Math.min(30, Number(sec)));
    const changed = next !== this.crossfadeOverride;
    this.crossfadeOverride = next;
    return changed;
  }

  get held(): boolean {
    return this.hold != null;
  }

  get seekLocked(): boolean {
    return this.seekLock != null;
  }

  /** The crossfade actually used: override when set, else the user's. */
  crossfade(userSec: number): number {
    return this.crossfadeOverride ?? userSec;
  }

  /** May this seek go through? */
  allowSeek(opts?: { force?: boolean }): boolean {
    return opts?.force === true || !this.seekLocked;
  }

  /** At the end of a track: advance the queue (false while held). */
  advanceOnEnd(): boolean {
    return !this.held;
  }
}
