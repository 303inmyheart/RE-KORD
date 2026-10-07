<script lang="ts">
  /** "Ultimi": the newest episodes across sources, and the live stations. */
  import { Skeleton } from "@rekord/ui";
  import { t, tp } from "../../lib/i18n.svelte";
  import { latestAcross, type PodcastSource } from "../../lib/podcastModel";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import SectionHeadLead from "../SectionHeadLead.svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import EpisodeRow from "./EpisodeRow.svelte";
  import LiveTile from "./LiveTile.svelte";

  let { sources }: { sources: PodcastSource[] } = $props();

  const latest = $derived(latestAcross(sources, 3, 12));
  const live = $derived(sources.filter((s) => s.live));
  /** The first (newest) row of each source may wear the "Nuovo" badge. */
  const firstOfSource = $derived.by(() => {
    const seen = new Set<number>();
    return latest.map((x) => {
      const first = !seen.has(x.source.id);
      seen.add(x.source.id);
      return first;
    });
  });
</script>

<div class="latest" class:latest--with-live={live.length > 0 && live.length <= 2}>
{#if live.length}
  <section class="album-hero__tracks rk-surface-card latest-live">
    <div class="latest__head">
      <SectionHeadLead eyebrow={t("podcasts.liveSection")} title={tp("podcasts.stationsCount", live.length)}>
        <UiIcon name="radio" />
      </SectionHeadLead>
    </div>
    <div class="latest-live__grid">
      {#each live as source (source.id)}
        <LiveTile {source} />
      {/each}
    </div>
  </section>
{/if}

{#if latest.length || podcasts.loading}
  <section class="album-hero__tracks rk-surface-card">
    <div class="latest__head">
      <SectionHeadLead eyebrow={t("podcasts.latestTab")} title={t("podcasts.latestTitle")}>
        <UiIcon name="podcast" />
      </SectionHeadLead>
    </div>
    {#if latest.length}
      <ul class="latest__list">
        {#each latest as item, i (`${item.source.id}:${item.ep.key}`)}
          <EpisodeRow source={item.source} ep={item.ep} showSource newest={firstOfSource[i]} />
        {/each}
      </ul>
    {:else}
      <Skeleton variant="row" count={4} />
    {/if}
  </section>
{/if}
</div>

<style>
  .latest {
    display: grid;
    gap: var(--rk-section-gap);
    min-width: 0;
  }

  /* One or two stations: a side column next to the episodes, no empty row. */
  @media (min-width: 1100px) {
    .latest--with-live {
      grid-template-columns: minmax(0, 1fr) 15rem;
      align-items: start;
    }

    .latest--with-live .latest-live {
      grid-column: 2;
      grid-row: 1;
    }

    .latest--with-live .latest-live__grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .latest__head {
    margin: 0 0 var(--rk-space-md, 0.85rem);
  }

  .latest__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rk-space-lg);
    container: track-list / inline-size;
  }

  .latest-live__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9.5rem, 11rem));
    gap: 1rem 0.85rem;
  }

  /* Phones: stations as rows (artwork, name, play), touch-sized. */
  @media (max-width: 599.98px) {
    .latest-live__grid {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.75rem;
    }

    .latest-live__grid :global(.live-tile) {
      flex-direction: row;
      align-items: center;
      gap: 0.85rem;
    }

    .latest-live__grid :global(.live-tile__media) {
      flex: 0 0 6.5rem;
      width: 6.5rem;
    }

    .latest-live__grid :global(.live-tile__play) {
      width: 2.5rem;
      height: 2.5rem;
      right: 0.4rem;
      bottom: 0.4rem;
    }

    .latest-live__grid :global(.live-tile__pill) {
      top: 0.35rem;
      left: 0.35rem;
    }
  }
</style>
