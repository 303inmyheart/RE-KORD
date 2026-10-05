/**
 * Chart cache: a small in-memory LRU (legacy `chartCache.ts`) backed by an
 * IndexedDB store so a track analysed once is ready instantly on the next
 * visit too (the legacy client only had the in-memory layer).
 */
import type { ChartSet } from "./types";

const MAX_MEMORY_ENTRIES = 8;
const MAX_DISK_ENTRIES = 60;
/** Bump to invalidate stale charts (holds kept on every level = 4, as legacy). */
export const CHART_CACHE_GENERATION = 4;

const DB_NAME = "rekord-plectr";
const DB_VERSION = 1;
const STORE = "charts";

type MemoryEntry = { gen: number; chartSet: ChartSet };
type DiskEntry = { key: string; gen: number; touchedAt: number; chartSet: ChartSet };

const memory = new Map<string, MemoryEntry>();

export function getCachedChart(key: string): ChartSet | null {
  const hit = memory.get(key);
  if (!hit || hit.gen !== CHART_CACHE_GENERATION) {
    if (hit) memory.delete(key);
    return null;
  }
  memory.delete(key);
  memory.set(key, hit);
  return hit.chartSet;
}

/** Like `getCachedChart` but read-only: does not refresh the LRU order. */
export function peekMemoryChart(key: string): ChartSet | null {
  const hit = memory.get(key);
  return hit && hit.gen === CHART_CACHE_GENERATION ? hit.chartSet : null;
}

export function setCachedChart(key: string, chartSet: ChartSet): void {
  if (memory.has(key)) memory.delete(key);
  memory.set(key, { gen: CHART_CACHE_GENERATION, chartSet });
  while (memory.size > MAX_MEMORY_ENTRIES) {
    const oldest = memory.keys().next().value;
    if (oldest === undefined) break;
    memory.delete(oldest);
  }
}

export function clearMemoryChartCache(): void {
  memory.clear();
}

/* ── IndexedDB layer (best effort: private mode / old WebViews just skip it) ── */

let dbPromise: Promise<IDBDatabase | null> | null = null;

function openDb(): Promise<IDBDatabase | null> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve) => {
    try {
      if (typeof indexedDB === "undefined") {
        resolve(null);
        return;
      }
      const req = indexedDB.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        if (!db.objectStoreNames.contains(STORE)) {
          const store = db.createObjectStore(STORE, { keyPath: "key" });
          store.createIndex("touchedAt", "touchedAt");
        }
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => resolve(null);
      req.onblocked = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
  return dbPromise;
}

function promisify<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function readDiskChart(key: string): Promise<ChartSet | null> {
  const db = await openDb();
  if (!db) return null;
  try {
    const tx = db.transaction(STORE, "readwrite");
    const store = tx.objectStore(STORE);
    const entry = (await promisify(store.get(key))) as DiskEntry | undefined;
    if (!entry) return null;
    if (entry.gen !== CHART_CACHE_GENERATION || !entry.chartSet?.charts) {
      store.delete(key);
      return null;
    }
    entry.touchedAt = Date.now();
    store.put(entry);
    return entry.chartSet;
  } catch {
    return null;
  }
}

export async function writeDiskChart(key: string, chartSet: ChartSet): Promise<void> {
  const db = await openDb();
  if (!db) return;
  try {
    const tx = db.transaction(STORE, "readwrite");
    const store = tx.objectStore(STORE);
    const entry: DiskEntry = {
      key,
      gen: CHART_CACHE_GENERATION,
      touchedAt: Date.now(),
      chartSet,
    };
    store.put(entry);
    const count = await promisify(store.count());
    if (count > MAX_DISK_ENTRIES) {
      let excess = count - MAX_DISK_ENTRIES;
      const cursorReq = store.index("touchedAt").openCursor();
      cursorReq.onsuccess = () => {
        const cursor = cursorReq.result;
        if (!cursor || excess <= 0) return;
        cursor.delete();
        excess -= 1;
        cursor.continue();
      };
    }
  } catch {
    /* quota / closed db: the in-memory layer still works */
  }
}

/** Every valid chart on disk (read-only; for lookups such as BPM). */
export async function listDiskCharts(): Promise<{ key: string; chartSet: ChartSet }[]> {
  const db = await openDb();
  if (!db) return [];
  try {
    const tx = db.transaction(STORE, "readonly");
    const all = (await promisify(tx.objectStore(STORE).getAll())) as DiskEntry[];
    const out: { key: string; chartSet: ChartSet }[] = [];
    for (const entry of all) {
      if (entry?.gen === CHART_CACHE_GENERATION && entry.chartSet?.charts) {
        out.push({ key: entry.key, chartSet: entry.chartSet });
      }
    }
    // Memory may hold charts whose disk write has not landed yet.
    for (const [key, hit] of memory) {
      if (hit.gen === CHART_CACHE_GENERATION && !out.some((e) => e.key === key)) {
        out.push({ key, chartSet: hit.chartSet });
      }
    }
    return out;
  } catch {
    return [...memory].map(([key, hit]) => ({ key, chartSet: hit.chartSet }));
  }
}

/** Drops every cached chart (memory + disk). */
export async function clearChartCache(): Promise<void> {
  clearMemoryChartCache();
  const db = await openDb();
  if (!db) return;
  try {
    const tx = db.transaction(STORE, "readwrite");
    await promisify(tx.objectStore(STORE).clear());
  } catch {
    /* ignore */
  }
}
