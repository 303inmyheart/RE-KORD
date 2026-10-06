<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import UiIcon from "../icons/UiIcon.svelte";
  import { api, type ExploreResult, type FsDirEntry } from "../../lib/api";
  import { downloadFlatCount, releasesListStream, type ReleaseItem } from "../../lib/api/studio";
  import { confirmDialog } from "../../lib/confirm.svelte";
  import { fmtNumber, t, tp } from "../../lib/i18n.svelte";
  import { session } from "../../lib/session.svelte";
  import { studioAccess } from "../../lib/studio/access.svelte";
  import { plannedReleaseFolders } from "../../lib/studio/downloadSummary";
  import { studioDownloads as dl } from "../../lib/studio/downloadStore.svelte";
  import { studioCodeText, studioErrorText } from "../../lib/studio/errors";
  import {
    buildStudioDownloadConfirm,
    isValidDownloadDestPath,
    normalizeDownloadDestPath,
    relPathLooksLikeAlbumFolderDest,
    resolveStudioDownloadOutputDir,
    studioDownloadKindForScope,
    type StudioDownloadScope,
  } from "../../lib/studioDownloadDest";
  import {
    detectStudioDlMode,
    looksLikeSupportedDownloadUrl,
    urlMatchesStudioDlMode,
    type DlVideoMode,
  } from "../../lib/youtubeUrl";
  import { partitionYoutubeReleaseEntries } from "../../lib/youtubeReleases";
  import StudioAccessNotice from "./StudioAccessNotice.svelte";
  import StudioLog from "./StudioLog.svelte";
  import StudioYtdlpStatus from "./StudioYtdlpStatus.svelte";

  const DEST_KEY = "rekord-dl-output";
  const EXPLORE_TYPES = new Set(["song", "album", "artist"]);
  const DEST_OK_KEY = "rekord-dl-ok";
  /** Planned folders listed in the batch confirm; the rest is counted. */
  const CONFIRM_ROWS = 30;

  let destPath = $state("");
  let destPicked = $state(false);
  let dirs = $state<FsDirEntry[]>([]);
  let newFolder = $state("");
  let fsBusy = $state(false);
  let err = $state<string | null>(null);
  /** Pane open again on the URL it had: do not re-detect a mode the user chose. */
  let lastDetectedUrl = dl.url.trim();

  let dirSearchOpen = $state(false);
  let dirQuery = $state("");
  let dirResults = $state<FsDirEntry[]>([]);
  let dirSearchBusy = $state(false);
  let dirSearchTimer: ReturnType<typeof setTimeout> | undefined;

  let exploreQ = $state("");
  let exploreResults = $state<ExploreResult[]>([]);
  let exploreBusy = $state(false);
  let releases = $state<ReleaseItem[]>([]);
  let releasesTitle = $state("");
  let releasesUploader = $state("");
  let releasesLoading = $state(false);
  /** Track counts still arriving (one yt-dlp probe per release). */
  let countsLoading = $state(false);
  let selectedReleases = $state<Set<string>>(new Set());
  let relQuery = $state("");
  let relAbort: AbortController | null = null;
  let cookiesOk = $state(false);

  const busy = $derived(dl.running);
  const canWrite = $derived(studioAccess.canWrite);
  const writeTitle = $derived(studioAccess.reason ?? undefined);

  /** Auto-detect the link type only when the URL changes (does not override a manual choice). */
  $effect(() => {
    const url = dl.url.trim();
    if (url === lastDetectedUrl) return;
    lastDetectedUrl = url;
    const detected = detectStudioDlMode(url);
    if (detected) dl.urlMode = detected;
  });

  let lastReleaseKey = `${dl.url}\u0001${dl.urlMode}`;
  $effect(() => {
    const key = `${dl.url}\u0001${dl.urlMode}`;
    if (key === lastReleaseKey) return;
    lastReleaseKey = key;
    resetReleases();
  });

  function resetReleases() {
    relAbort?.abort();
    relAbort = null;
    releases = [];
    releasesTitle = "";
    releasesUploader = "";
    selectedReleases = new Set();
    relQuery = "";
    releasesLoading = false;
    countsLoading = false;
  }

  onMount(() => {
    studioAccess.ensure();
    try {
      const saved = normalizeDownloadDestPath(sessionStorage.getItem(DEST_KEY) || "");
      const ok = sessionStorage.getItem(DEST_OK_KEY) === "1";
      if (saved && ok) {
        destPath = saved;
        destPicked = true;
      }
    } catch {
      /* ignore */
    }
    void refreshDirs();
    void api
      .downloadPreset()
      .then((p) => {
        cookiesOk = !!p.cookiesConfigured;
      })
      .catch(() => {});
    // A job started in another tab (or before a reload) gets its Cancel back,
    // once the rights are known (remote clients without them skip the call).
    void studioAccess.ready().then(() => dl.attach());
  });

  onDestroy(() => {
    relAbort?.abort();
  });

  function commitDest(path: string) {
    const normalized = normalizeDownloadDestPath(path);
    destPath = normalized;
    destPicked = Boolean(normalized);
    try {
      if (normalized) {
        sessionStorage.setItem(DEST_KEY, normalized);
        sessionStorage.setItem(DEST_OK_KEY, "1");
      } else {
        sessionStorage.removeItem(DEST_KEY);
        sessionStorage.removeItem(DEST_OK_KEY);
      }
    } catch {
      /* ignore */
    }
  }

  async function refreshDirs() {
    try {
      const list = await api.fsList(destPath);
      dirs = list.dirs;
      if (isValidDownloadDestPath(list.path ?? destPath)) {
        commitDest(list.path ?? destPath);
      }
      err = null;
    } catch (e) {
      err = studioErrorText(e);
      dirs = [];
    }
  }

  async function goUp() {
    if (!destPath) return;
    const parts = destPath.split("/").filter(Boolean);
    parts.pop();
    commitDest(parts.join("/"));
    await refreshDirs();
  }

  async function enterDir(rel: string) {
    commitDest(rel);
    dirSearchOpen = false;
    dirQuery = "";
    dirResults = [];
    await refreshDirs();
  }

  const destSegs = $derived(destPath.split("/").filter(Boolean));
  const inAlbumFolder = $derived(relPathLooksLikeAlbumFolderDest(destPath));
  const mkdirBlocked = $derived(inAlbumFolder);
  const hasValidDownloadDest = $derived(destPicked && isValidDownloadDestPath(destPath));
  const dlUrlValid = $derived(urlMatchesStudioDlMode(dl.url, dl.urlMode));
  const urlPlaceholder = $derived(
    dl.urlMode === "single"
      ? t("studio.dl.urlPhSingle")
      : dl.urlMode === "playlist"
        ? t("studio.dl.urlPhPlaylist")
        : t("studio.dl.urlPhReleases"),
  );

  const filteredReleases = $derived.by(() => {
    const q = relQuery.trim().toLowerCase();
    if (!q) return releases;
    return releases.filter(
      (r) => r.title.toLowerCase().includes(q) || r.url.toLowerCase().includes(q),
    );
  });
  const partitioned = $derived(
    partitionYoutubeReleaseEntries(
      filteredReleases.map((r) => ({ ...r, trackCount: r.trackCount ?? null })),
    ),
  );
  const albumKeys = $derived(session.allAlbums.map((a) => a.folder_key));
  /** Selected releases whose folder already exists in the library. */
  const selectedExisting = $derived.by(() => {
    if (!selectedReleases.size || !destPath) return 0;
    const titles = releases.filter((r) => selectedReleases.has(r.id)).map((r) => r.title);
    return plannedReleaseFolders(destPath, titles, albumKeys).filter((r) => r.exists).length;
  });

  async function createFolder() {
    const name = newFolder.trim();
    if (!name || mkdirBlocked) return;
    fsBusy = true;
    try {
      const r = await api.fsMkdir(destPath, name);
      newFolder = "";
      commitDest(r.relPath);
      await refreshDirs();
      dl.log("info", t("studio.dl.logFolderCreated", { path: r.relPath }));
    } catch (e) {
      err = studioErrorText(e);
    } finally {
      fsBusy = false;
    }
  }

  function onDirSearchInput() {
    clearTimeout(dirSearchTimer);
    dirSearchTimer = setTimeout(() => void runDirSearch(), 280);
  }

  async function runDirSearch() {
    const q = dirQuery.trim();
    if (q.length < 2) {
      dirResults = [];
      return;
    }
    dirSearchBusy = true;
    try {
      const r = await api.fsSearchDirs(q);
      dirResults = r.results || [];
    } catch (e) {
      err = studioErrorText(e);
      dirResults = [];
    } finally {
      dirSearchBusy = false;
    }
  }

  function confirmDownload(message: string, danger = false): Promise<boolean> {
    return confirmDialog({
      title: t("ui.studioDownload.confirmTitle"),
      message,
      confirmLabel: t("ui.studioDownload.confirmOk"),
      danger,
    });
  }

  /** Track count for the confirm; null when yt-dlp could not tell. */
  async function playlistCount(url: string): Promise<number | null> {
    try {
      const r = await downloadFlatCount(url.trim());
      const n = Number(r.count);
      if (r.count != null && r.known !== false && Number.isFinite(n) && n > 0) return n;
      dl.log("warn", t("studio.dl.logCountUnknown", { error: studioCodeText(r.error ?? "flat_count_failed", null) }));
      return null;
    } catch (e) {
      dl.log("warn", t("studio.dl.logCountUnknown", { error: studioErrorText(e) }));
      return null;
    }
  }

  function withCountLine(message: string, scope: StudioDownloadScope, count: number | null): string {
    if (scope !== "playlist" || count != null) return message;
    return `${message}\n\n${t("studio.dl.confirmTrackCountUnknown")}`;
  }

  async function startOne(url: string, scope: StudioDownloadScope, releaseTitle?: string, preamble?: string) {
    if (!url.trim()) throw new Error(t("studio.dl.errUrlMissing"));
    if (!looksLikeSupportedDownloadUrl(url)) throw new Error(t("studio.dl.errUrlUnsupported"));
    const trackCount = scope === "playlist" ? await playlistCount(url) : null;
    if (trackCount != null) dl.log("info", tp("studio.dl.logPlaylistCount", trackCount));
    const confirm = buildStudioDownloadConfirm(
      { dlPath: destPath, scope, releaseTitle, trackCount, preamble },
      t,
    );
    const message = withCountLine(confirm.message, scope, trackCount);
    if (!(await confirmDownload(message, confirm.variant === "danger"))) return;
    const out = resolveStudioDownloadOutputDir(destPath, scope, releaseTitle);
    await dl.runSingle(url, studioDownloadKindForScope(scope), out, releaseTitle);
  }

  async function onClassicDownload() {
    err = null;
    if (!hasValidDownloadDest) {
      err = t("studio.dl.errPickFolderFirst");
      return;
    }
    if (!dl.url.trim()) {
      err = t("studio.dl.errEnterUrl");
      return;
    }
    if (!urlMatchesStudioDlMode(dl.url, dl.urlMode)) {
      err = t("studio.dl.errUrlMismatch");
      return;
    }
    if (dl.urlMode === "releases") {
      err = t("studio.dl.errUseReleases");
      return;
    }
    const scope: StudioDownloadScope = dl.urlMode === "playlist" ? "playlist" : "single";
    if (scope === "single" && !inAlbumFolder) {
      err = t("studio.dl.errSingleNeedsAlbum");
      return;
    }
    try {
      await startOne(dl.url, scope);
    } catch (e) {
      err = studioErrorText(e);
    }
  }

  let exploreTimer: ReturnType<typeof setTimeout> | undefined;
  function onExploreInput() {
    clearTimeout(exploreTimer);
    exploreTimer = setTimeout(() => void runExplore(), 420);
  }

  async function runExplore() {
    const q = exploreQ.trim();
    if (q.length < 2) {
      exploreResults = [];
      return;
    }
    exploreBusy = true;
    try {
      const r = await api.youtubeExploreSearch(q);
      exploreResults = r.results;
      err = null;
    } catch (e) {
      err = studioErrorText(e);
    } finally {
      exploreBusy = false;
    }
  }

  /** List releases, then patch their track counts as yt-dlp finds them. */
  async function loadReleases(url: string, fallbackTitle: string) {
    resetReleases();
    const ctrl = new AbortController();
    relAbort = ctrl;
    releasesLoading = true;
    releasesTitle = fallbackTitle;
    let total = 0;
    try {
      await releasesListStream(
        url,
        {
          onMeta: (m) => {
            releasesTitle = m.listTitle || fallbackTitle;
            releasesUploader = m.uploader || "";
            total = m.total;
          },
          onEntry: (e) => {
            releases = [...releases, e];
          },
          onListReady: () => {
            releasesLoading = false;
            countsLoading = true;
            total = total || releases.length;
            dl.log(
              "info",
              t("studio.dl.logReleasesFound", {
                releases: tp("studio.dl.releasesCount", total),
                uploader: releasesUploader ? ` — ${releasesUploader}` : "",
              }),
            );
            if (!releases.length) err = t("studio.dl.errNoReleases");
          },
          onPatch: (e) => {
            releases = releases.map((r) =>
              r.id === e.id ? { ...r, trackCount: e.trackCount ?? r.trackCount } : r,
            );
          },
        },
        ctrl.signal,
      );
    } catch (e) {
      if (ctrl.signal.aborted) return;
      err = studioErrorText(e);
    } finally {
      if (relAbort === ctrl) {
        releasesLoading = false;
        countsLoading = false;
        relAbort = null;
      }
    }
  }

  async function openArtistReleases(item: ExploreResult) {
    err = null;
    await loadReleases(item.url, item.title);
  }

  async function downloadExploreItem(item: ExploreResult) {
    err = null;
    if (!hasValidDownloadDest) {
      err = t("studio.dl.errPickDest");
      return;
    }
    const scope: StudioDownloadScope = item.type === "song" ? "single" : "playlist";
    if (scope === "single" && !inAlbumFolder) {
      err = t("studio.dl.errSingleNeedsAlbum");
      return;
    }
    try {
      await startOne(
        item.url,
        scope,
        scope === "playlist" ? item.title : undefined,
        t("studio.dl.preamble", { title: item.title }),
      );
    } catch (e) {
      err = studioErrorText(e);
    }
  }

  async function downloadSelectedReleases() {
    err = null;
    const picked = releases.filter((r) => selectedReleases.has(r.id));
    if (!picked.length) {
      err = t("studio.dl.errNeedSelection");
      return;
    }
    const base = normalizeDownloadDestPath(destPath);
    if (!base || !hasValidDownloadDest) {
      err = t("studio.dl.errPickArtistDest");
      return;
    }
    if (inAlbumFolder) {
      err = t("studio.dl.errManyNeedArtist");
      return;
    }
    const planned = plannedReleaseFolders(base, picked.map((r) => r.title), albumKeys);
    const existing = planned.filter((r) => r.exists).length;
    const lines = planned
      .slice(0, CONFIRM_ROWS)
      .map((r) =>
        r.exists
          ? t("studio.dl.confirmRowExisting", { path: r.path })
          : t("studio.dl.confirmRowNew", { path: r.path }),
      );
    if (planned.length > CONFIRM_ROWS) {
      lines.push(tp("studio.dl.confirmRowMore", planned.length - CONFIRM_ROWS));
    }
    let message = t("studio.dl.confirmReleasesLead", {
      releases: tp("studio.dl.releasesCount", picked.length),
      path: base,
    });
    message += `\n\n${lines.join("\n")}`;
    if (existing) message += `\n\n${tp("studio.dl.confirmExistingWarn", existing)}`;
    message += `\n\n${t("studio.dl.confirmFolderNameHint")}`;
    if (!(await confirmDownload(message, existing > 0))) return;
    const kind = /music\.youtube\.com/i.test(dl.url) ? "download_ytmusic" : "download_releases";
    await dl.runBatch(
      picked.map((r) => ({
        title: r.title,
        url: r.url,
        outputDir: resolveStudioDownloadOutputDir(base, "playlist", r.title),
      })),
      kind,
    );
  }

  async function loadReleasesFromUrl() {
    err = null;
    if (!dl.url.trim()) return;
    if (!urlMatchesStudioDlMode(dl.url, "releases")) {
      err = t("studio.dl.errReleasesUrl");
      return;
    }
    if (!hasValidDownloadDest) {
      err = t("studio.dl.errPickFolderFirst");
      return;
    }
    await loadReleases(dl.url.trim(), t("studio.dl.releasesFallbackTitle"));
  }

  function setModeManual(mode: DlVideoMode) {
    dl.urlMode = mode;
  }

  function toggleRel(id: string) {
    const next = new Set(selectedReleases);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedReleases = next;
  }

  function countLabel(n: number | null | undefined): string {
    if (n != null) return tp("studio.dl.tracksShort", n);
    return countsLoading ? "…" : t("studio.dl.tracksUnknown");
  }

  const crumbs = $derived(destPath.split("/").filter(Boolean));
  const rootLabel = $derived(
    session.stats?.music_root?.split(/[\\/]/).filter(Boolean).pop() || t("studio.dl.musicRoot"),
  );
  const progressPct = $derived(
    dl.progress && dl.progress.total > 0
      ? Math.max(2, Math.min(100, (dl.progress.current / dl.progress.total) * 100))
      : 0,
  );
  const batchPct = $derived(
    dl.batch && dl.batch.total > 0
      ? Math.max(2, Math.min(100, (dl.batch.current / dl.batch.total) * 100))
      : 0,
  );
