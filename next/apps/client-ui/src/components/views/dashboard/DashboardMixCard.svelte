<script lang="ts">
  /**
   * Dashboard › Generi e mood: pick genres and/or moods, see how many tracks
   * match, shuffle them. Genres are split ("Hip Hop; Pop Rap" counts for both)
   * and normalized ("Hip Hop" = "hip-hop") through `lib/genres`.
   */
  import { Button, Panel, Skeleton } from "@rekord/ui";
  import MoodFilterGrid from "../MoodFilterGrid.svelte";
  import SectionHeadLead from "../../SectionHeadLead.svelte";
  import TrackMoodGlyph from "../../TrackMoodGlyph.svelte";
  import UiIcon from "../../icons/UiIcon.svelte";
  import type { Album, Track } from "../../../lib/api";
  import { normalizeGenreKey, trackGenres } from "../../../lib/genres";
  import { t, tp } from "../../../lib/i18n.svelte";
  import { player } from "../../../lib/player";
  import { prefsRevision } from "../../../lib/prefsRevision.svelte";
  import { session } from "../../../lib/session.svelte";
  import { toasts } from "../../../lib/toasts.svelte";
  import {
    NO_GENRE_KEY,
    TRACK_MOOD_COLORS,
    TRACK_MOOD_IDS,
    resolveTrackMoods,
    trackMatchesMoodFilter,
    trackMoodLabelKey,
    type TrackMoodId,
  } from "../../../lib/trackMoods";
  import { loadUserPrefs } from "../../../lib/userPrefs";

  let { loading = false }: { loading?: boolean } = $props();

  /** Selected genre keys (`normalizeGenreKey`) or NO_GENRE_KEY. */
  let mixGenres = $state<string[]>([]);
  let mixMoods = $state<TrackMoodId[]>([]);
  let mixMatchAll = $state(false);

  const savedMoods = $derived.by(() => {
    void session.moodPrefsTick;
    void prefsRevision.moods;
    return loadUserPrefs().trackMoods;
  });
  const exclusionsRev = $derived(prefsRevision.exclusions);

  const albumById = $derived(new Map<number, Album>(session.allAlbums.map((a) => [a.id, a])));

  /** Genre keys per track (album genre as fallback), computed once per catalog. */
  const genreIndex = $derived.by(() => {
    const keys = new Map<number, string[]>();
    const labels = new Map<string, string>();
    for (const tr of session.catalogTracks) {
      const album = tr.album_id != null ? albumById.get(tr.album_id) : null;
      const list = trackGenres(tr, album);
      const trackKeys: string[] = [];
      for (const label of list) {
        const key = normalizeGenreKey(label);
        if (!key) continue;
        trackKeys.push(key);
        if (!labels.has(key)) labels.set(key, label);
      }
      keys.set(tr.id, trackKeys.length ? trackKeys : [NO_GENRE_KEY]);
    }
    return { keys, labels };
  });

  function genreLabel(key: string) {
    return key === NO_GENRE_KEY ? t("library.noGenre") : (genreIndex.labels.get(key) ?? key);
  }

  function moodLabel(id: TrackMoodId) {
    return t(trackMoodLabelKey(id));
  }

  function matching(pool: readonly Track[], prefs: Record<string, string[]>): Track[] {
    let list = pool.filter((tr) => !player.isTrackExcluded(tr));
    if (mixGenres.length) {
      const want = new Set(mixGenres);
      list = list.filter((tr) => (genreIndex.keys.get(tr.id) ?? [NO_GENRE_KEY]).some((k) => want.has(k)));
    }
    if (mixMoods.length) {
      list = list.filter((tr) =>
        trackMatchesMoodFilter(resolveTrackMoods(tr.id, tr.rel_path, prefs), mixMoods, mixMatchAll),
      );
    }
    return list;
  }

  const mixReady = $derived(mixGenres.length > 0 || mixMoods.length > 0);

  const previewCount = $derived.by(() => {
    void exclusionsRev;
    if (!mixReady) return 0;
    return matching(session.catalogTracks, savedMoods).length;
  });

  const moodCounts = $derived.by(() => {
    void exclusionsRev;
    const counts = Object.fromEntries(TRACK_MOOD_IDS.map((id) => [id, 0])) as Record<TrackMoodId, number>;
    for (const tr of session.catalogTracks) {
      if (player.isTrackExcluded(tr)) continue;
      for (const m of resolveTrackMoods(tr.id, tr.rel_path, savedMoods)) counts[m] += 1;
    }
    return counts;
  });

  const genreChips = $derived.by(() => {
    void exclusionsRev;
    const counts = new Map<string, number>();
    for (const tr of session.catalogTracks) {
      if (player.isTrackExcluded(tr)) continue;
      for (const key of genreIndex.keys.get(tr.id) ?? [NO_GENRE_KEY]) {
        counts.set(key, (counts.get(key) ?? 0) + 1);
      }
    }
    return [...counts.entries()]
      .map(([key, count]) => ({ key, label: genreLabel(key), count }))
      .sort((a, b) => {
        if (a.key === NO_GENRE_KEY) return 1;
        if (b.key === NO_GENRE_KEY) return -1;
        return b.count - a.count || a.label.localeCompare(b.label);
      })
      .slice(0, 16);
  });

  function toggleGenre(key: string) {
    mixGenres = mixGenres.includes(key) ? mixGenres.filter((g) => g !== key) : [...mixGenres, key];
  }

  function toggleMood(id: TrackMoodId) {
    mixMoods = mixMoods.includes(id) ? mixMoods.filter((m) => m !== id) : [...mixMoods, id];
  }

  function clearMix() {
    mixGenres = [];
    mixMoods = [];
  }

  async function playMix() {
    if (!mixReady) return;
    const pool = await session.ensureCatalogTracks();
    const list = matching(pool, loadUserPrefs().trackMoods);
    if (!list.length) {
      toasts.info(t("dashboard.mixNothing"));
      return;
    }
    session.playPoolShuffle(list);
    session.studioPane = "listen";
    session.navigate("studio");
  }

  function openGenres() {
    session.navigate("library");
    session.libraryBrowse = "genres";
  }
