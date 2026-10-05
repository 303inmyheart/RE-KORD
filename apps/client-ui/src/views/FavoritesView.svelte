<script lang="ts">
  import { Button, EmptyState } from "@rekord/ui";
  import PageToolbar from "../components/PageToolbar.svelte";
  import PlayCollectionButton from "../components/PlayCollectionButton.svelte";
  import TrackList from "../components/TrackList.svelte";
  import UiIcon from "../components/icons/UiIcon.svelte";
  import type { Track } from "../lib/api";
  import { collectionSubtitle } from "../lib/collectionInfo";
  import { i18n, t } from "../lib/i18n.svelte";
  import { playCountIn, prefsRevision } from "../lib/prefsRevision.svelte";
  import { session } from "../lib/session.svelte";

  /** Most played first, then by title (legacy AppShell favourites). */
  const favorites = $derived.by((): Track[] => {
    const counts = prefsRevision.playCountsMap;
    const collator = new Intl.Collator(i18n.sortLocale, { numeric: true, sensitivity: "base" });
    return session.favorites
      .map((track) => ({ track, plays: playCountIn(counts, track) }))
      .sort((a, b) => b.plays - a.plays || collator.compare(a.track.title, b.track.title))
      .map((x) => x.track);
  });
  const subtitle = $derived(favorites.length ? collectionSubtitle(favorites) : "");
</script>

<div class="view-page view-page--split collection-page">
  <PageToolbar eyebrow={t("page.favorites.eyebrow")} title={t("nav.favorites")} {subtitle}>
    {#snippet icon()}
      <UiIcon name="favorite" class="section-head__ic" />
    {/snippet}
    {#snippet tools()}
      {#if favorites.length > 0}
        <PlayCollectionButton
          label={t("page.favorites.play")}
          onclick={() => session.playPoolShuffle(favorites)}
        />
      {/if}
    {/snippet}
  </PageToolbar>

  <section class="rk-surface-card collection-page__list view-page__body">
    {#if favorites.length === 0}
      <EmptyState title={t("core.favorites.emptyTitle")} body={t("core.favorites.emptyBody")}>
        {#snippet icon()}<UiIcon name="favorite" />{/snippet}
        {#snippet action()}
          <Button onclick={() => session.navigate("library")}>
            <UiIcon name="disc" />
            {t("core.favorites.emptyCta")}
          </Button>
        {/snippet}
      </EmptyState>
    {:else}
      <TrackList
        tracks={favorites}
        favoriteIds={session.favoriteIds}
        playlistOptions={session.playlistOptions}
        activeTrackId={session.current?.id ?? null}
        onplay={(track, list) => session.playCollectionShuffle(track, list)}
        ontoggleFavorite={(track) => void session.toggleFavorite(track)}
        onaddToPlaylist={(playlistId, track) =>
          void session.addToPlaylist(playlistId, track.id)}
      />
    {/if}
  </section>
</div>
