<script lang="ts" module>
  import type { DifficultyId, GameResult } from "../../lib/plectr/types";

  export type RunEndReason = "end" | "interrupted";
  export type StagePhase = "countdown" | "play" | "paused" | "done";
  export type PauseReason = "user" | "background" | "device" | "external" | "blocked";

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
   * Plectr stage for one run: countdown with a note lead-in at the chart
   * tempo, the highway synced to the global player clock, real pause (input
   * ignored, loop stopped), grace period after joining / resuming, HUD strip,
   * judgements, pads. The stage drives the player only to start / pause the
   * song; the view owns the flow around it (pick, results, exit).
   */
  import { onMount, untrack } from "svelte";
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import { SHORTCUTS_OFF_ATTR } from "../../lib/shortcutList";
  import { PlectrBackdrop } from "../../lib/plectr/backdrop";
  import {
    CHALLENGE_FAIL_ACCURACY,
    CHALLENGE_MIN_JUDGED,
    GRACE_SECONDS,
    HIT_WINDOWS,
    LANES,
  } from "../../lib/plectr/config";
  import {
    activeFlash,
    applyMisses,
    comboMultiplier,
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
  import { END_GUARD_SECONDS } from "../../lib/plectr/playerHold";
  import type { StageBackdrop } from "../../lib/plectr/records";
  import {
    StageBackground,
    drawAccuracyMeter,
    drawNotes,
    drawParticles,
    drawStage,
    drawTopFade,
    hitLineY,
    topFadeGradient,
    type DrawContext,
    type StageArt,
  } from "../../lib/plectr/renderer";
  import { resolveRunEndTime } from "../../lib/plectr/runResult";
  import { resetSongClock, resolveSmoothSongTime } from "../../lib/plectr/smoothSongClock";
  import { FpsWatch } from "../../lib/plectr/stageQuality";
  import {
    countdownLabelAt,
    countdownPlan,
    gameTimeFromAudio,
    pxPerSecond,
    type CountdownLabel,
    type CountdownPlan,
  } from "../../lib/plectr/timing";
  import type { Chart } from "../../lib/plectr/types";
  import { canvasDprCap, prefersReducedMotion } from "../../lib/visualizer/renderQuality";
  import type { VizMode } from "../../lib/visualizer/vizCanvasEngine";

  let {
    chart,
    relPath,
    title = "",
    /** Song time the run starts from (0 = top of the song). */
    startAt = 0,
    /** Bump to start a new run (restart, difficulty change, next song). */
    startToken = 0,
    leadTime = 1.6,
    latencyMs = 0,
    keys = ["d", "f", "j", "k"],
    keyLetters = true,
    vibration = true,
    challenge = false,
    light = false,
    backdrop = "art",
    art = { image: null, tint: null },
    vizMode = "bars",
    /** Phone immersive layout (strip with title, no accuracy meter). */
    compact = false,
    /** An overlay (pause menu, results) covers the stage. */
    covered = false,
    /** Watch the frame rate and report a slow device once. */
    watchFps = false,
    onphase,
    onfinish,
    onstats,
    onlowfps,
  }: {
    chart: Chart;
    relPath: string;
    title?: string;
    startAt?: number;
    startToken?: number;
    leadTime?: number;
    latencyMs?: number;
    keys?: string[];
    keyLetters?: boolean;
    vibration?: boolean;
    challenge?: boolean;
    light?: boolean;
    backdrop?: StageBackdrop;
    art?: StageArt;
    vizMode?: VizMode;
    compact?: boolean;
    covered?: boolean;
    watchFps?: boolean;
    onphase?: (phase: StagePhase, reason?: PauseReason) => void;
    onfinish?: (report: RunReport) => void;
    onstats?: (stats: LiveStats) => void;
    onlowfps?: () => void;
  } = $props();

  let lanesEl = $state<HTMLElement | null>(null);
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let padEls: HTMLElement[] = [];

  let phase = $state<StagePhase>("countdown");
  let hud = $state({
    score: 0,
    combo: 0,
    feedback: "ready" as FeedbackCode,
    pulse: 0,
    progress: 0,
    milestone: 0,
    milestonePulse: 0,
  });
  let countdown = $state<CountdownLabel | null>(null);
  const fb = $derived(feedbackView(hud.feedback));
  const multiplier = $derived(comboMultiplier(hud.combo));
  const keyLabels = $derived(keys.map(keyLabel));

  /* ── Non-reactive game state (touched every frame) ── */
  let run: RunState | null = null;
  let bridge: PlectrPlayerBridge | null = null;
  let runChart: Chart | null = null;
  let runRelPath = "";
  let runFromStart = false;
  let raf = 0;
  let lastAudioTime = 0;
  /** performance.now() of `lastAudioTime` (seek detection vs a slow frame). */
  let lastAudioPerf = 0;
  let shownScore = 0;
  let lastFrameAt = 0;
  let hudSyncAt = 0;
  let statsAt = 0;
  let padSignature = "";
  let layout = { width: 0, height: 0, dpr: 1, hitY: 0, speed: 1 };
  let fade: CanvasGradient | null = null;
  /** Countdown: plan, start (performance.now), target song time (game clock). */
  let plan: CountdownPlan | null = null;
  let countdownStart = 0;
  let countdownTarget = 0;
  let audioRequested = false;
  let audioRequestedAt = 0;
  /** Game clock runs on the audio (vs the countdown's virtual clock). */
  let onAudioClock = false;
  /** We paused the player ourselves (not an external pause). */
  let selfPausing = false;
  let finishedReason: RunEndReason | null = null;
  const background = new StageBackground();
  const backdropViz = new PlectrBackdrop();
  const fps = new FpsWatch();
  /** The analyser only while a spectrum backdrop is drawn for a playing song. */
  const analyserLease = player.analyserLease();
  /** pointerId → lane, for multi-touch and sliding between lanes. */
  const pointerLanes = new Map<number, number>();
  const keyLanes = new Set<number>();
  const reducedMotion = prefersReducedMotion();

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
    return { now, onMiss: () => vibrate(45), onMilestone: () => vibrate(12) };
  }

  function setPhase(next: StagePhase, reason?: PauseReason) {
    if (phase === next) return;
    phase = next;
    onphase?.(next, reason);
  }

  /* ── Clock ── */

  function audioGameTime(now: number): number {
    const s = run;
    const b = bridge;
    if (!s || !b) return 0;
    return gameTimeFromAudio(resolveSmoothSongTime(s, now, b), latencyMs);
  }

  function isAudioPlaying(): boolean {
    const audio = bridge?.getAudio();
    return Boolean(audio && !audio.paused && !audio.ended);
  }

  /** Current game-clock time (countdown virtual clock, then the audio). */
  function clockNow(now: number): number {
    const s = run;
    if (!s) return 0;
    if (phase === "countdown" && plan && !onAudioClock) {
      const elapsed = (now - countdownStart) / 1000;
      // Freeze on "VIA!" until the audio really runs: no jump back.
      return Math.min(countdownTarget, countdownTarget - (plan.total - elapsed));
    }
    if (phase === "play" || (phase === "countdown" && onAudioClock)) return audioGameTime(now);
    return s.songTime;
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
    finishedReason = reason;
    releaseAll();
    analyserLease.set(false);
    const r = report(reason, failed);
    if (reason === "end") setPhase("done");
    if (r && (judgedNotes(s) > 0 || reason === "end")) onfinish?.(r);
  }

  function startRun(nextChart: Chart, nextRelPath: string, from: number) {
    if (run && !run.finished) finishRun("interrupted");
    runChart = nextChart;
    runRelPath = nextRelPath;
    run = initialRunState(nextChart.notes);
    bridge = createPlayerBridge(nextRelPath);
    finishedReason = null;
    shownScore = 0;
    padSignature = "";
    const target = gameTimeFromAudio(from, latencyMs);
    runFromStart = from < 0.5;
    if (!runFromStart) {
      // Joining mid-song (difficulty change, resume after a seek): what is
      // behind is not a miss, and the next 1.5 s are on the house.
      skipNotesBefore(run, target);
      startGrace(run, target, GRACE_SECONDS);
    }
    run.started = true;
    run.songTime = target;
    lastAudioTime = from;
    lastAudioPerf = 0;
    for (const lane of keyLanes) run.pressedLanes[lane] = true;
    beginCountdown(from);
  }

  function beginCountdown(from: number) {
    const s = run;
    if (!s || !runChart) return;
    if (player.playing) {
      selfPausing = true;
      player.pause();
    }
    plan = countdownPlan(runChart.stats.bpm);
    countdownStart = performance.now();
    countdownTarget = gameTimeFromAudio(from, latencyMs);
    audioRequested = false;
    onAudioClock = false;
    s.songTime = countdownTarget - plan.total;
    countdown = null;
    fps.reset(performance.now());
    setPhase("countdown");
    syncHud(true);
    schedule();
  }

  /** Pause: audio stops, input ignored, one static frame. */
  export function pause(reason: PauseReason = "user") {
    const s = run;
    if (!s || s.finished || phase === "paused" || phase === "done") return;
    if (player.playing) {
      selfPausing = true;
      player.pause();
    }
    releaseAll();
    countdown = null;
    analyserLease.set(false);
    if (onAudioClock || phase === "play") {
      const b = bridge;
      if (b) s.songTime = gameTimeFromAudio(b.getCurrentTime(), latencyMs);
    }
    setPhase("paused", reason);
    drawOnce();
  }

  /** Resume with the countdown from where the song stopped; grace after it. */
  export function resume() {
    const s = run;
    const b = bridge;
    if (!s || !b || s.finished || phase !== "paused") return;
    const from = b.getCurrentTime();
    startGrace(s, gameTimeFromAudio(from, latencyMs), GRACE_SECONDS);
    beginCountdown(from);
  }

  export function isPaused(): boolean {
    return phase === "paused";
  }

  /* ── HUD ── */

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
      accuracy: judged ? s.hits / judged : 0,
      score: s.score,
    };
  }

  function syncHud(force = false) {
    const s = run;
    if (!s) return;
    const now = performance.now();
    const dt = lastFrameAt ? Math.min(0.1, (now - lastFrameAt) / 1000) : 0.016;
    shownScore = force || reducedMotion ? s.score : shownScore + (s.score - shownScore) * Math.min(1, dt * 10);
    if (Math.abs(s.score - shownScore) < 1) shownScore = s.score;
    const score = Math.round(shownScore);
    const feedbackChanged = s.feedback !== hud.feedback || s.feedbackPulse !== hud.pulse;
    const changed = feedbackChanged || score !== hud.score || s.combo !== hud.combo || s.milestonePulse !== hud.milestonePulse;
    const duration = runChart ? resolveRunEndTime(runChart.duration, bridge?.getDuration() || undefined) : 0;
    const progress = duration > 0 ? Math.min(1, Math.max(0, s.songTime / duration)) : 0;
    const progressMoved = Math.abs(progress - hud.progress) >= 0.004;
    if (!force && !feedbackChanged && (!changed || now - hudSyncAt < 50) && (!progressMoved || now - hudSyncAt < 250)) {
      return;
    }
    hudSyncAt = now;
    hud = {
      score,
      combo: s.combo,
      feedback: s.feedback,
      pulse: s.feedbackPulse,
      progress,
      milestone: s.milestone,
      milestonePulse: s.milestonePulse,
    };
    if (onstats && (force || now - statsAt > 250)) {
      statsAt = now;
      onstats(liveStats(s));
    }
  }

  function syncPads(now: number) {
    const s = run;
    if (!s) return;
    let sig = "";
    for (let i = 0; i < LANES.length; i += 1) {
      const holding = isHoldingLane(s, i);
      const flash = activeFlash(s, i, now);
      sig += `${s.pressedLanes[i] || holding ? 1 : 0}${flash === "hit" ? "h" : flash === "miss" ? "m" : "-"}`;
    }
    if (sig === padSignature) return;
    padSignature = sig;
    for (let i = 0; i < LANES.length; i += 1) {
      const pad = padEls[i];
      if (!pad) continue;
      const holding = isHoldingLane(s, i);
      const flash = activeFlash(s, i, now);
      pad.classList.toggle("is-pressed", s.pressedLanes[i] || holding);
      pad.classList.toggle("is-hit", flash === "hit");
      pad.classList.toggle("is-miss", flash === "miss");
    }
  }

  /* ── Drawing ── */

  function measure() {
    const canvas = canvasEl;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const dpr = light ? 1 : canvasDprCap({ lite: true });
    const width = Math.max(1, rect.width);
    const height = Math.max(1, rect.height);
    const hitY = hitLineY(height);
    const speed = pxPerSecond(hitY, leadTime);
    layout = { width, height, dpr, hitY, speed };
    const bw = Math.max(1, Math.floor(width * dpr));
    const bh = Math.max(1, Math.floor(height * dpr));
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
    }
    fade = null;
    // For QA bots / tests: where the hit line is and how fast notes travel.
    canvas.dataset.hitY = String(Math.round(hitY));
    canvas.dataset.speed = String(Math.round(speed));
  }

  function draw(now: number, songTime: number) {
    const s = run;
    const canvas = canvasEl;
    if (!s || !canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    if (layout.width < 1) measure();
    const { width, height, dpr, hitY, speed } = layout;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const useViz = !light && backdrop === "bars" && vizMode !== "karaoke";
    const vizPlaying = useViz && phase === "play" && isAudioPlaying();
    analyserLease.set(vizPlaying);
    if (useViz) {
      backdropViz.draw(
        ctx,
        width,
        height,
        { mode: vizMode, analyser: vizPlaying ? player.getAnalyser() : null, isPlaying: vizPlaying },
        "#06080f",
      );
    }
    const bg = background.get(width, height, dpr, light || backdrop !== "art" ? { image: null, tint: art.tint } : art, !useViz);
    if (bg) {
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(bg, 0, 0);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    const d: DrawContext = {
      cssWidth: width,
      cssHeight: height,
      hitY,
      laneWidth: width / LANES.length,
      speed,
      songTime,
      state: s,
      now,
      light,
      reducedMotion,
      showMeter: !compact,
      vizUnderlay: useViz,
    };
    drawStage(ctx, d);
    drawNotes(ctx, d);
    drawParticles(ctx, d);
    fade ??= topFadeGradient(ctx, height);
    drawTopFade(ctx, d, fade);
    drawAccuracyMeter(ctx, d);
  }

  function drawOnce() {
    const s = run;
    if (!s) return;
    draw(performance.now(), s.songTime);
  }

  /* ── Loop ── */

  function frame() {
    raf = 0;
    const s = run;
    const b = bridge;
    if (!s || !b || !canvasEl) return;
    const now = performance.now();
    if (phase === "paused" || phase === "done") {
      drawOnce();
      return;
    }
    if (watchFps && phase === "play" && fps.tick(now)) onlowfps?.();

    const onTrack = b.isOnTrack();
    if (!onTrack) {
      // The player moved on to another track.
      finishRun("interrupted");
      drawOnce();
      return;
    }

    if (phase === "countdown") stepCountdown(now);
    else stepPlay(now);
    if (!run || run !== s || s.finished) {
      if (run) drawOnce();
      return;
    }

    const songTime = clockNow(now);
    s.songTime = songTime;
    if (!covered) {
      const e = env(now);
      applyMisses(s, songTime, e);
      completeHeldNotes(s, songTime, e);
    }

    draw(now, songTime);
    syncPads(now);
    syncHud();
    lastFrameAt = now;
    checkEnd(s, songTime);
    schedule();
  }

  function stepCountdown(now: number) {
    const s = run;
    const b = bridge;
    if (!s || !b || !plan) return;
    const elapsed = (now - countdownStart) / 1000;
    const label = countdownLabelAt(plan, elapsed);
    if (label !== countdown) countdown = label;
    if (elapsed >= plan.total && !audioRequested) {
      audioRequested = true;
      audioRequestedAt = now;
      if (!player.playing) void player.toggle();
    }
    if (audioRequested && !onAudioClock) {
      const audioNow = b.getCurrentTime();
      if (isAudioPlaying() && audioNow > lastAudioTime + 0.005) {
        // The song runs: hand the clock over to the audio.
        onAudioClock = true;
        resetSongClock(s, audioNow, now);
        lastAudioTime = audioNow;
        lastAudioPerf = now;
      } else if (now - audioRequestedAt > 2500) {
        // Autoplay refused / audio stuck: wait for the player in the pause menu.
        pause("blocked");
        return;
      }
    }
    if (onAudioClock && elapsed >= plan.total + plan.stepSec) {
      countdown = null;
      setPhase("play");
    }
  }

  function stepPlay(now: number) {
    const s = run;
    const b = bridge;
    if (!s || !b) return;
    const audioNow = b.getCurrentTime();
    if (!isAudioPlaying()) {
      if (nearTrackEnd(audioNow)) {
        finishRun("end");
        return;
      }
      if (selfPausing) return;
      // Paused from the dock, media keys, a headset unplugged…
      pause("external");
      return;
    }
    if (Math.abs(audioNow - lastAudioTime) < 1e-4 && lastAudioPerf && now - lastAudioPerf > 3000) {
      // "Playing" but the audio clock froze (output lost, stream stalled):
      // pause instead of letting notes fall without music.
      pause("device");
      return;
    }
    if (audioNow < lastAudioTime - 1.5) {
      // Seek back (or the track looped): a fresh run from here, mid-song.
      const from = audioNow;
      finishRun(audioNow < 2 && nearTrackEnd(lastAudioTime) ? "end" : "interrupted");
      if (runChart && !nearTrackEnd(lastAudioTime)) startRun(runChart, runRelPath, from);
      return;
    }
    const wall = lastAudioPerf ? (now - lastAudioPerf) / 1000 : 0;
    if (audioNow - lastAudioTime - wall > 0.6) {
      // Seek forward (the audio jumped, not just a slow frame): skipped
      // notes are not misses; grace after the jump.
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
    return d > 0 && d - audioTime <= END_GUARD_SECONDS + 0.6;
  }

  function checkEnd(s: RunState, songTime: number) {
    if (s.finished || phase !== "play") return;
    if (challenge) {
      const judged = judgedNotes(s);
      if (judged >= CHALLENGE_MIN_JUDGED && s.hits / judged < CHALLENGE_FAIL_ACCURACY) {
        finishRun("end", true);
        if (player.playing) {
          selfPausing = true;
          player.pause();
        }
        return;
      }
    }
    if (!s.awaitingTrackEnd && isChartRunComplete(s, songTime)) {
      s.awaitingTrackEnd = true;
    }
    const lastEnd = runChart?.notes.reduce((m, n) => Math.max(m, n.time + n.duration), 0) ?? 0;
    // Chart done (+ a breath): results now, the player keeps going to the end.
    if (s.awaitingTrackEnd && songTime >= lastEnd + 1.2) {
      finishRun("end");
      return;
    }
    const runEnd = resolveRunEndTime(runChart?.duration ?? 0, bridge?.getDuration() || undefined);
    if (runEnd > 0 && songTime >= runEnd - END_GUARD_SECONDS - 0.1) finishRun("end");
  }

  function schedule() {
    if (raf || typeof document === "undefined" || document.hidden) return;
    raf = requestAnimationFrame(frame);
  }

  /* ── Input ── */

  function canJudge(): boolean {
    const s = run;
    if (!s || s.finished || covered) return false;
    return phase === "play" || phase === "countdown";
  }

  function press(lane: number) {
    const s = run;
    if (!s || s.finished) return;
    if (!canJudge()) return;
    // Judge on the freshest clock, not the last frame's.
    s.songTime = clockNow(performance.now());
    pressLane(s, lane, env());
    syncPads(performance.now());
    syncHud();
  }

  function release(lane: number) {
    const s = run;
    if (!s) return;
    if (!canJudge()) {
      s.pressedLanes[lane] = false;
      return;
    }
    s.songTime = clockNow(performance.now());
    releaseLane(s, lane, env());
    syncPads(performance.now());
    syncHud();
  }

  function laneAt(clientX: number): number {
    const rect = lanesEl?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return 0;
    const ix = Math.floor(((clientX - rect.left) / rect.width) * LANES.length);
    return Math.min(LANES.length - 1, Math.max(0, ix));
  }

  function onPointerDown(event: PointerEvent) {
    if (!canJudge()) return;
    if (event.button !== 0 && event.pointerType === "mouse") return;
    event.preventDefault();
    const lane = laneAt(event.clientX);
    pointerLanes.set(event.pointerId, lane);
    try {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    } catch {
      /* synthetic pointer */
    }
    press(lane);
  }

  function onPointerMove(event: PointerEvent) {
    const prev = pointerLanes.get(event.pointerId);
    if (prev === undefined) return;
    const lane = laneAt(event.clientX);
    if (lane === prev) return;
    // Sliding a finger across lanes: let go of the old one, press the new one.
    pointerLanes.set(event.pointerId, lane);
    if (![...pointerLanes.values()].includes(prev) && !keyLanes.has(prev)) release(prev);
    if (run && !run.pressedLanes[lane]) press(lane);
  }

  function onPointerUp(event: PointerEvent) {
    const lane = pointerLanes.get(event.pointerId);
    if (lane === undefined) return;
    pointerLanes.delete(event.pointerId);
    if (![...pointerLanes.values()].includes(lane) && !keyLanes.has(lane)) release(lane);
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
    const k = event.key.toLowerCase();
    return keys.indexOf(k);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (isTypingTarget(event.target) || modalOpen()) return;
    const isSpace = event.key === " " || event.code === "Space";
    if ((isSpace || event.key === "Escape") && !event.repeat) {
      // Buttons keep Space / Enter for themselves (pause menu, results).
      if (isSpace && event.target instanceof HTMLElement && event.target.closest("button, a, [role=button]")) {
        return;
      }
      if (phase === "done" || covered) return;
      event.preventDefault();
      event.stopPropagation();
      if (phase === "paused") resume();
      else pause("user");
      return;
    }
    const lane = laneForKey(event);
    if (lane < 0) return;
    // Paused: arrow lane keys move the pause menu's difficulty choice instead.
    if (phase === "paused" && event.target instanceof HTMLElement && event.target.closest('[role="radiogroup"]')) {
      return;
    }
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
    if (![...pointerLanes.values()].includes(lane)) release(lane);
  }

  function releaseAll() {
    const lanes = new Set<number>([...keyLanes, ...pointerLanes.values()]);
    keyLanes.clear();
    pointerLanes.clear();
    const s = run;
    if (s && !s.finished && canJudge()) for (const lane of lanes) release(lane);
    if (s) s.pressedLanes = s.pressedLanes.map(() => false);
    if (s) syncPads(performance.now());
  }

  /* (Re)start a run whenever the chart (track × difficulty) or the token changes. */
  $effect(() => {
    const c = chart;
    const rel = relPath;
    const from = startAt;
    void startToken;
    untrack(() => {
      startRun(c, rel, from);
    });
  });

  /* Settings that change the geometry or the look: re-measure, redraw. */
  $effect(() => {
    void light;
    void leadTime;
    void backdrop;
    void art;
    untrack(() => {
      measure();
      if (phase === "paused" || phase === "done") drawOnce();
    });
  });

  /* Overlay on top: drop held lanes. */
  $effect(() => {
    if (covered) untrack(releaseAll);
  });

  onMount(() => {
    padEls = lanesEl ? [...lanesEl.querySelectorAll<HTMLElement>(".plectr-pad")] : [];
    measure();
    const ro = new ResizeObserver(() => {
      measure();
      if (phase === "paused" || phase === "done") drawOnce();
    });
    if (canvasEl) ro.observe(canvasEl);

    const onVisibility = () => {
      if (document.hidden) {
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        // Background: a real pause (notes would scroll by unseen).
        if (phase === "play" || phase === "countdown") pause("background");
        analyserLease.set(false);
        releaseAll();
      } else {
        schedule();
      }
    };
    const onBlur = () => releaseAll();
    const offPlayState = player.subscribePlayState(() => {
      // Our own pause went through: later pauses are external again.
      if (!player.playing) selfPausing = false;
      if (player.playing && phase === "paused" && !covered) {
        // Resumed from the dock / media keys while paused here: pause again
        // and go through the countdown instead of running blind.
        selfPausing = true;
        player.pause();
      }
    });

    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("blur", onBlur);
    // Capture: lane keys must not reach the app-wide shortcut handler.
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);

    return () => {
      ro.disconnect();
      offPlayState();
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      if (run && !run.finished) finishRun("interrupted");
      run = null;
      bridge = null;
      background.dispose();
      backdropViz.dispose();
      analyserLease.dispose();
    };
  });

  const hitWindowMs = Math.round(HIT_WINDOWS.ok * 1000);
  void finishedReason;
</script>

<div
  {...{ [SHORTCUTS_OFF_ATTR]: "" }}
  class="plectr-stage"
  class:is-compact={compact}
  class:is-light={light}
  class:is-covered={covered}
  data-phase={phase}
  role="application"
  aria-label={t("plectr.stageAria", { keys: keyLabels.join(" "), ms: hitWindowMs })}
>
  <div class="plectr-strip">
    <div
      class="plectr-strip__progress"
      role="progressbar"
      aria-label={t("plectr.timeAria")}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(hud.progress * 100)}
    >
      <span style="transform: scaleX({hud.progress})"></span>
    </div>
    <button
      type="button"
      class="plectr-strip__pause"
      aria-label={phase === "paused" ? t("plectr.resume") : t("plectr.pause")}
      title={phase === "paused" ? t("plectr.resume") : t("plectr.pause")}
      disabled={phase === "done"}
      onclick={() => (phase === "paused" ? resume() : pause("user"))}
    >
      {#if phase === "paused"}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
      {:else}
        <svg viewBox="0 0 24 24" aria-hidden="true"
          ><path d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" fill="currentColor" /></svg
        >
      {/if}
    </button>
    {#if compact}
      <div class="plectr-strip__title" aria-hidden="true"><span>{title}</span></div>
    {/if}
    <div class="plectr-strip__score" aria-label={t("plectr.hudScore")}>{fmtNumber(hud.score)}</div>
    <div class="plectr-strip__combo" aria-label={t("plectr.hudCombo")}>
      <strong>×{hud.combo}</strong>
      <span class="plectr-pips" aria-label={t("plectr.multiplier", { n: multiplier })}>
        {#each [1, 2, 3, 4] as pip (pip)}
          <i class:is-on={pip <= multiplier}></i>
        {/each}
      </span>
    </div>
  </div>

  <div
    bind:this={lanesEl}
    class="plectr-lanes"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onlostpointercapture={onPointerUp}
    oncontextmenu={(e) => e.preventDefault()}
    role="presentation"
  >
    <canvas bind:this={canvasEl} class="plectr-stage__canvas" aria-hidden="true"></canvas>

    {#if fb.labelKey && phase !== "paused"}
      {#key `${hud.feedback}-${hud.pulse}`}
        <div class="plectr-judge plectr-judge--{fb.tone}" aria-hidden="true">{t(fb.labelKey)}</div>
      {/key}
    {/if}
    {#if hud.combo >= COMBO_LABEL_FROM && phase === "play"}
      <div class="plectr-judge__combo" aria-hidden="true">{t("plectr.comboLabel", { n: hud.combo })}</div>
    {/if}
    {#if hud.milestone > 0 && phase === "play"}
      {#key hud.milestonePulse}
        <div class="plectr-milestone" aria-hidden="true">{t("plectr.milestone", { n: hud.milestone })}</div>
      {/key}
    {/if}
    {#if countdown}
      {#key countdown}
        <div class="plectr-countdown" class:is-go={countdown === "go"} aria-live="assertive">
          {countdown === "go" ? t("plectr.countdownGo") : countdown}
        </div>
      {/key}
    {/if}

    <div class="plectr-pads" aria-hidden="true">
      {#each LANES as lane, i (lane.name)}
        <div class="plectr-pad" data-lane={i} style="--lane-color: {lane.color}">
          {#if keyLetters}<span class="plectr-pad__key">{keyLabels[i]}</span>{/if}
        </div>
      {/each}
    </div>
  </div>
</div>
