<script lang="ts">
  import { uiLabels } from "../lib/uiLabels.svelte";

  /**
   * Empty / zero-data state: icon, title, one line of body, one call to action.
   *
   * ```svelte
   * <EmptyState title="No favourites" body="Tap the heart on a track to save it here.">
   *   {#snippet icon()}<UiIcon name="favorite" />{/snippet}
   *   {#snippet action()}<Button onclick={goLibrary}>Open the library</Button>{/snippet}
   * </EmptyState>
   * ```
   *
   * `message` alone (older callers) renders the quiet one-line variant.
   */
  let {
    title = "",
    body = "",
    message,
    /** `inline` for a panel body or a list (less padding, left aligned). */
    variant = "block",
    icon,
    action,
    children,
    class: className = "",
  }: {
    title?: string;
    body?: string;
    message?: string;
    variant?: "block" | "inline";
    icon?: import("svelte").Snippet;
    action?: import("svelte").Snippet;
    /** Extra content under the body (a list of tips, a secondary link). */
    children?: import("svelte").Snippet;
    class?: string;
  } = $props();

  const rich = $derived(Boolean(title || body || icon || action || children));
</script>

{#if rich}
  <div class="rk-empty-state rk-empty-state--{variant} {className}" role="status">
    {#if icon}
      <span class="rk-empty-state__icon" aria-hidden="true">{@render icon()}</span>
    {/if}
    {#if title}
      <p class="rk-empty-state__title">{title}</p>
    {/if}
    {#if body || (!title && message)}
      <p class="rk-empty-state__body">{body || message}</p>
    {/if}
    {@render children?.()}
    {#if action}
      <div class="rk-empty-state__action">{@render action()}</div>
    {/if}
  </div>
{:else}
  <p class="rk-empty {className}">{message ?? uiLabels.empty}</p>
{/if}

<style>
  .rk-empty {
    margin: 0;
    padding: 1.5rem 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-3);
  }

  .rk-empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--rk-space-sm);
    padding: var(--rk-space-2xl) var(--rk-space-xl);
    color: var(--rk-muted);
  }

  .rk-empty-state--inline {
    padding: var(--rk-space-xl) var(--rk-space-lg);
  }

  .rk-empty-state__icon {
    display: grid;
    place-items: center;
    width: 3rem;
    height: 3rem;
    margin-bottom: var(--rk-space-2xs);
    border-radius: var(--rk-radius-round);
    background: var(--rk-accent2-soft);
    color: var(--rk-accent-2);
  }

  .rk-empty-state__icon :global(svg) {
    width: 1.5rem;
    height: 1.5rem;
  }

  .rk-empty-state__title {
    margin: 0;
    color: var(--rk-ink);
    font-size: var(--rk-fs-4);
    font-weight: 700;
    line-height: var(--rk-lh-snug);
    text-wrap: balance;
  }

  .rk-empty-state__body {
    margin: 0;
    max-width: 42ch;
    font-size: var(--rk-fs-2);
    line-height: var(--rk-lh);
    text-wrap: pretty;
  }

  .rk-empty-state__action {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--rk-space-sm);
    margin-top: var(--rk-space-sm);
  }
</style>
