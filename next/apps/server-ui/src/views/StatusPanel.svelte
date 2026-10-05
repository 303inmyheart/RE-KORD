<script lang="ts">
  import { ActionRow, Button, Panel, StatList, type StatItem } from "@rekord/ui";
  import { admin, humanBytes, humanTime } from "../lib/admin.svelte";
  import { formatNumber, t } from "../lib/i18n.svelte";
  import ScanReportCard from "./ScanReportCard.svelte";

  let {
    items = [],
    busy = false,
    onrefresh,
  }: {
    items?: StatItem[];
    busy?: boolean;
    onrefresh: () => void;
  } = $props();

  const diag = $derived(admin.diagnostics);
</script>

<Panel title={t("nav.status")}>
  {#snippet actions()}
    <Button variant="secondary" disabled={busy} onclick={onrefresh}>{t("common.refresh")}</Button>
  {/snippet}

  <StatList {items} />

  {#if diag}
    <div class="grid">
      <div class="cell">
        <span class="k">{t("status.musicRoot")}</span>
        <span class="v">{diag.musicRoot ?? t("status.musicRootUnset")}</span>
      </div>
      <div class="cell">
        <span class="k">{t("status.dataDir")}</span>
        <span class="v">{diag.dataDir}</span>
      </div>
      <div class="cell">
        <span class="k">{t("status.database")}</span>
        <span class="v">{humanBytes(diag.db.sizeBytes)}</span>
      </div>
      <div class="cell">
        <span class="k">{t("status.watcher")}</span>
        <span class="v">
          {diag.watcher.running
            ? diag.watcher.pending
              ? t("status.watcherActivePending")
              : t("status.watcherActive")
            : !diag.watcher.enabled
              ? t("status.watcherOff")
              : diag.musicRoot
                ? t("status.watcherStarting")
                : t("status.watcherWaiting")}
        </span>
      </div>
      <div class="cell">
        <span class="k">{t("status.downloads")}</span>
        <span class="v">{formatNumber(diag.activeDownloads)}</span>
      </div>
      <div class="cell">
        <span class="k">{t("status.lastFolderEvent")}</span>
        <span class="v">{humanTime(diag.watcher.lastEventAt)}</span>
      </div>
    </div>
  {/if}

  <ScanReportCard warningsOnly />

  <ActionRow>
    <Button
      variant="secondary"
      disabled={busy || !admin.canManage}
      onclick={() => void admin.runScan("incremental")}
    >
      {t("status.updateLibrary")}
    </Button>
    <Button variant="ghost" onclick={() => void admin.show("jobs")}>{t("status.seeJobs")}</Button>
    <Button variant="ghost" onclick={() => void admin.show("diagnostics")}>
      {t("nav.diagnostics")}
    </Button>
  </ActionRow>
</Panel>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr));
    gap: 0.55rem 1.1rem;
    margin: 0.85rem 0;
  }

  .cell {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }

  .k {
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }

  .v {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
</style>
