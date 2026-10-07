<script lang="ts">
  import { isExternalTrack } from "../../lib/externalItems";
  /**
   * Plectr pick screen: the selected track (big card + Play), difficulty with
   * its level, suggestion carousels (track of the day, recent in Plectr, your
   * records, up next in the queue) before typing, then library search.
   */
  import { radioGroupKeys } from "../../lib/radioGroupKeys";
  import UiIcon from "../icons/UiIcon.svelte";
  import GradeChips from "./GradeChips.svelte";
  import PlectrCover from "./PlectrCover.svelte";
  import type { Track } from "../../lib/api";
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { DIFFICULTIES } from "../../lib/plectr/config";
  import {
    lookupByRelPathAliases,
    selectPlectrTrackRecords,
    type PlectrStore,
  } from "../../lib/plectr/records";
  import { dailyIndex, dayKey } from "../../lib/plectr/timing";
  import type { DifficultyId } from "../../lib/plectr/types";
  import { session } from "../../lib/session.svelte";

  let {
    store,
    selected,
    difficulty,
    busy = false,
    onselect,
    onplay,
    ondifficulty,
    onrecords,
    onsettings,
    onback,
  }: {
    store: PlectrStore;
    selected: Track | null;
    difficulty: DifficultyId;
    busy?: boolean;
    onselect: (track: Track) => void;
    onplay: (track: Track) => void;
    ondifficulty: (id: DifficultyId) => void;
    onrecords: () => void;
    onsettings: () => void;
    /** Back to the game running on the player's song (opened via "Change song"). */
    onback?: () => void;
  } = $props();

  const ROW_LIMIT = 12;
  let query = $state("");

  const byPath = $derived.by(() => {
    const map = new Map<string, Track>();
    for (const tr of session.catalogTracks) map.set(tr.rel_path, tr);
    for (const tr of session.queue) map.set(tr.rel_path, tr);
    return map;
  });

  function trackFor(relPath: string): Track | undefined {
    return byPath.get(relPath) ?? byPath.get(relPath.replace("/Tracce/", "/Tracks/"));
  }

  const daily = $derived.by(() => {
    const pool = session.catalogTracks;
    if (!pool.length) return null;
    const sorted = [...pool].sort((a, b) => a.rel_path.localeCompare(b.rel_path));
    return sorted[dailyIndex(dayKey(), sorted.length)] ?? null;
  });

  const featured = $derived(
    selected ?? (isExternalTrack(session.current) ? null : session.current) ?? daily,
  );
  const featuredBests = $derived(
    featured ? (lookupByRelPathAliases(store.byDifficulty, featured.rel_path) ?? null) : null,
  );
  const featuredIsDaily = $derived(!!featured && !!daily && featured.rel_path === daily.rel_path);

  const recentRows = $derived(
    store.recent
      .map((p) => trackFor(p))
      .filter((tr): tr is Track => !!tr)
      .slice(0, ROW_LIMIT),
  );

  const recordRows = $derived.by(() => {
    const rows = selectPlectrTrackRecords(store)
      .filter((r) => r.best)
      .sort((a, b) => (b.best?.score ?? 0) - (a.best?.score ?? 0));
    const out: Track[] = [];
    for (const r of rows) {
      const tr = trackFor(r.relPath);
      if (tr) out.push(tr);
      if (out.length >= ROW_LIMIT) break;
    }
    return out;
  });

  const queueRows = $derived.by(() => {
    const queue = session.queue;
    if (!queue.length) return [] as Track[];
    const start = Math.max(0, session.currentIndex);
    const out: Track[] = [];
    for (let i = 0; i < queue.length && out.length < ROW_LIMIT; i += 1) {
      out.push(queue[(start + i) % queue.length]!);
    }
    return out;
  });

  const searchRows = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase();
    if (!q) return [] as Track[];
    const out: Track[] = [];
    for (const tr of session.catalogTracks) {
      if (
        tr.title.toLocaleLowerCase().includes(q) ||
        tr.artist_name.toLocaleLowerCase().includes(q) ||
        tr.album_name.toLocaleLowerCase().includes(q)
      ) {
        out.push(tr);
        if (out.length >= 30) break;
      }
    }
    return out;
  });

  const catalogLoading = $derived(!session.catalogTracks.length);

  function bestScore(tr: Track): string | null {
    const best = lookupByRelPathAliases(store.bests, tr.rel_path);
    return best ? fmtNumber(best.score) : null;
  }

  function pickRandom() {
    const pool = session.catalogTracks;
    if (!pool.length) return;
    onselect(pool[Math.floor(Math.random() * pool.length)]!);
  }
</script>

