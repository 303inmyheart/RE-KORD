<script lang="ts" module>
  import type { DifficultyId, GameResult } from "../../lib/plectr/types";

  export type RunEndReason = "end" | "interrupted";
  /**
   * loading: no chart yet · live: the song plays, notes fall · paused: the
   * song is paused (notes frozen on it) · done: the chart is over.
   */
  export type StagePhase = "loading" | "live" | "paused" | "done";
  /** user: our pause button / key · external: dock, media keys… · idle: never started. */
  export type PauseReason = "user" | "external" | "idle";

  export type RunReport = {
    result: GameResult;
    relPath: string;
    difficulty: DifficultyId;
    perfects: number;
    goods: number;
    oks: number;
    earlies: number;
    lates: number;
    skipped: number;
    /** Skipped by joining late or seeking (not the resume grace). */
    jumped: number;
    /** Notes judged (skipped notes excluded; a dropped hold counts once). */
    judged: number;
    /** Notes played cleanly (a dropped hold is not one). */
    notesHit: number;
    totalNotes: number;
    reason: RunEndReason;
    /** Started at the top of the song. */
    fromStart: boolean;
    failed: boolean;
    fc: boolean;
    ap: boolean;
  };

  export type LiveStats = {
    perfects: number;
    goods: number;
    earlies: number;
    lates: number;
    misses: number;
    skipped: number;
    combo: number;
    maxCombo: number;
    judged: number;
    totalNotes: number;
    accuracy: number;
    score: number;
  };
</script>

