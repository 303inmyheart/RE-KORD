/**
 * The account's queue as stored in its hub user-state settings (pure, no
 * stores: tested in node).
 */
export type HubQueueSnapshot = {
  relPaths: string[];
  index: number;
  relPath: string | null;
  time: number;
  updatedAt: number;
};

/**
 * The account's queue from its hub settings: `queue` (list, maybe with its
 * own cursor) and `queueCursor` (index + position, updated more often). The
 * newer cursor wins when it still points into the list.
 */
export function parseHubQueue(rawQueue: unknown, rawCursor: unknown): HubQueueSnapshot | null {
  if (!rawQueue || typeof rawQueue !== "object") return null;
  const q = rawQueue as Record<string, unknown>;
  const relPaths = Array.isArray(q.relPaths)
    ? q.relPaths.filter((p): p is string => typeof p === "string" && p.length > 0)
    : [];
  if (!relPaths.length) return null;
  const num = (v: unknown, fallback = 0) => {
    const n = Number(v);
    return Number.isFinite(n) && n >= 0 ? n : fallback;
  };
  let index = Math.min(Math.floor(num(q.currentIndex ?? q.index)), relPaths.length - 1);
  let relPath = typeof q.relPath === "string" ? q.relPath : (relPaths[index] ?? null);
  let time = num(q.time);
  let updatedAt = num(q.updatedAt);
  if (rawCursor && typeof rawCursor === "object") {
    const c = rawCursor as Record<string, unknown>;
    const cIndex = Math.floor(num(c.currentIndex ?? c.index, -1));
    const cPath = typeof c.relPath === "string" ? c.relPath : null;
    const cAt = num(c.updatedAt);
    if (cAt >= updatedAt && cIndex >= 0 && cIndex < relPaths.length && relPaths[cIndex] === cPath) {
      index = cIndex;
      relPath = cPath;
      time = num(c.time);
      updatedAt = cAt;
    }
  }
  if (relPath !== relPaths[index]) {
    relPath = relPaths[index] ?? null;
    time = 0;
  }
  return { relPaths, index, relPath, time, updatedAt };
}

