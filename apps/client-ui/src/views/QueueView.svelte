<script lang="ts">
  import { Button, EmptyState } from "@rekord/ui";
  import PageToolbar from "../components/PageToolbar.svelte";
  import TrackRow from "../components/TrackRow.svelte";
  import UiIcon from "../components/icons/UiIcon.svelte";
  import { collectionSubtitle } from "../lib/collectionInfo";
  import { dragReorder } from "../lib/dragReorder";
  import { t } from "../lib/i18n.svelte";
  import { player } from "../lib/player";
  import { prefsRevision } from "../lib/prefsRevision.svelte";
  import { session } from "../lib/session.svelte";
  import {
    virtualList,
    type VirtualListApi,
    type VirtualWindow,
  } from "../lib/virtualList";

  const VIRTUAL_FROM = 40;

  /** First window before the action measures anything: about one viewport. */
  let win = $state<VirtualWindow>({
    start: 0,
    end: VIRTUAL_FROM,
    padTop: 0,
    padBottom: 0,
    rowPx: 0,
  });
  let dragging = $state(false);
  let rowsApi: VirtualListApi | null = null;

  const queue = $derived(session.queue);
  const empty = $derived(queue.length === 0);
  const virtualized = $derived(queue.length >= VIRTUAL_FROM);
  const windowQueue = $derived(virtualized ? queue.slice(win.start, win.end) : queue);
  const currentOffscreen = $derived(
    virtualized &&
      session.currentIndex >= 0 &&
      (session.currentIndex < win.start || session.currentIndex >= win.end),
  );
  const subtitle = $derived(
    empty
      ? ""
      : collectionSubtitle(queue, {
          position: session.currentIndex >= 0 ? session.currentIndex + 1 : null,
        }),
  );
  /**
   * Rows re-derive play counts / moods / exclusions only when those change,
   * not on every player tick (every row is "in queue" here anyway).
   */
  const rowRevision = $derived(prefsRevision.any);
</script>

<div class="view-page view-page--split queue-page">
  <PageToolbar eyebrow={t("page.queue.eyebrow")} title={t("nav.queue")} {subtitle}>
    {#snippet icon()}
      <UiIcon name="list" class="section-head__ic" />
    {/snippet}
    {#snippet tools()}
      {#if !empty}
        {#if currentOffscreen}
          <Button variant="ghost" size="sm" onclick={() => rowsApi?.scrollToIndex(session.currentIndex)}>
            {t("page.queue.goToCurrent")}
          </Button>
        {/if}
        <input
          class="ghost-input queue-name-input"
          bind:value={session.queuePlaylistName}
          placeholder={t("page.queue.namePlaceholder")}
          aria-label={t("page.queue.namePlaceholder")}
        />
        <Button onclick={() => void session.saveQueueAsPlaylist()}>
          <UiIcon name="queueMusic" />
          {t("page.queue.save")}
        </Button>
        <Button variant="ghost" tone="danger" onclick={() => player.clearQueue()}>
          {t("page.queue.clear")}
        </Button>
      {/if}
    {/snippet}
  </PageToolbar>

  <section class="rk-surface-card queue-page__list view-page__body">
    {#if empty}
      <EmptyState title={t("core.queue.emptyTitle")} body={t("core.queue.emptyBody")}>
        {#snippet icon()}<UiIcon name="list" />{/snippet}
        {#snippet action()}
          <div class="empty-actions">
            <Button onclick={() => session.navigate("library")}>
              <UiIcon name="disc" />
              {t("core.queue.emptyCta")}
            </Button>
            <Button variant="ghost" onclick={() => void session.shuffleLibrary()}>
              <UiIcon name="shuffle" />
              {t("core.queue.emptyShuffle")}
            </Button>
          </div>
        {/snippet}
      </EmptyState>
    {:else}
      <ul
        class="list"
        style:padding-top={virtualized ? `${win.padTop}px` : null}
        style:padding-bottom={virtualized ? `${win.padBottom}px` : null}
        use:dragReorder={{
          onmove: (from, to) => player.moveQueueItem(from, to),
          enabled: queue.length > 1,
          ondragstate: (active) => (dragging = active),
        }}
        use:virtualList={{
          count: queue.length,
          threshold: VIRTUAL_FROM,
          frozen: dragging,
          onwindow: (next) => (win = next),
          onready: (api) => (rowsApi = api),
        }}
      >
        {#each windowQueue as track, offset (track.id + "-" + (virtualized ? win.start + offset : offset))}
          {@const index = virtualized ? win.start + offset : offset}
          <TrackRow
            {track}
            {index}
            reorderIndex={index}
            autoFocusActive={!virtualized}
            onreorderStep={(delta) =>
              player.moveQueueItem(
                index,
                Math.max(0, Math.min(queue.length - 1, index + delta)),
              )}
            revision={rowRevision}
            favorited={session.favoriteIds.has(track.id)}
            active={index === session.currentIndex}
            playlistOptions={session.playlistOptions}
            onplay={() => session.playQueueIndex(index)}
            ontoggleFavorite={() => void session.toggleFavorite(track)}
            onaddToPlaylist={(playlistId) => void session.addToPlaylist(playlistId, track.id)}
            onremoveFromQueue={() => player.removeFromQueue(index)}
            ontoggleExclude={() => player.toggleExcludeTrack(track)}
            onremove={() => player.removeFromQueue(index)}
            removeInline={false}
          />
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rk-space-lg);
  }

  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    justify-content: center;
  }
</style>