<script lang="ts">
  /**
   * Plectr stage, legacy dock behaviour: the run follows the song the global
   * player is playing, from wherever it is — no seek, no restart, no
   * countdown. Pausing pauses the song (the notes freeze on it), resuming
   * plays it again at once. A difficulty change starts a new run from the
   * current position. The stage never stops or restarts the song by itself.
   *
   * Performance (WebKitGTK): one requestAnimationFrame loop that sleeps while
   * the song is paused, a pre-composed static layer + note sprites, HUD text
   * written straight to the DOM on change (no Svelte state per frame).
   */
  import { onMount, untrack, type Snippet } from "svelte";
  import { pushBackLayer } from "@rekord/ui";
  import UiIcon from "../icons/UiIcon.svelte";
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import { SHORTCUTS_OFF_ATTR } from "../../lib/shortcutList";
  import { PlectrBackdrop } from "../../lib/plectr/backdrop";
  import {
    CHALLENGE_FAIL_ACCURACY,
    CHALLENGE_MIN_JUDGED,
    DIFFICULTIES,
    GRACE_SECONDS,
    HIT_WINDOWS,
    LANES,
    NOTE_SPEED,
    STAGE_BG,
  } from "../../lib/plectr/config";
  import {
    activeFlash,
    applyMisses,
    completeHeldNotes,
    initialRunState,
    isChartRunComplete,
    isHoldingLane,
    judgedNotes,
    pressLane,
    releaseLane,
    runResultOf,
    skipNotesBefore,
    startGrace,
    type FeedbackCode,
    type JudgeEnv,
    type RunState,
  } from "../../lib/plectr/engine";
  import { COMBO_LABEL_FROM, feedbackView } from "../../lib/plectr/feedback";
  import { createPlayerBridge, type PlectrPlayerBridge } from "../../lib/plectr/playerBridge";
  import type { StageBackdrop } from "../../lib/plectr/records";
  import { NoteSprites, StageLayer, drawLanes, drawNotes, hitLineY, type DrawContext, type StageArt } from "../../lib/plectr/renderer";
  import { resetSongClock, resolveSmoothSongTime } from "../../lib/plectr/smoothSongClock";
  import { FpsWatch } from "../../lib/plectr/stageQuality";
  import { gameTimeFromAudio } from "../../lib/plectr/timing";
  import type { Chart } from "../../lib/plectr/types";
  import { canvasDprCap, prefersReducedMotion } from "../../lib/visualizer/renderQuality";
  import type { VizMode } from "../../lib/visualizer/vizCanvasEngine";

  let {
    chart,
    relPath,
    title = "",
    /** Bump to start a new run on the same chart (replay). */
    startToken = 0,
    /** Note speed, px/s (legacy 280 at 1.0x, same on every screen). */
    noteSpeed = NOTE_SPEED,
    latencyMs = 0,
    keys = ["d", "f", "j", "k"],
    keyLetters = true,
    vibration = true,
    challenge = false,
    light = false,
    backdrop = "art",
    art = { image: null, tint: null },
    vizMode = "bars",
    difficulty,
    playable = [],
    best = null,
    lastRun = null,
    /** Phone immersive layout. */
    compact = false,
    /** A dialog (settings, results) covers the stage: no judging, no misses. */
    covered = false,
    /** Watch the frame rate and report a slow device once. */
    watchFps = false,
    overlay,
    onphase,
    onfinish,
    onstats,
    onlowfps,
    ondifficulty,
    onsettings,
    onexit,
  }: {
    chart: Chart | null;
    relPath: string;
    title?: string;
    startToken?: number;
    noteSpeed?: number;
    latencyMs?: number;
    keys?: string[];
    keyLetters?: boolean;
    vibration?: boolean;
    challenge?: boolean;
    light?: boolean;
    backdrop?: StageBackdrop;
    art?: StageArt;
    vizMode?: VizMode;
    difficulty: DifficultyId;
    playable?: DifficultyId[];
    best?: { score: number; grade: string } | null;
    lastRun?: { score: number; grade: string } | null;
    compact?: boolean;
    covered?: boolean;
    watchFps?: boolean;
    /** Status / pause / results layer drawn over the lanes (header stays usable). */
    overlay?: Snippet;
    onphase?: (phase: StagePhase, reason?: PauseReason) => void;
    onfinish?: (report: RunReport) => void;
    onstats?: (stats: LiveStats) => void;
    onlowfps?: () => void;
    ondifficulty?: (id: DifficultyId) => void;
    onsettings?: () => void;
    onexit?: () => void;
  } = $props();

  let rootEl = $state<HTMLElement | null>(null);
  let lanesEl = $state<HTMLElement | null>(null);
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let scoreEl = $state<HTMLElement | null>(null);
  let comboEl = $state<HTMLElement | null>(null);
  let progressEl = $state<HTMLElement | null>(null);
  let padEls: HTMLElement[] = [];

  let phase = $state<StagePhase>("loading");
  /** The song plays (drives the pause / play button, also after the chart ends). */
  let songPlaying = $state(false);
  /** Event-rate HUD state (changes on judgements, never per frame). */
  let judge = $state<{ code: FeedbackCode; pulse: number; combo: number; milestone: number; milestonePulse: number }>({
    code: "ready",
    pulse: 0,
    combo: 0,
    milestone: 0,
    milestonePulse: 0,
  });
  /** Difficulty name flashed in the centre after a live switch. */
  let diffFlash = $state<{ id: DifficultyId; n: number } | null>(null);
  const fb = $derived(feedbackView(judge.code));
  const keyLabels = $derived(keys.map(keyLabel));
  const record = $derived(lastRun ? { last: true, r: lastRun } : best ? { last: false, r: best } : null);

  /* ── Non-reactive game state (touched every frame) ── */
  let run: RunState | null = null;
  let idleState: RunState = initialRunState([]);
  let bridge: PlectrPlayerBridge | null = null;
  let runChart: Chart | null = null;
  let runRelPath = "";
  let runFromStart = false;
  let raf = 0;
  let mounted = false;
  let ctx2d: CanvasRenderingContext2D | null = null;
  let lastAudioTime = 0;
  /** performance.now() of `lastAudioTime` (seek detection vs a slow frame). */
  let lastAudioPerf = 0;
  /** The song played at least once during this run (else a pause is "idle"). */
  let runHeardAudio = false;
  /** The song played on the previous frame (resume detection). */
  let wasPlaying = false;
  let pauseRequestedAt = 0;
  /** Skip what scrolled by unseen (tab hidden, stage covered) on the next frame. */
  let resyncPending = false;
  let watchedAudio: HTMLAudioElement | null = null;
  let hudScore = -1;
  let hudCombo = -1;
  let hudPulse = -1;
  let hudMilestone = -1;
  let progressAt = 0;
  let statsAt = 0;
  let padMask = -1;
  let layout = { width: 0, height: 0, dpr: 1, hitY: 0, speed: 1 };
  const layer = new StageLayer();
  const sprites = new NoteSprites();
  const backdropViz = new PlectrBackdrop();
  const fps = new FpsWatch();
  /** The analyser only while a spectrum backdrop is drawn for a playing song. */
  const analyserLease = player.analyserLease();
  /** pointerId → lane, for multi-touch and sliding between lanes. */
  const pointerLanes = new Map<number, number>();
  const keyLanes = new Set<number>();
  const reducedMotion = prefersReducedMotion();
  const judgeEnv: JudgeEnv = { now: 0, onMiss: () => vibrate(45), onMilestone: () => vibrate(12) };
  const drawCtx: DrawContext = {
    cssWidth: 0,
    cssHeight: 0,
    dpr: 1,
    hitY: 0,
    laneWidth: 0,
    speed: 1,
    songTime: 0,
    state: idleState,
    now: 0,
    light: false,
    vizUnderlay: false,
  };

  function keyLabel(key: string): string {
    switch (key) {
      case "arrowleft":
        return "←";
      case "arrowright":
        return "→";
      case "arrowup":
        return "↑";
      case "arrowdown":
        return "↓";
      default:
        return key.length === 1 ? key.toUpperCase() : key.slice(0, 3).toUpperCase();
    }
  }

  function vibrate(ms: number) {
    if (!vibration || typeof navigator === "undefined" || !("vibrate" in navigator)) return;
    // Phones / tablets only: desktops have no motor (and some engines prompt).
    if (!matchMedia("(pointer: coarse)").matches) return;
    try {
      navigator.vibrate(ms);
    } catch {
      /* not allowed */
    }
  }

  function env(now = performance.now()): JudgeEnv {
    judgeEnv.now = now;
    return judgeEnv;
  }

  /* ── Clock ── */

  function isAudioPlaying(): boolean {
    const audio = bridge?.getAudio();
    return Boolean(audio && !audio.paused && !audio.ended);
  }

  function clockNow(now: number): number {
    const s = run;
    const b = bridge;
    if (!s || !b) return 0;
    return gameTimeFromAudio(resolveSmoothSongTime(s, now, b), latencyMs);
  }

  /** Keep the smooth clock on the player across seeks / play / pause (legacy). */
  function onAudioEvent() {
    const s = run;
    const b = bridge;
    if (s && b) resetSongClock(s, b.getCurrentTime(), performance.now());
    schedule();
  }

  function watchAudio() {
    const el = bridge?.getAudio();
    const next = el instanceof HTMLAudioElement ? el : null;
    if (next === watchedAudio) return;
    for (const type of ["play", "pause", "seeking", "seeked", "ratechange"] as const) {
      watchedAudio?.removeEventListener(type, onAudioEvent);
      next?.addEventListener(type, onAudioEvent);
    }
    watchedAudio = next;
  }

  /* ── Run lifecycle ── */

  function totalNotes(): number {
    return runChart?.notes.length ?? 0;
  }

  function report(reason: RunEndReason, failed = false): RunReport | null {
    const s = run;
    if (!s || !runChart) return null;
    const judged = judgedNotes(s);
    const total = totalNotes();
    const clean = reason === "end" && !failed && s.skipped === 0 && s.misses === 0 && s.hits === total && total > 0;
    return {
      result: runResultOf(s, failed),
      relPath: runRelPath,
      difficulty: runChart.difficulty.id,
      perfects: s.perfects,
      goods: s.goods,
      oks: s.oks,
      earlies: s.earlies,
      lates: s.lates,
      skipped: s.skipped,
      jumped: s.jumped,
      judged,
      notesHit: s.hits - s.holdFails,
      totalNotes: total,
      reason,
      fromStart: runFromStart,
      failed,
      fc: clean,
      ap: clean && s.perfects === total,
    };
  }

  function finishRun(reason: RunEndReason, failed = false) {
    const s = run;
    if (!s || s.finished) return;
    s.finished = true;
    releaseAll();
    const r = report(reason, failed);
    syncHud(true);
    if (r && (judgedNotes(s) > 0 || reason === "end")) onfinish?.(r);
    updatePhase(performance.now());
  }

  /** The run being left (track change, repeat): did it get to the end of the song? */
  function reachedEnd(s: RunState): boolean {
    const d = bridge?.getDuration() || runChart?.duration || 0;
    return s.songTime >= s.lastEnd - 1 || (d > 0 && d - s.songTime <= 15);
  }

  function startRun(nextChart: Chart, nextRelPath: string) {
    if (run && !run.finished) finishRun(runRelPath !== nextRelPath && reachedEnd(run) ? "end" : "interrupted");
    runChart = nextChart;
    runRelPath = nextRelPath;
    bridge = createPlayerBridge(nextRelPath);
    const now = performance.now();
    const from = bridge.isOnTrack() ? bridge.getCurrentTime() : 0;
    const s = initialRunState(nextChart.notes);
    run = s;
    const target = gameTimeFromAudio(from, latencyMs);
    runFromStart = from < 0.5;
    if (!runFromStart) {
      // Joining mid-song (opening Plectr on a playing song, difficulty
      // change): what is behind is not a miss, the first notes are on the house.
      skipNotesBefore(s, target);
      startGrace(s, target, GRACE_SECONDS);
    }
    s.started = true;
    s.songTime = target;
    resetSongClock(s, from, now);
    lastAudioTime = from;
    lastAudioPerf = 0;
    runHeardAudio = isAudioPlaying();
    wasPlaying = runHeardAudio;
    resyncPending = false;
    for (const lane of keyLanes) s.pressedLanes[lane] = true;
    fps.reset(now);
    watchAudio();
    syncHud(true);
    updatePhase(now);
    schedule();
  }

  /** Pause = pause the song (the notes freeze on it). */
  export function pause() {
    pauseRequestedAt = performance.now();
    releaseAll();
    if (player.playing) player.pause();
    schedule();
  }

  /** Resume = play the song again, right away (no countdown). */
  export function resume() {
    if (!player.current) return;
    if (!player.playing) void player.toggle();
    schedule();
  }

  export function isPaused(): boolean {
    return phase === "paused";
  }

  /** The song itself: pause it when it plays, play it otherwise. */
  function togglePause() {
    if (songPlaying) pause();
    else resume();
  }

  function updatePhase(now: number) {
    const audible = isAudioPlaying();
    if (audible !== songPlaying) songPlaying = audible;
    const s = run;
    let next: StagePhase;
    let reason: PauseReason | undefined;
    if (!chart || !s) next = "loading";
    else if (s.finished) next = "done";
    else if (isAudioPlaying()) next = "live";
    else {
      next = "paused";
      reason = !runHeardAudio ? "idle" : now - pauseRequestedAt < 1500 ? "user" : "external";
    }
    if (next === phase) return;
    phase = next;
    onphase?.(next, reason);
  }

  /* ── HUD (DOM writes on change only) ── */

  function liveStats(s: RunState): LiveStats {
    const judged = judgedNotes(s);
    return {
      perfects: s.perfects,
      goods: s.goods,
      earlies: s.earlies,
      lates: s.lates,
      misses: s.misses,
      skipped: s.skipped,
      combo: s.combo,
      maxCombo: s.maxCombo,
      judged,
      totalNotes: totalNotes(),
      // Same formula as the results card (`buildGameResult`): hits / (hits + misses).
      accuracy: s.hits + s.misses ? s.hits / (s.hits + s.misses) : 0,
      score: s.score,
    };
  }

  function syncHud(force = false) {
    const s = run ?? idleState;
    const now = performance.now();
    if (force || s.score !== hudScore) {
      hudScore = s.score;
      if (scoreEl) scoreEl.textContent = fmtNumber(s.score);
    }
    if (force || s.combo !== hudCombo) {
      hudCombo = s.combo;
      if (comboEl) comboEl.textContent = `${s.combo}x`;
    }
    if (force || s.feedbackPulse !== hudPulse || s.milestonePulse !== hudMilestone || s.combo !== judge.combo) {
      hudPulse = s.feedbackPulse;
      hudMilestone = s.milestonePulse;
      judge = {
        code: s.feedback,
        pulse: s.feedbackPulse,
        combo: s.combo,
        milestone: s.milestone,
        milestonePulse: s.milestonePulse,
      };
    }
    if (force || now - progressAt > 250) {
      progressAt = now;
      const d = bridge?.getDuration() || runChart?.duration || 0;
      const p = d > 0 ? Math.min(1, Math.max(0, s.songTime / d)) : 0;
      if (progressEl) progressEl.style.transform = `scaleX(${p.toFixed(4)})`;
    }
    if (onstats && run && (force || now - statsAt > 300)) {
      statsAt = now;
      onstats(liveStats(run));
    }
  }

  function syncPads(now: number) {
    const s = run ?? idleState;
    let mask = 0;
    for (let i = 0; i < LANES.length; i += 1) {
      const holding = s.activeHolds.length > 0 && isHoldingLane(s, i);
      const flash = activeFlash(s, i, now);
      const bits = (s.pressedLanes[i] || holding ? 1 : 0) | (holding ? 2 : 0) | (flash === "hit" ? 4 : flash === "miss" ? 8 : 0);
      mask |= bits << (i * 4);
    }
    if (mask === padMask) return;
    padMask = mask;
    for (let i = 0; i < LANES.length; i += 1) {
      const pad = padEls[i];
      if (!pad) continue;
      const bits = (mask >> (i * 4)) & 15;
      pad.classList.toggle("is-pressed", (bits & 1) !== 0);
      pad.classList.toggle("is-holding", (bits & 2) !== 0);
      pad.classList.toggle("is-hit", (bits & 4) !== 0);
      pad.classList.toggle("is-miss", (bits & 8) !== 0);
    }
  }

  /* ── Drawing ── */

  function measure() {
    const canvas = canvasEl;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const dpr = light ? 1 : Math.min(2, canvasDprCap({ lite: true }));
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    const hitY = hitLineY(height);
    const speed = noteSpeed;
    layout = { width, height, dpr, hitY, speed };
    const bw = Math.max(1, Math.round(width * dpr));
    const bh = Math.max(1, Math.round(height * dpr));
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
      ctx2d = null;
    }
    // For QA bots / tests: where the hit line is and how fast notes travel.
    canvas.dataset.hitY = String(Math.round(hitY));
    canvas.dataset.speed = String(Math.round(speed));
  }

  function useViz(): boolean {
    return !light && backdrop === "bars" && vizMode !== "karaoke";
  }

  function draw(now: number, songTime: number) {
    const canvas = canvasEl;
    if (!canvas) return;
    if (layout.width < 2) measure();
    ctx2d ??= canvas.getContext("2d", { alpha: false });
    const ctx = ctx2d;
    if (!ctx) return;
    const { width, height, dpr, hitY, speed } = layout;
    const viz = useViz();
    const playing = isAudioPlaying();
    analyserLease.set(viz && playing);
    layer.configure(width, height, dpr, hitY, light || backdrop !== "art" ? { image: null, tint: art.tint } : art, viz);
    if (viz) {
      const f = backdropViz.frame(
        width,
        height,
        { mode: vizMode, analyser: playing ? player.getAnalyser() : null, isPlaying: playing },
        STAGE_BG,
      );
      if (f) layer.compose(f.canvas, f.stamp);
    }
    if (!layer.blit(ctx)) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = STAGE_BG;
      ctx.fillRect(0, 0, width, height);
    }
    sprites.configure(width / LANES.length, dpr, light);
    const d = drawCtx;
    d.cssWidth = width;
    d.cssHeight = height;
    d.dpr = dpr;
    d.hitY = hitY;
    d.laneWidth = width / LANES.length;
    d.speed = speed;
    d.songTime = songTime;
    d.state = run ?? idleState;
    d.now = now;
    d.light = light;
    d.vizUnderlay = viz;
    drawLanes(ctx, d);
    if (run) drawNotes(ctx, d, sprites);
  }

  /* ── Loop ── */

  function frame(now: number) {
    raf = 0;
    if (!canvasEl || !mounted) return;
    now = performance.now();
    const s = run;
    const b = bridge;
    if (!s || !b) {
      draw(now, 0);
      syncPads(now);
      updatePhase(now);
      if (isAudioPlaying() || player.playing) schedule();
      return;
    }
    watchAudio();
    const playing = isAudioPlaying();
    if (playing) runHeardAudio = true;
    if (!s.finished && b.isOnTrack()) {
      if (playing) stepLive(now);
      else if (nearTrackEnd(b.getCurrentTime()) && (b.getAudio()?.ended || !player.playing) && runHeardAudio) {
        // The song ended and the player stopped (end of the queue).
        finishRun("end");
      }
    }
    if (run !== s) return; // a seek back started a new run
    const songTime = s.finished ? s.songTime : clockNow(now);
    if (!s.finished) {
      s.songTime = songTime;
      // Resumed: no countdown, but a note due right now is not a miss yet.
      if (playing && !wasPlaying) startGrace(s, songTime, 0.75);
      if (resyncPending || covered) {
        // Hidden / covered: notes that went by unseen are skipped, not missed.
        resyncPending = false;
        skipNotesBefore(s, songTime + (covered ? 0.4 : 0));
        startGrace(s, songTime, covered ? 0.6 : 1);
      }
      if (playing) {
        const e = env(now);
        applyMisses(s, songTime, e);
        completeHeldNotes(s, songTime, e);
      }
      if (watchFps && playing && fps.tick(now)) onlowfps?.();
    }
    wasPlaying = playing;
    draw(now, songTime);
    syncPads(now);
    syncHud();
    if (!s.finished) checkEnd(s, songTime);
    updatePhase(now);
    if (playing || pointerLanes.size || keyLanes.size || flashing(s, now)) schedule();
  }

  function flashing(s: RunState, now: number): boolean {
    for (let i = 0; i < LANES.length; i += 1) if (activeFlash(s, i, now)) return true;
    return false;
  }

  function stepLive(now: number) {
    const s = run;
    const b = bridge;
    if (!s || !b) return;
    const audioNow = b.getCurrentTime();
    if (audioNow < lastAudioTime - 1.5) {
      // Seek back (or the track looped): a fresh run from here.
      const looped = audioNow < 2 && nearTrackEnd(lastAudioTime);
      finishRun(looped ? "end" : "interrupted");
      if (runChart) startRun(runChart, runRelPath);
      return;
    }
    const wall = lastAudioPerf ? (now - lastAudioPerf) / 1000 : 0;
    if (lastAudioPerf && audioNow - lastAudioTime - wall > 0.6) {
      // Seek forward: skipped notes are not misses; grace after the jump.
      const target = gameTimeFromAudio(audioNow, latencyMs);
      skipNotesBefore(s, target);
      startGrace(s, target, GRACE_SECONDS);
    }
    if (Math.abs(audioNow - lastAudioTime) >= 1e-4 || !lastAudioPerf) {
      lastAudioTime = audioNow;
      lastAudioPerf = now;
    }
  }

  function nearTrackEnd(audioTime: number): boolean {
    const d = bridge?.getDuration() || runChart?.duration || 0;
    return d > 0 && d - audioTime <= 1.2;
  }

  function checkEnd(s: RunState, songTime: number) {
    if (challenge) {
      const judged = judgedNotes(s);
      if (judged >= CHALLENGE_MIN_JUDGED && s.hits / judged < CHALLENGE_FAIL_ACCURACY) {
        // Failed: the run ends, the song keeps playing (legacy never stopped it).
        finishRun("end", true);
        return;
      }
    }
    if (!s.awaitingTrackEnd && isChartRunComplete(s, songTime)) s.awaitingTrackEnd = true;
    // Chart done (+ a breath): results now, the song plays on.
    if (s.awaitingTrackEnd && songTime >= s.lastEnd + 1.2) finishRun("end");
  }

  function schedule() {
    if (raf || !mounted || typeof document === "undefined" || document.hidden) return;
    raf = requestAnimationFrame(frame);
  }

  /* ── Input ── */

  function canJudge(): boolean {
    const s = run;
    return !!s && !s.finished && !covered && phase === "live";
  }

  function press(lane: number) {
    const s = run;
    if (!s || !canJudge()) return;
    const now = performance.now();
    // Judge on the freshest clock, not the last frame's.
    s.songTime = clockNow(now);
    pressLane(s, lane, env(now));
    syncPads(now);
    syncHud();
    schedule();
  }

  function release(lane: number) {
    const s = run;
    if (!s) return;
    if (!canJudge()) {
      s.pressedLanes[lane] = false;
      syncPads(performance.now());
      return;
    }
    const now = performance.now();
    s.songTime = clockNow(now);
    releaseLane(s, lane, env(now));
    syncPads(now);
    syncHud();
    schedule();
  }

  function laneAt(clientX: number): number {
    const rect = lanesEl?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return 0;
    const ix = Math.floor(((clientX - rect.left) / rect.width) * LANES.length);
    return Math.min(LANES.length - 1, Math.max(0, ix));
  }

  function inOverlay(target: EventTarget | null): boolean {
    return target instanceof Element && !!target.closest(".plectr-overlay");
  }

  function onPointerDown(event: PointerEvent) {
    // Overlays (pause card, results, status) keep their own buttons.
    if (inOverlay(event.target)) return;
    // The lanes own every press: no focus change, no click-through, no menu.
    event.preventDefault();
    if (event.button !== 0 && event.pointerType === "mouse") return;
    const lane = laneAt(event.clientX);
    pointerLanes.set(event.pointerId, lane);
    try {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    } catch {
      /* synthetic pointer */
    }
    press(lane);
  }

  function laneHeldElsewhere(lane: number): boolean {
    if (keyLanes.has(lane)) return true;
    for (const l of pointerLanes.values()) if (l === lane) return true;
    return false;
  }

  function onPointerMove(event: PointerEvent) {
    const prev = pointerLanes.get(event.pointerId);
    if (prev === undefined) return;
    const lane = laneAt(event.clientX);
    if (lane === prev) return;
    // Sliding a finger across lanes: let go of the old one, press the new one.
    pointerLanes.set(event.pointerId, lane);
    if (!laneHeldElsewhere(prev)) release(prev);
    if (run && !run.pressedLanes[lane]) press(lane);
  }

  function onPointerUp(event: PointerEvent) {
    const lane = pointerLanes.get(event.pointerId);
    if (lane === undefined) return;
    pointerLanes.delete(event.pointerId);
    if (!laneHeldElsewhere(lane)) release(lane);
  }

  /** Touch: no synthetic click / double-tap zoom / scroll from the lanes. */
  function onTouch(event: TouchEvent) {
    if (inOverlay(event.target)) return;
    if (event.target instanceof Element && event.target.closest(".plectr-lanes")) event.preventDefault();
  }

  function isTypingTarget(el: EventTarget | null): boolean {
    if (!(el instanceof HTMLElement)) return false;
    const tag = el.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
  }

  function modalOpen(): boolean {
    return typeof document !== "undefined" && !!document.querySelector(".rk-modal-back");
  }

  function laneForKey(event: KeyboardEvent): number {
    return keys.indexOf(event.key.toLowerCase());
  }

  function switchDifficulty(id: DifficultyId) {
    if (id === difficulty || !playable.includes(id)) return;
    ondifficulty?.(id);
    diffFlash = { id, n: (diffFlash?.n ?? 0) + 1 };
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (isTypingTarget(event.target) || modalOpen()) return;
    const isSpace = event.key === " " || event.code === "Space";
    if ((isSpace || event.key === "Escape") && !event.repeat) {
      // Buttons keep Space / Enter for themselves (pause card, results).
      if (isSpace && event.target instanceof HTMLElement && event.target.closest("button, a, [role=button], [role=radio]")) {
        return;
      }
      if (covered) return;
      // Esc pauses; it never starts the song.
      if (event.key === "Escape" && !songPlaying) return;
      event.preventDefault();
      event.stopPropagation();
      togglePause();
      return;
    }
    const diffIx = ["1", "2", "3"].indexOf(event.key);
    if (diffIx >= 0 && !event.repeat && !covered && keys.indexOf(event.key) < 0) {
      event.preventDefault();
      event.stopPropagation();
      switchDifficulty(DIFFICULTIES[diffIx]!.id);
      return;
    }
    const lane = laneForKey(event);
    if (lane < 0) return;
    // Paused: arrow lane keys move the pause card's difficulty choice instead.
    if (phase === "paused" && event.target instanceof HTMLElement && event.target.closest('[role="radiogroup"]')) return;
    // Lane keys belong to the game while it is on screen (no app shortcuts).
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat || keyLanes.has(lane)) return;
    if (!canJudge()) return;
    keyLanes.add(lane);
    press(lane);
  }

  function onKeyUp(event: KeyboardEvent) {
    const lane = laneForKey(event);
    if (lane < 0 || !keyLanes.has(lane)) return;
    event.stopPropagation();
    keyLanes.delete(lane);
    if (!laneHeldElsewhere(lane)) release(lane);
  }

  function releaseAll() {
    const lanes = new Set<number>([...keyLanes, ...pointerLanes.values()]);
    keyLanes.clear();
    pointerLanes.clear();
    const s = run;
    if (s && !s.finished && canJudge()) for (const lane of lanes) release(lane);
    if (s) for (let i = 0; i < s.pressedLanes.length; i += 1) s.pressedLanes[i] = false;
    if (s) syncPads(performance.now());
  }

  /* (Re)start a run when the chart (track × difficulty) or the token changes. */
  $effect(() => {
    const c = chart;
    const rel = relPath;
    void startToken;
    untrack(() => {
      if (c) startRun(c, rel);
      else {
        if (run && !run.finished) finishRun(runRelPath !== rel && reachedEnd(run) ? "end" : "interrupted");
        run = null;
        runChart = null;
        bridge = createPlayerBridge(rel);
        updatePhase(performance.now());
        syncHud(true);
        schedule();
      }
    });
  });

  /* Settings that change the geometry or the look: re-measure, redraw. */
  $effect(() => {
    void light;
    void noteSpeed;
    void backdrop;
    void art;
    void vizMode;
    untrack(() => {
      measure();
      schedule();
    });
  });

  /* A dialog on top: drop held lanes; skip what goes by under it. */
  $effect(() => {
    if (covered) untrack(releaseAll);
    else untrack(() => (resyncPending = true));
  });

  /* Android Back / browser Back while notes fall: pause first (legacy trapped it). */
  $effect(() => {
    if (phase !== "live") return;
    const release = pushBackLayer(() => pause());
    return release;
  });

  onMount(() => {
    mounted = true;
    padEls = lanesEl ? [...lanesEl.querySelectorAll<HTMLElement>(".plectr-pad")] : [];
    measure();
    const ro = new ResizeObserver(() => {
      measure();
      schedule();
    });
    if (canvasEl) ro.observe(canvasEl);

    const onVisibility = () => {
      if (document.hidden) {
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        // The song keeps playing (legacy); notes that go by unseen are skipped.
        analyserLease.set(false);
        releaseAll();
      } else {
        resyncPending = true;
        schedule();
      }
    };
    const onBlur = () => releaseAll();
    const offPlayState = player.subscribePlayState(() => {
      if (player.playing) pauseRequestedAt = 0;
      schedule();
    });
    const offState = player.subscribe(schedule);
    // Wake-up for changes nobody announces (remote outputs, buffering).
    const poll = window.setInterval(schedule, 500);

    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("blur", onBlur);
    // Capture: lane keys must not reach the app-wide shortcut handler.
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    const root = rootEl;
    root?.addEventListener("touchstart", onTouch, { passive: false });
    root?.addEventListener("touchmove", onTouch, { passive: false });
    root?.addEventListener("touchend", onTouch, { passive: false });
    schedule();

    return () => {
      mounted = false;
      ro.disconnect();
      offPlayState();
      offState();
      window.clearInterval(poll);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      root?.removeEventListener("touchstart", onTouch);
      root?.removeEventListener("touchmove", onTouch);
      root?.removeEventListener("touchend", onTouch);
      for (const type of ["play", "pause", "seeking", "seeked", "ratechange"] as const) {
        watchedAudio?.removeEventListener(type, onAudioEvent);
      }
      watchedAudio = null;
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      if (run && !run.finished) finishRun(reachedEnd(run) ? "end" : "interrupted");
      run = null;
      bridge = null;
      layer.dispose();
      sprites.dispose();
      backdropViz.dispose();
      analyserLease.dispose();
    };
  });

  const hitWindowMs = Math.round(HIT_WINDOWS.ok * 1000);
