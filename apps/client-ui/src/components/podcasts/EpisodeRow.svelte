<script lang="ts">
  /**
   * One episode (or the LIVE item of a radio): the library track-row look,
   * with the date, the length and the listening state instead of plays.
   */
  import { CoverArt } from "@rekord/ui";
  import { apiUrl } from "../../lib/config";
  import { fmtRelative, t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import {
    formatDuration,
    minutesLeft,
    type PodcastEpisode,
    type PodcastSource,
  } from "../../lib/podcastModel";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import { session } from "../../lib/session.svelte";
  import GraphicEq from "../icons/GraphicEq.svelte";
  import UiIcon from "../icons/UiIcon.svelte";

  let {
    source,
    ep,
    showSource = false,
  }: {
    source: PodcastSource;
    ep: PodcastEpisode;
    /** Mixed lists (history): say which source it comes from. */
    showSource?: boolean;
  } = $props();

  const live = $derived(source.live || !!ep.live);
  const current = $derived.by(() => {
    void session.tick;
    return podcasts.isCurrent(source, ep);
  });
  const playingNow = $derived(current && session.playing);
  const prog = $derived(podcasts.progress(source, ep));
  const left = $derived(minutesLeft(podcasts.entry(source.id, ep.key), ep.durationSecs));
  const art = $derived(
    ep.hasArt || source.hasArt
      ? apiUrl(`/api/v1/podcasts/art/${source.id}/${encodeURIComponent(ep.key)}`)
      : null,
  );
  const when = $derived(ep.publishedAt ? fmtRelative(ep.publishedAt) : "");
  const length = $derived(formatDuration(ep.durationSecs));

  function play() {
    if (playingNow) {
      player.pause();
      return;
    }
    podcasts.play(source, ep);
  }
</script>

<li
  class="track-row podcast-row"
  class:is-active={current}
  class:is-listened={prog.listened && !current}
>
  <div class="track-row__art-wrap">
    <CoverArt kind="track" title={ep.title} src={art} size="md" />
    <button
      type="button"
      class={playingNow ? "track-row__art-studio" : "track-row__art-play"}
      title={playingNow ? t("player.playPause") : t("podcasts.play")}
      aria-label={playingNow ? t("player.playPause") : t("podcasts.play")}
      onclick={play}
    >
      {#if playingNow}
        <GraphicEq animated />
      {:else}
        <UiIcon name="play" />
      {/if}
    </button>
  </div>

  <button type="button" class="track-row__main" onclick={play}>
    <span class="track-row__title-row">
      <span class="track-row__title">{ep.title}</span>
      {#if live}
        <span class="podcast-row__live">LIVE</span>
      {:else if prog.listened}
        <span class="podcast-row__done" title={t("podcasts.listened")}>
          <UiIcon name="check" />
        </span>
      {/if}
    </span>
    <span class="podcast-row__meta">
      {#if showSource}
        <span class="podcast-row__src">{source.name}</span>
      {/if}
      {#if live}
        <span>{t("podcasts.liveHint")}</span>
      {:else}
        {#if when}<span>{when}</span>{/if}
        {#if length}<span class="rk-num">{length}</span>{/if}
        {#if left != null}
          <span class="podcast-row__left">{t("podcasts.minutesLeft", { n: left })}</span>
        {/if}
      {/if}
    </span>
    {#if !live && prog.ratio > 0 && !prog.listened}
      <span class="podcast-row__bar" aria-hidden="true">
        <span class="podcast-row__fill" style:transform={`scaleX(${prog.ratio})`}></span>
      </span>
    {/if}
  </button>

  <div class="track-row__actions">
    {#if !live}
      <button
        type="button"
        class="track-row__ic"
        title={t("podcasts.addQueue")}
        aria-label={t("podcasts.addQueue")}
        onclick={() => podcasts.play(source, ep, { queue: true })}
      >
        <span class="track-row__ic-glyph track-row__ic-glyph--svg" aria-hidden="true">
          <UiIcon name="add" />
        </span>
      </button>
      <button
        type="button"
        class="track-row__ic podcast-row__mark"
        class:is-on={prog.listened}
        aria-pressed={prog.listened}
        title={prog.listened ? t("podcasts.markUnlistened") : t("podcasts.markListened")}
        aria-label={prog.listened ? t("podcasts.markUnlistened") : t("podcasts.markListened")}
        onclick={() => podcasts.setListened(source, ep, !prog.listened)}
      >
        <span class="track-row__ic-glyph track-row__ic-glyph--svg" aria-hidden="true">
          <UiIcon name="check" />
        </span>
      </button>
    {/if}
  </div>
</li>

<style>
  .podcast-row.is-listened .track-row__title {
    color: var(--rk-muted);
    font-weight: 600;
  }

  .podcast-row__meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--rk-space-sm);
    min-width: 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    font-variant-numeric: tabular-nums;
  }

  .podcast-row__meta > span + span::before {
    content: "·";
    margin-right: var(--rk-space-sm);
    opacity: 0.8;
  }

  .podcast-row__src {
    font-weight: 650;
    color: color-mix(in srgb, var(--rk-ink) 55%, var(--rk-muted));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 14rem;
  }

  .podcast-row__left {
    color: var(--rk-accent);
    font-weight: 650;
  }

  .podcast-row__live {
    flex: 0 0 auto;
    font-size: var(--rk-fs-1);
    font-weight: 800;
    letter-spacing: 0.06em;
    color: var(--rk-danger, #e5484d);
  }

  .podcast-row__done {
    display: inline-flex;
    color: var(--rk-success, #2fa66a);
  }

  .podcast-row__done :global(.ui-ic) {
    width: 1rem;
    height: 1rem;
  }

  .podcast-row__bar {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--rk-ink) 12%, transparent);
    overflow: hidden;
    margin-top: 0.15rem;
    max-width: 18rem;
  }

  .podcast-row__fill {
    display: block;
    height: 100%;
    background: var(--rk-accent);
    transform-origin: left center;
  }

  .podcast-row__mark.is-on {
    color: var(--rk-success, #2fa66a);
  }
</style>
