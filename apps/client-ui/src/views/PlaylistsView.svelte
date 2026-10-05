<script lang="ts" module>
  import type { Playlist, Track } from "../lib/api";

  /**
   * First track of each playlist (for its thumbnail), keyed by playlist id
   * and its track count: a count change means the first track may have too.
   * Shared across mounts; the hub list carries no cover of its own.
   */
  const firstTracks = new Map<string, { count: number; track: Track | null }>();

  function cachedFirst(pl: Playlist): Track | null | undefined {
    const hit = firstTracks.get(pl.id);
    return hit && hit.count === pl.track_count ? hit.track : undefined;
  }
</script>

<script lang="ts">
  import { onDestroy } from "svelte";
  import { Button, CoverArt, EmptyState } from "@rekord/ui";
  import PageToolbar from "../components/PageToolbar.svelte";
  import TrackList from "../components/TrackList.svelte";
  import UiIcon from "../components/icons/UiIcon.svelte";
  import { api, coverUrlFor } from "../lib/api";
  import { collectionSubtitle } from "../lib/collectionInfo";
  import { confirmDialog } from "../lib/confirm.svelte";
  import { fmtDate, t, tp } from "../lib/i18n.svelte";
  import { session } from "../lib/session.svelte";

  const activePlaylist = $derived(
    session.playlists.find((p) => p.id === session.activePlaylistId) ?? null,
  );
  const detailSubtitle = $derived(
    activePlaylist && session.playlistTracks.length
      ? collectionSubtitle(session.playlistTracks)
      : activePlaylist
        ? tp("core.count.tracks", activePlaylist.track_count)
        : "",
  );
  const listSubtitle = $derived(
    session.playlists.length ? tp("core.count.playlists", session.playlists.length) : "",
  );

  /** Bumps when a thumbnail arrives (the cache itself is not reactive). */
  let thumbsRev = $state(0);
  let menuFor = $state<string | null>(null);
  let renaming = $state(false);
  let renameDraft = $state("");
  let renameEl: HTMLInputElement | null = $state(null);
  let newNameEl: HTMLInputElement | null = $state(null);
  let destroyed = false;

  onDestroy(() => {
    destroyed = true;
  });

  function thumbFor(pl: Playlist): string | null {
    void thumbsRev;
    // The open playlist already has its tracks.
    const first =
      pl.id === session.activePlaylistId && session.playlistTracks.length
        ? session.playlistTracks[0]!
        : cachedFirst(pl);
    return first ? coverUrlFor(first, 128) : null;
  }

  /** Fetch first tracks for the thumbnails, two playlists at a time. */
  $effect(() => {
    const todo = session.playlists.filter(
      (pl) => pl.track_count > 0 && cachedFirst(pl) === undefined,
    );
    if (!todo.length) return;
    let cancelled = false;
    const queue = todo.slice(0, 60);
    const worker = async () => {
      while (!cancelled && !destroyed && queue.length) {
        const pl = queue.shift()!;
        try {
          const data = await api.playlistTracks(pl.id);
          firstTracks.set(pl.id, { count: pl.track_count, track: data.tracks[0] ?? null });
        } catch {
          firstTracks.set(pl.id, { count: pl.track_count, track: null });
        }
        if (!cancelled) thumbsRev += 1;
      }
    };
    void worker();
    void worker();
    return () => {
      cancelled = true;
    };
  });

  // The open playlist's tracks are the freshest source for its thumbnail.
  $effect(() => {
    const pl = activePlaylist;
    const tracks = session.playlistTracks;
    if (!pl || !tracks.length) return;
    firstTracks.set(pl.id, { count: pl.track_count, track: tracks[0] ?? null });
  });

  // Leaving a playlist ends its rename.
  $effect(() => {
    void session.activePlaylistId;
    renaming = false;
  });

  async function playPlaylist(id: string) {
    await session.openPlaylist(id);
    if (session.playlistTracks.length) {
      session.playAll(session.playlistTracks);
    }
  }

  async function askDelete(pl: Playlist) {
    menuFor = null;
    const ok = await confirmDialog({
      title: t("core.playlists.deleteTitle", { playlist: pl.name }),
      message: t("core.playlists.deleteMessage"),
      confirmLabel: t("core.playlists.delete"),
      danger: true,
    });
    if (ok) await session.deletePlaylist(pl.id);
  }

  async function startRename(pl: Playlist) {
    menuFor = null;
    if (session.activePlaylistId !== pl.id) await session.openPlaylist(pl.id);
    renameDraft = pl.name;
    renaming = true;
    queueMicrotask(() => {
      renameEl?.focus();
      renameEl?.select();
    });
  }

  function commitRename() {
    const pl = activePlaylist;
    renaming = false;
    if (!pl) return;
    const next = renameDraft.trim();
    if (!next || next === pl.name) return;
    void session.renamePlaylist(pl.id, next);
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitRename();
    } else if (e.key === "Escape") {
      e.preventDefault();
      renaming = false;
    }
  }

  function toggleMenu(id: string, e: MouseEvent) {
    e.stopPropagation();
    menuFor = menuFor === id ? null : id;
  }

  function onWindowClick() {
    if (menuFor) menuFor = null;
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key === "Escape" && menuFor) menuFor = null;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKey} />

