/**
 * Chart for a library track (port of legacy `analyzeLibraryTrack.ts`):
 * cache → fetch media → decode → analysis in a Web Worker (main-thread
 * fallback) → cache. In-flight requests are shared; prefetch is best effort.
 */
import { mediaUrl } from "../config";
import { analyzeLibraryBuffer } from "./audioAnalysis";
import { chartCacheKey } from "./cacheKey";
import {
  getCachedChart,
  readDiskChart,
  setCachedChart,
  writeDiskChart,
} from "./chartCache";
import { sanitizeChartSetForRekord } from "./chartSanitize";
import { MIN_NOTES_PER_DIFFICULTY } from "./config";
import { yieldUi } from "./yieldUi";
import type { AnalysisWorkerResponse } from "./analysisWorker";
import type { ChartSet } from "./types";

export type AnalyzeStage = "fetch" | "decode" | "analyze" | "ready";
export type AnalyzeProgress = (progress: number, stage: AnalyzeStage) => void;
export type RhythmErrorCode = "fetch" | "decode" | "sparse" | "timeout";

export type AnalyzableTrack = { rel_path: string; title?: string | null };

export class RhythmAnalyzeError extends Error {
  readonly code: RhythmErrorCode;

  constructor(code: RhythmErrorCode) {
    super(code);
    this.name = "RhythmAnalyzeError";
    this.code = code;
  }
}

const FETCH_TIMEOUT_MS = 120_000;

function chartIsPlayable(chartSet: ChartSet): boolean {
  return Object.values(chartSet.charts).some(
    (chart) => chart.notes.length >= MIN_NOTES_PER_DIFFICULTY,
  );
}

function mediaFetchUrl(relPath: string): string {
  const path = mediaUrl(relPath);
  try {
    return new URL(path, window.location.origin).href;
  } catch {
    return path;
  }
}

export { getCachedBpm, readAllCachedBpm } from "./bpmCache";

/** Memory-cached chart, synchronously (no disk lookup). */
export function peekCachedChart(relPath: string): ChartSet | null {
  return getCachedChart(chartCacheKey(relPath));
}

/* ── Worker plumbing ── */

let analysisWorker: Worker | null | undefined;
let workerRequestId = 0;
const workerPending = new Map<
  number,
  { resolve: (c: ChartSet) => void; reject: (e: Error) => void }
>();

function getAnalysisWorker(): Worker | null {
  if (analysisWorker !== undefined) return analysisWorker;
  try {
    analysisWorker = new Worker(new URL("./analysisWorker.ts", import.meta.url), {
      type: "module",
    });
    analysisWorker.onmessage = (event: MessageEvent<AnalysisWorkerResponse>) => {
      const pending = workerPending.get(event.data.id);
      if (!pending) return;
      workerPending.delete(event.data.id);
      if (event.data.ok) pending.resolve(event.data.chartSet);
      else pending.reject(new Error(event.data.error));
    };
    analysisWorker.onerror = () => {
      for (const pending of workerPending.values()) pending.reject(new Error("worker error"));
      workerPending.clear();
      // A broken worker stays broken: next analyses fall back to the main thread.
      analysisWorker?.terminate();
      analysisWorker = null;
    };
  } catch {
    analysisWorker = null;
  }
  return analysisWorker;
}

/**
 * Terminates the shared worker when nothing is pending (view unmounted).
 * A later analysis spawns a new one.
 */
export function releaseAnalysisWorker(): void {
  if (!analysisWorker || workerPending.size > 0 || analyzeInFlight.size > 0) return;
  analysisWorker.terminate();
  analysisWorker = undefined;
}

/** Copy channels in chunks (with yields): ~80MB at once would block the main thread. */
const COPY_CHUNK_SAMPLES = 1 << 21;

async function buildTransferChannels(buffer: AudioBuffer): Promise<ArrayBuffer[]> {
  const out: ArrayBuffer[] = [];
  for (let ch = 0; ch < buffer.numberOfChannels; ch += 1) {
    const src = buffer.getChannelData(ch);
    const dst = new Float32Array(src.length);
    for (let offset = 0; offset < src.length; offset += COPY_CHUNK_SAMPLES) {
      const end = Math.min(src.length, offset + COPY_CHUNK_SAMPLES);
      dst.set(src.subarray(offset, end), offset);
      if (end < src.length) await yieldUi();
    }
    out.push(dst.buffer);
  }
  return out;
}

function analyzeInWorker(buffer: AudioBuffer, relPath: string, title: string): Promise<ChartSet> | null {
  const worker = getAnalysisWorker();
  if (!worker) return null;
  return (async () => {
    const channels = await buildTransferChannels(buffer);
    const id = ++workerRequestId;
    return new Promise<ChartSet>((resolve, reject) => {
      workerPending.set(id, { resolve, reject });
      worker.postMessage(
        {
          id,
          relPath,
          title,
          sampleRate: buffer.sampleRate,
          duration: buffer.duration,
          length: buffer.length,
          channels,
        },
        channels,
      );
    });
  })();
}

