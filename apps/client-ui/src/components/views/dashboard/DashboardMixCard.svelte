<script lang="ts">
  /**
   * Dashboard › Playlist al volo: pick genres and/or moods, see live how many
   * tracks match (with a few of their covers), shuffle them or queue them.
   *
   * Genres are split ("Hip Hop; Pop Rap" counts for both) and normalized
   * ("Hip Hop" = "hip-hop") through `lib/genres`. The matching and every chip
   * count come from one pass over a per-track index (`lib/mixFilter`), rebuilt
   * only when the catalog, the saved moods or the exclusions change.
   * The last selection is remembered per account (localStorage, this device).
   */
  import { Button, CoverArt, Panel, Skeleton } from "@rekord/ui";
  import MoodFilterGrid from "../MoodFilterGrid.svelte";
  import SectionHeadLead from "../../SectionHeadLead.svelte";
  import UiIcon from "../../icons/UiIcon.svelte";
  import { coverUrlFor, type Album, type Track } from "../../../lib/api";
  import { formatTotalDuration } from "../../../lib/collectionInfo";
  import { t, tp } from "../../../lib/i18n.svelte";
  import {
    computeMixFacets,
    computeMixTotals,
    mixGenresOf,
    parseMixSelection,
    pickCoverTracks,
    pruneMixSelection,
    sortGenreKeys,
    visibleGenreKeys,
    type MixEntry,
    type MixSelection,
  } from "../../../lib/mixFilter";
  import { player } from "../../../lib/player";
  import { prefsRevision } from "../../../lib/prefsRevision.svelte";
  import { session } from "../../../lib/session.svelte";
  import { buildSmartRandomQueue, CARD_QUEUE_CAP } from "../../../lib/smartShuffle";
  import { toasts } from "../../../lib/toasts.svelte";
  import { NO_GENRE_KEY, resolveTrackMoods, type TrackMoodId } from "../../../lib/trackMoods";
  import { loadUserPrefs } from "../../../lib/userPrefs";

  let { loading = false }: { loading?: boolean } = $props();

  /** Genres shown before "+N altri". */
  const GENRE_LIMIT = 10;
  const STORE_PREFIX = "rekord.next.dashMix.";

  function readSaved(key: string): MixSelection {
    try {
      return parseMixSelection(localStorage.getItem(key));
    } catch {
      return parseMixSelection(null);
    }
  }

  // ── Remembered selection (per account, on this device) ───────────────────
  const storeKey = $derived(STORE_PREFIX + (session.activeAccountId || "default"));
  let loadedKey = STORE_PREFIX + (session.activeAccountId || "default");
  const initial = readSaved(loadedKey);

  /** Selected genre chip keys (`genreLabelKey`) or NO_GENRE_KEY. */
  let mixGenres = $state<string[]>([...initial.genres]);
  let mixMoods = $state<TrackMoodId[]>([...initial.moods]);
  let mixMatchAll = $state(initial.matchAll);
  let genresExpanded = $state(false);

  // Account switch: load that account's last selection.
  $effect(() => {
    const key = storeKey;
    if (key === loadedKey) return;
    loadedKey = key;
    const saved = readSaved(key);
    mixGenres = [...saved.genres];
    mixMoods = [...saved.moods];
    mixMatchAll = saved.matchAll;
  });

  function persist() {
    try {
      const empty = !mixGenres.length && !mixMoods.length && !mixMatchAll;
      if (empty) localStorage.removeItem(storeKey);
      else
        localStorage.setItem(
          storeKey,
          JSON.stringify({ genres: mixGenres, moods: mixMoods, matchAll: mixMatchAll }),
        );
    } catch {
      /* private mode / quota: the selection just is not remembered */
    }
  }

  // ── Per-track index (catalog / moods / exclusions change) ─────────────────
  const savedMoods = $derived.by(() => {
    void session.moodPrefsTick;
    void prefsRevision.moods;
    return loadUserPrefs().trackMoods;
  });

  const albumById = $derived(new Map<number, Album>(session.allAlbums.map((a) => [a.id, a])));

  /** Genre chip keys per track (album genre as fallback, aliases merged) + label per key. */
  const genreIndex = $derived.by(() => {
    const keys = new Map<number, string[]>();
    const labels = new Map<string, string>();
    for (const tr of session.catalogTracks) {
      const album = tr.album_id != null ? albumById.get(tr.album_id) : null;
      const chips = mixGenresOf(tr, album);
      for (const c of chips) if (!labels.has(c.key)) labels.set(c.key, c.label);
      keys.set(tr.id, chips.length ? chips.map((c) => c.key) : [NO_GENRE_KEY]);
    }
    return { keys, labels };
  });

  /** Shuffle-eligible tracks (exclusions honoured, like every random mix). */
  const pool = $derived.by(() => {
    void prefsRevision.exclusions;
    return session.catalogTracks.filter((tr) => !player.isTrackExcluded(tr));
  });

  const entries = $derived<MixEntry[]>(
    pool.map((tr) => ({
      genres: genreIndex.keys.get(tr.id) ?? [NO_GENRE_KEY],
      moods: resolveTrackMoods(tr.id, tr.rel_path, savedMoods),
      durationMs: tr.duration_ms,
    })),
  );

  const catalogReady = $derived(session.catalogTracks.length > 0);
  const totals = $derived(computeMixTotals(entries));

  function genreLabel(key: string) {
    return key === NO_GENRE_KEY ? t("library.noGenre") : (genreIndex.labels.get(key) ?? key);
  }

  const orderedGenres = $derived(sortGenreKeys(totals.genres, genreLabel));

  // ── Selection → matches + live counts (one pass per click) ───────────────
  const selection = $derived.by<MixSelection>(() => {
    const raw = { genres: mixGenres, moods: mixMoods, matchAll: mixMatchAll };
    return catalogReady ? pruneMixSelection(raw, (k) => totals.genres.has(k)) : raw;
  });
  const hasSelection = $derived(selection.genres.length > 0 || selection.moods.length > 0);
  const facets = $derived(computeMixFacets(entries, selection));
  const matchCount = $derived(facets.matches.length);
  const duration = $derived(formatTotalDuration(facets.totalMs));

  const genreRow = $derived(
    visibleGenreKeys(orderedGenres, selection.genres, GENRE_LIMIT, genresExpanded),
  );

  const coverTracks = $derived(
    pickCoverTracks(
      facets.matches.slice(0, 400).map((i) => pool[i]!),
      (tr) => coverUrlFor(tr, 128) != null,
      5,
    ),
  );

  const zeroBecauseAll = $derived(
    hasSelection && matchCount === 0 && selection.matchAll && selection.moods.length >= 2,
  );

  // ── Actions ───────────────────────────────────────────────────────────────
  function toggleGenre(key: string) {
    void session.ensureCatalogTracks();
    mixGenres = mixGenres.includes(key) ? mixGenres.filter((g) => g !== key) : [...mixGenres, key];
    persist();
  }

  function toggleMood(id: TrackMoodId) {
    void session.ensureCatalogTracks();
    mixMoods = mixMoods.includes(id) ? mixMoods.filter((m) => m !== id) : [...mixMoods, id];
    persist();
  }

  function setMatchAll(all: boolean) {
    mixMatchAll = all;
    persist();
  }

  function clearMix() {
    mixGenres = [];
    mixMoods = [];
    mixMatchAll = false;
    persist();
  }

  /** Matching tracks against the freshest catalog (a click may beat the load). */
  async function matchingNow(): Promise<Track[]> {
    await session.ensureCatalogTracks();
    return facets.matches.map((i) => pool[i]!).filter(Boolean);
  }

  async function playMix() {
    const list = await matchingNow();
    if (!list.length) {
      toasts.info(t("mix.nothing"));
      return;
    }
    session.playPoolShuffle(list);
    session.studioPane = "listen";
    session.navigate("studio");
  }

  async function queueMix() {
    const list = await matchingNow();
    if (!list.length) {
      toasts.info(t("mix.nothing"));
      return;
    }
    const shuffled = buildSmartRandomQueue(list).slice(0, CARD_QUEUE_CAP);
    player.addToQueue(shuffled);
    toasts.info(tp("mix.queued", shuffled.length));
  }

  function openLibrary() {
    session.navigate("library");
    session.libraryBrowse = "genres";
  }
