<script lang="ts">
  /**
   * Mood picker shared by the dashboard "Playlist al volo" and Library › Mood.
   *
   * - one toggle chip per mood: glyph (mood colour) + name + track count, so it
   *   reads without hovering (icon-only stays for dense track rows);
   * - moods nobody tagged in the library are tucked behind "+N senza brani";
   *   a mood with tracks in the library but none in the current choice
   *   (`counts` = 0) stays in place, inert, so chips never jump;
   * - "Almeno uno / Tutti insieme" only appears once two moods are picked, with
   *   one line saying what it does.
   */
  import { Segmented } from "@rekord/ui";
  import TrackMoodGlyph from "../TrackMoodGlyph.svelte";
  import { t, tp } from "../../lib/i18n.svelte";
  import { visibleMoodIds } from "../../lib/mixFilter";
  import {
    TRACK_MOOD_COLORS,
    trackMoodLabelKey,
    type TrackMoodId,
  } from "../../lib/trackMoods";

  let {
    selected,
    counts,
    totals = counts,
    matchAll,
    /** Counts are known (catalog loaded): zero-count moods become inert. */
    countsReady = true,
    label = "",
    ontoggle,
    onmatch,
    tools,
  }: {
    selected: readonly string[];
    /** Tracks per mood in the current context (live). */
    counts: Record<TrackMoodId, number>;
    /** Library-wide tracks per mood: 0 here = tucked away. Defaults to `counts`. */
    totals?: Record<TrackMoodId, number>;
    matchAll: boolean;
    countsReady?: boolean;
    label?: string;
    ontoggle: (id: TrackMoodId) => void;
    onmatch: (all: boolean) => void;
    tools?: import("svelte").Snippet;
  } = $props();

  let showAll = $state(false);

  const picked = $derived(selected as readonly TrackMoodId[]);
  const rows = $derived(
    countsReady ? visibleMoodIds(totals, picked, showAll) : visibleMoodIds(totals, picked, true),
  );
  const noneTagged = $derived(countsReady && !picked.length && rows.shown.length === 0);
</script>

<div class="mood-filter">
  {#if label || tools}
    <div class="mood-filter__head">
      {#if label}<span class="mix-row-label">{label}</span>{/if}
      {#if tools}<span class="mood-filter__tools">{@render tools()}</span>{/if}
    </div>
  {/if}

  {#if noneTagged}
    <p class="mix-row-note">{t("mix.noMoodsYet")}</p>
  {:else}
  <div class="mix-chips" role="group" aria-label={label || t("dashboard.moods")}>
    {#each rows.shown as id (id)}
      {@const count = counts[id] ?? 0}
      {@const on = picked.includes(id)}
      {@const empty = countsReady && count === 0 && !on}
      {@const name = t(trackMoodLabelKey(id))}
      <button
        type="button"
        class="mix-chip mix-chip--mood"
        class:is-on={on}
        style="--mood-c:{TRACK_MOOD_COLORS[id]}"
        disabled={empty}
        aria-pressed={on}
        aria-label={tp("mix.chipAria", count, { name })}
        onclick={() => ontoggle(id)}
      >
        <span class="mix-chip__glyph"><TrackMoodGlyph mood={id} inheritColor /></span>
        <span class="mix-chip__name">{name}</span>
        <span class="mix-chip__count" aria-hidden="true">{count}</span>
      </button>
    {/each}
    {#if countsReady && (rows.hidden > 0 || showAll)}
      <button
        type="button"
        class="mix-more"
        aria-expanded={showAll}
        onclick={() => (showAll = !showAll)}
      >
        {showAll ? t("mix.hideEmptyMoods") : tp("mix.moreMoods", rows.hidden)}
      </button>
    {/if}
  </div>
  {/if}

  {#if picked.length >= 2}
    <div class="mix-match">
      <span class="mix-match__label">{t("mix.matchLabel")}</span>
      <Segmented
        ariaLabel={t("library.moodMatchAria")}
        value={matchAll ? "all" : "any"}
        onchange={(v) => onmatch(v === "all")}
        options={[
          { value: "any", label: t("library.moodMatchAny") },
          { value: "all", label: t("library.moodMatchAll") },
        ]}
      />
      <span class="mix-match__help">
        {matchAll ? t("library.moodMatchAllTitle") : t("library.moodMatchAnyTitle")}
      </span>
    </div>
  {/if}
</div>

<style>
  .mood-filter {
    display: grid;
    gap: var(--rk-space-sm);
    min-width: 0;
  }

  .mood-filter__head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--rk-space-xs) var(--rk-space-md);
    min-height: 1.5rem;
  }

  .mood-filter__tools {
    display: inline-flex;
    gap: var(--rk-space-sm);
    margin-left: auto;
  }
</style>