</script>

<Panel class="session-card dashboard-session-card dashboard-mix-card dashboard-page__full dashboard-page__mix">
  <header class="section-head section-head--page-toolbar">
    <SectionHeadLead eyebrow={t("dashboard.mixEyebrow")} title={t("dashboard.mixTitle")}>
      <UiIcon name="music" />
    </SectionHeadLead>
    <div class="section-head__tools">
      {#if mixReady}
        <button type="button" class="text-btn" onclick={clearMix}>{t("library.moodClear")}</button>
      {/if}
      <button type="button" class="text-btn" onclick={openGenres}>{t("dashboard.openLibrary")}</button>
    </div>
  </header>
  <div class="dashboard-mix-body">
    {#if mixReady}
      <div class="dashboard-mix-selection" aria-live="polite">
        <div class="dashboard-mix-selection__chips">
          {#each mixGenres as g (g)}
            <button
              type="button"
              class="dashboard-mix-pill"
              title={t("dashboard.remove", { name: genreLabel(g) })}
              aria-label={t("dashboard.remove", { name: genreLabel(g) })}
              onclick={() => toggleGenre(g)}
            >
              <span>{genreLabel(g)}</span>
              <UiIcon name="close" class="dashboard-mix-pill__x" />
            </button>
          {/each}
          {#each mixMoods as id (id)}
            <button
              type="button"
              class="dashboard-mix-pill dashboard-mix-pill--mood"
              style="--mood-c:{TRACK_MOOD_COLORS[id]}"
              title={t("dashboard.remove", { name: moodLabel(id) })}
              aria-label={t("dashboard.remove", { name: moodLabel(id) })}
              onclick={() => toggleMood(id)}
            >
              <TrackMoodGlyph mood={id} />
              <span>{moodLabel(id)}</span>
              <UiIcon name="close" class="dashboard-mix-pill__x" />
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="dashboard-mix-panels">
      <div class="dashboard-mix-panel">
        <div class="dashboard-mix-panel__head">
          <span class="dash-mix-label">{t("dashboard.genres")}</span>
          {#if mixGenres.length}
            <button type="button" class="text-btn" onclick={() => (mixGenres = [])}>
              {t("dashboard.clear")}
            </button>
          {/if}
        </div>
        {#if loading && !session.catalogTracks.length}
          <Skeleton variant="text" lines={3} />
        {:else}
          <div class="dashboard-mix-genre-chips">
            {#each genreChips as g (g.key)}
              <button
                type="button"
                class="dashboard-mix-genre-chip"
                class:is-on={mixGenres.includes(g.key)}
                aria-pressed={mixGenres.includes(g.key)}
                title={g.label}
                onclick={() => toggleGenre(g.key)}
              >
                <span class="dashboard-mix-genre-chip__label">{g.label}</span>
                <span class="dashboard-mix-genre-chip__count">{g.count}</span>
              </button>
            {:else}
              <p class="dashboard-mix-empty">{t("dashboard.noGenres")}</p>
            {/each}
          </div>
        {/if}
      </div>

      <div class="dashboard-mix-panel">
        <MoodFilterGrid
          label={t("dashboard.moods")}
          selected={mixMoods}
          counts={moodCounts}
          matchAll={mixMatchAll}
          countsReady={session.catalogTracks.length > 0}
          onmatch={(all) => (mixMatchAll = all)}
          ontoggle={(id) => {
            void session.ensureCatalogTracks();
            toggleMood(id);
          }}
        />
      </div>
    </div>

    <div class="dashboard-mix-footer">
      <p class="dashboard-mix-footer__hint">
        {#if !mixReady}
          {t("dashboard.mixHintPick")}
        {:else if previewCount === 0}
          {t("dashboard.mixHintEmpty")}
        {:else}
          {tp("dashboard.mixQueued", previewCount)}
        {/if}
      </p>
      <Button
        class="dashboard-mix-footer__listen"
        disabled={!mixReady || previewCount === 0}
        title={mixReady ? t("dashboard.mixListenTitle") : t("dashboard.mixListenDisabled")}
        onclick={() => void playMix()}
      >
        <UiIcon name="shuffle" />
        {t("library.listen")}
      </Button>
    </div>
  </div>
</Panel>

<style>
  .dash-mix-label {
    font-size: var(--rk-fs-sm);
    font-weight: 650;
    color: var(--rk-ink);
  }

</style>
