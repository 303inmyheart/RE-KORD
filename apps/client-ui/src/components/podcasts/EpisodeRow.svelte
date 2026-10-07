<script lang="ts">
  /**
   * One episode, in the library track-row look (cover with play on hover,
   * active highlight while it plays): date, length, time left and a thin
   * progress bar instead of plays. Queue and "listened" are always-visible
   * square actions, dimmed while off.
   */
  import { CoverArt } from "@rekord/ui";
  import { apiUrl } from "../../lib/config";
  import { fmtRelative, t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import {
    formatDuration,
    isNewEpisode,
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
    newest = false,
  }: {
    source: PodcastSource;
    ep: PodcastEpisode;
    /** Mixed lists ("Ultimi", history): say which source it comes from. */
    showSource?: boolean;
    /** Newest episode of its source: a "Nuovo" badge when it is fresh. */
    newest?: boolean;
  } = $props();

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
  const fresh = $derived(newest && isNewEpisode(ep) && !prog.listened);

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
      class={current ? "track-row__art-studio" : "track-row__art-play"}
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

  <button type="button" class="track-row__main podcast-row__main" onclick={play}>
    <span class="podcast-row__title-line">
      <span class="track-row__title">{ep.title}</span>
      {#if fresh}<span class="podcast-row__new">{t("podcasts.new")}</span>{/if}
    </span>
    <span class="podcast-row__meta">
      {#if showSource}<span class="podcast-row__src">{source.name}</span>{/if}
      {#if when}<span>{when}</span>{/if}
      {#if length}<span class="rk-num">{length}</span>{/if}
      {#if left != null}
        <span class="podcast-row__left">{t("podcasts.minutesLeft", { n: left })}</span>
      {/if}
    </span>
    {#if prog.ratio > 0 && !prog.listened}
      <span class="podcast-row__bar" aria-hidden="true">
        <span class="podcast-row__fill" style:transform={`scaleX(${prog.ratio})`}></span>
      </span>
    {/if}
  </button>

  <div class="podcast-row__actions">
    <button
      type="button"
      class="podcast-act"
      title={t("podcasts.addQueue")}
      aria-label={t("podcasts.addQueue")}
      onclick={() => podcasts.play(source, ep, { queue: true })}
    >
      <UiIcon name="add" />
    </button>
    <button
      type="button"
      class="podcast-act podcast-act--done"
      class:is-on={prog.listened}
      aria-pressed={prog.listened}
      title={prog.listened ? t("podcasts.markUnlistened") : t("podcasts.markListened")}
      aria-label={prog.listened ? t("podcasts.markUnlistened") : t("podcasts.markListened")}
      onclick={() => podcasts.setListened(source, ep, !prog.listened)}
    >
      <UiIcon name="check" />
    </button>
  </div>
</li>

<style>
  .podcast-row {
    min-height: 3.6rem;
  }

  .podcast-row__main {
    gap: 0.18rem;
  }

  .podcast-row__title-line {
    display: flex;
    align-items: center;
    gap: var(--rk-space-sm);
    min-width: 0;
    width: 100%;
  }

  .podcast-row__title-line .track-row__title {
    min-width: 0;
    flex: 0 1 auto;
  }

  .podcast-row.is-listened .track-row__title {
    color: var(--rk-muted);
    font-weight: 600;
  }

  .podcast-row__new {
    flex: 0 0 auto;
    padding: 0.08rem 0.42rem;
    border-radius: var(--rk-radius-sm, 6px);
    font-size: var(--rk-fs-1, 0.68rem);
    font-weight: 800;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--rk-accent);
    background: color-mix(in srgb, var(--rk-accent) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--rk-accent) 40%, transparent);
  }

  .podcast-row__meta {
    width: 100%;
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
    color: color-mix(in srgb, var(--rk-ink) 60%, var(--rk-muted));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 16rem;
  }

  .podcast-row__left {
    color: var(--rk-accent);
    font-weight: 650;
  }

  .podcast-row__bar {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--rk-ink) 12%, transparent);
    overflow: hidden;
    margin-top: 0.12rem;
    width: min(100%, 16rem);
  }

  .podcast-row__fill {
    display: block;
    height: 100%;
    background: var(--rk-accent);
    transform-origin: left center;
  }

  .podcast-row__actions {
    display: flex;
    align-items: center;
    gap: var(--rk-space-xs, 0.35rem);
  }

  /* Square actions (legacy chips): always visible, dimmed while off. */
  .podcast-act {
    display: inline-grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    padding: 0;
    border-radius: var(--rk-radius, 8px);
    border: 1px solid var(--rk-line);
    background: color-mix(in srgb, var(--rk-surface-3) 55%, transparent);
    color: var(--rk-muted);
    cursor: pointer;
    transition:
      color 0.12s ease,
      border-color 0.12s ease,
      background 0.12s ease;
  }

  .podcast-act :global(.ui-ic) {
    width: 1rem;
    height: 1rem;
  }

  .podcast-act:hover {
    color: var(--rk-ink);
    border-color: color-mix(in srgb, var(--rk-accent) 40%, var(--rk-line));
  }

  .podcast-act--done.is-on {
    color: var(--rk-success, #2fa66a);
    border-color: color-mix(in srgb, var(--rk-success, #2fa66a) 45%, var(--rk-line));
    background: color-mix(in srgb, var(--rk-success, #2fa66a) 12%, transparent);
  }

  /* Narrow lists: the source on its own line, so no "·" starts a line. */
  @container track-list (max-width: 650.98px) {
    .podcast-row__src {
      flex-basis: 100%;
      max-width: 100%;
    }

    .podcast-row__src + span::before {
      display: none;
    }

    .podcast-row__title-line {
      align-items: flex-start;
    }

    .podcast-row__title-line .track-row__title {
      white-space: normal;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow-wrap: anywhere;
      line-height: 1.25;
    }

    .podcast-row__new {
      margin-top: 0.1rem;
    }
  }

  @media (pointer: coarse) {
    .podcast-act {
      width: 2.5rem;
      height: 2.5rem;
    }
  }
</style>
