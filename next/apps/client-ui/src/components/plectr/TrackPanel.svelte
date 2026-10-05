<script lang="ts">
  /** Left panel (desktop) / info sheet (tablet): the track, its records per difficulty, chart facts. */
  import UiIcon from "../icons/UiIcon.svelte";
  import PlectrCover from "./PlectrCover.svelte";
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { DIFFICULTIES } from "../../lib/plectr/config";
  import { lookupByRelPathAliases, type PlectrStore } from "../../lib/plectr/records";
  import type { Chart, DifficultyId, GameResult } from "../../lib/plectr/types";

  let {
    track,
    store,
    difficulty,
    chart = null,
    lastRun = null,
    onchange,
    onsettings,
  }: {
    track: { rel_path: string; title: string; artist_name: string; album_id: number | null } | null;
    store: PlectrStore;
    difficulty: DifficultyId;
    chart?: Chart | null;
    lastRun?: GameResult | null;
    onchange?: () => void;
    onsettings?: () => void;
  } = $props();

  const slots = $derived(track ? (lookupByRelPathAliases(store.byDifficulty, track.rel_path) ?? {}) : {});
  const level = $derived(DIFFICULTIES.find((d) => d.id === difficulty)?.level ?? 0);
</script>

<section class="plectr-panel plectr-panel--track" aria-label={t("plectr.panel.track")}>
  {#if track}
    <PlectrCover {track} size={256} class="plectr-panel__art" />
    <div class="plectr-panel__title">
      <strong>{track.title}</strong>
      <span>{track.artist_name}</span>
    </div>
  {:else}
    <p class="plectr-empty">{t("plectr.noTrackHint")}</p>
  {/if}

  <p class="plectr-panel__diff">
    <span class="plectr-dot plectr-dot--{difficulty}"></span>
    {t(`plectr.diff.${difficulty}`)} · {t("plectr.level", { n: level })}
  </p>

  {#if track}
    <ul class="plectr-panel__bests">
      {#each DIFFICULTIES as d (d.id)}
        {@const best = slots[d.id]}
        <li class:is-current={d.id === difficulty}>
          <span>{t(`plectr.diff.${d.id}`)}</span>
          {#if best}
            <strong class="plectr-grade plectr-grade--{best.grade.toLowerCase()}">{best.grade}</strong>
            <span class="plectr-panel__score">{fmtNumber(best.score)}</span>
            {#if best.ap}<em>{t("plectr.ap")}</em>{:else if best.fc}<em>{t("plectr.fc")}</em>{/if}
          {:else}
            <span class="plectr-panel__score is-empty">—</span>
          {/if}
        </li>
      {/each}
    </ul>
    <dl class="plectr-panel__facts">
      <div>
        <dt>{t("plectr.lastRun")}</dt>
        <dd>{lastRun ? `${fmtNumber(lastRun.score)} · ${lastRun.grade}` : "—"}</dd>
      </div>
      {#if chart}
        <div><dt>{t("plectr.bpm")}</dt><dd>{Math.round(chart.stats.bpm)}</dd></div>
        <div><dt>{t("plectr.notes")}</dt><dd>{chart.notes.length}</dd></div>
      {/if}
    </dl>
  {/if}

  <div class="plectr-panel__actions">
    {#if onchange}
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onchange}>
        <UiIcon name="queueMusic" />
        {t("plectr.changeTrack")}
      </button>
    {/if}
    {#if onsettings}
      <button type="button" class="plectr-icon-btn" onclick={onsettings} aria-label={t("plectr.settings.open")} title={t("plectr.settings.open")}>
        <UiIcon name="settings" />
      </button>
    {/if}
  </div>
</section>