/* ── Decode ── */

type DecodeContext = { decodeAudioData(data: ArrayBuffer): Promise<AudioBuffer>; close?: () => Promise<void> };

function createDecodeContext(): DecodeContext {
  // OfflineAudioContext needs no output device and never starts the audio
  // hardware (iOS/Android): prefer it, fall back to a regular one.
  const Offline =
    (globalThis as { OfflineAudioContext?: typeof OfflineAudioContext }).OfflineAudioContext ??
    (globalThis as { webkitOfflineAudioContext?: typeof OfflineAudioContext }).webkitOfflineAudioContext;
  if (Offline) {
    try {
      return new Offline(2, 44100, 44100);
    } catch {
      /* fall through */
    }
  }
  const Ctor =
    (globalThis as { AudioContext?: typeof AudioContext }).AudioContext ??
    (globalThis as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) throw new RhythmAnalyzeError("decode");
  return new Ctor();
}

const prefetchInFlight = new Set<string>();
const analyzeInFlight = new Map<string, Promise<ChartSet>>();

/** Warms the cache (e.g. next track in the queue). */
export function prefetchRhythmChart(track: AnalyzableTrack): void {
  const relPath = track.rel_path;
  if (peekCachedChart(relPath) || prefetchInFlight.has(relPath) || analyzeInFlight.has(relPath)) {
    return;
  }
  prefetchInFlight.add(relPath);
  void analyzeLibraryTrack(track)
    .catch(() => {
      /* best effort */
    })
    .finally(() => {
      prefetchInFlight.delete(relPath);
    });
}

/** Raw (unsanitized) chart set for a track — cached across visits. */
export async function analyzeLibraryTrack(
  track: AnalyzableTrack,
  onProgress?: AnalyzeProgress,
  signal?: AbortSignal,
): Promise<ChartSet> {
  const relPath = track.rel_path;
  const key = chartCacheKey(relPath);
  const cached = getCachedChart(key);
  if (cached) {
    onProgress?.(1, "ready");
    return cached;
  }

  const inflight = analyzeInFlight.get(relPath);
  if (inflight) return inflight;

  const run = (async (): Promise<ChartSet> => {
    const fromDisk = await readDiskChart(key);
    if (fromDisk) {
      setCachedChart(key, fromDisk);
      onProgress?.(1, "ready");
      return fromDisk;
    }

    onProgress?.(0.08, "fetch");
    const controller = new AbortController();
    const timeoutId = window.setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
    const abortHandler = () => controller.abort();
    signal?.addEventListener("abort", abortHandler);
    try {
      const response = await fetch(mediaFetchUrl(relPath), {
        cache: "force-cache",
        signal: controller.signal,
      });
      if (!response.ok) throw new RhythmAnalyzeError("fetch");
      onProgress?.(0.28, "fetch");
      const arrayBuffer = await response.arrayBuffer();
      onProgress?.(0.35, "decode");
      await yieldUi();
      const ctx = createDecodeContext();
      try {
        const buffer = await ctx.decodeAudioData(arrayBuffer);
        onProgress?.(0.55, "analyze");
        await yieldUi();
        const title = track.title || relPath;
        const viaWorker = analyzeInWorker(buffer, relPath, title);
        const chartSet = viaWorker
          ? await viaWorker.catch(() =>
              // Worker unavailable or failed: chunked analysis on the main thread.
              analyzeLibraryBuffer(buffer, relPath, title),
            )
          : await analyzeLibraryBuffer(buffer, relPath, title);
        if (!chartIsPlayable(sanitizeChartSetForRekord(chartSet))) {
          throw new RhythmAnalyzeError("sparse");
        }
        onProgress?.(1, "ready");
        setCachedChart(key, chartSet);
        void writeDiskChart(key, chartSet);
        return chartSet;
      } finally {
        if (typeof ctx.close === "function") void ctx.close().catch(() => {});
      }
    } catch (err) {
      if (signal?.aborted && !(err instanceof RhythmAnalyzeError)) {
        throw new RhythmAnalyzeError("timeout");
      }
      if (err instanceof RhythmAnalyzeError) throw err;
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new RhythmAnalyzeError("timeout");
      }
      if (err instanceof TypeError) throw new RhythmAnalyzeError("fetch");
      throw new RhythmAnalyzeError("decode");
    } finally {
      window.clearTimeout(timeoutId);
      signal?.removeEventListener("abort", abortHandler);
    }
  })();

  analyzeInFlight.set(relPath, run);
  try {
    return await run;
  } finally {
    analyzeInFlight.delete(relPath);
  }
}
