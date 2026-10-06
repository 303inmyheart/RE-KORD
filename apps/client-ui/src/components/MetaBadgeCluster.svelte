<script lang="ts">
  /**
   * Status pills of a track / album / artist. Cards render only the badges
   * that carry a value; track rows (inline) keep the legacy ghosted slots.
   * Pills use the shared `.rk-pill` recipe (@rekord/ui base.css) with flat
   * pre-mixed colours — no `filter`, no opacity tricks (cheap on WebKitGTK).
   */
  import { t } from "../lib/i18n.svelte";
  import UiIcon from "./icons/UiIcon.svelte";
  import TrackMoodGlyph from "./TrackMoodGlyph.svelte";
  import type { TrackMoodId } from "../lib/trackMoods";
  import { TRACK_MOOD_COLORS, trackMoodLabelKey } from "../lib/trackMoods";

  let {
    /** Track/inline: meta gap + mood. Card/hero: album/artist status chips. */
    variant = "foot" as "foot" | "hero" | "inline",
    /** Track/genre only: shows mood. */
    moods = [] as TrackMoodId[],
    missingMeta = false,
    /** Count of albums without meta (artist) or album flag (if >0 → A on). */
    albumsMissingMetaCount = 0,
    tracksMissingMetaCount = 0,
    favoriteCount = 0,
    /** Whole album excluded. */
    albumExcluded = false,
    /** N albums excluded (artist). */
    albumsExcludedCount = 0,
    /** Excluded tracks (individually or by album). */
    tracksExcludedCount = 0,
    /** Loose album card: no "album metadata" badge. */
    loose = false,
  }: {
    variant?: "foot" | "hero" | "inline";
    moods?: TrackMoodId[];
    missingMeta?: boolean;
    albumsMissingMetaCount?: number;
    tracksMissingMetaCount?: number;
    favoriteCount?: number;
    albumExcluded?: boolean;
    albumsExcludedCount?: number;
    tracksExcludedCount?: number;
    loose?: boolean;
  } = $props();

  const isCard = $derived(variant === "foot" || variant === "hero");
  const albumMetaOn = $derived(!loose && (albumsMissingMetaCount > 0 || missingMeta));
  const albumMetaCount = $derived(albumsMissingMetaCount > 0 ? albumsMissingMetaCount : 0);
  const albumExclOn = $derived(albumExcluded || albumsExcludedCount > 0);
  const shownMoods = $derived(moods.slice(0, 3));
  // Track rows (inline) always show their two slots, like legacy: the meta-gap
  // note and the moods, ghosted when off so every row keeps the same rhythm.
  const any = $derived(
    isCard
      ? albumMetaOn ||
          tracksMissingMetaCount > 0 ||
          favoriteCount > 0 ||
          albumExclOn ||
          tracksExcludedCount > 0
      : true,
  );
  const moodSummary = $derived(
    shownMoods.length
      ? t("rowBadges.moodOnTitle", { labels: shownMoods.map((m) => t(trackMoodLabelKey(m))).join(", ") })
      : t("rowBadges.moodOffTitle"),
  );
</script>

