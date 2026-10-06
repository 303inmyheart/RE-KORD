<script lang="ts">
  /**
   * Left panel (desktop) / info sheet (tablet): the song the game runs on —
   * cover, title, a "now playing" pulse, chart facts and the records per
   * difficulty. Read-only: the difficulty is chosen on the stage only.
   */
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
    live = false,
    eyebrow = null,
    /** Cover + title block (off on the pick screen, which shows the song itself). */
    hero = true,
    onchange,
    onsettings,
  }: {
    track: { rel_path: string; title: string; artist_name: string; album_id: number | null; album_name?: string } | null;
    store: PlectrStore;
    difficulty: DifficultyId;
    chart?: Chart | null;
    lastRun?: GameResult | null;
    /** The song is playing under the game (animated "now playing" mark). */
    live?: boolean;
    eyebrow?: string | null;
    hero?: boolean;
    onchange?: () => void;
    onsettings?: () => void;
  } = $props();

  const slots = $derived(track ? (lookupByRelPathAliases(store.byDifficulty, track.rel_path) ?? {}) : {});
</script>

<section class="plectr-panel plectr-panel--track rk-surface-card" aria-label={t("plectr.panel.track")}>
  {#if track && !hero}
    <div class="plectr-panel__mini">
      <span class="plectr-panel__eyebrow">{eyebrow ?? t("plectr.pick.selected")}</span>
      <strong>{track.title}</strong>
    </div>
  {/if}
  {#if track}
    {#if hero}
    <div class="plectr-panel__hero">
      <PlectrCover {track} size={256} class="plectr-panel__art" />
      <div class="plectr-panel__title">
        <span class="plectr-panel__eyebrow" class:is-live={live}>
          {#if live}<i class="plectr-eq" aria-hidden="true"><b></b><b></b><b></b></i>{/if}
          {eyebrow ?? t("plectr.nowPlaying")}
        </span>
        <strong>{track.title}</strong>
        <span>{track.artist_name}{#if track.album_name}{" · "}{track.album_name}{/if}</span>
      </div>
    </div>
    {/if}

    {#if chart}
      <ul class="plectr-panel__tags" aria-label={t("plectr.panel.track")}>
        <li><strong>{Math.round(chart.stats.bpm)}</strong> {t("plectr.bpm")}</li>
        <li><strong>{fmtNumber(chart.notes.length)}</strong> {t("plectr.notes")}</li>
      </ul>
    {/if}

    <div class="plectr-panel__section">
      <h3 class="plectr-panel__heading">{t("plectr.records.open")}</h3>
      <ul class="plectr-panel__bests">
        {#each DIFFICULTIES as d (d.id)}
          {@const best = slots[d.id]}
          <li class="is-{d.id}" class:is-current={d.id === difficulty} aria-current={d.id === difficulty ? "true" : undefined}>
            <span class="plectr-panel__diffname"><i class="plectr-dot plectr-dot--{d.id}"></i>{t(`plectr.diff.${d.id}`)}</span>
            {#if best}
              {#if best.ap}<em>{t("plectr.ap")}</em>{:else if best.fc}<em>{t("plectr.fc")}</em>{/if}
              <span class="plectr-panel__score">{fmtNumber(best.score)}</span>
              <strong class="plectr-badge plectr-grade--{best.grade.toLowerCase()}">{best.grade}</strong>
            {:else}
              <span class="plectr-panel__score is-empty">{t("plectr.panel.notPlayed")}</span>
            {/if}
          </li>
        {/each}
      </ul>
      {#if lastRun}
        <p class="plectr-panel__last">
          {t("plectr.lastRun")} <strong>{fmtNumber(lastRun.score)}</strong>
          <span class="plectr-grade plectr-grade--{lastRun.grade.toLowerCase()}">{lastRun.grade}</span>
        </p>
      {/if}
    </div>
  {:else}
    <p class="plectr-empty">{t("plectr.noTrackHint")}</p>
  {/if}

  {#if onchange || onsettings}
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
  {/if}
</section>
