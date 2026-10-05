<script lang="ts">
  /** Plectr records: career card + one row per track (E / N / H grade chips), sortable and filterable. */
  import { radioGroupKeys } from "../../lib/radioGroupKeys";
  import UiIcon from "../icons/UiIcon.svelte";
  import CareerCard from "./CareerCard.svelte";
  import GradeChips from "./GradeChips.svelte";
  import PlectrCover from "./PlectrCover.svelte";
  import type { Track } from "../../lib/api";
  import { fmtDate, fmtNumber, t } from "../../lib/i18n.svelte";
  import { selectPlectrCareer, selectPlectrTrackRecords, type PlectrStore } from "../../lib/plectr/records";
  import { session } from "../../lib/session.svelte";

  let {
    store,
    onback,
    onpick,
  }: {
    store: PlectrStore;
    onback: () => void;
    onpick: (track: Track) => void;
  } = $props();

  type SortKey = "date" | "score" | "title";
  let sort = $state<SortKey>("date");
  let query = $state("");

  const career = $derived(selectPlectrCareer(store));

  const byPath = $derived.by(() => {
    const map = new Map<string, Track>();
    for (const tr of session.catalogTracks) map.set(tr.rel_path, tr);
    for (const tr of session.queue) map.set(tr.rel_path, tr);
    return map;
  });

  const rows = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase();
    const out = selectPlectrTrackRecords(store).map((r) => {
      const track = byPath.get(r.relPath) ?? byPath.get(r.relPath.replace("/Tracce/", "/Tracks/")) ?? null;
      const name = r.relPath.split("/").pop() ?? r.relPath;
      return { ...r, track, title: track?.title ?? name.replace(/\.[a-z0-9]+$/i, ""), artist: track?.artist_name ?? "" };
    });
    const filtered = q
      ? out.filter((r) => r.title.toLocaleLowerCase().includes(q) || r.artist.toLocaleLowerCase().includes(q))
      : out;
    if (sort === "score") filtered.sort((a, b) => (b.best?.score ?? 0) - (a.best?.score ?? 0));
    else if (sort === "title") filtered.sort((a, b) => a.title.localeCompare(b.title));
    return filtered;
  });

</script>

<div class="plectr-records">
  <header class="plectr-records__head">
    <button type="button" class="rk-btn rk-btn--ghost" onclick={onback}>
      <UiIcon name="chevronLeft" />
      {t("plectr.records.back")}
    </button>
    <h2>{t("plectr.records.title")}</h2>
  </header>

  <CareerCard {career} />

  <div class="plectr-records__tools">
    <div class="plectr-search">
      <UiIcon name="search" />
      <input type="search" bind:value={query} placeholder={t("plectr.records.filter")} aria-label={t("plectr.records.filter")} />
    </div>
    <div class="plectr-seg" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.records.sort")}>
      {#each ["date", "score", "title"] as const as key (key)}
        <button type="button" role="radio" aria-checked={sort === key} class:is-on={sort === key} onclick={() => (sort = key)}
          >{t(`plectr.records.sort_${key}`)}</button
        >
      {/each}
    </div>
  </div>

  {#if rows.length === 0}
    <p class="plectr-empty">{query.trim() ? t("plectr.picker.noResults") : t("plectr.picker.recordsEmpty")}</p>
  {:else}
    <ul class="plectr-records__list">
      {#each rows as r (r.relPath)}
        <li class="plectr-record-row">
          <PlectrCover track={r.track ?? { album_id: null, rel_path: r.relPath }} class="plectr-record-row__art" />
          <span class="plectr-record-row__text">
            <strong>{r.title}</strong>
            <span>{r.artist}</span>
          </span>
          <GradeChips bests={r.byDifficulty} />
          <span class="plectr-record-row__score">{r.best ? fmtNumber(r.best.score) : "—"}</span>
          <span class="plectr-record-row__date">{r.updatedAt ? fmtDate(r.updatedAt, "date") : "—"}</span>
          <button
            type="button"
            class="plectr-icon-btn"
            disabled={!r.track}
            aria-label={t("plectr.records.play", { title: r.title })}
            title={t("plectr.pick.play")}
            onclick={() => r.track && onpick(r.track)}
          >
            <UiIcon name="play" />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
