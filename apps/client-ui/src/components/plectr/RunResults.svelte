<script lang="ts" module>
  import type { DifficultyId, GameResult } from "../../lib/plectr/types";

  export type ResultsData = {
    relPath: string;
    title: string;
    artist: string;
    albumId: number | null;
    difficulty: DifficultyId;
    result: GameResult;
    perfects: number;
    goods: number;
    earlies: number;
    lates: number;
    skipped: number;
    judged: number;
    notesHit: number;
    totalNotes: number;
    counted: boolean;
    newRecord: boolean;
    previous: GameResult | null;
    fc: boolean;
    ap: boolean;
  };
</script>

<script lang="ts">
  /**
   * Results over the whole stage: animated grade stamp, score count-up,
   * new-record ribbon (or the previous best), accuracy ring, max combo,
   * judgement bar, notes hit out of the notes judged (skipped excluded),
   * FC / AP badges, then Continue (when the music plays on) / Play again /
   * Next song / Change song / Exit.
   */
  import { onMount } from "svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import PlectrCover from "./PlectrCover.svelte";
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { prefersReducedMotion } from "../../lib/visualizer/renderQuality";


  let {
    data,
    hasNext = true,
    onreplay,
    onnext,
    onchange,
    onexit,
    onshare,
    oncontinue,
  }: {
    data: ResultsData;
    hasNext?: boolean;
    onreplay: () => void;
    onnext: () => void;
    onchange: () => void;
    onexit: () => void;
    onshare?: () => void;
    /** The music plays on (next song already running): close and keep playing. */
    oncontinue?: () => void;
  } = $props();

  const r = $derived(data.result);
  const pct = (n: number) => `${Math.round(n * 1000) / 10}%`;
  const okCount = $derived(data.earlies + data.lates);
  const parts = $derived(
    data.judged > 0
      ? [
          { key: "perfect", n: data.perfects },
          { key: "good", n: data.goods },
          { key: "timing", n: okCount },
          { key: "miss", n: r.misses },
        ].filter((p) => p.n > 0)
      : [],
  );

  let shownScore = $state(0);
  let replayBtn = $state<HTMLButtonElement | null>(null);
  const RING = 2 * Math.PI * 34;

  onMount(() => {
    replayBtn?.focus({ preventScroll: true });
    const target = r.score;
    if (prefersReducedMotion() || target <= 0) {
      shownScore = target;
      return;
    }
    const start = performance.now();
    const dur = 900;
    let raf = 0;
    const step = (now: number) => {
      const k = Math.min(1, (now - start) / dur);
      shownScore = Math.round(target * (1 - Math.pow(1 - k, 3)));
      if (k < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div class="plectr-results plectr-overlay" role="dialog" aria-modal="true" aria-labelledby="plectr-results-title">
  <div class="plectr-results__card">
    <header class="plectr-results__head">
      <PlectrCover track={{ album_id: data.albumId, rel_path: data.relPath, title: data.title }} class="plectr-results__art" />
      <div class="plectr-results__track">
        <span class="plectr-results__eyebrow">
          {r.failed ? t("plectr.results.failed") : t("plectr.results.complete")} · {t(`plectr.diff.${data.difficulty}`)}
        </span>
        <h3 id="plectr-results-title">{data.title}</h3>
        <span class="plectr-results__artist">{data.artist}</span>
      </div>
    </header>

    <div class="plectr-results__hero">
      <span class="plectr-results__grade plectr-results__grade--{r.grade.toLowerCase()}">{r.grade}</span>
      <div class="plectr-results__score">
        <strong>{fmtNumber(shownScore)}</strong>
        {#if data.newRecord}
          <span class="plectr-results__ribbon"><UiIcon name="trophy" /> {t("plectr.results.newRecord")}</span>
        {:else if data.previous}
          <span class="plectr-results__prev">
            {t("plectr.results.previousBest", { score: fmtNumber(data.previous.score), grade: data.previous.grade })}
          </span>
        {/if}
        {#if data.ap}
          <span class="plectr-results__badge plectr-results__badge--ap">{t("plectr.results.allPerfect")}</span>
        {:else if data.fc}
          <span class="plectr-results__badge">{t("plectr.results.fullCombo")}</span>
        {/if}
      </div>
    </div>

    <div class="plectr-results__metrics">
      <div class="plectr-results__ring" role="img" aria-label={`${t("plectr.results.accuracy")} ${pct(r.accuracy)}`}>
        <svg viewBox="0 0 80 80" aria-hidden="true">
          <circle cx="40" cy="40" r="34" class="plectr-ring__track" />
          <circle
            cx="40"
            cy="40"
            r="34"
            class="plectr-ring__value"
            stroke-dasharray={RING}
            stroke-dashoffset={RING * (1 - r.accuracy)}
          />
        </svg>
        <span><strong>{pct(r.accuracy)}</strong><small>{t("plectr.results.accuracy")}</small></span>
      </div>
      <dl class="plectr-results__facts">
        <div>
          <dt>{t("plectr.results.maxCombo")}</dt>
          <dd>×{r.maxCombo}</dd>
        </div>
        <div>
          <dt>{t("plectr.results.notesJudged")}</dt>
          <dd>{t("plectr.results.notesValue", { hit: data.notesHit, total: data.judged })}</dd>
        </div>
        {#if data.skipped > 0}
          <div>
            <dt>{t("plectr.results.skipped")}</dt>
            <dd>{data.skipped}</dd>
          </div>
        {/if}
      </dl>
    </div>

    {#if parts.length}
      <div class="plectr-results__bar" aria-hidden="true">
        {#each parts as p (p.key)}
          <span class="plectr-results__seg plectr-results__seg--{p.key}" style="flex-grow: {p.n}"></span>
        {/each}
      </div>
    {/if}
    <ul class="plectr-results__legend">
      <li class="is-perfect"><i></i>{t("plectr.results.perfect")} <strong>{data.perfects}</strong></li>
      <li class="is-good"><i></i>{t("plectr.results.good")} <strong>{data.goods}</strong></li>
      <li class="is-timing">
        <i></i>{t("plectr.results.earlyLate")} <strong>{data.earlies}/{data.lates}</strong>
      </li>
      <li class="is-miss"><i></i>{t("plectr.results.miss")} <strong>{r.misses}</strong></li>
    </ul>
    {#if !data.counted}
      <p class="plectr-results__note">{t("plectr.results.notCounted")}</p>
    {/if}

    <div class="plectr-results__actions">
      {#if oncontinue}
        <button type="button" class="rk-btn rk-btn--primary plectr-results__continue" onclick={oncontinue}>
          <UiIcon name="play" />
          {t("plectr.continue")}
        </button>
      {/if}
      <button bind:this={replayBtn} type="button" class="rk-btn {oncontinue ? 'rk-btn--secondary' : 'rk-btn--primary'}" onclick={onreplay}>
        <UiIcon name="repeat" />
        {t("plectr.replay")}
      </button>
      <button type="button" class="rk-btn rk-btn--secondary" disabled={!hasNext} onclick={onnext}>
        <UiIcon name="next" />
        {t("plectr.nextTrack")}
      </button>
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onchange}>
        <UiIcon name="queueMusic" />
        {t("plectr.changeTrack")}
      </button>
      <button type="button" class="rk-btn rk-btn--ghost" onclick={onexit}>
        <UiIcon name="close" />
        {t("plectr.exit")}
      </button>
    </div>
    {#if onshare}
      <button type="button" class="plectr-results__share" onclick={onshare}>
        <UiIcon name="download" />
        {t("plectr.results.share")}
      </button>
    {/if}
  </div>
</div>
