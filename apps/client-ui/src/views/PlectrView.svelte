<script lang="ts">
  import { isExternalTrack } from "../lib/externalItems";
  /**
   * Plectr — rhythm game on the song the player is playing (legacy dock
   * behaviour): opening the view starts the game at once on the current
   * track, from where it is, without pausing, seeking or restarting it. The
   * game follows the player: next song → new chart, new run; pause in the
   * dock → the notes freeze. Difficulty switches live (1 / 2 / 3 or the
   * buttons on the stage) from the current position. There is no track
   * picker: the song is chosen in the library like everywhere else; with
   * nothing in the player the stage shows an empty state pointing there.
   *
   * A portrait 9:16 stage in every layout: a framed device between two side
   * panels on desktop, centred with an info sheet on tablets, immersive full
   * screen on phones — the app chrome hidden and inert, only the player bar
   * kept at the bottom (legacy dock layout), guarded against stray taps.
   */
  import { onMount, untrack } from "svelte";
  import "../styles/plectr.css";
  import UiIcon from "../components/icons/UiIcon.svelte";
  import CareerCard from "../components/plectr/CareerCard.svelte";
  import GameStage, {
    type LiveStats,
    type PauseReason,
    type RunReport,
    type StagePhase,
  } from "../components/plectr/GameStage.svelte";
  import PauseMenu from "../components/plectr/PauseMenu.svelte";
  import PlectrCover from "../components/plectr/PlectrCover.svelte";
  import PlectrSettings from "../components/plectr/PlectrSettings.svelte";
  import RecordsView from "../components/plectr/RecordsView.svelte";
  import RunResults, { type ResultsData } from "../components/plectr/RunResults.svelte";
  import SessionPanel from "../components/plectr/SessionPanel.svelte";
  import TrackPanel from "../components/plectr/TrackPanel.svelte";
  import type { Track } from "../lib/api";
  import { confirmDialog } from "../lib/confirm.svelte";
  import { t } from "../lib/i18n.svelte";
  import { player } from "../lib/player";
  import { session } from "../lib/session.svelte";
  import { loadUserPrefs } from "../lib/userPrefs";
  import {
    analyzeLibraryTrack,
    peekCachedChart,
    prefetchRhythmChart,
    releaseAnalysisWorker,
    RhythmAnalyzeError,
  } from "../lib/plectr/analyze";
  import { albumArt } from "../lib/plectr/art";
  import { plectrBackdropMode } from "../lib/plectr/backdrop";
  import { ChartLoader, idleChartState, type ChartLoaderState } from "../lib/plectr/chartLoader";
  import { DIFFICULTIES, MIN_NOTES_PER_DIFFICULTY } from "../lib/plectr/config";
  import { plectrNav, type PlectrSection } from "../lib/plectr/nav.svelte";
  import { plectrRecords } from "../lib/plectr/persist.svelte";
  import { difficultyBest, selectPlectrCareer } from "../lib/plectr/records";
  import { downloadShareCard } from "../lib/plectr/shareCard";
  import { resolveLightStage, writeMeasuredSlow } from "../lib/plectr/stageQuality";
  import { isRecordEligible, noteSpeedFor } from "../lib/plectr/timing";
  import type { DifficultyId, GameResult } from "../lib/plectr/types";
  import type { VizMode } from "../lib/visualizer/vizCanvasEngine";

  type Phase = "empty" | "stage";
  type Layout = "phone" | "tablet" | "desktop";
  type PlectrTrack = Pick<Track, "rel_path" | "title" | "artist_name" | "album_id"> & Partial<Track>;

  /* ── Chart loading (follows the player's track) ── */
  let chartState = $state<ChartLoaderState>(idleChartState());
  const loader = new ChartLoader(
    {
      analyze: analyzeLibraryTrack,
      peekCached: peekCachedChart,
      errorCodeOf: (err) => (err instanceof RhythmAnalyzeError ? err.code : "decode"),
    },
    (next) => {
      chartState = next;
    },
  );

  let rootEl = $state<HTMLElement | null>(null);
  let stageRef = $state<GameStage | null>(null);
  let section = $state<PlectrSection>(plectrNav.take());
  let stagePhase = $state<StagePhase>("loading");
  let pauseReason = $state<PauseReason>("idle");
  let layout = $state<Layout>("desktop");
  /** Touch-first device: no key letters on the pads. */
  let coarsePointer = $state(false);
  let startToken = $state(0);
  let results = $state<ResultsData | null>(null);
  let lastRun = $state<{ relPath: string; result: GameResult } | null>(null);
  let stats = $state<LiveStats | null>(null);
  let showSettings = $state(false);
  let showInfo = $state(false);
  let slowTick = $state(0);
  let artState = $state<{
    albumId: number | null;
    /** Blurred cover for the share card. */
    image: CanvasImageSource | null;
    tint: string | null;
    dataUrl: string | null;
  }>({ albumId: null, image: null, tint: null, dataUrl: null });
  let vizMode = $state<VizMode>(plectrBackdropMode(loadUserPrefs().visualizerMode));
  let exiting = false;
  /** Results of a song the player already left close by themselves after this. */
  const RESULTS_LINGER_MS = 7000;
  let resultsTimer = 0;

  const store = $derived(plectrRecords.store);
  const settings = $derived(store.settings);
  const difficulty = $derived(store.difficulty);
  const light = $derived.by(() => {
    void slowTick;
    return resolveLightStage(settings.lightStage);
  });
  const career = $derived(selectPlectrCareer(store));
  const noteSpeed = $derived(noteSpeedFor(settings.speed));

  /**
   * The game runs on whatever the player holds (playing or paused); nothing
   * there → the empty state. Podcast episodes and live streams are never a game.
   */
  const target = $derived<PlectrTrack | null>(
    isExternalTrack(session.current) ? null : session.current,
  );
  const externalInPlayer = $derived(!!session.current && isExternalTrack(session.current));
  const phase = $derived<Phase>(target ? "stage" : "empty");

  const chartSet = $derived(
    chartState.phase === "ready" && target && chartState.relPath === target.rel_path ? chartState.chartSet : null,
  );
  const playableIds = $derived(
    DIFFICULTIES.filter((d) => (chartSet?.charts[d.id]?.notes.length ?? 0) >= MIN_NOTES_PER_DIFFICULTY).map(
      (d) => d.id,
    ),
  );
  /** The saved difficulty, or the first playable one on sparse tracks. */
  const activeDifficulty = $derived<DifficultyId>(
    !chartSet || playableIds.includes(difficulty) ? difficulty : (playableIds[0] ?? difficulty),
  );
  const chart = $derived(chartSet ? (chartSet.charts[activeDifficulty] ?? null) : null);
  const errorCode = $derived(
    target && chartState.phase === "error" && chartState.relPath === target.rel_path ? chartState.errorCode : null,
  );
  const loadProgress = $derived(
    chartSet
      ? 1
      : chartState.stage === "fetch"
        ? 0.05 + chartState.progress * 0.3
        : chartState.stage === "decode"
          ? 0.35 + chartState.progress * 0.25
          : 0.6 + chartState.progress * 0.4,
  );
  const loadLabel = $derived(
    chartState.stage === "fetch"
      ? t("plectr.loading.fetch")
      : chartState.stage === "decode"
        ? t("plectr.loading.decode")
        : t("plectr.loading.analyze"),
  );

  /** Track shown in the panels: the run's. */
  const panelTrack = $derived<PlectrTrack | null>(target);
  const panelIsPlaying = $derived(!!panelTrack && session.playing);
  const panelEyebrow = $derived(session.playing ? t("plectr.nowPlaying") : t("plectr.paused.title"));
  const immersive = $derived(layout === "phone" && phase === "stage" && section === "play");
  const framed = $derived(layout !== "phone");
  const live = $derived(phase === "stage" && stagePhase === "live");
  const hasNext = $derived(nextQueueIndex() >= 0);
  const currentLastRun = $derived(lastRun && panelTrack && lastRun.relPath === panelTrack.rel_path ? lastRun.result : null);
  const currentBest = $derived(target ? difficultyBest(store, target.rel_path, activeDifficulty) : null);

  function nextQueueIndex(): number {
    const queue = session.queue;
    const idx = session.currentIndex;
    if (!queue.length || idx < 0) return -1;
    if (idx + 1 < queue.length) return idx + 1;
    return player.repeat === "all" && queue.length > 1 ? 0 : -1;
  }

  /* ── Flow ── */

  /** Records ▶ / replay of a song the player already left: play it, the game follows. */
  function playTrack(track: PlectrTrack) {
    results = null;
    stats = null;
    showInfo = false;
    section = "play";
    if (player.current?.rel_path === track.rel_path) {
      // Already in the player: keep its position, just make sure it plays.
      if (!player.playing) void player.toggle();
      return;
    }
    const idx = session.queue.findIndex((tr) => tr.rel_path === track.rel_path);
    if (idx >= 0) session.playQueueIndex(idx);
    else if (track.id != null) void session.playGlobalRadio(track as Track);
  }

  /* The chart follows the player's track. */
  $effect(() => {
    const tr = target;
    const rel = tr?.rel_path ?? null;
    untrack(() => {
      if (!tr || !rel) {
        loader.load(null);
        return;
      }
      loader.load({ rel_path: rel, title: tr.title });
      // A new song: the last song's results stay a moment, then the new run takes over.
      if (results && results.relPath !== rel) {
        const shown = results;
        window.clearTimeout(resultsTimer);
        resultsTimer = window.setTimeout(() => {
          if (results === shown) results = null;
        }, RESULTS_LINGER_MS);
      }
      stats = null;
    });
  });

  /* Chart ready: warm the next track in the queue (legacy prefetch). */
  $effect(() => {
    if (!chartSet || phase !== "stage") return;
    untrack(() => {
      const idx = nextQueueIndex();
      const next = idx >= 0 ? session.queue[idx] : null;
      if (next && !isExternalTrack(next) && next.rel_path !== target?.rel_path) {
        prefetchRhythmChart({ rel_path: next.rel_path, title: next.title });
      }
    });
  });

  function onStagePhase(next: StagePhase, reason?: PauseReason) {
    stagePhase = next;
    if (next === "paused") pauseReason = reason ?? "user";
  }

  function onFinish(report: RunReport) {
    const tr = target;
    if (report.reason !== "end" || !tr || report.relPath !== tr.rel_path) {
      // The song moved on right at its end: still record it, no results card.
      if (report.reason === "end" && report.judged > 0) recordReport(report);
      return;
    }
    const { eligible, newRecord, previous } = recordReport(report);
    results = {
      relPath: report.relPath,
      title: tr.title,
      artist: tr.artist_name,
      albumId: tr.album_id,
      difficulty: report.difficulty,
      result: report.result,
      perfects: report.perfects,
      goods: report.goods,
      earlies: report.earlies,
      lates: report.lates,
      skipped: report.skipped,
      judged: report.judged,
      notesHit: report.notesHit,
      totalNotes: report.totalNotes,
      counted: eligible,
      newRecord: eligible && newRecord,
      previous,
      fc: report.fc,
      ap: report.ap,
    };
  }

  function recordReport(report: RunReport) {
    const eligible = isRecordEligible(report);
    const out = plectrRecords.recordRun(report.relPath, report.result, report.difficulty, {
      eligible,
      // Grace skips after a pause / resume don't spoil a full run; a join or seek does.
      fullRun: eligible && report.fromStart && report.jumped === 0,
      fc: eligible && report.fc,
      ap: eligible && report.ap,
    });
    if (report.judged > 0) lastRun = { relPath: report.relPath, result: report.result };
    return { eligible, ...out };
  }

  /** Live switch: the stage starts a new run on the new chart from here. */
  function setDifficulty(id: DifficultyId) {
    if (chartSet && !playableIds.includes(id)) return;
    if (id === difficulty) return;
    plectrRecords.setDifficulty(id);
  }

  /** Explicit "from the top": seek to 0 and play at once (no countdown). */
  function restart() {
    if (!target) return;
    results = null;
    player.seek(0);
    if (!player.playing) void player.toggle();
    window.setTimeout(() => (startToken += 1), 60);
  }

  /** Results → play again: from the top of that song (it may have ended already). */
  function replay() {
    const rel = results?.relPath;
    if (!rel || rel === target?.rel_path) {
      restart();
      return;
    }
    const tr = session.queue.find((x) => x.rel_path === rel) ?? session.catalogTracks.find((x) => x.rel_path === rel);
    results = null;
    if (tr) playTrack(tr);
  }

  function playNext() {
    results = null;
    if (nextQueueIndex() >= 0) void player.next();
  }

  function openLibrary() {
    session.navigate("library");
  }

  function shuffleLibrary() {
    void session.shuffleLibrary().catch(() => {});
  }

  /** Resolves on the next `popstate`, or after `ms` when none comes. */
  function nextPop(ms: number): Promise<void> {
    return new Promise((resolve) => {
      const done = () => {
        window.removeEventListener("popstate", done);
        window.clearTimeout(timer);
        resolve();
      };
      const timer = window.setTimeout(done, ms);
      window.addEventListener("popstate", done);
    });
  }

  /**
   * Back to where Plectr was opened from (the music keeps its state). A run
   * in progress holds a Back layer (Back = pause): it goes first, or
   * `history.back()` would only pop it and pause the game — on phones the ✕
   * used to do just that and then ignore every later tap.
   */
  async function exitPlectr() {
    if (exiting) return;
    exiting = true;
    try {
      if (stageRef?.dropBackLayer()) await nextPop(600);
      if (session.view !== "plectr") return;
      const idx = (history.state as { rkIdx?: unknown } | null)?.rkIdx;
      if (typeof idx === "number" && idx > 0) {
        history.back();
        await nextPop(600);
      }
      // No entry to go back to, or the pop did not land: leave anyway.
      if (session.view === "plectr") session.navigate("dashboard");
    } finally {
      exiting = false;
    }
  }

  async function resetRecords() {
    const ok = await confirmDialog({
      title: t("plectr.reset.title"),
      message: t("plectr.reset.message"),
      confirmLabel: t("plectr.reset.confirm"),
      cancelLabel: t("plectr.reset.cancel"),
      danger: true,
    });
    if (!ok) return;
    plectrRecords.resetRecords();
    lastRun = null;
  }

  function openSettings() {
    showSettings = true;
  }

  function onLowFps() {
    writeMeasuredSlow(true);
    slowTick += 1;
  }

  function share() {
    if (results) void downloadShareCard(results, artState.image);
  }

  /**
   * Esc closes what sits on top: the records view (back to the game), the
   * results card. The stage handles its own Esc (pause); dialogs and the
   * tablet info sheet close themselves.
   */
  function onWindowKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape" || event.defaultPrevented || event.repeat) return;
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    if (showSettings || showInfo || document.querySelector(".rk-modal-back")) return;
    const el = event.target;
    // A search field with text clears itself first.
    if (el instanceof HTMLInputElement && el.value) return;
    if (section === "records") {
      event.preventDefault();
      section = "play";
    } else if (results) {
      event.preventDefault();
      results = null;
    }
  }

  /* ── Art (page backdrop, device glow, stage tint) ── */
  $effect(() => {
    const id = panelTrack?.album_id ?? null;
    if (id === artState.albumId) return;
    untrack(() => {
      if (id == null) {
        artState = { albumId: null, image: null, tint: null, dataUrl: null };
        return;
      }
      void albumArt(panelTrack).then((a) => {
        if ((panelTrack?.album_id ?? null) !== id) return;
        artState = { albumId: id, image: a.image, tint: a.color, dataUrl: a.dataUrl };
      });
    });
  });

  /* ── Shell integration: immersive (phones) and "playing" (dock de-emphasised) ── */
  $effect(() => {
    const root = document.documentElement;
    if (immersive) root.setAttribute("data-plectr-immersive", "");
    else root.removeAttribute("data-plectr-immersive");
    if (live) root.setAttribute("data-plectr-playing", "");
    else root.removeAttribute("data-plectr-playing");
  });

  /*
   * Phones: the game owns the screen. The app chrome underneath (header,
   * rail, bottom nav) is hidden and made inert, so a tap on the stage can
   * never reach it. The player bar stays, below the stage (legacy dock).
   */
  $effect(() => {
    if (!immersive) return;
    const els = [...document.querySelectorAll<HTMLElement>("header.top, nav.bottom, aside.rail")].filter(
      (el) => !el.inert,
    );
    for (const el of els) el.inert = true;
    return () => {
      for (const el of els) el.inert = false;
    };
  });

  /*
   * Phones, notes falling: the player bar sits right under the pads. A
   * press on the stage is captured by the lanes (it can never end on the
   * bar); on top of that, a press on the bar that comes right after a pad
   * press is a stray thumb, not a decision — it is swallowed. A deliberate
   * tap (a moment away from the pads) works as always.
   */
  const BAR_GUARD_MS = 450;
  $effect(() => {
    if (!immersive || !live) return;
    const page = rootEl;
    if (!page) return;
    let lastStageInput = 0;
    /** The current press on the bar started too close to a pad press. */
    let swallowing = false;
    const onStage = () => {
      lastStageInput = performance.now();
    };
    const onBar = (event: Event) => {
      const el = event.target;
      if (!(el instanceof Element) || !el.closest("footer.player-dock")) return;
      if (event.type === "pointerdown") swallowing = performance.now() - lastStageInput < BAR_GUARD_MS;
      if (!swallowing) return;
      if (event.type === "click") swallowing = false;
      event.preventDefault();
      event.stopPropagation();
    };
    const stageOpts = { capture: true, passive: true } as const;
    const barOpts = { capture: true, passive: false } as const;
    const stageTypes = ["pointerdown", "pointerup"] as const;
    const barTypes = ["pointerdown", "pointermove", "pointerup", "click"] as const;
    for (const type of stageTypes) page.addEventListener(type, onStage, stageOpts);
    for (const type of barTypes) document.addEventListener(type, onBar, barOpts);
    return () => {
      for (const type of stageTypes) page.removeEventListener(type, onStage, stageOpts);
      for (const type of barTypes) document.removeEventListener(type, onBar, barOpts);
    };
  });

  /* Deep link while the view is already open (Statistics → records). */
  $effect(() => {
    if (plectrNav.section === "play") return;
    untrack(() => {
      section = plectrNav.take();
    });
  });

  onMount(() => {
    void plectrRecords.ensureReady();
    if (!session.catalogTracks.length) void session.ensureCatalogTracks().catch(() => {});

    const mqPhone = window.matchMedia("(max-width: 599.98px)");
    const mqTablet = window.matchMedia("(max-width: 999.98px)");
    const mqCoarse = window.matchMedia("(pointer: coarse)");
    const syncLayout = () => {
      layout = mqPhone.matches ? "phone" : mqTablet.matches ? "tablet" : "desktop";
      coarsePointer = mqCoarse.matches;
    };
    syncLayout();
    mqPhone.addEventListener("change", syncLayout);
    mqTablet.addEventListener("change", syncLayout);
    mqCoarse.addEventListener("change", syncLayout);

    // Height available inside the scrolling content area (header, dock, nav excluded).
    const content = rootEl?.closest("main") as HTMLElement | null;
    const measure = () => {
      if (!content || !rootEl) return;
      const cs = getComputedStyle(content);
      const h = content.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
      rootEl.style.setProperty("--plectr-avail-h", `${Math.max(320, Math.floor(h))}px`);
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (content) ro.observe(content);

    const onPrefs = () => {
      vizMode = plectrBackdropMode(loadUserPrefs().visualizerMode);
    };
    window.addEventListener("storage", onPrefs);

    return () => {
      mqPhone.removeEventListener("change", syncLayout);
      mqTablet.removeEventListener("change", syncLayout);
      mqCoarse.removeEventListener("change", syncLayout);
      ro.disconnect();
      window.removeEventListener("storage", onPrefs);
      document.documentElement.removeAttribute("data-plectr-immersive");
      document.documentElement.removeAttribute("data-plectr-playing");
      window.clearTimeout(resultsTimer);
      loader.destroy();
      // Let a prefetch finish; the worker goes once nothing is pending.
      window.setTimeout(releaseAnalysisWorker, 30_000);
    };
  });
</script>

{#snippet stageOverlay()}
  {#if errorCode}
    <div class="plectr-status plectr-status--error plectr-overlay" role="alert">
      <PlectrCover track={target} size={256} class="plectr-status__art" />
      <p>{t(`plectr.errors.${errorCode}`)}</p>
      <p class="plectr-status__sub">{t("plectr.loading.keepPlaying")}</p>
      <div class="plectr-status__actions">
        <button
          type="button"
          class="rk-btn rk-btn--secondary"
          onclick={() => target && loader.load({ rel_path: target.rel_path, title: target.title }, true)}
        >
          <UiIcon name="sync" />
          {t("plectr.retry")}
        </button>
        <button type="button" class="rk-btn rk-btn--secondary" onclick={openLibrary}>
          <UiIcon name="disc" />
          {t("plectr.empty.openLibrary")}
        </button>
        {#if hasNext}
          <button type="button" class="rk-btn rk-btn--ghost" onclick={playNext}>
            <UiIcon name="next" />
            {t("plectr.nextTrack")}
          </button>
        {/if}
      </div>
    </div>
  {:else if !chart}
    <div class="plectr-status plectr-status--loading plectr-overlay" aria-live="polite" aria-busy="true">
      <div class="plectr-prepare">
        <svg viewBox="0 0 120 120" class="plectr-prepare__ring" aria-hidden="true">
          <circle cx="60" cy="60" r="54" class="plectr-ring__track" />
          <circle
            cx="60"
            cy="60"
            r="54"
            class="plectr-ring__value"
            stroke-dasharray={2 * Math.PI * 54}
            stroke-dashoffset={2 * Math.PI * 54 * (1 - loadProgress)}
          />
        </svg>
        <PlectrCover track={target} size={256} class="plectr-prepare__art" />
      </div>
      <p>{loadLabel}</p>
      <p class="plectr-status__sub">{t("plectr.loading.keepPlaying")}</p>
    </div>
  {:else if results}
    <RunResults
      data={results}
      {hasNext}
      oncontinue={player.playing ? () => (results = null) : undefined}
      onreplay={replay}
      onnext={playNext}
      onexit={exitPlectr}
      onshare={share}
    />
  {:else if stagePhase === "paused" && !showSettings}
    <PauseMenu
      reason={pauseReason}
      onresume={() => stageRef?.resume()}
      onrestart={restart}
      onsettings={openSettings}
      onexit={exitPlectr}
    />
  {/if}
{/snippet}

{#snippet emptyState()}
  <section class="plectr-idle" aria-labelledby="plectr-idle-title">
    <header class="plectr-idle__head">
      <span class="plectr-brand"><UiIcon name="plectrum" /> {t("plectr.title")}</span>
      <div class="plectr-idle__tools">
        <button type="button" class="plectr-icon-btn" onclick={() => (section = "records")} aria-label={t("plectr.records.open")} title={t("plectr.records.open")}>
          <UiIcon name="trophy" />
        </button>
        <button type="button" class="plectr-icon-btn" onclick={openSettings} aria-label={t("plectr.settings.open")} title={t("plectr.settings.open")}>
          <UiIcon name="settings" />
        </button>
      </div>
    </header>
    <div class="plectr-idle__body">
      <span class="plectr-idle__icon" aria-hidden="true"><UiIcon name="plectrum" /></span>
      <h2 id="plectr-idle-title">{t("plectr.empty.title")}</h2>
      <p>{externalInPlayer ? t("plectr.empty.external") : t("plectr.empty.hint")}</p>
      <div class="plectr-idle__actions">
        <button type="button" class="rk-btn rk-btn--primary" onclick={openLibrary}>
          <UiIcon name="disc" />
          {t("plectr.empty.openLibrary")}
        </button>
        <button type="button" class="rk-btn rk-btn--secondary" onclick={shuffleLibrary}>
          <UiIcon name="shuffle" />
          {t("plectr.empty.shuffle")}
        </button>
      </div>
    </div>
  </section>
{/snippet}

{#snippet stageScreen()}
  {#if !target}
    {@render emptyState()}
  {:else}
    <GameStage
      bind:this={stageRef}
      {chart}
      relPath={target.rel_path}
      title={target.title}
      {startToken}
      {noteSpeed}
      latencyMs={settings.latencyMs}
      keys={settings.keys}
      keyLetters={settings.keyLetters && layout !== "phone" && !coarsePointer}
      vibration={settings.vibration}
      challenge={settings.challenge}
      {light}
      backdrop={settings.backdrop}
      tint={artState.tint}
      {vizMode}
      difficulty={activeDifficulty}
      playable={chartSet ? playableIds : []}
      best={currentBest}
      lastRun={currentLastRun}
      compact={layout === "phone"}
      covered={!!results || showSettings || showInfo}
      watchFps={settings.lightStage === "auto" && !light}
      overlay={stageOverlay}
      onphase={onStagePhase}
      onfinish={onFinish}
      onstats={(s) => (stats = s)}
      onlowfps={onLowFps}
      ondifficulty={setDifficulty}
      onsettings={openSettings}
      onexit={layout === "phone" ? exitPlectr : undefined}
    />
  {/if}
{/snippet}

<svelte:window onkeydown={onWindowKeyDown} />

<div
  bind:this={rootEl}
  class="plectr-page"
  class:is-immersive={immersive}
  class:is-records={section === "records"}
  class:is-light={light}
  data-layout={layout}
  data-phase={phase}
  style:--plectr-glow={artState.tint ?? "var(--rk-accent-2)"}
>
  {#if section === "records"}
    <RecordsView
      {store}
      onback={() => (section = "play")}
      onplay={(tr) => playTrack(tr)}
    />
  {:else}
    {#if framed}
      <div
        class="plectr-backdrop"
        aria-hidden="true"
        style:background-image={artState.dataUrl ? `url(${artState.dataUrl})` : undefined}
      ></div>
    {/if}

    <div class="plectr-layout">
      {#if layout === "desktop"}
        <aside class="plectr-side plectr-side--left">
          {#if panelTrack}
            <TrackPanel
              track={panelTrack}
              {store}
              difficulty={activeDifficulty}
              {chart}
              lastRun={currentLastRun}
              live={panelIsPlaying}
              eyebrow={panelEyebrow}
              onsettings={openSettings}
            />
          {/if}
        </aside>
      {/if}

      <div class="plectr-device" class:is-framed={framed} class:is-empty={phase === "empty"}>
        {#if layout === "tablet" && phase === "stage"}
          <button
            type="button"
            class="plectr-info-btn"
            aria-label={t("plectr.panel.info")}
            title={t("plectr.panel.info")}
            onclick={() => (showInfo = true)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"
              ><path
                fill="currentColor"
                d="M11 7h2v2h-2zm0 4h2v6h-2zm1-9a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16z"
              /></svg
            >
          </button>
        {/if}
        <div class="plectr-device__screen">
          {@render stageScreen()}
        </div>
      </div>

      {#if layout === "desktop"}
        <aside class="plectr-side plectr-side--right">
          {#if phase === "empty"}
            <CareerCard {career} compact />
          {:else}
            <SessionPanel {stats} keys={settings.keys} {live} />
          {/if}
        </aside>
      {/if}
    </div>

    {#if layout === "tablet" && showInfo && panelTrack}
      <div class="plectr-sheet" role="dialog" aria-modal="true" aria-label={t("plectr.panel.info")}>
        <button type="button" class="plectr-sheet__scrim" aria-label={t("plectr.close")} onclick={() => (showInfo = false)}></button>
        <div class="plectr-sheet__panel">
          <TrackPanel
            track={panelTrack}
            {store}
            difficulty={activeDifficulty}
            {chart}
            lastRun={currentLastRun}
            live={panelIsPlaying}
            eyebrow={panelEyebrow}
            onsettings={() => {
              showInfo = false;
              showSettings = true;
            }}
          />
          <SessionPanel {stats} keys={settings.keys} {live} />
          <button type="button" class="rk-btn rk-btn--secondary" onclick={() => (showInfo = false)}>{t("plectr.close")}</button>
        </div>
      </div>
    {/if}
  {/if}
</div>

<PlectrSettings
  open={showSettings}
  {settings}
  canReset={career.tracksPlayed > 0 || career.runs > 0}
  onchange={(patch) => plectrRecords.setSettings(patch)}
  onreset={() => void resetRecords()}
  onclose={() => (showSettings = false)}
/>
