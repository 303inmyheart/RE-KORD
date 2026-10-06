<script lang="ts">
  /**
   * Status pills of a track / album / artist. Only badges that carry a value
   * are rendered (no ghost placeholders): a clean card shows nothing at all.
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
  const any = $derived(
    isCard
      ? albumMetaOn ||
          tracksMissingMetaCount > 0 ||
          favoriteCount > 0 ||
          albumExclOn ||
          tracksExcludedCount > 0
      : missingMeta || shownMoods.length > 0,
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
      {#if missingMeta}
        <span
          class="rk-pill rk-pill--warning rk-pill--icon"
          role="img"
          title={t("badges.metaIncomplete")}
          aria-label={t("badges.metaIncomplete")}
        >
          <UiIcon name="music" />
        </span>
      {/if}
      {#each shownMoods as m (m)}
        <span
          class="rk-pill rk-pill--icon lib-meta-mood"
          role="img"
          title={t(trackMoodLabelKey(m))}
          aria-label={t(trackMoodLabelKey(m))}
          style:--pill-c={TRACK_MOOD_COLORS[m]}
        >
          <TrackMoodGlyph mood={m} inheritColor />
        </span>
      {/each}
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

  /* Inline (track rows): a touch smaller so the stats line keeps one height. */
  .lib-meta-badges--inline .rk-pill--icon {
    min-width: 1.25rem;
    min-height: 1.25rem;
  }

  .lib-meta-badges :global(.rk-pill .ui-ic),
  .lib-meta-badges :global(.rk-pill .mood-g) {
    width: 0.8125rem;
    height: 0.8125rem;
  }

  /* Mood glyphs read in the mood colour, nudged towards the text colour so
     yellow / lime stay legible on light themes too. */
  .lib-meta-mood {
    color: color-mix(in srgb, var(--pill-c) 82%, var(--rk-ink));
  }
</style>
