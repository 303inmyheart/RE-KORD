<script lang="ts">
  /**
   * Library › album page (legacy album hero): back to the artist first, then a
   * cover + title block (stacked and centred on phones), one primary
   * "Riproduci album" with its icon, secondary actions, genre chips with a
   * labelled "Aggiungi genere", and a numbered tracklist.
   */
  import { onMount } from "svelte";
  import { Button, CoverArt } from "@rekord/ui";
  import EntityInfoAction from "../../EntityInfoAction.svelte";
  import MetaBadgeCluster from "../../MetaBadgeCluster.svelte";
  import SectionHeadLead from "../../SectionHeadLead.svelte";
  import TrackList from "../../TrackList.svelte";
  import UiIcon from "../../icons/UiIcon.svelte";
  import { api, coverUrlFor, type Album, type Track } from "../../../lib/api";
  import { confirmDialog } from "../../../lib/confirm.svelte";
  import {
    canonicalGenreLabel,
    normalizeGenreKey,
    parseTrackGenres,
    serializeTrackGenres,
    trackHasGenre,
  } from "../../../lib/genres";
  import { fmtDate, i18n, t, tp } from "../../../lib/i18n.svelte";
  import { player } from "../../../lib/player";
  import { prefsRevision } from "../../../lib/prefsRevision.svelte";
  import { session } from "../../../lib/session.svelte";
  import { GENRE_POOL, albumHasAlbumMeta, trackHasFileMeta } from "../../../lib/trackMoods";

  let { album }: { album: Album } = $props();

  let genrePickerOpen = $state(false);
  let genreBusy = $state(false);
  let genreErr = $state<string | null>(null);
  let genreAddWrapEl = $state<HTMLDivElement | null>(null);

  onMount(() => {
    const onDocPointer = (ev: MouseEvent) => {
      if (!genrePickerOpen) return;
      const target = ev.target;
      if (target instanceof Node && genreAddWrapEl?.contains(target)) return;
      genrePickerOpen = false;
    };
    document.addEventListener("mousedown", onDocPointer);
    return () => document.removeEventListener("mousedown", onDocPointer);
  });

  $effect(() => {
    void album.id;
    genrePickerOpen = false;
    genreErr = null;
  });

  const tracks = $derived(session.tracks);

  const exclusions = $derived.by(() => {
    void prefsRevision.exclusions;
    return {
      paths: new Set(player.getExcludedRelPaths()),
      albums: new Set(player.getExcludedAlbumIds()),
    };
  });
  const albumExcluded = $derived(exclusions.albums.has(album.id));
  const tracksExcludedCount = $derived(
    albumExcluded ? album.track_count : tracks.filter((tr) => exclusions.paths.has(tr.rel_path)).length,
  );
  const favCount = $derived(session.favorites.filter((tr) => tr.album_id === album.id).length);
  const tracksMissingMeta = $derived(tracks.filter((tr) => !trackHasFileMeta(tr)).length);

  /** Folder segments for entity info (folder_key = "artist/album…"). */
  const artistDir = $derived(album.folder_key.split("/")[0]?.trim() || album.artist_name);
  const albumDir = $derived.by(() => {
    const slash = album.folder_key.indexOf("/");
    const rest = slash >= 0 ? album.folder_key.slice(slash + 1).trim() : "";
    return rest || album.name;
  });

  /** Full release date when the hub has one ("15 nov 2024"), else the year. */
  const releaseLabel = $derived.by(() => {
    const raw = album.release_date?.trim() ?? "";
    if (/^\d{4}-\d{2}-\d{2}/.test(raw)) return fmtDate(raw);
    const year = raw.match(/\d{4}/)?.[0];
    return year ?? null;
  });
  const label = $derived(album.label?.trim() || null);
  const totalMinutes = $derived(
    Math.round(tracks.reduce((sum, tr) => sum + (tr.duration_ms || 0), 0) / 60000),
  );
  const expected = $derived.by(() => {
    const n = album.expected_track_count;
    return n != null && n > 0 ? n : null;
  });

  /** Genres present on the album's tracks, canonical label → track count. */
  const trackGenreStats = $derived.by(() => {
    const byKey = new Map<string, { label: string; count: number }>();
    for (const tr of tracks) {
      for (const g of parseTrackGenres(tr.genre)) {
        const key = normalizeGenreKey(g);
        const cur = byKey.get(key);
        if (cur) cur.count += 1;
        else byKey.set(key, { label: canonicalGenreLabel(g), count: 1 });
      }
    }
    return [...byKey.entries()]
      .map(([key, v]) => ({ key, ...v }))
      .sort((a, b) => a.label.localeCompare(b.label, i18n.sortLocale, { numeric: true }));
  });

  /** Library genres (or the pool) not on the album yet — for the add picker. */
  const genreOptions = $derived.by(() => {
    const have = new Set(trackGenreStats.map((g) => g.key));
    const byKey = new Map<string, string>();
    const add = (g: string) => {
      const key = normalizeGenreKey(g);
      if (key && !have.has(key) && !byKey.has(key)) byKey.set(key, canonicalGenreLabel(g));
    };
    for (const tr of session.catalogTracks) for (const g of parseTrackGenres(tr.genre)) add(g);
    for (const g of GENRE_POOL) add(g);
    return [...byKey.values()].sort((a, b) => a.localeCompare(b, i18n.sortLocale, { numeric: true }));
  });

  async function applyGenre(token: string, mode: "add" | "remove", target: Track[] = tracks) {
    const g = token.trim();
    if (!g || !target.length) return;
    const key = normalizeGenreKey(g);
    genreBusy = true;
    genreErr = null;
    try {
      for (const tr of target) {
        const cur = parseTrackGenres(tr.genre);
        const has = cur.some((x) => normalizeGenreKey(x) === key);
        if (mode === "add" ? has : !has) continue;
        const next = mode === "add" ? [...cur, g] : cur.filter((x) => normalizeGenreKey(x) !== key);
        const serialized = serializeTrackGenres(next);
        await api.trackInfoSave(tr.rel_path, { genre: serialized ?? "" });
        // Lists are immutable ($state.raw): replace the track everywhere it appears.
        session.patchTrack(tr.rel_path, { genre: serialized });
      }
    } catch (e) {
      genreErr = e instanceof Error ? e.message : String(e);
    } finally {
      genreBusy = false;
    }
  }

  async function addGenre(g: string) {
    await applyGenre(g, "add");
    genrePickerOpen = false;
  }

  async function fillGenre(g: string) {
    const missing = tracks.filter((tr) => !trackHasGenre(tr.genre, g));
    if (!missing.length) return;
    const ok = await confirmDialog({ title: t("albumMeta.addGenreMissingConfirm", { g, n: missing.length }) });
    if (ok) await applyGenre(g, "add", missing);
  }

  async function removeGenre(g: string) {
    const ok = await confirmDialog({ title: t("albumMeta.removeGenreAllConfirm", { g }), danger: true });
    if (ok) await applyGenre(g, "remove");
  }
