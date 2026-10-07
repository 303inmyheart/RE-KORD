<script lang="ts">
  /**
   * Recenti › podcast listens of this account (only when "show in Recenti"
   * is on). Built from the listening state: no request until something plays.
   */
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n.svelte";
  import { recentEntries, type PodcastEpisode, type PodcastSource } from "../../lib/podcastModel";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import SourceBlockRow from "./EpisodeRow.svelte";

  let { limit = 20 }: { limit?: number } = $props();

  onMount(() => podcasts.bindTracker());

  /** Entries as rows: the source as known now, else as remembered. */
  const rows = $derived.by(() => {
    const known = new Map(podcasts.sources.map((s) => [s.id, s]));
    return recentEntries(podcasts.state, limit).map((e) => {
      const src = known.get(e.sid);
      const source: PodcastSource = src ?? {
        id: e.sid,
        name: e.s,
        kind: e.live ? "live" : "rss",
        live: !!e.live,
        episodeCount: 1,
        hasArt: !!e.a,
        fetchedAt: null,
        error: null,
        episodes: [],
      };
      const ep: PodcastEpisode = src?.episodes.find((x) => x.key === e.k) ?? {
        key: e.k,
        title: e.t,
        durationSecs: e.d || null,
        hasArt: !!e.a,
        live: !!e.live,
      };
      return { id: `${e.sid}:${e.k}`, source, ep };
    });
  });
</script>

{#if rows.length}
  <section class="rk-surface-card recent-podcasts">
    <h2 class="recent-podcasts__title">{t("podcasts.recentTitle")}</h2>
    <ul class="recent-podcasts__list">
      {#each rows as row (row.id)}
        <SourceBlockRow source={row.source} ep={row.ep} showSource />
      {/each}
    </ul>
  </section>
{/if}

<style>
  .recent-podcasts {
    display: grid;
    gap: var(--rk-space-md);
    padding: var(--rk-space-lg);
  }

  .recent-podcasts__title {
    margin: 0;
    font-size: var(--rk-fs-md, 1rem);
    font-weight: 750;
  }

  .recent-podcasts__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rk-space-sm);
    container: track-list / inline-size;
  }
</style>
