/**
 * BPM from Plectr's analysis cache (legacy `estimateBpm` read the same cache).
 *
 * Plectr analyses tracks into rhythm charts whose stats carry a detected BPM.
 * The cache module is loaded lazily (dynamic import) so the Library / Dashboard
 * bundles don't pull the analysis code. `ensureChartBpmSource()` reads every
 * cached BPM (IndexedDB + memory, via `readAllCachedBpm`) into a map that
 * `chartBpm()` answers from synchronously; charts analysed after that read are
 * still found through the in-memory chart cache. Until the first read — or for
 * tracks never analysed — `chartBpm()` returns null and the nebula falls back
 * to tag BPM / the heuristic. `revision` bumps whenever the map changes so
 * models rebuild.
 */
import type { ChartSet } from "../plectr/types";

class ChartBpmSource {
  revision = $state(0);
}

export const chartBpmSource = new ChartBpmSource();

type AnalyzeModule = {
  peekCachedChart: (relPath: string) => ChartSet | null;
  readAllCachedBpm: () => Promise<Map<string, number>>;
};

let mod: AnalyzeModule | null = null;
let moduleLoad: Promise<AnalyzeModule | null> | null = null;
let refreshing: Promise<void> | null = null;
let bpmByPath = new Map<string, number>();

function loadModule(): Promise<AnalyzeModule | null> {
  moduleLoad ??= import("../plectr/analyze")
    .then((m) => {
      mod = m;
      return m as AnalyzeModule;
    })
    .catch(() => null /* Plectr unavailable: heuristic only */);
  return moduleLoad;
}

function sameMap(a: Map<string, number>, b: Map<string, number>): boolean {
  if (a.size !== b.size) return false;
  for (const [k, v] of a) if (b.get(k) !== v) return false;
  return true;
}

/** Load (or re-read) the cached BPMs; cheap to call on every mount. */
export function ensureChartBpmSource(): Promise<void> {
  refreshing ??= (async () => {
    const m = await loadModule();
    if (!m) return;
    let next: Map<string, number>;
    try {
      next = await m.readAllCachedBpm();
    } catch {
      next = bpmByPath;
    }
    if (!sameMap(next, bpmByPath) || chartBpmSource.revision === 0) {
      bpmByPath = next;
      chartBpmSource.revision += 1;
    }
  })().finally(() => {
    refreshing = null;
  });
  return refreshing;
}

function plausible(bpm: number | null | undefined): number | null {
  return bpm != null && bpm > 40 && bpm < 220 ? bpm : null;
}

export function chartBpm(relPath: string): number | null {
  const cached = plausible(bpmByPath.get(relPath));
  if (cached != null) return cached;
  const set = mod?.peekCachedChart(relPath);
  return plausible(
    set?.charts.normal?.stats.bpm ?? set?.charts.hard?.stats.bpm ?? set?.charts.easy?.stats.bpm,
  );
}
