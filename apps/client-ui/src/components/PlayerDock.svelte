<script lang="ts">
  import { onMount } from "svelte";
  import { CoverArt, IconButton } from "@rekord/ui";
  import { coverUrlFor, type Track } from "../lib/api";
  import { isExternalTrack, isLiveTrack } from "../lib/externalItems";
  import { watchDown } from "../lib/breakpoints";
  import { castController } from "../lib/cast/castController.svelte";
  import CastButton from "./cast/CastButton.svelte";
  import CastIcon from "./cast/CastIcon.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { RepeatMode } from "../lib/player";
  import { playerSwipe } from "../lib/playerSwipe";
  import UiIcon from "./icons/UiIcon.svelte";
  import PlayerTimeline from "./PlayerTimeline.svelte";
  import PlayerTransport from "./PlayerTransport.svelte";
  import SleepTimerControl from "./SleepTimerControl.svelte";

  let {
    current = null,
    playing = false,
    currentTime = 0,
    duration = 0,
    shuffle = false,
    repeat = "off" as RepeatMode,
    favorited = false,
    excluded = false,
    excludeLocked = false,
    ontoggle,
    onprev,
    onnext,
    onseek,
    ontoggleShuffle,
    oncycleRepeat,
    ontoggleFavorite,
    ontoggleExclude,
    onradio,
    onopenAlbum,
    onopenArtist,
    onopenStudio,
  }: {
    current?: Track | null;
    playing?: boolean;
    currentTime?: number;
    duration?: number;
    shuffle?: boolean;
    repeat?: RepeatMode;
    favorited?: boolean;
    excluded?: boolean;
    excludeLocked?: boolean;
    ontoggle: () => void;
    onprev: () => void;
    onnext: () => void;
    onseek: (seconds: number) => void;
    ontoggleShuffle: () => void;
    oncycleRepeat: () => void;
    ontoggleFavorite: () => void;
    ontoggleExclude: () => void;
    onradio: () => void;
    onopenAlbum: () => void;
    onopenArtist: () => void;
    onopenStudio?: () => void;
  } = $props();

  /** Podcast episode / live stream: no favourite, exclude or radio; crumbs open Podcasts. */
  const external = $derived(isExternalTrack(current));
  const live = $derived(isLiveTrack(current));

  /**
   * While casting the player itself mirrors the receiver (play state, position)
   * and routes transport to it (`player.setRemoteOutput`), so the dock needs no
   * special casing beyond the Cast button and the device badge.
   */
  const casting = $derived(castController.connected);
  const castDeviceName = $derived(castController.status.deviceName ?? t("cast.device"));

  onMount(() => {
    // The Cast SDK is fetched lazily: keep it off the startup path.
    const id = window.setTimeout(() => castController.init(), 1500);
    return () => window.clearTimeout(id);
  });

  let dockEl: HTMLElement | undefined = $state();
  let menuWrapEl: HTMLDivElement | null = $state(null);
  let isMobileLayout = $state(false);
  let menuOpen = $state(false);

  function openStudioListen() {
    if (onopenStudio) onopenStudio();
    else onopenAlbum();
  }

  /** Legacy: click empty area of the top player row → Studio → Listen. */
  function openListenFromTopBar(event: MouseEvent) {
    if (isMobileLayout) return;
    const el = event.target as HTMLElement | null;
    if (!el) return;
    if (el.closest("button, input, a, .crumb, .progress2, .transport-wrap")) {
      return;
    }
    openStudioListen();
  }

  /** On the phone the secondary actions live in the menu: run them and close it. */
  function runFromMenu(fn: () => void) {
    menuOpen = false;
    fn();
  }

  $effect(() =>
    watchDown("xxl", (matches) => {
      isMobileLayout = matches;
      if (!matches) menuOpen = false;
    }),
  );

  /** The menu closes on an outside tap, on Esc or when the window changes size. */
  $effect(() => {
    if (!menuOpen) return;

    const onPointerDown = (e: PointerEvent) => {
      if (e.target instanceof Node && menuWrapEl?.contains(e.target)) return;
      menuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") menuOpen = false;
    };
    const close = () => {
      menuOpen = false;
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
    };
  });

  /** On a track change the menu would stay open on stale data: close it. */
  $effect(() => {
    void current?.id;
    menuOpen = false;
  });

  /* --rk-dock-h = the whole footer (floating card + the gap under it), so the
     page can clear it with one variable. */
  $effect(() => {
    const dock = dockEl;
    if (!dock) return;

    const root = document.documentElement;
    const apply = () => {
      const h = Math.max(0, Math.ceil(dock.getBoundingClientRect().height));
      root.style.setProperty("--rk-dock-h", `${h}px`);
    };

    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(dock);

    return () => {
      ro.disconnect();
      root.style.removeProperty("--rk-dock-h");
    };
  });
