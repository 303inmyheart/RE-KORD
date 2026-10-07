<script lang="ts">
  /**
   * Dashboard › Podcast e notizie: every source with its latest episodes.
   * Loaded (code and data) only when the hub has the module on; the list is
   * asked for when the card appears, never polled.
   */
  import { onMount } from "svelte";
  import { EmptyState, Panel, Skeleton } from "@rekord/ui";
  import SectionHeadLead from "../SectionHeadLead.svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { visibleSources } from "../../lib/podcastModel";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import { session } from "../../lib/session.svelte";
  import SourceBlock from "./SourceBlock.svelte";

  /** The card stays a card: the view lists everything. */
  const MAX_SOURCES = 6;

  onMount(() => {
    void podcasts.load();
  });

  const sources = $derived(visibleSources(podcasts.sources).slice(0, MAX_SOURCES));
  const more = $derived(Math.max(0, visibleSources(podcasts.sources).length - MAX_SOURCES));
</script>

<Panel class="session-card dashboard-session-card dashboard-page__full dashboard-page__tile podcast-card">
  <header class="section-head section-head--page-toolbar">
    <SectionHeadLead eyebrow={t("podcasts.eyebrow")} title={t("nav.podcasts")}>
      <UiIcon name="podcast" />
    </SectionHeadLead>
    <div class="section-head__tools">
      <button
        type="button"
        class="text-btn"
        disabled={podcasts.loading}
        onclick={() => void podcasts.load({ force: true })}
      >
        {podcasts.loading ? t("podcasts.refreshing") : t("podcasts.refresh")}
      </button>
      <button type="button" class="text-btn" onclick={() => session.navigate("podcasts")}>
        {t("podcasts.openAll")}
      </button>
    </div>
  </header>

  {#if !podcasts.loaded && podcasts.loading}
    <Skeleton variant="row" count={3} label={t("podcasts.loading")} />
  {:else if podcasts.error && !podcasts.sources.length}
    <p class="podcast-card__error" role="alert">{podcasts.error}</p>
  {:else if !sources.length}
    <EmptyState variant="inline" title={t("podcasts.emptyTitle")} body={t("podcasts.emptyBody")}>
      {#snippet icon()}<UiIcon name="podcast" />{/snippet}
      {#snippet action()}
        <a class="text-btn" href={session.hubPanelUrl} target="_blank" rel="noopener noreferrer">
          {t("podcasts.openAdmin")}
        </a>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="podcast-card__grid">
      {#each sources as source (source.id)}
        <SourceBlock {source} compact loading={podcasts.loading} />
      {/each}
    </div>
    {#if more > 0}
      <button type="button" class="text-btn podcast-card__more" onclick={() => session.navigate("podcasts")}>
        {t("podcasts.moreSources", { n: more })}
      </button>
    {/if}
  {/if}
</Panel>

<style>
  .podcast-card__grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
    gap: var(--rk-space-xl, 1.25rem) var(--rk-space-xl, 1.25rem);
  }

  @media (min-width: 1100px) {
    .podcast-card__grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .podcast-card__error {
    margin: 0;
    color: color-mix(in srgb, var(--rk-danger) 70%, var(--rk-ink));
    font-size: var(--rk-fs-sm);
  }

  .podcast-card__more {
    justify-self: start;
    margin-top: var(--rk-space-sm);
  }
</style>
