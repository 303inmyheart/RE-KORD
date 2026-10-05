<script lang="ts">
  /**
   * Mood filter: "Almeno uno / Tutti" segmented control + one labelled button
   * per mood (glyph, name, count) filling the row. Moods with no tracks stay
   * visible but quiet and inert.
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
        onclick={() => ontoggle(id)}
      >
        <TrackMoodGlyph mood={id} inheritColor />
        <span class="mood-filter__name">{t(trackMoodLabelKey(id))}</span>
        <span class="mood-filter__count">{count}</span>
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
    grid-template-columns: repeat(auto-fill, minmax(min(10rem, 100%), 1fr));
    gap: 0.4rem;
  }

  .mood-filter__btn {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
    min-height: 2.6rem;
    padding: 0.35rem 0.65rem;
    border-radius: var(--rk-radius-md, 10px);
    border: 1px solid color-mix(in srgb, var(--mood-c) 32%, var(--rk-line));
    background: color-mix(in srgb, var(--mood-c) 8%, var(--rk-surface-2));
    color: color-mix(in srgb, var(--mood-c) 75%, var(--rk-ink));
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .mood-filter__btn :global(svg) {
    width: 1.1rem;
    height: 1.1rem;
  }

  .mood-filter__btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--mood-c) 60%, var(--rk-line));
  }

  .mood-filter__btn.is-on {
    background: color-mix(in srgb, var(--mood-c) 24%, var(--rk-surface-2));
    border-color: color-mix(in srgb, var(--mood-c) 75%, var(--rk-line));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--mood-c) 45%, transparent);
  }

  /* No tracks with this mood: readable but clearly inert (not a ghost at 25%). */
  .mood-filter__btn.is-empty {
    cursor: default;
    color: var(--rk-muted);
    border-color: var(--rk-line);
    border-style: dashed;
    background: transparent;
  }

  .mood-filter__name {
    min-width: 0;
    font-size: var(--rk-fs-sm);
    font-weight: 600;
    color: var(--rk-ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mood-filter__btn.is-empty .mood-filter__name {
    color: var(--rk-muted);
  }

  .mood-filter__count {
    font-size: var(--rk-fs-xs);
    font-variant-numeric: tabular-nums;
    color: var(--rk-muted);
  }
</style>