</script>

<footer class="dock player-dock" bind:this={dockEl}>
  <div class="bar player-bar">
    <div
      class="row top"
      class:open-listen={!isMobileLayout}
      onclick={openListenFromTopBar}
      title={isMobileLayout ? undefined : t("player.openListen")}
      role={isMobileLayout ? undefined : "link"}
      use:playerSwipe={{
        enabled: isMobileLayout,
        onprev,
        onnext,
        ontap: openStudioListen,
        ignoreSelector: ".transport-wrap, .mobile-transport",
        tapIgnoreSelector: ".crumb",
      }}
    >
      <div class="identity">
        <button
          type="button"
          class="art-hit"
          onclick={openStudioListen}
          title={t("player.openListen")}
        >
          <CoverArt
            kind="track"
            title={current?.title ?? ""}
            src={coverUrlFor(current, 128)}
            size={isMobileLayout ? "md" : "dock"}
          />
        </button>
        <div class="meta">
          {#if current}
            <button
              type="button"
              class="title-hit"
              title={current.title}
              onclick={openStudioListen}
            >
              <strong>{current.title}</strong>
            </button>
            <div class="byline">
              <button
                type="button"
                class="crumb crumb--artist"
                title={current.artist_name}
                onclick={onopenArtist}
              >
                {current.artist_name}
              </button>
              {#if live}
                <span class="sep" aria-hidden="true">·</span>
                <span class="live-pill">LIVE</span>
              {:else if !external}
                <span class="sep" aria-hidden="true">·</span>
                <button
                  type="button"
                  class="crumb crumb--album"
                  title={current.album_name}
                  onclick={onopenAlbum}
                >
                  {current.album_name}
                </button>
              {/if}
              {#if casting}
                <span class="sep">·</span>
                <span class="cast-badge" title={t("cast.castingTo", { name: castDeviceName })}>
                  <CastIcon connected />
                  <span class="cast-badge__name">{castDeviceName}</span>
                </span>
              {/if}
            </div>
          {:else}
            <strong>{t("player.idleTitle")}</strong>
            <div class="byline idle">{t("player.idleHint")}</div>
          {/if}
        </div>
      </div>

      {#if isMobileLayout}
        <!-- Phone: only play/pause in plain view, the rest in the menu; prev/next by swipe. -->
        <div class="mobile-transport">
          <div class="menu-wrap" bind:this={menuWrapEl}>
            <IconButton
              label={t("player.moreActions")}
              active={menuOpen}
              onclick={() => {
                menuOpen = !menuOpen;
              }}
            >
              <UiIcon name="more" />
            </IconButton>
            {#if menuOpen}
              <ul class="dock-menu" role="menu">
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitem"
                    class="dock-menu__item"
                    onclick={() => runFromMenu(onprev)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="prev" /></span>
                    <span class="dock-menu__label">{t("player.prev")}</span>
                  </button>
                </li>
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitem"
                    class="dock-menu__item"
                    onclick={() => runFromMenu(onnext)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="next" /></span>
                    <span class="dock-menu__label">{t("player.next")}</span>
                  </button>
                </li>
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitemcheckbox"
                    class="dock-menu__item"
                    class:is-on={shuffle}
                    aria-checked={shuffle}
                    onclick={() => runFromMenu(ontoggleShuffle)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="shuffle" /></span>
                    <span class="dock-menu__label">{t("player.shuffle")}</span>
                  </button>
                </li>
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitemcheckbox"
                    class="dock-menu__item"
                    class:is-on={repeat !== "off"}
                    aria-checked={repeat !== "off"}
                    onclick={() => runFromMenu(oncycleRepeat)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="repeat" /></span>
                    <span class="dock-menu__label">
                      {t("player.repeat")}{repeat === "one" ? " 1" : ""}
                    </span>
                  </button>
                </li>
                {#if !external}
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitemcheckbox"
                    class="dock-menu__item"
                    class:is-on={favorited}
                    aria-checked={favorited}
                    onclick={() => runFromMenu(ontoggleFavorite)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="favorite" /></span>
                    <span class="dock-menu__label">{t("player.favorite")}</span>
                  </button>
                </li>
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitemcheckbox"
                    class="dock-menu__item"
                    class:is-on={excluded || excludeLocked}
                    aria-checked={excluded || excludeLocked}
                    disabled={excludeLocked}
                    onclick={() => runFromMenu(ontoggleExclude)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="exclude" /></span>
                    <span class="dock-menu__label">
                      {excludeLocked
                        ? t("player.excludeLocked")
                        : excluded
                          ? t("player.excludeOn")
                          : t("player.excludeOff")}
                    </span>
                  </button>
                </li>
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitem"
                    class="dock-menu__item"
                    onclick={() => runFromMenu(onradio)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="radio" /></span>
                    <span class="dock-menu__label">{t("player.radio")}</span>
                  </button>
                </li>
                {/if}
                {#if castController.available}
                  <li role="presentation">
                    <button
                      type="button"
                      role="menuitemcheckbox"
                      class="dock-menu__item"
                      class:is-on={casting}
                      aria-checked={casting}
                      onclick={() =>
                        runFromMenu(() =>
                          casting ? void castController.disconnect() : void castController.connect(),
                        )}
                    >
                      <span class="dock-menu__glyph"><CastIcon connected={casting} /></span>
                      <span class="dock-menu__label">
                        {casting ? t("cast.stopOn", { name: castDeviceName }) : t("cast.start")}
                      </span>
                    </button>
                  </li>
                {/if}
                <li role="presentation">
                  <button
                    type="button"
                    role="menuitem"
                    class="dock-menu__item"
                    onclick={() => runFromMenu(openStudioListen)}
                  >
                    <span class="dock-menu__glyph"><UiIcon name="headphones" /></span>
                    <span class="dock-menu__label">{t("player.openListen")}</span>
                  </button>
                </li>
              </ul>
            {/if}
          </div>
          <IconButton label={t("player.playPause")} emphasis onclick={ontoggle}>
            <UiIcon name={playing ? "pause" : "play"} />
          </IconButton>
        </div>
      {:else}
        <div class="transport-wrap">
          <PlayerTransport
            {playing}
            {shuffle}
            {repeat}
            {favorited}
            {excluded}
            {excludeLocked}
            libraryActions={!external}
            {ontoggle}
            {onprev}
            {onnext}
            {ontoggleShuffle}
            {oncycleRepeat}
            {ontoggleFavorite}
            {ontoggleExclude}
          />
        </div>

        <div class="rail-end">
          <CastButton />
          <SleepTimerControl />
          <IconButton label={t("player.radio")} disabled={external} onclick={onradio}>
            <UiIcon name="radio" />
          </IconButton>
        </div>
      {/if}
    </div>
    <div class="row seek">
      <PlayerTimeline {currentTime} {duration} {live} {onseek} />
    </div>
  </div>
</footer>

<style>
  .live-pill {
    font-size: var(--rk-fs-xs, 0.72rem);
    font-weight: 800;
    letter-spacing: 0.06em;
    color: var(--rk-danger, #e5484d);
  }

  .dock {
    position: fixed;
    left: var(--rk-rail-w);
    right: 0;
    bottom: 0;
    /* Above fullscreen Listen viz (z-index 19) — always interactable. */
    z-index: var(--rk-z-player);
    padding-bottom: env(safe-area-inset-bottom, 0px);
  }

  .bar {
    position: relative;
    isolation: isolate;
    /* Opaque when glass is off; glass-surfaces.css makes .player-bar transparent + blur. */
    background:
      linear-gradient(var(--rk-surface-2), var(--rk-surface-2)),
      var(--rk-bg);
    border-top: 1px solid var(--rk-line);
    padding: var(--rk-space-md)
      max(var(--rk-space-lg), env(safe-area-inset-right, 0px))
      var(--rk-space-xs) max(var(--rk-space-lg), env(safe-area-inset-left, 0px));
    display: flex;
    flex-direction: column;
    gap: var(--rk-space-xs);
  }

  .row.top {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    column-gap: 0.75rem;
    width: 100%;
    min-width: 0;
  }

  .row.top.open-listen {
    cursor: pointer;
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    min-width: 0;
    /* Fills its column instead of sizing to its content: long titles and
       artists are cut with "…" (full text in the tooltip) and never widen
       the column or push the transport off centre. */
    justify-self: stretch;
  }

  .art-hit {
    border: 0;
    padding: 0;
    background: transparent;
    cursor: pointer;
    border-radius: var(--rk-radius);
    line-height: 0;
  }

  .meta {
    display: grid;
    gap: 0.22rem;
    min-width: 0;
  }

  .title-hit {
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
    min-width: 0;
  }

  .meta strong,
  .title-hit strong {
    font-weight: 700;
    font-size: var(--rk-fs-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .cast-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
    color: var(--rk-accent-2);
  }

  .cast-badge :global(.cast-ic) {
    width: 0.95rem;
    height: 0.95rem;
  }

  .cast-badge__name {
    max-width: 12rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Artist and album on one line, cut with "…": wrapping made the bar grow. */
  .byline {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    min-width: 0;
    gap: 0 0.35rem;
    font-size: var(--rk-fs-2);
    font-weight: 600;
    color: color-mix(in srgb, var(--rk-ink) 72%, var(--rk-muted) 28%);
  }

  .byline.idle {
    font-weight: 500;
    color: var(--rk-muted);
  }

  .crumb {
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    font: inherit;
    font-weight: 600;
    color: color-mix(in srgb, var(--rk-accent-2) 82%, var(--rk-ink) 18%);
  }

  .crumb {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* Buttons centre their text: a short album name inside its 3rem
       minimum would float away from the separator. */
    text-align: start;
  }

  /* The album gives way first (it shrinks 6 times faster), the artist keeps
     the larger share. */
  .crumb--artist {
    flex: 0 1 auto;
  }

  .crumb--album {
    flex: 0 6 auto;
    min-width: 3rem;
  }

  .sep {
    flex: 0 0 auto;
  }

  .cast-badge {
    flex: 0 1 auto;
  }

  .crumb:hover {
    color: var(--rk-accent-2);
    text-decoration: underline;
    text-underline-offset: 0.18em;
  }

  .sep {
    color: var(--rk-muted);
  }

  .transport-wrap {
    justify-self: center;
    grid-column: 2;
  }

  .rail-end {
    display: none;
  }

  .mobile-transport {
    grid-column: 2;
    justify-self: end;
    display: inline-flex;
    align-items: center;
    gap: var(--rk-space-2xs);
  }

  /* The only two commands visible on the phone: full finger-sized target. */
  .mobile-transport :global(.rk-icon) {
    width: 2.75rem;
    height: 2.75rem;
  }

  .menu-wrap {
    position: relative;
  }

  .dock-menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + var(--rk-space-2xs));
    z-index: var(--rk-z-popover);
    min-width: 12rem;
    padding: var(--rk-space-2xs);
    margin: 0;
    list-style: none;
    border-radius: var(--rk-radius);
    border: 1px solid var(--rk-line);
    background: var(--rk-surface-3);
    box-shadow: var(--rk-shadow-2);
  }

  .dock-menu li {
    margin: 0;
  }

  .dock-menu__item {
    display: flex;
    align-items: center;
    gap: var(--rk-space-md);
    width: 100%;
    /* Item as tall as a touch target: the menu is the mobile path to these actions. */
    min-height: 2.75rem;
    padding: var(--rk-space-sm) var(--rk-space-md);
    border: none;
    border-radius: var(--rk-radius);
    background: transparent;
    color: var(--rk-ink);
    font: inherit;
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh-snug);
    text-align: left;
    cursor: pointer;
  }

  .dock-menu__item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .dock-menu__glyph {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    color: var(--rk-muted);
    --ui-ic-size: 1.05rem;
  }

  .dock-menu__label {
    flex: 1;
    min-width: 0;
  }

  .dock-menu__item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
  }

  .dock-menu__item:hover:not(:disabled) .dock-menu__glyph,
  .dock-menu__item.is-on:not(:disabled) .dock-menu__glyph {
    color: inherit;
  }

  .dock-menu__item.is-on:not(:disabled) .dock-menu__glyph {
    color: var(--rk-accent-2);
  }

  /* Desktop: a floating card above the page, like 5.x — margin all round,
     xl radius, its own border and shadow. The footer around it lets clicks
     through so the strip beside the card stays part of the page. */
  @media (min-width: 1000px) {
    .dock {
      padding: 0 var(--rk-space-xl) calc(var(--rk-space-lg) + env(safe-area-inset-bottom, 0px));
      pointer-events: none;
    }

    .bar {
      pointer-events: auto;
      /* Settings › Interface, per device ("default" keeps 72rem). */
      max-width: var(--rk-user-dock-max, 72rem);
      margin-inline: auto;
      border: 1px solid var(--rk-line-strong);
      border-radius: var(--rk-radius-sheet);
      padding: var(--rk-space-md) var(--rk-space-xl) var(--rk-space-xs);
      box-shadow:
        0 14px 36px color-mix(in srgb, black 42%, transparent),
        0 0 0 1px color-mix(in srgb, var(--rk-bg) 40%, transparent);
    }

    /* WebKitGTK repaints the strip under a fixed element on every scrolled
       frame: a short shadow costs a fraction of the wide one. */
    :global(:root[data-rk-lowfx]) .bar {
      box-shadow: 0 4px 14px color-mix(in srgb, black 38%, transparent);
    }

    .rail-end {
      display: inline-flex;
      grid-column: 3;
      justify-self: end;
      align-items: center;
      gap: 0.42rem;
    }
  }

  @media (max-width: 999.98px) {
    .dock {
      left: 0;
      /* The dock rests on top of the mobile nav, which already carries the bottom inset:
         if it added it too, an empty strip would remain between the two. */
      bottom: calc(var(--rk-mobile-nav-h) + env(safe-area-inset-bottom, 0px));
      padding-bottom: 0;
    }

    .bar {
      padding: var(--rk-space-xs)
        max(var(--rk-space-md), env(safe-area-inset-right, 0px))
        var(--rk-space-3xs) max(var(--rk-space-md), env(safe-area-inset-left, 0px));
      gap: var(--rk-space-3xs);
    }

    /* A single row: track on the left, play and menu on the right. */
    .row.top {
      grid-template-columns: minmax(0, 1fr) auto;
      column-gap: var(--rk-space-sm);
      /* The horizontal swipe is ours, vertical scrolling stays with the browser. */
      touch-action: pan-y;
    }

    .identity {
      gap: var(--rk-space-sm);
    }

    /* A tap on the row opens Listen: text selection would get in the way. */
    .meta {
      user-select: none;
    }

    .byline {
      gap: 0 0.3rem;
    }
  }
</style>
