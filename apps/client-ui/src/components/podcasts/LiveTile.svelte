<script lang="ts">
  /** A live radio station: artwork, LIVE pill and one big play button. */
  import { CoverArt } from "@rekord/ui";
  import { apiUrl } from "../../lib/config";
  import { t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import { LIVE_EPISODE_KEY, type PodcastSource } from "../../lib/podcastModel";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import { session } from "../../lib/session.svelte";
  import UiIcon from "../icons/UiIcon.svelte";

  let { source, large = false }: { source: PodcastSource; large?: boolean } = $props();

  const ep = $derived(
    source.episodes[0] ?? { key: LIVE_EPISODE_KEY, title: source.name, live: true, hasArt: source.hasArt },
  );
  const art = $derived(source.hasArt ? apiUrl(`/api/v1/podcasts/art/${source.id}/_`) : null);
  const current = $derived.by(() => {
    void session.tick;
    return podcasts.isCurrent(source, ep);
  });
  const playingNow = $derived(current && session.playing);

  function toggle() {
    if (playingNow) player.pause();
    else podcasts.play(source, ep);
  }
</script>

<div class="live-tile" class:live-tile--large={large} class:is-active={current}>
  <div class="live-tile__media">
    {#if art}
      <CoverArt kind="album" class="live-tile__art" title={source.name} src={art} rounded={false} />
    {:else}
      <span class="live-tile__fallback" aria-hidden="true"><UiIcon name="radio" /></span>
    {/if}
    <span class="live-tile__pill"><span class="live-tile__dot" aria-hidden="true"></span>LIVE</span>
    <button
      type="button"
      class="live-tile__play"
      title={playingNow ? t("player.playPause") : t("podcasts.listenLive")}
      aria-label={playingNow ? t("player.playPause") : t("podcasts.listenLive")}
      onclick={toggle}
    >
      <UiIcon name={playingNow ? "pause" : "play"} />
    </button>
  </div>
  <button type="button" class="live-tile__meta" onclick={toggle}>
    <span class="live-tile__name" title={source.name}>{source.name}</span>
    <span class="live-tile__sub">{current ? t("podcasts.liveNow") : t("podcasts.kind.live")}</span>
  </button>
</div>

<style>
  .live-tile {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    min-width: 0;
  }

  .live-tile__media {
    position: relative;
    aspect-ratio: 1;
    border-radius: var(--rk-radius-lg);
    overflow: hidden;
    isolation: isolate;
    background: linear-gradient(
      145deg,
      color-mix(in srgb, var(--rk-accent-2) 22%, var(--rk-surface-3)),
      color-mix(in srgb, var(--rk-accent) 16%, var(--rk-surface-2))
    );
  }

  .live-tile.is-active .live-tile__media {
    box-shadow: 0 0 0 2px var(--rk-accent);
  }

  .live-tile__media :global(.rk-cover.live-tile__art) {
    width: 100%;
    height: 100%;
    border: 0;
  }

  .live-tile__fallback {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--rk-ink);
    opacity: 0.55;
  }

  .live-tile__fallback :global(.ui-ic) {
    width: 34%;
    height: 34%;
  }

  .live-tile__pill {
    position: absolute;
    top: 0.5rem;
    left: 0.5rem;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--rk-radius-sm, 6px);
    background: var(--rk-danger-solid, #d93a3f);
    color: #fff;
    font-size: var(--rk-fs-1, 0.68rem);
    font-weight: 800;
    letter-spacing: 0.06em;
  }

  .live-tile__dot {
    width: 0.4rem;
    height: 0.4rem;
    border-radius: 50%;
    background: currentColor;
  }

  .live-tile__play {
    position: absolute;
    right: 0.6rem;
    bottom: 0.6rem;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 3rem;
    height: 3rem;
    border-radius: 50%;
    border: none;
    padding: 0;
    color: var(--rk-accent-ink, #fff);
    background: var(--rk-cta, var(--rk-accent));
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
    cursor: pointer;
  }

  .live-tile--large .live-tile__play {
    width: 3.75rem;
    height: 3.75rem;
  }

  .live-tile__play :global(.ui-ic) {
    width: 45%;
    height: 45%;
  }

  .live-tile__meta {
    display: grid;
    gap: 0.1rem;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .live-tile__name {
    font-weight: 700;
    font-size: var(--rk-fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .live-tile__sub {
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }
</style>
