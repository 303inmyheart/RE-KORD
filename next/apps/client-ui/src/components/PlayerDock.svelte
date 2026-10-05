<script lang="ts">
  import { onMount } from "svelte";
  import { CoverArt, IconButton } from "@rekord/ui";
  import { coverUrlFor, type Track } from "../lib/api";
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

  /** Legacy: click empty area of the top player row → Studio → Ascolta. */
  function openListenFromTopBar(event: MouseEvent) {
    if (isMobileLayout) return;
    const el = event.target as HTMLElement | null;
    if (!el) return;
    if (el.closest("button, input, a, .crumb, .progress2, .transport-wrap")) {
      return;
    }
    openStudioListen();
  }

  /** Su telefono le azioni secondarie stanno nel menu: eseguile e chiudilo. */
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

  /** Il menu si chiude toccando fuori, con Esc o quando la finestra cambia taglia. */
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

  /** Cambiando brano il menu resta aperto su dati vecchi: chiudilo. */
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
            <button type="button" class="title-hit" onclick={openStudioListen}>
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
              <span class="sep" aria-hidden="true">·</span>
              <button
                type="button"
                class="crumb crumb--album"
                title={current.album_name}
                onclick={onopenAlbum}
              >
                {current.album_name}
              </button>
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
        <!-- Telefono: solo play/pausa in chiaro, il resto nel menu; prev/next con swipe. -->
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
          <IconButton label={t("player.radio")} onclick={onradio}>
            <UiIcon name="radio" />
          </IconButton>
        </div>
      {/if}
    </div>
    <div class="row seek">
      <PlayerTimeline {currentTime} {duration} {onseek} />
    </div>
  </div>
</footer>

<style>
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
    justify-self: start;
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

  .byline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.15rem 0.35rem;
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

  /* Gli unici due comandi visibili sul telefono: target pieno da dito. */
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
    /* Voce alta come un target touch: il menu è la via mobile a queste azioni. */
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
      max-width: 72rem;
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
      /* Il dock si appoggia sopra la nav mobile, che porta già l'inset in basso:
         se lo aggiungesse anche lui resterebbe una fascia vuota fra i due. */
      bottom: calc(var(--rk-mobile-nav-h) + env(safe-area-inset-bottom, 0px));
      padding-bottom: 0;
    }

    .bar {
      padding: var(--rk-space-xs)
        max(var(--rk-space-md), env(safe-area-inset-right, 0px))
        var(--rk-space-3xs) max(var(--rk-space-md), env(safe-area-inset-left, 0px));
      gap: var(--rk-space-3xs);
    }

    /* Una riga sola: brano a sinistra, play e menu a destra. */
    .row.top {
      grid-template-columns: minmax(0, 1fr) auto;
      column-gap: var(--rk-space-sm);
      /* Lo swipe orizzontale è nostro, lo scorrimento verticale resta al browser. */
      touch-action: pan-y;
    }

    .identity {
      gap: var(--rk-space-sm);
      /* Riempie la colonna invece di dimensionarsi sul contenuto: solo così i
         nomi lunghi vengono troncati e non finiscono sotto play e menu. */
      justify-self: stretch;
    }

    /* Il tocco sulla riga apre Ascolta: la selezione del testo darebbe fastidio. */
    .meta {
      user-select: none;
    }

    /* Artista e album su una riga sola: se vanno a capo il dock cresce di 20px. */
    .byline {
      flex-wrap: nowrap;
      min-width: 0;
    }

    .crumb {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* L'album cede per primo (si accorcia 6 volte più in fretta), l'artista
       tiene almeno un terzo della riga: «Bring Me the Horizon · Album - Lo…»
       invece di «Bring Me the … · Album - Lo-…». */
    .byline {
      gap: 0 0.3rem;
    }

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
      min-width: 0;
    }
  }
</style>
