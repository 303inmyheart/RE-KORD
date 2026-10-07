<script lang="ts">
  /** A source (podcast, news bulletin, radio) and its latest episodes. */
  import { CoverArt, Skeleton } from "@rekord/ui";
  import { translateHubError } from "../../lib/api/http";
  import { apiUrl } from "../../lib/config";
  import { fmtRelative, t } from "../../lib/i18n.svelte";
  import { episodesFor, type PodcastSource } from "../../lib/podcastModel";
  import EpisodeRow from "./EpisodeRow.svelte";

  let {
    source,
    compact = false,
    loading = false,
  }: {
    source: PodcastSource;
    /** Dashboard card: smaller header, no "updated" line. */
    compact?: boolean;
    loading?: boolean;
  } = $props();

  const art = $derived(source.hasArt ? apiUrl(`/api/v1/podcasts/art/${source.id}/_`) : null);
  const episodes = $derived(episodesFor(source));
  const errorText = $derived(source.error ? translateHubError(source.error) : "");
</script>

<section class="podcast-source" class:podcast-source--compact={compact}>
  <header class="podcast-source__head">
    <CoverArt kind="album" title={source.name} src={art} size={compact ? "sm" : "md"} />
    <div class="podcast-source__text">
      <h3 class="podcast-source__name">{source.name}</h3>
      <p class="podcast-source__sub">
        {#if source.live}
          <span class="podcast-source__live">LIVE</span>
          <span>{t("podcasts.kind.live")}</span>
        {:else}
          <span>{t(`podcasts.kind.${source.kind}`)}</span>
          {#if !compact && source.fetchedAt}
            <span>{t("podcasts.updated", { when: fmtRelative(source.fetchedAt) })}</span>
          {/if}
        {/if}
      </p>
    </div>
  </header>

  {#if errorText}
    <p class="podcast-source__error" role="status">
      {episodes.length ? t("podcasts.staleError", { error: errorText }) : errorText}
    </p>
  {/if}

  {#if episodes.length}
    <ul class="podcast-source__list">
      {#each episodes as ep (ep.key)}
        <EpisodeRow {source} {ep} />
      {/each}
    </ul>
  {:else if loading}
    <Skeleton variant="row" count={Math.min(3, source.episodeCount)} />
  {:else if !errorText}
    <p class="podcast-source__empty">{t("podcasts.noEpisodes")}</p>
  {/if}
</section>

<style>
  .podcast-source {
    display: grid;
    align-content: start;
    gap: var(--rk-space-sm);
    min-width: 0;
  }

  .podcast-source__head {
    display: flex;
    align-items: center;
    gap: var(--rk-space-md);
    min-width: 0;
  }

  .podcast-source__text {
    min-width: 0;
    display: grid;
    gap: 0.1rem;
  }

  .podcast-source__name {
    margin: 0;
    font-size: var(--rk-fs-md, 1rem);
    font-weight: 750;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .podcast-source--compact .podcast-source__name {
    font-size: var(--rk-fs-base, 0.95rem);
  }

  .podcast-source__sub {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--rk-space-sm);
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
  }

  .podcast-source__sub > span + span::before {
    content: "·";
    margin-right: var(--rk-space-sm);
  }

  .podcast-source__live {
    font-weight: 800;
    letter-spacing: 0.06em;
    color: var(--rk-danger, #e5484d);
  }

  .podcast-source__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rk-space-sm);
    container: track-list / inline-size;
  }

  .podcast-source__error {
    margin: 0;
    padding: 0.45rem 0.7rem;
    border-radius: var(--rk-radius);
    border: 1px solid color-mix(in srgb, var(--rk-danger) 40%, var(--rk-line));
    font-size: var(--rk-fs-sm);
    color: color-mix(in srgb, var(--rk-danger) 70%, var(--rk-ink));
  }

  .podcast-source__empty {
    margin: 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
  }
</style>
