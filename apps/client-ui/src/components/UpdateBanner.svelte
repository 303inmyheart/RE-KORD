<script lang="ts">
  /**
   * Avviso di compatibilita' client/hub. Vive in una radice Svelte sua, montata
   * da `main.ts` accanto all'app: cosi' compare sopra qualunque schermata (anche
   * la procedura di connessione) senza toccare AppShell.
   */
  import { t } from "../lib/i18n.svelte";
  import { compat } from "../lib/platform/compatState.svelte";
  import { DOWNLOAD_URL } from "../lib/platform/compat";
  import { openExternal } from "../lib/platform/externalLinks";
  import { reloadWithFreshShell } from "../lib/platform/pwa";
  import { APP_VERSION } from "../lib/version";

  const verdict = $derived(compat.verdict);
  const blocking = $derived(verdict.kind === "client-too-old");

  const vars = $derived({
    hub: verdict.kind === "ok" ? "" : verdict.hubVersion || "?",
    client: APP_VERSION,
    min: verdict.kind === "client-too-old" ? verdict.minClientVersion : "",
  });

  const copy = $derived.by(() => {
    switch (verdict.kind) {
      case "client-too-old":
        return {
          title: t("platform.update.clientTooOld.title"),
          body: t("platform.update.clientTooOld.body", vars),
        };
      case "hub-newer":
        return {
          title: t("platform.update.hubNewer.title"),
          body: t("platform.update.hubNewer.body", vars),
        };
      case "hub-older":
        return {
          title: t("platform.update.hubOlder.title"),
          body: t("platform.update.hubOlder.body", vars),
        };
      case "reload":
        return {
          title: t("platform.update.reload.title"),
          body: t("platform.update.reload.body", vars),
        };
      default:
        return null;
    }
  });

  function download() {
    void openExternal(DOWNLOAD_URL).catch(() => window.open(DOWNLOAD_URL, "_blank", "noopener"));
  }
</script>

{#if compat.visible && copy}
  <div
    class="rk-update"
    class:rk-update--blocking={blocking}
    role={blocking ? "alert" : "status"}
    aria-live="polite"
  >
    <div class="rk-update__text">
      <strong>{copy.title}</strong>
      <span>{copy.body}</span>
    </div>
    <div class="rk-update__actions">
      {#if verdict.kind === "reload"}
        <button type="button" class="rk-update__primary" onclick={() => void reloadWithFreshShell()}>
          {t("platform.update.reload")}
        </button>
      {:else if verdict.kind === "hub-older"}
        <button type="button" class="rk-update__primary" onclick={download}>
          {t("platform.update.howTo")}
        </button>
      {:else}
        <button type="button" class="rk-update__primary" onclick={download}>
          {t("platform.update.download")}
        </button>
      {/if}
      {#if !blocking}
        <button
          type="button"
          class="rk-update__close"
          aria-label={t("platform.update.dismiss")}
          title={t("platform.update.dismiss")}
          onclick={() => compat.dismiss()}
        >
          ×
        </button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .rk-update {
    position: fixed;
    z-index: 2147483000;
    top: calc(env(safe-area-inset-top, 0px) + 0.5rem);
    left: 50%;
    transform: translateX(-50%);
    width: min(36rem, calc(100vw - 1rem));
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.65rem 0.75rem 0.65rem 0.9rem;
    border-radius: var(--rk-radius, 12px);
    border: 1px solid var(--rk-line-strong, rgba(255, 255, 255, 0.18));
    background: var(--rk-surface-3, #1b2230);
    color: inherit;
    font: inherit;
    font-size: 0.875rem;
    line-height: 1.35;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }

  .rk-update--blocking {
    border-color: var(--rk-danger, #ff6b6b);
  }

  .rk-update__text {
    display: grid;
    gap: 0.15rem;
    min-width: 0;
    flex: 1;
  }

  .rk-update__text span {
    color: var(--rk-muted-strong, rgba(255, 255, 255, 0.75));
  }

  .rk-update__actions {
    display: flex;
    gap: 0.35rem;
    align-items: center;
    flex-shrink: 0;
  }

  .rk-update__primary {
    border: 0;
    border-radius: var(--rk-radius-sm, 8px);
    padding: 0.45rem 0.8rem;
    background: var(--rk-accent, #4fd4c4);
    color: var(--rk-accent-ink, #031018);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .rk-update__close {
    border: 0;
    background: transparent;
    color: var(--rk-muted, rgba(255, 255, 255, 0.6));
    font-size: 1.25rem;
    line-height: 1;
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    cursor: pointer;
  }

  .rk-update__close:hover,
  .rk-update__close:focus-visible {
    background: var(--rk-accent-soft, rgba(255, 255, 255, 0.08));
  }

  @media (max-width: 559.98px) {
    .rk-update {
      flex-direction: column;
      align-items: stretch;
    }

    .rk-update__actions {
      justify-content: flex-end;
    }
  }
</style>
