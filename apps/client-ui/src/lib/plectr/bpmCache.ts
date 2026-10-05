/**
 * Read-only BPM lookup from the Plectr chart cache (for other surfaces, e.g.
 * Sonic Nebula, that want the real tempo of tracks Plectr already analysed).
 * Nothing is analysed here: tracks never played in Plectr have no entry.
 */
import { chartCacheKey, chartCacheScopePrefix } from "./cacheKey";
import { listDiskCharts, peekMemoryChart } from "./chartCache";
import type { ChartSet } from "./types";

function bpmOf(chartSet: ChartSet | null | undefined): number | undefined {
  if (!chartSet?.charts) return undefined;
  for (const chart of Object.values(chartSet.charts)) {
    const bpm = chart?.stats?.bpm;
    if (typeof bpm === "number" && Number.isFinite(bpm) && bpm > 0) return bpm;
  }
  return undefined;
}

/** BPM of a track whose chart is in the in-memory cache (synchronous, no LRU touch). */
export function getCachedBpm(relPath: string): number | undefined {
  return bpmOf(peekMemoryChart(chartCacheKey(relPath)));
}

/**
 * rel_path → BPM for every chart cached on this device for the current hub
 * (IndexedDB + memory). Side-effect free: no analysis, no LRU refresh.
 */
export async function readAllCachedBpm(): Promise<Map<string, number>> {
  const prefix = chartCacheScopePrefix();
  const out = new Map<string, number>();
  for (const { key, chartSet } of await listDiskCharts()) {
    if (!key.startsWith(prefix)) continue;
    const bpm = bpmOf(chartSet);
    if (bpm !== undefined) out.set(key.slice(prefix.length), bpm);
  }
  return out;
}
