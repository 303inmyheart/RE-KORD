/**
 * While a Plectr run is up (prepare → results), the global player must not
 * crossfade into the next track nor advance on its own: the results screen
 * has to refer to the song that just ended, with the player waiting.
 *
 * Preferred: the player's own hold API (`setHold` / `setCrossfadeOverride`,
 * requested from the player owner). Until it exists this works through the
 * public API: crossfade forced to 0 without touching the saved preference
 * (`applyCrossfadeSec`) and a pause a moment before the end of the track
 * (`endGuard`), so `ended` — and the auto-advance behind it — never fires.
 */
import { player } from "../player";
import { loadUserPrefs } from "../userPrefs";

type HoldCapablePlayer = {
  setHold?: (reason: string | null) => void;
  setCrossfadeOverride?: (sec: number | null) => void;
  setSeekLock?: (reason: string | null) => void;
  onHeldEnd?: (fn: (track: { rel_path: string } | null) => void) => () => void;
};

const REASON = "plectr";
/** Pause this close to the end (timeupdate fires every ~250 ms). */
export const END_GUARD_SECONDS = 0.45;

function native(): HoldCapablePlayer {
  return player as unknown as HoldCapablePlayer;
}

/** True when the player itself can hold (no end-of-track pause needed). */
export function playerHasHoldApi(): boolean {
  return typeof native().setHold === "function";
}

export class PlectrPlayerHold {
  private active = false;
  private offProgress: (() => void) | null = null;
  /** Track the hold protects (end guard), null = any. */
  private relPath: string | null = null;
  /** Set when the end guard paused the track. */
  pausedAtEnd = false;

  get held(): boolean {
    return this.active;
  }

  acquire(relPath: string | null): void {
    this.relPath = relPath;
    this.pausedAtEnd = false;
    if (this.active) return;
    this.active = true;
    const p = native();
    if (typeof p.setHold === "function") p.setHold(REASON);
    // During a run the chart owns the clock: dock, media keys and shortcuts can't seek.
    if (typeof p.setSeekLock === "function") p.setSeekLock(REASON);
    if (typeof p.setCrossfadeOverride === "function") p.setCrossfadeOverride(0);
    else player.applyCrossfadeSec(0);
    if (!playerHasHoldApi()) {
      this.offProgress = player.subscribeProgress(() => this.guard());
    } else if (typeof p.onHeldEnd === "function") {
      // The player stops at the end by itself and tells us.
      this.offProgress = p.onHeldEnd((track) => {
        if (!this.relPath || track?.rel_path === this.relPath) this.pausedAtEnd = true;
      });
    }
  }

  /** Re-target the end guard (replay / next track). */
  retarget(relPath: string | null): void {
    this.relPath = relPath;
    this.pausedAtEnd = false;
  }

  private guard(): void {
    if (!this.active) return;
    // Someone re-applied the saved crossfade (account switch): force it off again.
    if (player.crossfadeSec !== 0 && typeof native().setCrossfadeOverride !== "function") {
      player.applyCrossfadeSec(0);
    }
    const cur = player.current;
    if (!cur || (this.relPath && cur.rel_path !== this.relPath)) return;
    const d = player.duration;
    if (!player.playing || !(d > 0)) return;
    if (d - player.currentTime <= END_GUARD_SECONDS) {
      this.pausedAtEnd = true;
      player.pause();
    }
  }

  release(): void {
    if (!this.active) return;
    this.active = false;
    this.offProgress?.();
    this.offProgress = null;
    const p = native();
    if (typeof p.setHold === "function") p.setHold(null);
    if (typeof p.setSeekLock === "function") p.setSeekLock(null);
    if (typeof p.setCrossfadeOverride === "function") p.setCrossfadeOverride(null);
    else player.applyCrossfadeSec(loadUserPrefs().crossfadeSec);
  }
}