</script>

<div class="album-page">
  <div class="album-page__back">
    <button type="button" class="album-page__back-btn" onclick={() => void session.backLibrary()}>
      <UiIcon name="chevronLeft" />
      <span>{album.artist_name}</span>
    </button>
  </div>

  <section class="album-hero rk-surface-card album-detail">
    <button
      type="button"
      class="album-detail__cover"
      title={t("library.editCover")}
      aria-label={t("library.uploadCoverAria")}
      onclick={() => session.openCoverEdit()}
    >
      <CoverArt kind="album" class="album-detail__cover-art" title={album.name} src={coverUrlFor(album)} size="xl" loading="eager" />
      <span class="album-detail__cover-badge" aria-hidden="true"><UiIcon name="image" /></span>
    </button>

    <div class="album-detail__info">
      <p class="rk-eyebrow">{album.loose ? t("library.looseAlbum") : t("library.albumDetail")}</p>
      <h1 class="album-detail__title">{album.name}</h1>
      <p class="album-detail__meta">
        <button type="button" class="album-detail__artist" onclick={() => void session.backLibrary()}>
          {album.artist_name}
        </button>
        {#if releaseLabel}<span>{releaseLabel}</span>{/if}
        {#if label}<span>{label}</span>{/if}
        <span>{tp("library.tracksCount", tracks.length || album.track_count)}</span>
        {#if totalMinutes > 0}<span>{t("library.albumMinutes", { n: totalMinutes })}</span>{/if}
      </p>

      <MetaBadgeCluster
        variant="hero"
        missingMeta={!albumHasAlbumMeta(album)}
        tracksMissingMetaCount={tracksMissingMeta}
        favoriteCount={favCount}
        {albumExcluded}
        {tracksExcludedCount}
        loose={album.loose}
      />

      <div class="album-detail__actions">
        <Button class="album-detail__play" disabled={!tracks.length} onclick={() => session.playSequence(tracks, 0)}>
          <UiIcon name="play" />
          {t("library.playAlbum")}
        </Button>
        <Button
          variant="ghost"
          disabled={!tracks.length}
          title={t("library.shuffleAlbum")}
          onclick={() => session.playPoolShuffle(tracks)}
        >
          <UiIcon name="shuffle" />
          <span class="album-detail__btn-label">{t("library.shuffleAlbum")}</span>
        </Button>
        <EntityInfoAction {artistDir} albumDir={album.loose ? null : albumDir} loose={album.loose} title={album.name} />
        <Button variant="ghost" title={t("library.editAlbum")} aria-label={t("library.editAlbum")} onclick={() => session.openAlbumEdit()}>
          <UiIcon name="edit" />
        </Button>
        <Button
          variant="ghost"
          aria-pressed={albumExcluded}
          title={t("library.shuffleBlock")}
          aria-label={t("library.shuffleBlock")}
          onclick={() => player.toggleExcludeAlbum(album.id)}
        >
          <UiIcon name="exclude" />
        </Button>
      </div>

      <div class="album-detail__genres" role="list" aria-label={t("library.albumGenresAria")}>
        {#each trackGenreStats as g (g.key)}
          <span class="album-hero__genre-chip" role="listitem">
            <button
              type="button"
              class="album-hero__genre-chip__text"
              disabled={genreBusy}
              title={t("albumMeta.applyGenreMissingTitle", { g: g.label, n: g.count, total: tracks.length })}
              onclick={() => void fillGenre(g.label)}
            >
              {g.label}
              {#if g.count < tracks.length}<span class="album-detail__genre-count">{g.count}/{tracks.length}</span>{/if}
            </button>
            <button
              type="button"
              class="album-hero__genre-chip__x"
              disabled={genreBusy}
              aria-label={t("trackMeta.fieldGenreRemoveAria", { g: g.label })}
              onclick={() => void removeGenre(g.label)}
            >
              <UiIcon name="close" class="album-hero__genre-chip__x-ic" />
            </button>
          </span>
        {/each}
        {#if genreOptions.length > 0}
          <div class="album-hero__genre-add-wrap" bind:this={genreAddWrapEl}>
            <button
              type="button"
              class="album-detail__genre-add"
              disabled={genreBusy}
              aria-expanded={genrePickerOpen}
              onclick={(e) => {
                e.stopPropagation();
                genrePickerOpen = !genrePickerOpen;
              }}
            >
              <UiIcon name="add" />
              <span>{t("trackMeta.fieldGenreAdd")}</span>
            </button>
            {#if genrePickerOpen}
              <ul class="track-row__overflow-menu album-hero__genre-menu rk-scroll" role="menu">
                {#each genreOptions as opt (opt)}
                  <li role="presentation">
                    <button type="button" role="menuitem" class="track-row__overflow-item" onclick={() => void addGenre(opt)}>
                      <span class="track-row__overflow-item-glyph" aria-hidden="true"><UiIcon name="style" /></span>
                      <span class="track-row__overflow-item-label">{opt}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
        {#if genreErr}
          <p class="album-hero__genre-err" role="alert">{genreErr}</p>
        {/if}
      </div>
    </div>
  </section>

  <section class="album-hero__tracks rk-surface-card">
    <div class="album-detail__tracks-head">
      <SectionHeadLead eyebrow={t("library.tracklist")} title={tp("library.tracksCount", tracks.length)}>
        <UiIcon name="music" />
      </SectionHeadLead>
      {#if expected != null}
        <span class="album-detail__expected" title={t("library.expectedTracksTitle", { n: tracks.length, total: expected })}>
          <UiIcon name="queueMusic" />
          {tracks.length}/{expected}
        </span>
      {/if}
    </div>
    <TrackList
      {tracks}
      numbered
      favoriteIds={session.favoriteIds}
      playlistOptions={session.playlistOptions}
      activeTrackId={session.current?.id ?? null}
      onplay={(track, list) => {
        const idx = list.findIndex((x) => x.id === track.id);
        session.playSequence(list, idx >= 0 ? idx : 0);
      }}
      ontoggleFavorite={(track) => void session.toggleFavorite(track)}
      onaddToPlaylist={(playlistId, track) => void session.addToPlaylist(playlistId, track.id)}
    />
  </section>
</div>

<style>
  .album-page {
    display: grid;
    gap: var(--rk-section-gap, 1rem);
    min-width: 0;
  }

  .album-page__back {
    display: flex;
  }

  .album-page__back-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    max-width: 100%;
    min-height: 2.25rem;
    padding: 0.25rem 0.75rem 0.25rem 0.4rem;
    border-radius: 999px;
    border: 1px solid var(--rk-line);
    background: var(--rk-surface-2);
    color: var(--rk-ink);
    font: inherit;
    font-size: var(--rk-fs-sm);
    font-weight: 600;
    cursor: pointer;
  }

  .album-page__back-btn span {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .album-page__back-btn :global(svg) {
    width: 1.15rem;
    height: 1.15rem;
    flex-shrink: 0;
  }

  .album-page__back-btn:hover {
    border-color: var(--rk-line-strong, var(--rk-line));
  }

  .album-detail {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 1.5rem;
    align-items: start;
    padding: var(--rk-space-lg, 1.25rem);
  }

  .album-detail__cover {
    position: relative;
    padding: 0;
    border: none;
    background: none;
    border-radius: var(--rk-radius-cover, 12px);
    cursor: pointer;
    line-height: 0;
  }

  .album-detail__cover :global(.rk-cover.album-detail__cover-art) {
    width: clamp(160px, 18vw, 220px);
    height: clamp(160px, 18vw, 220px);
    --cover-initials: 3rem;
  }

  .album-detail__cover-badge {
    position: absolute;
    right: 0.5rem;
    bottom: 0.5rem;
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--rk-surface) 88%, transparent);
    border: 1px solid var(--rk-line);
    color: var(--rk-ink);
    opacity: 0;
    transition: opacity 0.15s ease;
  }

  .album-detail__cover:hover .album-detail__cover-badge,
  .album-detail__cover:focus-visible .album-detail__cover-badge {
    opacity: 1;
  }

  .album-detail__cover-badge :global(svg) {
    width: 1rem;
    height: 1rem;
  }

  .album-detail__info {
    display: grid;
    gap: 0.55rem;
    min-width: 0;
    align-content: start;
  }

  .album-detail__info :global(.rk-eyebrow) {
    margin: 0;
  }

  .album-detail__title {
    margin: 0;
    font-size: clamp(1.5rem, 3vw, 2rem);
    line-height: 1.15;
    letter-spacing: -0.02em;
    overflow-wrap: anywhere;
  }

  .album-detail__meta {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.15rem 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
  }

  .album-detail__meta > :not(:first-child)::before {
    content: "·";
    margin: 0 0.45rem;
    color: var(--rk-muted);
  }

  .album-detail__artist {
    padding: 0;
    border: none;
    background: none;
    color: var(--rk-ink);
    font: inherit;
    font-weight: 650;
    cursor: pointer;
  }

  .album-detail__artist:hover {
    text-decoration: underline;
  }

  .album-detail__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .album-detail__actions :global(svg) {
    width: 1.1rem;
    height: 1.1rem;
    flex-shrink: 0;
  }

  .album-detail__genres {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .album-detail__genre-count {
    margin-left: 0.3rem;
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
    font-variant-numeric: tabular-nums;
  }

  .album-detail__genre-add {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-height: 1.9rem;
    padding: 0.2rem 0.7rem 0.2rem 0.5rem;
    border-radius: 999px;
    border: 1px dashed var(--rk-line-strong, var(--rk-line));
    background: transparent;
    color: var(--rk-muted-strong, var(--rk-muted));
    font: inherit;
    font-size: var(--rk-fs-xs);
    font-weight: 600;
    cursor: pointer;
  }

  .album-detail__genre-add :global(svg) {
    width: 0.95rem;
    height: 0.95rem;
  }

  .album-detail__genre-add:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .album-detail__genre-add:hover:not(:disabled) {
    color: var(--rk-ink);
    border-color: var(--rk-accent);
  }

  .album-detail__tracks-head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.65rem 1.25rem;
    margin: 0 0 var(--rk-space-md, 0.85rem);
  }

  .album-detail__expected {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--rk-muted);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .album-detail__expected :global(svg) {
    width: 18px;
    height: 18px;
  }

  /* Phones: cover centred on top, then title, meta and a full-width primary action. */
  @media (max-width: 719.98px) {
    .album-detail {
      grid-template-columns: minmax(0, 1fr);
      justify-items: center;
      gap: 1rem;
      padding: 1rem;
    }

    .album-detail__cover :global(.rk-cover.album-detail__cover-art) {
      width: min(62vw, 240px);
      height: min(62vw, 240px);
    }

    .album-detail__info {
      justify-items: center;
      text-align: center;
      width: 100%;
    }

    .album-detail__meta,
    .album-detail__actions,
    .album-detail__genres {
      justify-content: center;
    }

    .album-detail__actions :global(.album-detail__play) {
      flex: 1 1 100%;
      justify-content: center;
    }

    .album-detail__btn-label {
      display: none;
    }
  }
</style>
