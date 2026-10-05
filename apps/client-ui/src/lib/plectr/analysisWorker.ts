/**
 * Plectr chart analysis worker: the whole DSP (mono mix, frame stats, onset,
 * BPM, note generation) runs here, off the main thread — no micro-freezes in
 * the game while the next track is prefetched. Decoding stays on the main
 * thread (no decodeAudioData in workers); channels arrive as transferred
 * ArrayBuffers.
 */
import { analyzeLibraryBuffer } from "./audioAnalysis";
import type { ChartSet } from "./types";

export type AnalysisWorkerRequest = {
  id: number;
  relPath: string;
  title: string;
  sampleRate: number;
  duration: number;
  length: number;
  channels: ArrayBuffer[];
};

export type AnalysisWorkerResponse =
  | { id: number; ok: true; chartSet: ChartSet }
  | { id: number; ok: false; error: string };

const ctx = self as unknown as {
  onmessage: ((event: MessageEvent<AnalysisWorkerRequest>) => void) | null;
  postMessage: (message: AnalysisWorkerResponse) => void;
};

ctx.onmessage = async (event: MessageEvent<AnalysisWorkerRequest>) => {
  const { id, relPath, title, sampleRate, duration, length, channels } = event.data;
  try {
    const floats = channels.map((c) => new Float32Array(c));
    const audio = {
      length,
      numberOfChannels: floats.length,
      sampleRate,
      duration,
      getChannelData: (channel: number) => floats[channel],
    };
    const chartSet = await analyzeLibraryBuffer(audio, relPath, title);
    ctx.postMessage({ id, ok: true, chartSet });
  } catch (err) {
    ctx.postMessage({ id, ok: false, error: String((err as Error)?.message || err) });
  }
};