</script>

{#snippet releaseRow(r: ReleaseItem, i: number)}
  <li class="tools-dl-releases__row">
    <label class="tools-dl-releases__check">
      <input type="checkbox" checked={selectedReleases.has(r.id)} onchange={() => toggleRel(r.id)} />
      <span class="tools-dl-releases__title" title={r.title}>{r.title}</span>
      <span class="tools-dl-releases__trackcount" class:is-pending={r.trackCount == null}>
        {countLabel(r.trackCount)}
      </span>
    </label>
  </li>
{/snippet}

{#snippet releasesPicker()}
  <div class="tools-dl-releases__picks tools-dl-releases__picks--full">
    <p class="subtle sm">
      {releasesTitle}{releasesUploader && releasesUploader !== releasesTitle ? ` — ${releasesUploader}` : ""}
      {#if countsLoading}
        <span class="tools-dl-releases__counting"> · {t("studio.dl.countingTracks")}</span>
      {/if}
    </p>
    <div class="tools-dl-releases__toolbar">
      {#if releases.length > 1}
        <input
          type="search"
          class="ghost-input"
          bind:value={relQuery}
          placeholder={t("studio.dl.filterPh")}
          aria-label={t("studio.dl.filterAria")}
        />
      {/if}
      <button
        type="button"
        class="ghost-btn ghost-btn--sm"
        onclick={() => {
          selectedReleases = new Set(filteredReleases.map((e) => e.id));
        }}
      >
        {t("studio.dl.selectAll")}
      </button>
      <button type="button" class="ghost-btn ghost-btn--sm" onclick={() => (selectedReleases = new Set())}>
        {t("studio.dl.selectNone")}
      </button>
    </div>
    <div class="tools-dl-releases__sections">
      {#if partitioned.albums.length}
        <section class="tools-dl-releases__section" aria-label={t("studio.dl.sectionAlbums")}>
          <h4 class="tools-dl-releases__section-title">
            {t("studio.dl.sectionAlbums")}
            <span class="tools-dl-releases__section-count">{partitioned.albums.length}</span>
          </h4>
          <ul class="tools-dl-releases__list tools-dl-releases__list--grid">
            {#each partitioned.albums as r, i (`${r.id}:${i}`)}
              {@render releaseRow(r, i)}
            {/each}
          </ul>
        </section>
      {/if}
      {#if partitioned.songs.length}
        <section class="tools-dl-releases__section" aria-label={t("studio.dl.sectionSingles")}>
          <h4 class="tools-dl-releases__section-title">
            {t("studio.dl.sectionSingles")}
            <span class="tools-dl-releases__section-count">{partitioned.songs.length}</span>
          </h4>
          <ul class="tools-dl-releases__list tools-dl-releases__list--grid">
            {#each partitioned.songs as r, i (`${r.id}:${i}`)}
              {@render releaseRow(r, i)}
            {/each}
          </ul>
        </section>
      {/if}
    </div>
    {#if selectedExisting > 0}
      <p class="subtle sm warnline tools-dl-releases__existing" role="note">
        {tp("studio.dl.selectedExistingWarn", selectedExisting)}
      </p>
    {/if}
  </div>
{/snippet}

<div class="studio-pane tools-download" role="region" aria-label={t("studio.dl.regionAria")}>
  <StudioAccessNotice what={t("studio.access.downloadReadOnly")} />

  {#if dl.running}
    <div class="dl-status-card" role="status" aria-live="polite">
      <div class="dl-status-card__head">
        <span class="dl-status-card__title">
          {#if dl.refreshing}
            {t("studio.dl.statusIndexing")}
          {:else if dl.activeTitle}
            {t("studio.dl.statusRunningTitle", { title: dl.activeTitle })}
          {:else if dl.busy}
            {t("studio.dl.statusRunning")}
          {:else}
            {tp("studio.dl.remoteRunning", dl.remote.length)}
          {/if}
        </span>
        <button
          type="button"
          class="ghost-btn ghost-btn--sm dl-status-card__stop"
          disabled={dl.stopRequested || dl.refreshing}
          onclick={() => void dl.cancel()}
        >
          {dl.stopRequested ? t("studio.dl.stopping") : t("studio.dl.stop")}
        </button>
      </div>
      {#if dl.batch && dl.batch.total > 0}
        <div class="dl-progress-block">
          <div class="dl-progress-top">
            <span>{t("studio.dl.progressReleases")}</span>
            <span>{fmtNumber(dl.batch.current)}/{fmtNumber(dl.batch.total)}</span>
          </div>
          <div class="dl-progress-rail">
            <div class="dl-progress-fill" style="width: {batchPct}%"></div>
          </div>
        </div>
      {/if}
      {#if dl.progress && dl.progress.total > 0}
        <div class="dl-progress-block">
          <div class="dl-progress-top">
            <strong>{t("studio.dl.progressTracks")}</strong>
            <span>{fmtNumber(dl.progress.current)}/{fmtNumber(dl.progress.total)}</span>
          </div>
          <div class="dl-progress-rail">
            <div class="dl-progress-fill" style="width: {progressPct}%"></div>
          </div>
        </div>
      {:else}
        <div class="dl-progress-rail dl-progress-rail--indeterminate" aria-hidden="true">
          <div class="dl-progress-fill"></div>
        </div>
      {/if}
    </div>
  {/if}

  <div class="studio-panel tools-dl-dest">
    <div class="tools-dl-dest__head">
      <div class="tools-dl-dest__head-text">
        <h4 class="studio-panel-title">{t("studio.dl.saveFolder")}</h4>
        <p class="subtle sm tools-dl-dest__lead">
          {t("studio.dl.saveFolderLead")}
        </p>
      </div>
      <div class="tools-dl-studio-switch tools-dl-dest__mode-switch" role="group" aria-label={t("studio.dl.modeAria")}>
        <span class="tools-dl-studio-switch__label" class:is-active={dl.studioMode === "classic"}>
          {t("studio.dl.modeClassic")}
        </span>
        <button
          type="button"
          role="switch"
          class="tools-dl-studio-switch__track"
          aria-checked={dl.studioMode === "explore"}
          aria-label={t("studio.dl.modeAria")}
          onclick={() => (dl.studioMode = dl.studioMode === "classic" ? "explore" : "classic")}
        >
          <span class="tools-dl-studio-switch__thumb" aria-hidden="true"></span>
        </button>
        <span class="tools-dl-studio-switch__label" class:is-active={dl.studioMode === "explore"}>
          {t("studio.dl.modeExplore")}
        </span>
      </div>
    </div>
    <div class="tools-dl-dest__shell">
      <div class="tools-dl-dest__pathheader">
        <p class="tools-dl-dest__label" id="tools-dl-dest-where">{t("studio.dl.currentFolder")}</p>
        <div class="tools-dl-dest__pathrow">
          <div class="tools-dl-dest__pathbar">
            <button
              type="button"
              class="tools-dl-dest__up-icon"
              disabled={!destPath || dl.busy}
              title={t("studio.dl.upFolder")}
              aria-label={t("studio.dl.upFolder")}
              onclick={() => void goUp()}
            >
              <UiIcon name="chevronLeft" />
            </button>
            <nav class="breadcrumbs tools-dl-dest__crumbs" aria-labelledby="tools-dl-dest-where">
              <button
                type="button"
                class="crumb"
                onclick={() => {
                  commitDest("");
                  void refreshDirs();
                }}
              >
                {rootLabel}
              </button>
              {#each crumbs as c, i (i)}
                <span class="tools-dl-dest__bc">
                  <span class="tools-dl-dest__bc-sep" aria-hidden="true">/</span>
                  <button
                    type="button"
                    class="crumb"
                    onclick={() => {
                      commitDest(crumbs.slice(0, i + 1).join("/"));
                      void refreshDirs();
                    }}
                  >
                    {c}
                  </button>
                </span>
              {/each}
            </nav>
          </div>
          <div class="tools-dl-dest__search" class:is-open={dirSearchOpen}>
            <button
              type="button"
              class="tools-dl-dest__search-toggle"
              aria-label={t("studio.dl.folderSearchAria")}
              aria-pressed={dirSearchOpen}
              onclick={() => {
                dirSearchOpen = !dirSearchOpen;
                if (!dirSearchOpen) {
                  dirQuery = "";
                  dirResults = [];
                }
              }}
            >
              <UiIcon name="search" class="tools-dl-dest__search-toggle-ic" />
            </button>
            <div class="tools-dl-dest__search-field">
              <input
                type="search"
                class="ghost-input tools-dl-dest__search-input"
                placeholder={t("studio.dl.folderSearchPh")}
                bind:value={dirQuery}
                oninput={onDirSearchInput}
              />
            </div>
            {#if dirSearchOpen && (dirResults.length || dirSearchBusy || dirQuery.trim().length >= 2)}
              <div class="tools-dl-dest__search-results rk-scroll">
                {#if dirSearchBusy}
                  <p class="subtle sm">{t("studio.dl.searching")}</p>
                {:else if dirResults.length}
                  <ul class="tools-dl-dest__dirlist">
                    {#each dirResults as d (d.relPath)}
                      <li>
                        <button type="button" class="tools-dl-dest__dirbtn" onclick={() => void enterDir(d.relPath)}>
                          <UiIcon name="album" class="tools-dl-dest__dir-ic" />
                          <span class="tools-dl-dest__dir-name">{d.relPath}</span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                {:else}
                  <p class="subtle sm">{t("studio.dl.folderSearchEmpty")}</p>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      </div>

      <div class="tools-dl-dest__browser" role="group" aria-label={t("studio.dl.subfolders")}>
        {#if dirs.length === 0}
          <p class="subtle sm tools-dl-dest__empty">{t("studio.dl.emptyFolders")}</p>
        {/if}
        <ul class="tools-dl-dest__dirlist">
          {#each dirs as d (d.relPath)}
            <li>
              <button type="button" class="tools-dl-dest__dirbtn" onclick={() => void enterDir(d.relPath)}>
                <UiIcon name="album" class="tools-dl-dest__dir-ic" />
                <span class="tools-dl-dest__dir-name">{d.name}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>

      <div class="tools-dl-dest__create">
        {#if mkdirBlocked}
          <p class="subtle sm tools-dl-dest__mkdir-blocked">
            {t("studio.dl.mkdirBlockedInAlbum")}
          </p>
        {:else}
          <p class="tools-dl-dest__label tools-dl-dest__label--inline">{t("studio.dl.newSubLabel")}</p>
          <div class="tools-dl-dest__newrow">
            <input
              type="text"
              class="ghost-input tools-dl-dest__newinput"
              placeholder={t("studio.dl.newFolderPh")}
              bind:value={newFolder}
              disabled={fsBusy || !canWrite}
              onkeydown={(e) => {
                if (e.key === "Enter" && newFolder.trim()) {
                  e.preventDefault();
                  void createFolder();
                }
              }}
            />
            <button
              type="button"
              class="ghost-btn ghost-btn--sm"
              disabled={fsBusy || !newFolder.trim() || !canWrite}
              title={writeTitle}
              onclick={() => void createFolder()}
            >
              {t("studio.dl.createHere")}
            </button>
          </div>
        {/if}
      </div>

      {#if hasValidDownloadDest}
        <div class="tools-dl-dest__picked" role="status">
          {t("studio.dl.destination")} <code>{destPath}</code>
          {#if inAlbumFolder}
            {t("studio.dl.destAlbumFolder")}
          {:else if destSegs.length === 1}
            {t("studio.dl.destArtistFolder")}
          {/if}
        </div>
      {:else}
        <p class="subtle sm warnline tools-dl-dest__warn">
          {t("studio.dl.pickFolderWarn")}
        </p>
      {/if}
    </div>
  </div>

  <div class="studio-panel">
    {#if dl.studioMode === "explore"}
      <h4 class="studio-panel-title">{t("studio.dl.modeExplore")}</h4>
      <p class="subtle sm studio-panel-gap">{t("studio.dl.exploreLead")}</p>
      <input
        type="search"
        class="ghost-input"
        placeholder={t("studio.dl.explorePh")}
        bind:value={exploreQ}
        oninput={onExploreInput}
      />
      {#if exploreBusy}
        <p class="subtle sm">{t("studio.dl.exploreSearching")}</p>
      {/if}
      <div class="studio-explore-results studio-panel-gap">
        {#each exploreResults as item, i (`${item.url}:${i}`)}
          <div class="studio-catalog-list-tile studio-catalog-list-tile--row">
            <div class="studio-catalog-list-tile__main">
              {#if item.thumbnailUrl}
                <img class="studio-catalog-web-thumb" src={item.thumbnailUrl} alt="" width="48" height="48" loading="lazy" />
              {/if}
              <div class="studio-catalog-list-tile__text">
                <div class="library-list-tile__title">{item.title}</div>
                <div class="library-list-tile__meta">
                  {EXPLORE_TYPES.has(item.type) ? t(`studio.dl.exploreType.${item.type}`) : item.type}
                  {#if item.subtitle}· {item.subtitle}{/if}
                </div>
              </div>
            </div>
            <div class="studio-catalog-list-tile__actions">
              {#if item.type === "artist"}
                <button
                  type="button"
                  class="ghost-btn ghost-btn--sm"
                  disabled={releasesLoading}
                  onclick={() => void openArtistReleases(item)}
                >
                  {t("studio.dl.releases")}
                </button>
              {:else}
                <button
                  type="button"
                  class="primary-btn primary-btn--sm"
                  disabled={busy || !canWrite}
                  title={writeTitle}
                  onclick={() => void downloadExploreItem(item)}
                >
                  {t("studio.dl.download")}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
      {#if releasesLoading}
        <p class="subtle sm">{t("studio.dl.loadingReleases")}</p>
      {/if}
      {#if releases.length}
        {@render releasesPicker()}
        <div class="studio-action-row">
          <button
            type="button"
            class="primary-btn"
            disabled={busy || !selectedReleases.size || inAlbumFolder || !canWrite}
            title={writeTitle ?? (inAlbumFolder ? t("studio.dl.needArtistFolder") : undefined)}
            onclick={() => void downloadSelectedReleases()}
          >
            {t("studio.dl.downloadSelected", { n: selectedReleases.size })}
          </button>
        </div>
      {/if}
    {:else}
      <h4 class="studio-panel-title">{t("studio.dl.link")}</h4>
      <div class="tools-dl-modes">
        <div class="tools-dl-mode">
          <div class="tools-dl-mode__seg" role="group" aria-label={t("studio.dl.typeAria")}>
            <button
              type="button"
              class="tools-dl-mode__btn"
              class:is-on={dl.urlMode === "single"}
              aria-pressed={dl.urlMode === "single"}
              onclick={() => setModeManual("single")}
            >
              {t("studio.dl.typeSingle")}
            </button>
            <button
              type="button"
              class="tools-dl-mode__btn"
              class:is-on={dl.urlMode === "playlist"}
              aria-pressed={dl.urlMode === "playlist"}
              onclick={() => setModeManual("playlist")}
            >
              {t("studio.dl.typePlaylist")}
            </button>
            <button
              type="button"
              class="tools-dl-mode__btn"
              class:is-on={dl.urlMode === "releases"}
              aria-pressed={dl.urlMode === "releases"}
              onclick={() => setModeManual("releases")}
            >
              {t("studio.dl.typeReleases")}
            </button>
          </div>
          <span class="tools-dl-mode__help-wrap">
            <button type="button" class="tools-dl-mode__help" aria-label={t("studio.dl.modeHelpAria")}>?</button>
            <span class="tools-dl-mode__tip" role="tooltip">
              {t("studio.dl.modeGuide")}
            </span>
          </span>
        </div>
      </div>
      <input
        type="url"
        class="ghost-input"
        placeholder={urlPlaceholder}
        bind:value={dl.url}
        disabled={dl.busy}
        aria-label={t("studio.dl.urlAria")}
        aria-invalid={dl.url.trim() !== "" && !dlUrlValid}
      />
      {#if dl.url.trim() && !dlUrlValid}
        <p class="subtle sm warnline">
          {t("studio.dl.urlIncompatible", {
            mode:
              dl.urlMode === "single"
                ? t("studio.dl.typeSingle")
                : dl.urlMode === "playlist"
                  ? t("studio.dl.typePlaylist")
                  : t("studio.dl.typeReleases"),
          })}
        </p>
      {/if}

      {#if dl.urlMode === "releases"}
        <div class="tools-dl-releases">
          {#if releasesLoading}
            <p class="subtle sm">{t("studio.dl.loadingReleases")}</p>
          {/if}
          {#if releases.length}
            {@render releasesPicker()}
          {/if}
          <div class="studio-inline-actions studio-inline-actions--spaced tools-dl-actions-row">
            <div class="tools-dl-disclaimer">
              <p class="tools-dl-disclaimer__text">{t("studio.dl.disclaimer")}</p>
              <StudioYtdlpStatus cookies={cookiesOk} />
            </div>
            {#if !releases.length}
              <button
                type="button"
                class="primary-btn"
                disabled={releasesLoading || !dl.url.trim() || !dlUrlValid || !hasValidDownloadDest}
                onclick={() => void loadReleasesFromUrl()}
              >
                {releasesLoading ? t("studio.dl.loading") : t("studio.dl.loadReleases")}
              </button>
            {:else}
              <button
                type="button"
                class="ghost-btn"
                disabled={releasesLoading || !dl.url.trim() || !dlUrlValid}
                onclick={() => void loadReleasesFromUrl()}
              >
                {t("studio.dl.reloadList")}
              </button>
              <button
                type="button"
                class="primary-btn"
                disabled={busy || !selectedReleases.size || inAlbumFolder || !canWrite}
                title={writeTitle ?? (inAlbumFolder ? t("studio.dl.needArtistFolder") : undefined)}
                onclick={() => void downloadSelectedReleases()}
              >
                {t("studio.dl.downloadSelected", { n: selectedReleases.size })}
              </button>
            {/if}
          </div>
        </div>
      {:else}
        <div class="studio-inline-actions studio-inline-actions--spaced tools-dl-actions-row">
          <div class="tools-dl-disclaimer">
            <p class="tools-dl-disclaimer__text">{t("studio.dl.disclaimer")}</p>
            <StudioYtdlpStatus cookies={cookiesOk} />
          </div>
          <button
            type="button"
            class="primary-btn"
            disabled={busy || !dl.url.trim() || !dlUrlValid || !hasValidDownloadDest || !canWrite}
            title={writeTitle}
            onclick={() => void onClassicDownload()}
          >
            {dl.busy ? t("studio.dl.running") : t("studio.dl.run")}
          </button>
        </div>
      {/if}
    {/if}
  </div>

  {#if err}
    <p class="subtle sm warnline" role="alert">{err}</p>
  {/if}

  <StudioLog
    class="studio-dl-log"
    entries={dl.entries}
    raw={dl.raw}
    bind:showRaw={dl.showRaw}
    onclear={() => {
      dl.clearLog();
      err = null;
    }}
  />
</div>
