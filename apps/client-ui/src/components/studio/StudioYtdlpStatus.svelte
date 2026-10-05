<script lang="ts" module>
  import type { YtdlpStatus } from "../../lib/api/studio";

  /** Diagnostics spawn `yt-dlp --version`: ask at most once a minute. */
  let cached: { at: number; value: YtdlpStatus | null } | null = null;
  const CACHE_MS = 60_000;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { ytdlpStatus, ytdlpUpdate } from "../../lib/api/studio";
  import { t } from "../../lib/i18n.svelte";
  import { studioAccess } from "../../lib/studio/access.svelte";
  import { studioErrorText } from "../../lib/studio/errors";
  import { toasts } from "../../lib/toasts.svelte";

  let { cookies = false }: { cookies?: boolean } = $props();

  let status = $state<YtdlpStatus | null>(cached?.value ?? null);
  let loaded = $state(cached != null);
  let updating = $state(false);
  let updateSupported = $state(true);
  let note = $state<string | null>(null);

  async function load(force = false) {
    if (!force && cached && Date.now() - cached.at < CACHE_MS) {
      status = cached.value;
      loaded = true;
      return;
    }
    try {
      status = await ytdlpStatus();
    } catch {
      status = null;
    }
    cached = { at: Date.now(), value: status };
    loaded = true;
  }

  onMount(() => {
    void load();
  });

  async function update() {
    if (updating) return;
    updating = true;
    note = null;
    try {
      const r = await ytdlpUpdate();
      if (r === null) {
        updateSupported = false;
        note = t("studio.ytdlp.updateUnsupported");
        return;
      }
      note = r.updated
        ? t("studio.ytdlp.updated", { version: r.version ?? "?" })
        : t("studio.ytdlp.upToDate", { version: r.version ?? status?.version ?? "?" });
      cached = null;
      toasts.ok(note);
      await load(true);
    } catch (e) {
      note = studioErrorText(e);
    } finally {
      updating = false;
    }
  }

  const canOffer = $derived(
    updateSupported && status?.available !== false && status?.updatable !== false,
  );
</script>

<div class="studio-ytdlp" class:is-stale={status?.stale} class:is-missing={status?.available === false}>
  <span class="studio-ytdlp__state">
    {#if !loaded}
      {t("studio.ytdlp.checking")}
    {:else if !status}
      {t("studio.ytdlp.unknown")}
    {:else if !status.available}
      {t("studio.ytdlp.missing")}
    {:else}
      {t("studio.ytdlp.version", { version: status.version ?? "?" })}
    {/if}
    {#if cookies}
      <span class="studio-ytdlp__chip">{t("studio.ytdlp.cookies")}</span>
    {/if}
  </span>
  {#if status?.stale}
    <span class="studio-ytdlp__warn">
      {status.ageDays != null
        ? t("studio.ytdlp.staleDays", { n: status.ageDays })
        : t("studio.ytdlp.stale")}
    </span>
  {/if}
  {#if loaded && status && canOffer && studioAccess.canManageMachine}
    <button
      type="button"
      class="studio-ytdlp__update {status.stale || !status.available ? 'primary-btn primary-btn--sm' : 'linkbtn'}"
      disabled={updating}
      onclick={() => void update()}
    >
      {updating ? t("studio.ytdlp.updating") : t("studio.ytdlp.update")}
    </button>
  {:else if status?.stale && studioAccess.machineReason}
    <span class="studio-ytdlp__note">{t("studio.ytdlp.askAdmin")}</span>
  {/if}
  {#if note}
    <span class="studio-ytdlp__note" role="status">{note}</span>
  {/if}
</div>