</script>

<Panel class="session-card dashboard-session-card dashboard-mix-card dashboard-page__full dashboard-page__mix">
  <header class="section-head section-head--page-toolbar">
    <SectionHeadLead eyebrow={t("mix.eyebrow")} title={t("mix.title")}>
      <UiIcon name="music" />
    </SectionHeadLead>
    <div class="section-head__tools">
      <button type="button" class="text-btn" onclick={openLibrary}>{t("dashboard.openLibrary")}</button>
    </div>
  </header>
  <p class="mix-subtitle">{t("mix.subtitle")}</p>

  <div class="mix-body">
    <section class="mix-row" aria-labelledby="mix-genres-label">
      <div class="mix-row__head">
        <span class="mix-row-label" id="mix-genres-label">{t("dashboard.genres")}</span>
        <span class="mix-row-help">{t("mix.genresHelp")}</span>
      </div>
      {#if loading && !catalogReady}
        <Skeleton variant="text" lines={2} />
      {:else if !orderedGenres.length}
        <p class="mix-row-note">{t("dashboard.noGenres")}</p>
      {:else}
        <div class="mix-chips" role="group" aria-labelledby="mix-genres-label">
          {#each genreRow.shown as key (key)}
            {@const on = selection.genres.includes(key)}
            {@const count = facets.genreCounts.get(key) ?? 0}
            {@const name = genreLabel(key)}
            <button
              type="button"
              class="mix-chip"
              class:mix-chip--secondary={key === NO_GENRE_KEY}
              class:is-on={on}
              disabled={count === 0 && !on}
              aria-pressed={on}
              aria-label={tp("mix.chipAria", count, { name })}
              title={name}
              onclick={() => toggleGenre(key)}
            >
              <span class="mix-chip__name">{name}</span>
              <span class="mix-chip__count" aria-hidden="true">{count}</span>
            </button>
          {/each}
          {#if genreRow.hidden > 0 || genresExpanded}
            <button
              type="button"
              class="mix-more"
              aria-expanded={genresExpanded}
              onclick={() => (genresExpanded = !genresExpanded)}
            >
              {genresExpanded ? t("mix.fewer") : tp("mix.moreGenres", genreRow.hidden)}
            </button>
          {/if}
        </div>
      {/if}
    </section>

    <section class="mix-row" aria-label={t("dashboard.moods")}>
      <MoodFilterGrid
        label={t("dashboard.moods")}
        selected={selection.moods}
        counts={facets.moodCounts}
        totals={totals.moods}
        matchAll={mixMatchAll}
        countsReady={catalogReady}
        onmatch={setMatchAll}
        ontoggle={toggleMood}
      />
    </section>

    <div
      class="mix-result"
      class:has-selection={hasSelection}
      class:is-ready={hasSelection && matchCount > 0}
    >
      <div class="mix-result__summary" aria-live="polite">
        {#if hasSelection && matchCount > 0}
          {#if coverTracks.length}
            <span class="mix-covers" aria-hidden="true">
              {#each coverTracks as tr (tr.id)}
                <span class="mix-covers__item">
                  <CoverArt kind="album" size="xs" title={tr.album_name} src={coverUrlFor(tr, 128)} />
                </span>
              {/each}
            </span>
          {/if}
          <p class="mix-result__text">
            <strong>{tp("core.count.tracks", matchCount)}</strong>
            {#if duration}<span class="mix-result__dur">· {t("mix.about", { d: duration })}</span>{/if}
          </p>
        {:else if hasSelection}
          <p class="mix-result__text mix-result__text--empty">
            {zeroBecauseAll ? t("mix.noneAll") : t("mix.none")}
            {#if zeroBecauseAll}
              <button type="button" class="text-btn" onclick={() => setMatchAll(false)}>
                {t("mix.useAny")}
              </button>
            {/if}
          </p>
        {:else}
          <p class="mix-result__text mix-result__text--hint">
            <UiIcon name="shuffle" />
            {t("mix.pick")}
          </p>
        {/if}
      </div>
      <div class="mix-result__actions">
        {#if hasSelection}
          <button type="button" class="text-btn mix-result__clear" onclick={clearMix}>
            {t("mix.clear")}
          </button>
          <Button
            variant="secondary"
            class="mix-result__queue"
            disabled={matchCount === 0}
            title={t("mix.queue")}
            onclick={() => void queueMix()}
          >
            <UiIcon name="queueMusic" />
            <span class="mix-result__queue-label">{t("mix.queue")}</span>
          </Button>
        {/if}
        <Button
          class="mix-result__start"
          disabled={!hasSelection || matchCount === 0}
          title={t("mix.startTitle")}
          onclick={() => void playMix()}
        >
          <UiIcon name="shuffle" />
          {t("mix.start")}
        </Button>
      </div>
    </div>
  </div>
</Panel>