{#snippet tile(tr: Track, badge?: string)}
  <li>
    <button
      type="button"
      class="plectr-tile"
      class:is-selected={featured?.rel_path === tr.rel_path}
      onclick={() => onselect(tr)}
      ondblclick={() => onplay(tr)}
      title={`${tr.title} — ${tr.artist_name}`}
    >
      <PlectrCover track={tr} class="plectr-tile__art" />
      {#if badge}<span class="plectr-tile__badge">{badge}</span>{/if}
      <span class="plectr-tile__title">{tr.title}</span>
      <span class="plectr-tile__meta">{tr.artist_name}</span>
      <GradeChips bests={lookupByRelPathAliases(store.byDifficulty, tr.rel_path)} compact />
    </button>
  </li>
{/snippet}

{#snippet carousel(titleKey: string, rows: Track[], badge?: string)}
  {#if rows.length}
    <section class="plectr-carousel" aria-label={t(titleKey)}>
      <h3 class="plectr-carousel__title">{t(titleKey)}</h3>
      <ul class="plectr-carousel__row">
        {#each rows as tr (tr.rel_path)}
          {@render tile(tr, badge)}
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

<div class="plectr-pick">
  <header class="plectr-pick__head">
    <span class="plectr-brand"><UiIcon name="plectrum" /> {t("plectr.title")}</span>
    <div class="plectr-pick__tools">
      {#if onback}
        <button type="button" class="plectr-icon-btn" onclick={onback} aria-label={t("plectr.backToGame")} title={t("plectr.backToGame")}>
          <UiIcon name="close" />
        </button>
      {/if}
      <button type="button" class="plectr-icon-btn" onclick={onrecords} aria-label={t("plectr.records.open")} title={t("plectr.records.open")}>
        <UiIcon name="trophy" />
      </button>
      <button type="button" class="plectr-icon-btn" onclick={onsettings} aria-label={t("plectr.settings.open")} title={t("plectr.settings.open")}>
        <UiIcon name="settings" />
      </button>
    </div>
  </header>

  {#if featured}
    <section class="plectr-feature" aria-label={t("plectr.pick.selected")}>
      <PlectrCover track={featured} size={256} class="plectr-feature__art" />
      <div class="plectr-feature__text">
        <span class="plectr-feature__eyebrow">
          {#if featuredIsDaily && !selected}{t("plectr.pick.daily")}{:else if session.current?.rel_path === featured.rel_path}{session.playing ? t("plectr.nowPlaying") : t("plectr.paused.title")}{:else}{t("plectr.pick.selected")}{/if}
        </span>
        <strong class="plectr-feature__title">{featured.title}</strong>
        <span class="plectr-feature__meta">{featured.artist_name}</span>
        <GradeChips bests={featuredBests} />
      </div>
    </section>
  {/if}

  <div class="plectr-diff" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.difficulty")}>
    {#each DIFFICULTIES as d (d.id)}
      <button
        type="button"
        role="radio"
        aria-checked={difficulty === d.id}
        class="plectr-diff__opt plectr-diff__opt--{d.id}"
        class:is-on={difficulty === d.id}
        onclick={() => ondifficulty(d.id)}
      >
        <span>{t(`plectr.diff.${d.id}`)}</span>
        <small>{t("plectr.level", { n: d.level })}</small>
      </button>
    {/each}
  </div>

  <div class="plectr-pick__cta">
    <button
      type="button"
      class="rk-btn rk-btn--primary plectr-play-btn"
      disabled={!featured || busy}
      onclick={() => featured && onplay(featured)}
    >
      <UiIcon name="play" />
      {t("plectr.pick.play")}
    </button>
    <button type="button" class="rk-btn rk-btn--secondary" disabled={catalogLoading} onclick={pickRandom}>
      <UiIcon name="shuffle" />
      {t("plectr.picker.random")}
    </button>
  </div>

  <div class="plectr-search">
    <UiIcon name="search" />
    <input
      type="search"
      bind:value={query}
      placeholder={t("plectr.picker.searchPlaceholder")}
      aria-label={t("plectr.picker.searchPlaceholder")}
    />
  </div>

  {#if query.trim()}
    {#if searchRows.length === 0}
      <p class="plectr-empty">{catalogLoading ? t("plectr.picker.loading") : t("plectr.picker.noResults")}</p>
    {:else}
      <ul class="plectr-results-list">
        {#each searchRows as tr (tr.rel_path)}
          {@const score = bestScore(tr)}
          <li>
            <button
              type="button"
              class="plectr-pick-row"
              class:is-active={featured?.rel_path === tr.rel_path}
              onclick={() => onselect(tr)}
              ondblclick={() => onplay(tr)}
            >
              <PlectrCover track={tr} class="plectr-pick-row__art" />
              <span class="plectr-pick-row__text">
                <span class="plectr-pick-row__title">{tr.title}</span>
                <span class="plectr-pick-row__meta">{tr.artist_name} · {tr.album_name}</span>
              </span>
              {#if score}<span class="plectr-pick-row__aside">{score}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    {@render carousel("plectr.pick.recent", recentRows)}
    {@render carousel("plectr.pick.records", recordRows)}
    {@render carousel("plectr.pick.queue", queueRows)}
    {#if daily && !recentRows.length && !recordRows.length && !queueRows.length}
      {@render carousel("plectr.pick.daily", [daily])}
    {/if}
    {#if catalogLoading && !queueRows.length && !recentRows.length}
      <p class="plectr-empty">{t("plectr.picker.loading")}</p>
    {/if}
  {/if}
</div>
