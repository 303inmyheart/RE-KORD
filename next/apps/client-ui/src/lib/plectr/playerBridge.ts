/**
 * Read-only bridge from the Plectr stage to the global player: the audio
 * element that is playing the given track (for a precise clock), with a
 * fallback on the player's reported time when the element is not reachable.
 */
import { player } from "../player";
import type { ClockAudioLike, PlayerSyncBridgeLike } from "./smoothSongClock";

export type PlectrPlayerBridge = PlayerSyncBridgeLike & {
  /** Duration of the playing file (0 when unknown). */
  getDuration: () => number;
  /** True when the player is on this bridge's track. */
  isOnTrack: () => boolean;
};

export function createPlayerBridge(relPath: string): PlectrPlayerBridge {
  /** Stand-in clock when no local deck is reachable (remote output, loading). */
  const virtualAudio: ClockAudioLike = {
    currentTime: 0,
    paused: true,
    ended: false,
    playbackRate: 1,
  };

  /** Null while a remote output plays or no deck holds the file yet. */
  function findElement(): HTMLAudioElement | null {
    return player.getAudioForTrack(relPath);
  }

  function isOnTrack(): boolean {
    return player.current?.rel_path === relPath;
  }

  return {
    isOnTrack,
    getAudio: () => {
      const el = findElement();
      if (el) return el;
      if (!isOnTrack()) return null;
      virtualAudio.currentTime = player.currentTime;
      virtualAudio.paused = !player.playing;
      return virtualAudio;
    },
    getCurrentTime: () => {
      const el = findElement();
      if (el && Number.isFinite(el.currentTime)) return el.currentTime;
      return isOnTrack() ? player.currentTime : 0;
    },
    getDuration: () => {
      const el = findElement();
      if (el && Number.isFinite(el.duration) && el.duration > 0) return el.duration;
      return isOnTrack() && player.duration > 0 ? player.duration : 0;
    },
  };
}