{#if any}
  <span
    class="lib-meta-badges"
    class:lib-meta-badges--foot={variant === "foot"}
    class:lib-meta-badges--hero={variant === "hero"}
    class:lib-meta-badges--inline={variant === "inline"}
  >
    {#if isCard}
      {#if favoriteCount > 0}
        <span class="rk-pill rk-pill--favorite" title={t("badges.favorites", { n: favoriteCount })}>
          <UiIcon name="favorite" />{favoriteCount}
        </span>
      {/if}
      {#if albumMetaOn}
        <span class="rk-pill rk-pill--warning" title={t("badges.albumMetaMissing")}>
          <UiIcon name="album" />{#if albumMetaCount > 0}{albumMetaCount}{/if}
        </span>
      {/if}
      {#if tracksMissingMetaCount > 0}
        <span
          class="rk-pill rk-pill--warning"
          title={t("badges.tracksNoMeta", { n: tracksMissingMetaCount })}
        >
          <UiIcon name="music" />{tracksMissingMetaCount}
        </span>
      {/if}
      {#if albumExclOn}
        <span class="rk-pill rk-pill--accent2" title={t("badges.albumExcluded")}>
          <UiIcon name="exclude" /><UiIcon name="album" />{#if albumsExcludedCount > 0}{albumsExcludedCount}{/if}
        </span>
      {/if}
      {#if tracksExcludedCount > 0}
        <span class="rk-pill" title={t("badges.tracksExcluded", { n: tracksExcludedCount })}>
          <UiIcon name="exclude" />{tracksExcludedCount}
        </span>
      {/if}
    {:else}
      <span
        class="lib-meta-chip lib-meta-chip--ico"
        class:lib-meta-chip--on={missingMeta}
        role="img"
        title={missingMeta ? t("rowBadges.gapOnTitle") : t("rowBadges.gapOffTitle")}
        aria-label={missingMeta ? t("rowBadges.gapOnTitle") : t("rowBadges.gapOffTitle")}
      >
        <UiIcon name="music" />
      </span>
      <span class="track-meta-moods-cluster" title={moodSummary}>
        {#if shownMoods.length === 0}
          <span class="lib-meta-chip lib-meta-chip--ico lib-meta-chip--mood-off" role="img" aria-label={moodSummary}>
            <TrackMoodGlyph mood={null} inheritColor />
          </span>
        {:else}
          {#each shownMoods as m (m)}
            <span
              class="lib-meta-chip lib-meta-chip--ico lib-meta-chip--mood-tag"
              role="img"
              title={t(trackMoodLabelKey(m))}
              aria-label={t(trackMoodLabelKey(m))}
              style:--mood-c={TRACK_MOOD_COLORS[m]}
            >
              <TrackMoodGlyph mood={m} inheritColor />
            </span>
          {/each}
        {/if}
      </span>
    {/if}
  </span>
{/if}

<style>
  .lib-meta-badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
    max-width: 100%;
  }

  .lib-meta-badges--foot {
    margin-top: 0.35rem;
  }

  .lib-meta-badges--hero {
    margin: 0;
  }

  .lib-meta-badges--inline {
    display: inline-flex;
    flex-wrap: nowrap;
    gap: 0.2rem;
  }

  /* Card / hero badges keep the legacy chip shape: a small rounded square
     (6px), not a capsule. */
  .lib-meta-badges :global(.rk-pill) {
    min-height: 1.3rem;
    padding: 0.2rem 0.38rem;
    gap: 0.18rem;
    border-radius: var(--rk-radius, 6px);
    font-size: var(--rk-fs-2xs, 0.68rem);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
  }

  .lib-meta-badges :global(.rk-pill .ui-ic),
  .lib-meta-badges :global(.rk-pill .mood-g) {
    width: 0.75rem;
    height: 0.75rem;
  }

  /* Track-row chips (legacy `.lib-meta-chip`): small rounded squares, ghosted
     while off, tinted while on. Icons only — names live in the tooltip. */
  .lib-meta-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.12rem;
    flex: 0 0 auto;
    padding: 0.2rem 0.32rem;
    border-radius: var(--rk-radius, 6px);
    border: 1px solid color-mix(in srgb, var(--rk-line) 35%, transparent);
    background: color-mix(in srgb, var(--rk-muted) 3%, var(--rk-surface-2));
    color: color-mix(in srgb, var(--rk-muted) 55%, var(--rk-surface-2));
    line-height: 1;
    opacity: 0.28;
  }

  .lib-meta-chip :global(.ui-ic),
  .lib-meta-chip :global(.mood-g) {
    width: 0.72rem;
    height: 0.72rem;
  }

  .lib-meta-chip--on {
    opacity: 1;
    background: color-mix(in srgb, var(--rk-warning) 14%, var(--rk-surface-2));
    color: var(--rk-ink);
    border-color: color-mix(in srgb, var(--rk-warning) 32%, var(--rk-line));
  }

  .track-meta-moods-cluster {
    display: inline-flex;
    align-items: center;
    gap: 0.18rem;
    flex-wrap: nowrap;
    flex-shrink: 0;
  }

  .lib-meta-chip--mood-off {
    opacity: 0.14;
  }

  /* Mood colour nudged towards the text colour so yellow / lime stay legible
     on light themes too. */
  .lib-meta-chip--mood-tag {
    opacity: 1;
    color: color-mix(in srgb, var(--mood-c) 88%, var(--rk-ink));
    background: color-mix(in srgb, var(--mood-c) 18%, var(--rk-surface-2));
    border-color: color-mix(in srgb, var(--mood-c) 42%, var(--rk-line));
  }
</style>
