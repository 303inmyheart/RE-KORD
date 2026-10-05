/**
 * Loads the chart for whichever track is current (port of legacy
 * `useRhythmChart`): cached charts are ready at once, others are analysed with
 * an abortable request; a generation counter drops stale results when the
 * track changes faster than the analysis (no stuck "loading", no chart of the
 * previous track showing up late).
 */
import type { AnalyzableTrack, AnalyzeProgress, AnalyzeStage, RhythmErrorCode } from "./analyze";
import { sanitizeChartSetForRekord } from "./chartSanitize";
import type { ChartSet } from "./types";

export type ChartPhase = "idle" | "loading" | "ready" | "error";

export type ChartLoaderState = {
  phase: ChartPhase;
  /** Sanitized chart set (taps + holds) for `relPath`. */
  chartSet: ChartSet | null;
  relPath: string | null;
  stage: AnalyzeStage;
  progress: number;
  errorCode: RhythmErrorCode | null;
};

export type ChartLoaderDeps = {
  analyze: (
    track: AnalyzableTrack,
    onProgress?: AnalyzeProgress,
    signal?: AbortSignal,
  ) => Promise<ChartSet>;
  peekCached: (relPath: string) => ChartSet | null;
  prefetch?: (track: AnalyzableTrack) => void;
  errorCodeOf?: (err: unknown) => RhythmErrorCode;
};

export function idleChartState(): ChartLoaderState {
  return { phase: "idle", chartSet: null, relPath: null, stage: "fetch", progress: 0, errorCode: null };
}

export class ChartLoader {
  private gen = 0;
  private abort: AbortController | null = null;
  private current: ChartLoaderState = idleChartState();
  private pendingRelPath: string | null = null;
  private readonly deps: ChartLoaderDeps;
  private readonly onChange: (state: ChartLoaderState) => void;

  constructor(deps: ChartLoaderDeps, onChange: (state: ChartLoaderState) => void) {
    this.deps = deps;
    this.onChange = onChange;
  }

  get state(): ChartLoaderState {
    return this.current;
  }

  private set(next: Partial<ChartLoaderState>) {
    this.current = { ...this.current, ...next };
    this.onChange(this.current);
  }

  /** Switch to `track` (null = nothing playing). Same track = no-op unless `force`. */
  load(track: AnalyzableTrack | null, force = false): void {
    if (!track) {
      this.cancel();
      this.gen += 1;
      this.current = idleChartState();
      this.onChange(this.current);
      return;
    }
    const relPath = track.rel_path;
    if (!force) {
      const { phase } = this.current;
      if ((phase === "ready" || phase === "error") && this.current.relPath === relPath) return;
      if (phase === "loading" && this.pendingRelPath === relPath) return;
    }

    this.cancel();
    const gen = ++this.gen;
    const cached = this.deps.peekCached(relPath);
    if (cached) {
      this.pendingRelPath = null;
      this.set({
        phase: "ready",
        chartSet: sanitizeChartSetForRekord(cached),
        relPath,
        stage: "ready",
        progress: 1,
        errorCode: null,
      });
      this.deps.prefetch?.(track);
      return;
    }

    const abort = new AbortController();
    this.abort = abort;
    this.pendingRelPath = relPath;
    this.set({ phase: "loading", stage: "fetch", progress: 0, errorCode: null });
    void (async () => {
      try {
        const raw = await this.deps.analyze(
          track,
          (progress, stage) => {
            if (!abort.signal.aborted && this.gen === gen) this.set({ progress, stage });
          },
          abort.signal,
        );
        if (abort.signal.aborted || this.gen !== gen) return;
        this.pendingRelPath = null;
        this.set({
          phase: "ready",
          chartSet: sanitizeChartSetForRekord(raw),
          relPath,
          stage: "ready",
          progress: 1,
          errorCode: null,
        });
      } catch (err) {
        if (abort.signal.aborted || this.gen !== gen) return;
        this.pendingRelPath = null;
        this.set({
          phase: "error",
          relPath,
          chartSet: null,
          errorCode: this.deps.errorCodeOf?.(err) ?? "decode",
        });
      }
    })();
  }

  /** Aborts the analysis in flight (if any). */
  cancel(): void {
    this.abort?.abort();
    this.abort = null;
  }

  destroy(): void {
    this.cancel();
    this.pendingRelPath = null;
    this.gen += 1;
  }
}