</script>

<div
  bind:this={rootEl}
  {...{ [SHORTCUTS_OFF_ATTR]: "" }}
  class="plectr-stage"
  class:is-compact={compact}
  class:is-light={light}
  class:is-covered={covered}
  data-phase={phase}
  role="application"
  aria-label={t("plectr.stageAria", { keys: keyLabels.join(" "), ms: hitWindowMs })}
  oncontextmenu={(e) => e.preventDefault()}
>
  <header class="plectr-head">
    <div class="plectr-head__top">
      <span class="plectr-head__brand" title={title}>
        <UiIcon name="plectrum" />
        <span>{title || t("plectr.title")}</span>
      </span>
      <p class="plectr-head__record" aria-live="polite">
        {#if record}
          <span class="plectr-head__record-label">
            {#if record.last}{t("plectr.lastRun")}{:else}<UiIcon name="trophy" />{/if}
          </span>
          <strong>{fmtNumber(record.r.score)} · {record.r.grade}</strong>
        {:else}
          <span class="plectr-head__record-label"><UiIcon name="trophy" /></span>
          <span class="plectr-head__record-empty">—</span>
        {/if}
      </p>
      <div class="plectr-head__actions">
        <button
          type="button"
          class="plectr-head__btn"
          aria-label={songPlaying ? t("plectr.pause") : t("plectr.resume")}
          title={songPlaying ? t("plectr.pause") : t("plectr.resume")}
          onclick={togglePause}
        >
          <UiIcon name={songPlaying ? "pause" : "play"} />
        </button>
        {#if onsettings}
          <button type="button" class="plectr-head__btn" aria-label={t("plectr.settings.open")} title={t("plectr.settings.open")} onclick={onsettings}>
            <UiIcon name="settings" />
          </button>
        {/if}
        {#if onexit}
          <button type="button" class="plectr-head__btn" aria-label={t("plectr.exit")} title={t("plectr.exit")} onclick={onexit}>
            <UiIcon name="close" />
          </button>
        {/if}
      </div>
    </div>
    <div class="plectr-head__diffs" role="group" aria-label={t("plectr.difficulty")}>
      {#each DIFFICULTIES as d, i (d.id)}
        <button
          type="button"
          class="plectr-head__diff plectr-head__diff--{d.id}"
          class:is-active={difficulty === d.id}
          aria-pressed={difficulty === d.id}
          disabled={!playable.includes(d.id)}
          title={`${t(`plectr.diff.${d.id}`)} · ${t("plectr.level", { n: d.level })} (${i + 1})`}
          onclick={() => switchDifficulty(d.id)}
        >
          {t(`plectr.diff.${d.id}`)}
        </button>
      {/each}
    </div>
    <div class="plectr-head__progress" role="presentation"><span bind:this={progressEl}></span></div>
  </header>

  <div
    bind:this={lanesEl}
    class="plectr-lanes"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onlostpointercapture={onPointerUp}
    role="presentation"
  >
    <canvas bind:this={canvasEl} class="plectr-stage__canvas" aria-hidden="true"></canvas>

    <div class="plectr-hud" aria-hidden="true">
      <div class="plectr-hud__stat plectr-hud__stat--score">
        <span>{t("plectr.hudScore")}</span>
        <strong bind:this={scoreEl}>0</strong>
      </div>
      <div class="plectr-hud__stat plectr-hud__stat--combo">
        <span>{t("plectr.hudCombo")}</span>
        <strong bind:this={comboEl}>0x</strong>
      </div>
    </div>

    {#if fb.labelKey && phase === "live"}
      {#key judge.pulse}
        <div class="plectr-judge plectr-judge--{fb.tone}" aria-hidden="true">{t(fb.labelKey)}</div>
      {/key}
    {/if}
    {#if judge.combo >= COMBO_LABEL_FROM && phase === "live"}
      <div class="plectr-judge__combo" aria-hidden="true">{t("plectr.comboLabel", { n: judge.combo })}</div>
    {/if}
    {#if judge.milestone > 0 && phase === "live" && !reducedMotion}
      {#key judge.milestonePulse}
        <div class="plectr-milestone" aria-hidden="true">{t("plectr.milestone", { n: judge.milestone })}</div>
      {/key}
    {/if}
    {#if diffFlash}
      {#key diffFlash.n}
        <div class="plectr-diffflash plectr-diffflash--{diffFlash.id}" aria-live="polite">{t(`plectr.diff.${diffFlash.id}`)}</div>
      {/key}
    {/if}

    <div class="plectr-pads" aria-hidden="true">
      {#each LANES as lane, i (lane.name)}
        <div class="plectr-pad" data-lane={i} style="--lane-color: {lane.color}; --lane-shadow: {lane.shadow}">
          <span class="plectr-pad__pip"></span>
          {#if keyLetters}<span class="plectr-pad__key">{keyLabels[i]}</span>{/if}
        </div>
      {/each}
    </div>

    {#if overlay}
      {@render overlay()}
    {/if}
  </div>
</div>
