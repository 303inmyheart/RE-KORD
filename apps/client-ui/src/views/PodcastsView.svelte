<script lang="ts">
  /**
   * Podcast e notizie: every configured source with its latest episodes,
   * a manual refresh, "listened" marks and the "show in Recenti" switch.
   * Its own chunk, fetched only when the hub has the module on.
   */
  import { onMount } from "svelte";
  import { Button, EmptyState, Skeleton } from "@rekord/ui";
  import PageToolbar from "../components/PageToolbar.svelte";
  import SourceBlock from "../components/podcasts/SourceBlock.svelte";
  import UiIcon from "../components/icons/UiIcon.svelte";
  import { t, tp } from "../lib/i18n.svelte";
  import { visibleSources } from "../lib/podcastModel";
  import { podcasts } from "../lib/podcasts/store.svelte";
  import { session } from "../lib/session.svelte";

  onMount(() => {
    void podcasts.load();
  });

  const sources = $derived(visibleSources(podcasts.sources));
  const subtitle = $derived(sources.length ? tp("podcasts.sourcesCount", sources.length) : "");
</script>

<div class="view-page view-page--split podcasts-page">
  <PageToolbar eyebrow={t("podcasts.eyebrow")} title={t("nav.podcasts")} {subtitle}>
    {#snippet icon()}
      <UiIcon name="podcast" class="section-head__ic" />
    {/snippet}
    {#snippet tools()}
      <label class="podcasts-page__switch" title={t("podcasts.inRecentHint")}>
        <input
          type="checkbox"
          checked={podcasts.inRecent}
          onchange={(e) => (podcasts.inRecent = (e.currentTarget as HTMLInputElement).checked)}
        />
        <span>{t("podcasts.inRecent")}</span>
      </label>
      <Button variant="ghost" disabled={podcasts.loading} onclick={() => void podcasts.load({ force: true })}>
        <UiIcon name="sync" />
        {podcasts.loading ? t("podcasts.refreshing") : t("podcasts.refresh")}
      </Button>
    {/snippet}
  </PageToolbar>

  {#if !podcasts.loaded && podcasts.loading}
    <section class="rk-surface-card view-page__body">
      <Skeleton variant="row" count={6} label={t("podcasts.loading")} />
    </section>
  {:else if podcasts.error && !podcasts.sources.length}
    <section class="rk-surface-card view-page__body">
      <EmptyState title={t("podcasts.loadFailed")} body={podcasts.error}>
        {#snippet icon()}<UiIcon name="podcast" />{/snippet}
        {#snippet action()}
          <Button onclick={() => void podcasts.load({ force: true })}>{t("library.retry")}</Button>
        {/snippet}
      </EmptyState>
    </section>
  {:else if !sources.length}
    <section class="rk-surface-card view-page__body">
      <EmptyState title={t("podcasts.emptyTitle")} body={t("podcasts.emptyBody")}>
        {#snippet icon()}<UiIcon name="podcast" />{/snippet}
        {#snippet action()}
          <a class="text-btn" href={session.hubPanelUrl} target="_blank" rel="noopener noreferrer">
            {t("podcasts.openAdmin")}
          </a>
        {/snippet}
      </EmptyState>
    </section>
  {:else}
    {#each sources as source (source.id)}
      <section class="rk-surface-card podcasts-page__source">
        <SourceBlock {source} loading={podcasts.loading} />
      </section>
    {/each}
  {/if}
</div>

<style>
  .podcasts-page {
    display: flex;
    flex-direction: column;
    gap: var(--rk-section-gap);
    min-width: 0;
  }

  .podcasts-page__source {
    padding: var(--rk-space-lg);
  }

  .podcasts-page__switch {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: var(--rk-fs-sm);
    color: var(--rk-muted);
    cursor: pointer;
    white-space: nowrap;
  }

  .podcasts-page__switch input {
    accent-color: var(--rk-accent);
  }
</style>
