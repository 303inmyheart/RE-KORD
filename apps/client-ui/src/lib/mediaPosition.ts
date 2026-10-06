/** Pure helper of mediaSession.ts (kept apart so tests can load it). */

export type SentPosition = { duration: number; position: number; rate: number; at: number; playing: boolean };
/**
 * Whether a position update says something the OS can't extrapolate from the
 * previous one (same track length and rate, position where the clock puts it).
 */
export function positionChanged(
  prev: SentPosition | null,
  next: SentPosition,
  toleranceSec = 1.5,
): boolean {
  if (!prev) return true;
  if (prev.playing !== next.playing) return true;
  if (Math.abs(prev.duration - next.duration) > 0.5 || prev.rate !== next.rate) return true;
  const expected = prev.playing ? prev.position + ((next.at - prev.at) / 1000) * prev.rate : prev.position;
  return Math.abs(expected - next.position) > toleranceSec;
}
