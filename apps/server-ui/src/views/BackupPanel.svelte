<script lang="ts">
  import { ActionRow, Banner, Button, Panel } from "@rekord/ui";
  import { admin } from "../lib/admin.svelte";
  import { t } from "../lib/i18n.svelte";

  let fileInput = $state<HTMLInputElement | null>(null);

  const locked = $derived(admin.busy || !admin.canManage);

  function pickFile() {
    fileInput?.click();
  }

  function onPicked(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    if (!window.confirm(t("backup.restoreConfirm", { name: file.name }))) return;
    void admin.restoreBackup(file);
  }
</script>

<Panel title={t("backup.title")}>
  <p class="hint">{t("backup.hint")}</p>
  <div class="secret" role="note">
    <p class="secret-title">{t("backup.secret.title")}</p>
    <p>{t("backup.secret.body")}</p>
  </div>
  <ActionRow>
    <Button variant="secondary" disabled={locked} onclick={() => void admin.downloadBackup()}>
      {t("backup.download")}
    </Button>
    <Button disabled={locked} onclick={pickFile}>{t("backup.restore")}</Button>
  </ActionRow>
  <input
    bind:this={fileInput}
    class="hidden-file"
    type="file"
    accept=".zip,application/zip"
    onchange={onPicked}
  />
  {#if !admin.canManage}
    <Banner tone="info">{t("backup.machineOnly")}</Banner>
  {/if}
</Panel>

<Panel title={t("backup.legacy.title")}>
  <p class="hint">{t("backup.legacy.hint")}</p>
  <ActionRow>
    <Button variant="ghost" disabled={locked} onclick={() => void admin.syncLegacyMeta()}>
      {t("backup.legacy.sync")}
    </Button>
  </ActionRow>
  <p class="hint after">{t("legacy.hint")}</p>
</Panel>

<style>
  .hint {
    margin: 0 0 0.8rem;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh);
  }

  .hint.after {
    margin: 0.7rem 0 0;
  }

  .secret {
    margin: 0 0 0.9rem;
    padding: 0.65rem 0.85rem;
    border: 1px solid color-mix(in srgb, #fbbf24 45%, transparent);
    border-left-width: 3px;
    border-radius: var(--rk-radius);
    background: color-mix(in srgb, #fbbf24 9%, transparent);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh);
  }

  .secret p {
    margin: 0;
  }

  .secret-title {
    font-weight: 700;
    margin-bottom: 0.25rem !important;
  }

  .hidden-file {
    display: none;
  }
</style>
