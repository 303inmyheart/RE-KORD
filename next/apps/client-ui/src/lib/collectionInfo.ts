/**
 * Subtitles of collection pages (queue, favourites, recent, playlists):
 * "12 brani · 41 min", "Brano 3 di 12 · 1 h 05 min".
 */
import { t, tp } from "./i18n.svelte";

/** "41 min", "1 h 05 min"; empty when nothing is known. */
export function formatTotalDuration(totalMs: number): string {
  if (!Number.isFinite(totalMs) || totalMs <= 0) return "";
  const totalMin = Math.max(1, Math.round(totalMs / 60_000));
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h > 0
    ? t("core.duration.hm", { h, m: String(m).padStart(2, "0") })
    : t("core.duration.m", { m });
}

export function collectionSubtitle(
  tracks: readonly { duration_ms?: number | null }[],
  opts: { position?: number | null } = {},
): string {
  const n = tracks.length;
  let total = 0;
  for (const tr of tracks) total += tr.duration_ms && tr.duration_ms > 0 ? tr.duration_ms : 0;
  const parts = [
    opts.position != null && opts.position > 0
      ? t("core.count.position", { n: opts.position, total: n })
      : tp("core.count.tracks", n),
  ];
  const duration = formatTotalDuration(total);
  if (duration) parts.push(duration);
  return parts.join(" · ");
}
