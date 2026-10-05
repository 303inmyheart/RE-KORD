<script lang="ts">
  /**
   * Dashboard card with a live mini preview of Sonic Nebula (legacy
   * DashboardNebulaCard + SonicNebulaMiniPreview). Built from a deterministic
   * sample of the library so it stays cheap on large catalogs. It draws a
   * static frame and animates (≤10 fps) only while hovered or focused and on
   * screen. Clicking opens Library → Nebula in fullscreen.
   */
  import { Panel } from "@rekord/ui";
  import type { Track } from "../../lib/api";
  import { t } from "../../lib/i18n.svelte";
  import {
    buildNebulaModel,
    defaultNebulaCamera,
    sampleNebulaStarsForPreview,
    sampleTracksForNebulaBuild,
  } from "../../lib/nebula/model";
  import { requestNebulaFullscreen } from "../../lib/nebula/fullscreen";
  import { chartBpm, chartBpmSource, ensureChartBpmSource } from "../../lib/nebula/chartBpm.svelte";
  import { nebulaInputsFromTracks } from "../../lib/nebula/source";
  import { prefsRevision } from "../../lib/prefsRevision.svelte";
  import { session } from "../../lib/session.svelte";
  import { loadUserPrefs } from "../../lib/userPrefs";
  import SectionHeadLead from "../SectionHeadLead.svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import NebulaCanvas from "./NebulaCanvas.svelte";

  let { tracks }: { tracks: readonly Track[] } = $props();

  let hovered = $state(false);
  let focused = $state(false);
  const live = $derived(hovered || focused);

  const camera = defaultNebulaCamera(0.36);
  const favoriteRelPaths = $derived(new Set(session.favorites.map((f) => f.rel_path)));

  const model = $derived.by(() => {
    void prefsRevision.playCounts;
    void prefsRevision.moods;
    void session.moodPrefsTick;
    void chartBpmSource.revision;
    const prefs = loadUserPrefs();
    const counts = prefs.playCounts;
    return buildNebulaModel(
      nebulaInputsFromTracks(sampleTracksForNebulaBuild(tracks), prefs.trackMoods, chartBpm),
      { playCount: (rel) => counts[rel] ?? 0, favorites: favoriteRelPaths },
    );
  });

  const sortedStars = $derived(
    sampleNebulaStarsForPreview(model.stars, 300).sort((a, b) => a.radius - b.radius),
  );

  $effect(() => {
    void ensureChartBpmSource();
  });

  function openNebula() {
    session.selectedGenre = null;
    session.moodFilterIds = [];
    session.libraryLevel = "artists";
    session.libraryBrowse = "nebula";
    requestNebulaFullscreen();
    session.navigate("library");
  }
</script>

<Panel class="session-card dashboard-session-card dashboard-page__full dashboard-nebula">
  <header class="section-head section-head--page-toolbar">
    <SectionHeadLead eyebrow={t("nebula.dashboardEyebrow")} title={t("nebula.dashboardTitle")}>
      <UiIcon name="sparkle" />
    </SectionHeadLead>
  </header>
  <button
    type="button"
    class="dashboard-nebula-card"
    onclick={openNebula}
    onpointerenter={(e) => {
      if (e.pointerType === "mouse") hovered = true;
    }}
    onpointerleave={() => (hovered = false)}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    aria-label={t("nebula.openFull")}
    title={t("nebula.openFull")}
  >
    {#if model.stars.length}
      <NebulaCanvas
        fogs={model.fogs}
        {sortedStars}
        {camera}
        currentId={session.current?.rel_path ?? null}
        playing={session.playing}
        preview
        animated={live}
      />
    {:else}
      <span class="dashboard-nebula-card__empty">{t("nebula.empty")}</span>
    {/if}
  </button>
</Panel>

<style>
  .dashboard-nebula-card {
    position: relative;
    display: block;
    width: 100%;
    height: 260px;
    padding: 0;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-lg);
    overflow: hidden;
    background: #05060f;
    cursor: pointer;
    color: var(--rk-muted);
    font: inherit;
  }

  .dashboard-nebula-card:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--rk-accent-2) 70%, transparent);
    outline-offset: 2px;
  }

  .dashboard-nebula-card__empty {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 1rem;
    text-align: center;
  }
</style>
