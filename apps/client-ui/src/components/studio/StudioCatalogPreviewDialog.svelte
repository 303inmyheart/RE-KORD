<script lang="ts">
  import { Modal } from "@rekord/ui";
  import { api, type CatalogWebItem, type CatalogWebTrack } from "../../lib/api";
  import { t } from "../../lib/i18n.svelte";
  import { player } from "../../lib/player";
  import { session } from "../../lib/session.svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import { structuredError } from "../../lib/api/studio";
  import { studioCodeText, studioErrorCode, studioErrorText } from "../../lib/studio/errors";

  /** A failed audition: known causes get their message, anything else stays generic. */
  function previewFailure(e: unknown): string {
    if (e instanceof DOMException) return studioErrorText(e);
    const code = studioErrorCode(e);
    if (code) return studioErrorText(e);
    return t("studio.preview.unavailable");
  }

  let {
    item,
    onclose,
    onDownload,
  }: {
    item: CatalogWebItem | null;
    onclose: () => void;
    onDownload: (item: CatalogWebItem, mode: "single" | "playlist") => void;
  } = $props();

  /** Auditions stop here, like the legacy catalog preview. */
  const PREVIEW_MAX_SEC = 30;
  /** Volume ramp at the end, so the cut is not abrupt. */
  const FADE_SEC = 1.2;
  const TICK_MS = 60;

  let audioEl: HTMLAudioElement | null = $state(null);
  let tracks = $state<CatalogWebTrack[]>([]);
  let listTitle = $state<string | null>(null);
  let listBusy = $state(false);
  let listErr = $state<string | null>(null);
  let previewErr = $state<string | null>(null);
  let busyUrl = $state<string | null>(null);
  let playingUrl = $state<string | null>(null);
  let elapsed = $state(0);

  /** Bumped on every start/stop so late responses cannot revive a stale preview. */
  let previewSession = 0;
  let ticker: ReturnType<typeof setInterval> | null = null;
  let loadedFor: string | null = null;

  const progress = $derived(Math.min(1, elapsed / PREVIEW_MAX_SEC));

  function clearTicker() {
    if (ticker) clearInterval(ticker);
    ticker = null;
  }

  /** Silences the load error that follows detaching a source we no longer want. */
  let detaching = false;

  function pausePreview() {
    previewSession += 1;
    clearTicker();
    const audio = audioEl;
    if (audio) {
      audio.pause();
      audio.volume = 1;
    }
    playingUrl = null;
    busyUrl = null;
    elapsed = 0;
  }

  function stopPreview() {
    pausePreview();
    const audio = audioEl;
    if (!audio || !audio.getAttribute("src")) return;
    // Detaching frees the connection; `load()` resets the element so the next
    // audition starts from a clean state.
    detaching = true;
    audio.removeAttribute("src");
    audio.load();
  }

  function onTick() {
    const audio = audioEl;
    if (!audio) return;
    elapsed = audio.currentTime;
    const remaining = PREVIEW_MAX_SEC - elapsed;
    if (remaining <= 0) {
      stopPreview();
      return;
    }
    audio.volume = remaining < FADE_SEC ? Math.max(0, remaining / FADE_SEC) : 1;
  }

  async function playPreview(track: CatalogWebTrack) {
    if (playingUrl === track.url) {
      stopPreview();
      return;
    }
    // Only pause here: assigning a new src replaces the old resource, while
    // detaching it first would make this play() fail.
    pausePreview();
    const mySession = previewSession;
    previewErr = null;
    busyUrl = track.url;
    // Two sources at once is never wanted: the library player yields.
    if (session.playing) player.pause();
    try {
      const src = await api.catalogWebPreviewSrc(track.url);
      const audio = audioEl;
      if (mySession !== previewSession || !audio) return;
      detaching = false;
      audio.volume = 1;
      audio.src = src;
      await audio.play();
      if (mySession !== previewSession) return;
      busyUrl = null;
      playingUrl = track.url;
      elapsed = 0;
      clearTicker();
      ticker = setInterval(onTick, TICK_MS);
    } catch (e) {
      if (mySession !== previewSession) return;
      busyUrl = null;
      playingUrl = null;
      // A superseded play() aborts on purpose; only real failures are shown.
      if (e instanceof DOMException && e.name === "AbortError") return;
      previewErr = previewFailure(e);
    }
  }

  function onAudioError() {
    if (detaching) {
      detaching = false;
      return;
    }
    clearTicker();
    playingUrl = null;
    busyUrl = null;
    previewErr = t("studio.preview.unavailable");
  }

  async function loadTracks(target: CatalogWebItem) {
    listBusy = true;
    listErr = null;
    tracks = [];
    listTitle = null;
    try {
      const res = await api.catalogWebTracks(target.url);
      if (loadedFor !== target.url) return;
      tracks = res.tracks;
      listTitle = res.title ?? null;
      const err = structuredError(res.error);
      listErr = res.tracks.length
        ? null
        : err
          ? studioCodeText(err.code, err.message)
          : t("studio.preview.noTracks");
    } catch (e) {
      if (loadedFor !== target.url) return;
      listErr = studioErrorText(e);
    } finally {
      if (loadedFor === target.url) listBusy = false;
    }
  }

  $effect(() => {
    const target = item;
    if (!target) {
      if (loadedFor !== null) {
        loadedFor = null;
        stopPreview();
        tracks = [];
        listErr = null;
        previewErr = null;
      }
      return;
    }
    if (loadedFor === target.url) return;
    loadedFor = target.url;
    stopPreview();
    void loadTracks(target);
  });

  $effect(() => {
    return () => {
      stopPreview();
    };
  });
