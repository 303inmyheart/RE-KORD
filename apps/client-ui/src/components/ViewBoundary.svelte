<script lang="ts">
  import type { Snippet } from "svelte";
  import { Button } from "@rekord/ui";
  import { t } from "../lib/i18n.svelte";

  /**
   * Keeps a render exception inside one area: without it Svelte tears down the
   * whole tree and the app goes blank. The fallback offers Retry (re-mounts the
   * children) and, for views, a way home.
   */
  let {
    name,
    compact = false,
    loadError = null,
    onretry,
    onhome,
    children,
  }: {
    /** Area name, for the console log. */
    name: string;
    /** Slim inline fallback (player dock) instead of the page card. */
    compact?: boolean;
    /** Failure that happened before render (lazy chunk load). */
    loadError?: unknown;
    /** Extra work on Retry (e.g. re-import a chunk). */
    onretry?: () => void;
    /** Shows "Go to dashboard" when given. */
    onhome?: () => void;
    children: Snippet;
  } = $props();

  function describe(error: unknown): string {
    if (error instanceof Error) return error.message;
    return typeof error === "string" ? error : "";
  }

  function logError(error: unknown) {
    console.error(`[rekord] ${name} crashed`, error);
  }

  $effect(() => {
    if (loadError) logError(loadError);
  });
</script>

{#snippet fallback(error: unknown, retry: () => void)}
  <div class="vb" class:vb--compact={compact} role="alert">
    <div class="vb__text">
      <p class="vb__title">{t(compact ? "ui.boundary.compactTitle" : "ui.boundary.title")}</p>
      {#if !compact}
        <p class="vb__msg">{t("ui.boundary.message")}</p>
        {#if describe(error)}
          <p class="vb__detail">{describe(error)}</p>
        {/if}
      {/if}
    </div>
    <div class="vb__actions">
      <Button variant="secondary" size="sm" onclick={retry}>{t("ui.boundary.retry")}</Button>
      {#if onhome}
        <Button variant="ghost" size="sm" onclick={onhome}>{t("ui.boundary.home")}</Button>
      {/if}
    </div>
  </div>
{/snippet}

{#if loadError}
  {@render fallback(loadError, () => onretry?.())}
{:else}
  <svelte:boundary onerror={logError}>
    {@render children()}
    {#snippet failed(error, reset)}
      {@render fallback(error, () => {
        onretry?.();
        reset();
      })}
    {/snippet}
  </svelte:boundary>
{/if}

<style>
  .vb {
    display: grid;
    gap: 0.85rem;
    padding: 1.25rem 1.35rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-lg);
    background: var(--rk-surface);
  }

  .vb--compact {
    position: fixed;
    left: 50%;
    bottom: calc(env(safe-area-inset-bottom, 0px) + var(--rk-mobile-nav-h, 0px) + 0.75rem);
    transform: translateX(-50%);
    z-index: var(--rk-z-dock, 60);
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.55rem 0.75rem;
    max-width: calc(100vw - 2rem);
    box-shadow: var(--rk-shadow);
  }

  .vb__text {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }

  .vb__title {
    margin: 0;
    font-weight: 650;
    font-size: var(--rk-fs-base);
  }

  .vb--compact .vb__title {
    font-size: var(--rk-fs-sm);
  }

  .vb__msg {
    margin: 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
  }

  .vb__detail {
    margin: 0;
    font-family: var(--rk-mono);
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
    word-break: break-word;
  }

  .vb__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
</style>
