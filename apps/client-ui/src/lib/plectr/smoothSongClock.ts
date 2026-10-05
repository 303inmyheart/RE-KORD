/**
 * Smooth song clock for the note highway (port of legacy `smoothSongClock.ts`).
 *
 * The audio element only reports `currentTime` every few tens of ms (on mobile
 * sometimes much less often): read raw each frame the notes would stutter. The
 * clock extrapolates with `performance.now()` and eases towards new samples.
 */

/** What the clock needs from a media element (an HTMLAudioElement fits). */
export type ClockAudioLike = {
  currentTime: number;
  paused: boolean;
  ended: boolean;
  playbackRate: number;
};

export type PlayerSyncBridgeLike = {
  getCurrentTime: () => number;
  getAudio: () => ClockAudioLike | null;
};

/** Seek / large drift: snap straight back to the player. */
const CLOCK_HARD_SYNC_THRESHOLD_SECONDS = 0.45;
/** Time constant used to chase audio.currentTime without periodic jumps. */
const CLOCK_SMOOTH_TAU_SECONDS = 0.14;
/** Below this the performance.now extrapolation is enough (no micro-corrections). */
const CLOCK_MIN_CORRECTION_SECONDS = 0.0025;
/** Minimum change of audio.currentTime to count as a fresh sample from the browser. */
const AUDIO_SAMPLE_EPSILON_SECONDS = 0.0005;

export type SongClockState = {
  clockAnchorSong: number;
  clockAnchorPerf: number;
  smoothFramePerf: number;
  /** Last audio.currentTime sampled from the browser. */
  audioSampleSong: number;
  audioSamplePerf: number;
  audioPlaybackRate: number;
};

export function createSongClockState(): SongClockState {
  return {
    clockAnchorSong: 0,
    clockAnchorPerf: 0,
    smoothFramePerf: 0,
    audioSampleSong: 0,
    audioSamplePerf: 0,
    audioPlaybackRate: 1,
  };
}

export function resetSongClock(
  clock: SongClockState,
  songTime: number,
  perfNow: number,
): void {
  clock.clockAnchorSong = songTime;
  clock.clockAnchorPerf = perfNow;
  clock.smoothFramePerf = perfNow;
}

/**
 * On mobile audio.currentTime often stays frozen between timeupdates (even for
 * seconds). Comparing it every frame with the performance.now extrapolation
 * would pull the notes back until the next sample jumps forward.
 */
function readInterpolatedAudioTime(
  clock: SongClockState,
  bridge: PlayerSyncBridgeLike,
  perfNow: number,
  playing: boolean,
  playbackRate: number,
): number {
  const audio = bridge.getAudio();
  const raw = audio && Number.isFinite(audio.currentTime)
    ? audio.currentTime
    : bridge.getCurrentTime();

  if (!playing) {
    clock.audioSampleSong = raw;
    clock.audioSamplePerf = perfNow;
    clock.audioPlaybackRate = playbackRate;
    return raw;
  }

  if (
    clock.audioSamplePerf <= 0 ||
    Math.abs(raw - clock.audioSampleSong) > AUDIO_SAMPLE_EPSILON_SECONDS
  ) {
    clock.audioSampleSong = raw;
    clock.audioSamplePerf = perfNow;
    clock.audioPlaybackRate = playbackRate;
    return raw;
  }

  return (
    clock.audioSampleSong +
    ((perfNow - clock.audioSamplePerf) / 1000) * clock.audioPlaybackRate
  );
}

/** Smooth clock used to render notes in sync with the global player. */
export function resolveSmoothSongTime(
  clock: SongClockState,
  perfNow: number,
  bridge: PlayerSyncBridgeLike,
): number {
  const audio = bridge.getAudio();
  const playing = Boolean(audio && !audio.paused && !audio.ended);
  const playbackRate =
    audio?.playbackRate && Number.isFinite(audio.playbackRate)
      ? audio.playbackRate
      : 1;
  const audioT = readInterpolatedAudioTime(
    clock,
    bridge,
    perfNow,
    playing,
    playbackRate,
  );

  if (!playing) {
    resetSongClock(clock, audioT, perfNow);
    return audioT;
  }

  if (clock.clockAnchorPerf <= 0) {
    resetSongClock(clock, audioT, perfNow);
    return audioT;
  }

  const prevPerf =
    clock.smoothFramePerf > 0 ? clock.smoothFramePerf : perfNow;
  const dtSec = Math.min(0.05, Math.max(0, (perfNow - prevPerf) / 1000));
  clock.smoothFramePerf = perfNow;

  const t =
    clock.clockAnchorSong +
    ((perfNow - clock.clockAnchorPerf) / 1000) * playbackRate;
  const err = audioT - t;

  if (Math.abs(err) > CLOCK_HARD_SYNC_THRESHOLD_SECONDS) {
    resetSongClock(clock, audioT, perfNow);
    return audioT;
  }

  if (Math.abs(err) <= CLOCK_MIN_CORRECTION_SECONDS) {
    return t;
  }

  const blend = 1 - Math.exp(-dtSec / CLOCK_SMOOTH_TAU_SECONDS);
  const corrected = t + err * blend;
  resetSongClock(clock, corrected, perfNow);
  return corrected;
}
