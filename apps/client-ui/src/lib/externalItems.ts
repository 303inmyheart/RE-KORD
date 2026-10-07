/**
 * External items: podcast episodes and live streams played through the hub
 * proxy (`/api/v1/podcasts/play/<source>/<key>`), alongside library tracks.
 *
 * They ride the normal `Track` shape so the queue, the decks, the dock and
 * Media Session need no second model: the `rel_path` is a reserved
 * `ext:pod/<source>/<key>` path (never a file), the id is negative, and the
 * `external` field carries what the UI shows. Library features key off
 * `isExternalTrack` and leave them alone: no play counts, recents,
 * achievements, statistics, Plectr, favourites, exclusions, radio,
 * crossfade or hub queue sync.
 *
 * Pure and tiny on purpose: it ships in the entry chunk (the player needs it)
 * while the podcasts UI itself is lazy-loaded.
 */

export const EXTERNAL_PREFIX = "ext:pod/";

export type ExternalMeta = {
  kind: "episode" | "live";
  sourceId: number;
  key: string;
  sourceName: string;
  /** The hub has artwork for it (`/api/v1/podcasts/art/...`). */
  art: boolean;
  publishedAt?: string | null;
  mime?: string | null;
};

/** Minimal track-like shape (keeps this module free of the API types). */
type TrackLike = { rel_path: string; external?: ExternalMeta | null };

export function isExternalPath(relPath: string | null | undefined): boolean {
  return typeof relPath === "string" && relPath.startsWith(EXTERNAL_PREFIX);
}

export function isExternalTrack(track: TrackLike | null | undefined): boolean {
  return !!track && isExternalPath(track.rel_path);
}

export function isLiveTrack(track: TrackLike | null | undefined): boolean {
  return !!track && isExternalTrack(track) && track.external?.kind === "live";
}

/** Library tracks only: what play counts, recents and statistics may see. */
export function countsTowardStats(track: TrackLike | null | undefined): boolean {
  return !!track && !isExternalTrack(track);
}

export function externalRelPath(sourceId: number, key: string): string {
  return `${EXTERNAL_PREFIX}${sourceId}/${key}`;
}

/** `{ sourceId, key }` of an external path, or null. */
export function parseExternalPath(relPath: string): { sourceId: number; key: string } | null {
  if (!isExternalPath(relPath)) return null;
  const rest = relPath.slice(EXTERNAL_PREFIX.length);
  const slash = rest.indexOf("/");
  if (slash <= 0) return null;
  const sourceId = Number(rest.slice(0, slash));
  const key = rest.slice(slash + 1);
  if (!Number.isInteger(sourceId) || sourceId <= 0 || !/^[A-Za-z0-9_]{1,32}$/.test(key)) return null;
  return { sourceId, key };
}

/** Hub path of the proxied audio (relative: the caller adds the base). */
export function externalMediaPath(relPath: string): string | null {
  const p = parseExternalPath(relPath);
  return p ? `/api/v1/podcasts/play/${p.sourceId}/${encodeURIComponent(p.key)}` : null;
}

/** Hub path of the artwork thumbnail, or null when the hub has none. */
export function externalArtPath(track: TrackLike | null | undefined): string | null {
  if (!track || !isExternalTrack(track) || !track.external?.art) return null;
  const p = parseExternalPath(track.rel_path);
  return p ? `/api/v1/podcasts/art/${p.sourceId}/${encodeURIComponent(p.key)}` : null;
}

/** Stable negative id (library ids are positive; restore placeholders use -1, -2…). */
export function externalTrackId(sourceId: number, key: string): number {
  let h = 2166136261;
  const s = `${sourceId}/${key}`;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return -1_000_000 - ((h >>> 0) % 1_000_000_000);
}

export type ExternalTrackInput = {
  sourceId: number;
  sourceName: string;
  key: string;
  title: string;
  live: boolean;
  durationSecs?: number | null;
  publishedAt?: string | null;
  art?: boolean;
  mime?: string | null;
};

/** The `Track`-shaped item the player queues for an episode / live stream. */
export function makeExternalTrack(input: ExternalTrackInput) {
  return {
    id: externalTrackId(input.sourceId, input.key),
    rel_path: externalRelPath(input.sourceId, input.key),
    title: input.title,
    artist_name: input.sourceName,
    album_name: input.live ? "LIVE" : input.sourceName,
    duration_ms:
      !input.live && input.durationSecs && input.durationSecs > 0
        ? Math.round(input.durationSecs * 1000)
        : 0,
    track_number: null,
    album_id: null,
    artist_id: null,
    has_cover: !!input.art,
    external: {
      kind: input.live ? ("live" as const) : ("episode" as const),
      sourceId: input.sourceId,
      key: input.key,
      sourceName: input.sourceName,
      art: !!input.art,
      publishedAt: input.publishedAt ?? null,
      mime: input.mime ?? null,
    },
  };
}

/**
 * The queue as the hub stores it (`settings.queue`): library paths only.
 * External items stay on this device (the local session copy keeps them);
 * the cursor moves to the nearest library track before them.
 */
export function libraryQueueForSync<T extends TrackLike>(
  queue: readonly T[],
  index: number,
): { tracks: T[]; index: number; currentIsLibrary: boolean } {
  const tracks: T[] = [];
  let mapped = -1;
  for (let i = 0; i < queue.length; i++) {
    const t = queue[i]!;
    if (isExternalTrack(t)) continue;
    if (i <= index) mapped = tracks.length;
    tracks.push(t);
  }
  const current = queue[index];
  return {
    tracks,
    index: Math.max(0, mapped),
    currentIsLibrary: !!current && !isExternalTrack(current),
  };
}
