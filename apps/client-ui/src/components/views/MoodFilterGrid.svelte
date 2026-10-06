<script lang="ts">
  /**
   * Mood filter: "Almeno uno / Tutti" segmented control + one button per mood
   * (legacy: glyph + count, the mood's name in the tooltip / accessible label)
   * filling the row. Moods with no tracks stay visible but quiet and inert.
   */
  import { Segmented } from "@rekord/ui";
  import TrackMoodGlyph from "../TrackMoodGlyph.svelte";
  import { t } from "../../lib/i18n.svelte";
  import {
    TRACK_MOOD_COLORS,
    TRACK_MOOD_IDS,
    trackMoodLabelKey,
    type TrackMoodId,
  } from "../../lib/trackMoods";

  let {
    selected,
    counts,
    matchAll,
    /** Counts are known (catalog loaded): zero-count moods become inert. */
    countsReady = true,
    label = "",
    ontoggle,
    onmatch,
    tools,
  }: {
    selected: readonly string[];
    counts: Record<TrackMoodId, number>;
    matchAll: boolean;
    countsReady?: boolean;
    label?: string;
    ontoggle: (id: TrackMoodId) => void;
    onmatch: (all: boolean) => void;
    tools?: import("svelte").Snippet;
  } = $props();
</script>

<div class="mood-filter">
  <div class="mood-filter__head">
    {#if label}<span class="mood-filter__label">{label}</span>{/if}
    <Segmented
      ariaLabel={t("library.moodMatchAria")}
      value={matchAll ? "all" : "any"}
      onchange={(v) => onmatch(v === "all")}
      options={[
        { value: "any", label: t("library.moodMatchAny"), title: t("library.moodMatchAnyTitle") },
        { value: "all", label: t("library.moodMatchAll"), title: t("library.moodMatchAllTitle") },
      ]}
    />
    {#if tools}<span class="mood-filter__tools">{@render tools()}</span>{/if}
  </div>
  <div class="mood-filter__grid">
    {#each TRACK_MOOD_IDS as id (id)}
      {@const count = counts[id] ?? 0}
      {@const on = selected.includes(id)}
      {@const empty = countsReady && count === 0 && !on}
      <button
        type="button"
        class="mood-filter__btn"
        class:is-on={on}
        class:is-empty={empty}
        style="--mood-c:{TRACK_MOOD_COLORS[id]}"
        disabled={empty}
        aria-pressed={on}
        title={t(trackMoodLabelKey(id))}
        aria-label={`${t(trackMoodLabelKey(id))} (${count})`}
        onclick={() => ontoggle(id)}
      >
        <TrackMoodGlyph mood={id} inheritColor />
        <span class="mood-filter__count" aria-hidden="true">{count}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .mood-filter {
    display: grid;
    gap: 0.6rem;
    min-width: 0;
  }

  .mood-filter__head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
  }

  .mood-filter__label {
    font-size: var(--rk-fs-sm);
    font-weight: 650;
    color: var(--rk-ink);
    margin-right: auto;
  }

  .mood-filter__tools {
    display: inline-flex;
    gap: 0.5rem;
    margin-left: auto;
  }

  .mood-filter__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(4.6rem, 100%), 1fr));
    gap: 0.45rem;
  }

  .mood-filter__btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.42rem;
    min-height: 2.75rem;
    padding: 0.45rem 0.52rem;
    border-radius: var(--rk-radius-lg, 8px);
    border: 1px solid color-mix(in srgb, var(--rk-line) 62%, transparent);
    background: color-mix(in srgb, var(--rk-surface-2) 88%, transparent);
    color: color-mix(in srgb, var(--rk-muted) 78%, var(--rk-ink) 22%);
    font: inherit;
    line-height: 1;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
  }

  .mood-filter__btn :global(svg) {
    width: 1.22rem;
    height: 1.22rem;
  }

  .mood-filter__btn:hover:not(:disabled):not(.is-on) {
    border-color: color-mix(in srgb, var(--mood-c) 45%, var(--rk-line));
    background: color-mix(in srgb, var(--rk-surface-3) 76%, transparent);
    color: color-mix(in srgb, var(--mood-c) 70%, var(--rk-ink));
  }

  .mood-filter__btn.is-on {
    color: color-mix(in srgb, var(--mood-c) 90%, var(--rk-ink));
    background: color-mix(in srgb, var(--mood-c) 16%, transparent);
    border-color: color-mix(in srgb, var(--mood-c) 58%, var(--rk-line));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--mood-c) 22%, transparent);
  }

  /* No tracks with this mood: inert. */
  .mood-filter__btn.is-empty {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .mood-filter__count {
    font-size: var(--rk-fs-xs);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: inherit;
  }
</style>