</script>

<Modal
  open={Boolean(item)}
  eyebrow={t("studio.preview.eyebrow")}
  title={item?.title ?? ""}
  lede={item?.subtitle || undefined}
  panelClass="catalog-preview-dialog"
  onclose={() => {
    stopPreview();
    onclose();
  }}
>
  {#snippet lead()}
    {#if item?.thumbnailUrl}
      <img class="catalog-preview__cover" src={item.thumbnailUrl} alt="" width="52" height="52" />
    {/if}
  {/snippet}

  <p class="subtle sm">
    {t("studio.preview.lead", { n: PREVIEW_MAX_SEC })}
  </p>

  {#if listBusy}
    <p class="panel-empty">{t("studio.preview.loadingTracks")}</p>
  {:else if tracks.length}
    <div class="catalog-preview__tracks">
      {#each tracks as track (track.id)}
        {@const isPlaying = playingUrl === track.url}
        {@const isBusy = busyUrl === track.url}
        <button
          type="button"
          class="catalog-preview__track"
          class:is-active={isPlaying}
          class:is-loading={isBusy}
          aria-pressed={isPlaying}
          onclick={() => void playPreview(track)}
        >
          <span class="catalog-preview__track-ic" aria-hidden="true">
            <UiIcon name={isPlaying ? "pause" : "play"} />
          </span>
          <span class="catalog-preview__track-title">{track.title}</span>
          <span class="catalog-preview__track-state">
            {#if isBusy}
              {t("studio.preview.preparing")}
            {:else if isPlaying}
              {t("studio.preview.secondsLeft", { n: Math.max(0, Math.ceil(PREVIEW_MAX_SEC - elapsed)) })}
            {/if}
          </span>
          {#if isPlaying}
            <span
              class="catalog-preview__track-bar"
              style={`--rk-preview-progress:${progress}`}
              aria-hidden="true"
            ></span>
          {/if}
        </button>
      {/each}
    </div>
  {:else if listErr}
    <p class="subtle sm warnline">{listErr}</p>
  {/if}

  {#if previewErr}
    <p class="subtle sm warnline">{previewErr}</p>
  {/if}

  <!-- Auditions use their own element: the library player keeps its queue. -->
  <audio
    bind:this={audioEl}
    preload="none"
    class="sr-only"
    onended={stopPreview}
    onerror={onAudioError}
  ></audio>

  {#snippet footer()}
    {#if listTitle}
      <span class="subtle sm catalog-preview__list-title">{listTitle}</span>
    {/if}
    <span class="rk-modal-foot-spacer"></span>
    <button
      type="button"
      class="ghost-btn"
      onclick={() => {
        stopPreview();
        onclose();
      }}
    >
      {t("ui.close")}
    </button>
    {#if item}
      <button
        type="button"
        class="primary-btn"
        onclick={() => {
          stopPreview();
          onDownload(item, tracks.length > 1 ? "playlist" : "single");
        }}
      >
        <UiIcon name="download" />
        {t("studio.catalog.download")}
      </button>
    {/if}
  {/snippet}
</Modal>