<div class="view-page playlists-page">
  <PageToolbar
    eyebrow={t("page.playlists.eyebrow")}
    title={t("nav.playlists")}
    subtitle={listSubtitle}
  >
    {#snippet icon()}
      <UiIcon name="queueMusic" class="section-head__ic" />
    {/snippet}
    {#snippet tools()}
      <input
        bind:this={newNameEl}
        class="ghost-input queue-name-input"
        bind:value={session.newPlaylistName}
        placeholder={t("page.playlists.newPlaceholder")}
        aria-label={t("page.playlists.newPlaceholder")}
        onkeydown={(e) => {
          if (e.key === "Enter") void session.createPlaylist();
        }}
      />
      <Button disabled={!session.newPlaylistName.trim()} onclick={() => void session.createPlaylist()}>
        <UiIcon name="add" />
        {t("page.playlists.create")}
      </Button>
    {/snippet}
  </PageToolbar>

  <section class="playlists-page__main">
    <div class="view-stack">
      <section class="rk-surface-card">
        {#if session.playlists.length === 0}
          <EmptyState title={t("core.playlists.emptyTitle")} body={t("core.playlists.emptyBody")}>
            {#snippet icon()}<UiIcon name="queueMusic" />{/snippet}
            {#snippet action()}
              {#if session.hasQueue}
                <Button onclick={() => void session.saveQueueAsPlaylist()}>
                  <UiIcon name="list" />
                  {t("core.playlists.emptySaveQueue")}
                </Button>
              {:else}
                <Button onclick={() => newNameEl?.focus()}>
                  <UiIcon name="add" />
                  {t("core.playlists.emptyCreate")}
                </Button>
              {/if}
            {/snippet}
          </EmptyState>
        {:else}
          <ul class="pl-list">
            {#each session.playlists as pl (pl.id)}
              <li class="pl-row" class:is-active={session.activePlaylistId === pl.id}>
                <button
                  type="button"
                  class="pl-row__main"
                  aria-current={session.activePlaylistId === pl.id ? "true" : undefined}
                  onclick={() => void session.openPlaylist(pl.id)}
                >
                  <CoverArt kind="album" size="sm" title={pl.name} src={thumbFor(pl)} />
                  <span class="pl-row__text">
                    <span class="pl-row__name" title={pl.name}>{pl.name}</span>
                    <span class="pl-row__meta">
                      {tp("core.count.tracks", pl.track_count)}{#if pl.created_at}
                        · {fmtDate(pl.created_at)}{/if}
                    </span>
                  </span>
                </button>
                <div class="pl-row__actions">
                  <button
                    type="button"
                    class="pl-row__icon"
                    disabled={!pl.track_count}
                    title={t("core.playlists.play")}
                    aria-label={t("core.playlists.playNamed", { playlist: pl.name })}
                    onclick={() => void playPlaylist(pl.id)}
                  >
                    <UiIcon name="play" />
                  </button>
                  <div class="pl-menu">
                    <button
                      type="button"
                      class="pl-row__icon"
                      aria-haspopup="menu"
                      aria-expanded={menuFor === pl.id}
                      title={t("core.playlists.more")}
                      aria-label={t("core.playlists.moreNamed", { playlist: pl.name })}
                      onclick={(e) => toggleMenu(pl.id, e)}
                    >
                      <UiIcon name="more" />
                    </button>
                    {#if menuFor === pl.id}
                      <div class="pl-menu__pop" role="menu">
                        <button
                          type="button"
                          role="menuitem"
                          disabled={!session.current}
                          onclick={() => {
                            menuFor = null;
                            void session.addCurrentToPlaylist(pl.id);
                          }}
                        >
                          <UiIcon name="add" />
                          {t("core.playlists.addCurrentLong")}
                        </button>
                        <button type="button" role="menuitem" onclick={() => void startRename(pl)}>
                          <UiIcon name="edit" />
                          {t("page.playlists.rename")}
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          class="is-danger"
                          onclick={() => void askDelete(pl)}
                        >
                          <UiIcon name="close" />
                          {t("core.playlists.delete")}
                        </button>
                      </div>
                    {/if}
                  </div>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>

    <div class="view-stack">
      {#if activePlaylist}
        <section class="rk-surface-card pl-detail">
          <header class="pl-detail__head">
            <CoverArt
              kind="album"
              size="md"
              title={activePlaylist.name}
              src={thumbFor(activePlaylist)}
            />
            <div class="pl-detail__text">
              <p class="rk-eyebrow">{t("page.playlists.detailEyebrow")}</p>
              {#if renaming}
                <input
                  bind:this={renameEl}
                  class="pl-detail__rename"
                  bind:value={renameDraft}
                  aria-label={t("page.playlists.rename")}
                  maxlength="120"
                  onkeydown={onRenameKey}
                  onblur={commitRename}
                />
              {:else}
                <h3 class="pl-detail__title">
                  <span>{activePlaylist.name}</span>
                  <button
                    type="button"
                    class="pl-row__icon"
                    title={t("page.playlists.rename")}
                    aria-label={t("page.playlists.rename")}
                    onclick={() => void startRename(activePlaylist)}
                  >
                    <UiIcon name="edit" />
                  </button>
                </h3>
              {/if}
              <p class="pl-detail__meta">{detailSubtitle}</p>
            </div>
            {#if session.playlistTracks.length}
              <Button onclick={() => session.playAll(session.playlistTracks)}>
                <UiIcon name="play" />
                {t("core.playlists.play")}
              </Button>
            {/if}
          </header>
          {#if activePlaylist.track_count === 0 && session.playlistTracks.length === 0}
            <EmptyState
              variant="inline"
              title={t("core.playlists.detailEmptyTitle")}
              body={t("core.playlists.detailEmpty")}
            >
              {#snippet icon()}<UiIcon name="music" />{/snippet}
              {#snippet action()}
                {#if session.current}
                  <Button onclick={() => void session.addCurrentToPlaylist(activePlaylist.id)}>
                    <UiIcon name="add" />
                    {t("core.playlists.addCurrentLong")}
                  </Button>
                {:else}
                  <Button onclick={() => session.navigate("library")}>
                    <UiIcon name="disc" />
                    {t("core.playlists.openLibrary")}
                  </Button>
                {/if}
              {/snippet}
            </EmptyState>
          {:else}
            <TrackList
              tracks={session.playlistTracks}
              favoriteIds={session.favoriteIds}
              playlistOptions={session.playlistOptions}
              activeTrackId={session.current?.id ?? null}
              emptyMessage={t("core.recent.loading")}
              onplay={(track, list) => {
                const idx = list.findIndex((tr) => tr.id === track.id);
                session.playSequence(list, idx >= 0 ? idx : 0);
              }}
              ontoggleFavorite={(track) => void session.toggleFavorite(track)}
              onaddToPlaylist={(playlistId, track) =>
                void session.addToPlaylist(playlistId, track.id)}
              onremove={(track) => void session.removeFromPlaylist(activePlaylist.id, track.id)}
              onreorder={(from, to) =>
                void session.movePlaylistTrack(activePlaylist.id, from, to)}
            />
          {/if}
        </section>
      {:else if session.playlists.length}
        <section class="rk-surface-card">
          <EmptyState variant="inline" body={t("page.playlists.pickHint")}>
            {#snippet icon()}<UiIcon name="queueMusic" />{/snippet}
          </EmptyState>
        </section>
      {/if}
    </div>
  </section>
</div>

<style>
  .pl-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.5rem;
  }

  .pl-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.5rem 0.45rem 0.45rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    background: var(--rk-surface-2);
    transition:
      background 0.16s ease,
      border-color 0.16s ease;
  }

  .pl-row:hover {
    border-color: color-mix(in srgb, var(--rk-accent) 30%, var(--rk-line) 70%);
  }

  .pl-row.is-active {
    border-color: color-mix(in srgb, var(--rk-accent) 48%, var(--rk-line) 52%);
    background: color-mix(in srgb, var(--rk-accent) 10%, var(--rk-surface-2) 90%);
    box-shadow: inset 3px 0 0 var(--rk-accent);
  }

  .pl-row__main {
    min-width: 0;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.75rem;
    align-items: center;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pl-row__text {
    min-width: 0;
    display: grid;
    gap: 0.15rem;
  }

  .pl-row__name {
    font-weight: 700;
    color: var(--rk-ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pl-row__meta {
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pl-row__actions {
    display: flex;
    align-items: center;
    gap: 0.15rem;
  }

  .pl-row__icon {
    display: inline-grid;
    place-items: center;
    width: 2.25rem;
    height: 2.25rem;
    border: 0;
    border-radius: var(--rk-radius-round);
    background: transparent;
    color: var(--rk-muted-strong, var(--rk-ink));
    cursor: pointer;
  }

  .pl-row__icon:hover:not(:disabled) {
    background: var(--rk-surface-3);
    color: var(--rk-ink);
  }

  .pl-row__icon:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .pl-menu {
    position: relative;
  }

  .pl-menu__pop {
    position: absolute;
    right: 0;
    top: calc(100% + 0.25rem);
    z-index: var(--rk-z-popover, 40);
    min-width: 13rem;
    display: grid;
    padding: 0.3rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    background: var(--rk-surface-3);
    box-shadow: 0 12px 32px color-mix(in srgb, var(--rk-bg) 70%, transparent);
  }

  .pl-menu__pop button {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.55rem 0.65rem;
    border: 0;
    border-radius: calc(var(--rk-radius) - 2px);
    background: transparent;
    color: var(--rk-ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pl-menu__pop button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--rk-accent) 12%, transparent);
  }

  .pl-menu__pop button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .pl-menu__pop button.is-danger {
    color: var(--rk-danger, #e85d5d);
  }

  .pl-detail {
    display: grid;
    gap: 1rem;
  }

  .pl-detail__head {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0.9rem;
    align-items: center;
  }

  .pl-detail__text {
    min-width: 0;
  }

  .pl-detail__title {
    margin: 0.1rem 0 0;
    display: flex;
    align-items: center;
    gap: 0.25rem;
    min-width: 0;
    font-size: var(--rk-fs-lg, 1.25rem);
    font-weight: 800;
    color: var(--rk-ink);
  }

  .pl-detail__title span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pl-detail__rename {
    margin-top: 0.1rem;
    width: 100%;
    font: inherit;
    font-size: var(--rk-fs-lg, 1.25rem);
    font-weight: 800;
    color: var(--rk-ink);
    background: var(--rk-surface-3);
    border: 1px solid var(--rk-accent);
    border-radius: var(--rk-radius);
    padding: 0.2rem 0.5rem;
  }

  .pl-detail__meta {
    margin: 0.15rem 0 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    font-variant-numeric: tabular-nums;
  }

  @media (max-width: 559.98px) {
    .pl-detail__head {
      grid-template-columns: auto minmax(0, 1fr);
    }

    .pl-detail__head > :global(:last-child:not(.pl-detail__text)) {
      grid-column: 1 / -1;
    }
  }
</style>
