<script lang="ts">
  /**
   * Loading placeholder shaped like the content that is coming.
   *
   * - `variant="text"`: `lines` bars of text (last one shorter).
   * - `variant="row"`: a list row (square + two text lines), repeated `count` times.
   * - `variant="tile"`: a library card (cover + title + meta), repeated `count` times.
   * - `variant="metric"`: a KPI tile (label + big number).
   * - `variant="block"`: a plain box of `width` × `height`.
   *
   * Static on WebKitGTK/Tauri-Linux and with reduced motion (base.css).
   */
  let {
    variant = "block",
    count = 1,
    lines = 2,
    width = "100%",
    height = "1rem",
    label = "",
    class: className = "",
  }: {
    variant?: "text" | "row" | "tile" | "metric" | "block";
    count?: number;
    lines?: number;
    width?: string;
    height?: string;
    /** Screen-reader text (e.g. "Caricamento…"). */
    label?: string;
    class?: string;
  } = $props();

  const items = $derived(Array.from({ length: Math.max(1, count) }, (_, i) => i));
  const textLines = $derived(Array.from({ length: Math.max(1, lines) }, (_, i) => i));
</script>

<div
  class="rk-skel rk-skel--{variant} {className}"
  role={label ? "status" : undefined}
  aria-busy="true"
  aria-label={label || undefined}
  aria-hidden={label ? undefined : "true"}
>
  {#if variant === "text"}
    {#each textLines as i (i)}
      <span
        class="rk-skeleton rk-skel__line"
        style:width={i === textLines.length - 1 && textLines.length > 1 ? "62%" : width}
      ></span>
    {/each}
  {:else if variant === "row"}
    {#each items as i (i)}
      <span class="rk-skel__row">
        <span class="rk-skeleton rk-skel__thumb"></span>
        <span class="rk-skel__stack">
          <span class="rk-skeleton rk-skel__line" style:width="46%"></span>
          <span class="rk-skeleton rk-skel__line rk-skel__line--sm" style:width="28%"></span>
        </span>
      </span>
    {/each}
  {:else if variant === "tile"}
    {#each items as i (i)}
      <span class="rk-skel__tile">
        <span class="rk-skeleton rk-skel__cover"></span>
        <span class="rk-skel__stack">
          <span class="rk-skeleton rk-skel__line" style:width="70%"></span>
          <span class="rk-skeleton rk-skel__line rk-skel__line--sm" style:width="45%"></span>
        </span>
      </span>
    {/each}
  {:else if variant === "metric"}
    {#each items as i (i)}
      <span class="rk-skel__metric">
        <span class="rk-skeleton rk-skel__line rk-skel__line--sm" style:width="40%"></span>
        <span class="rk-skeleton rk-skel__value"></span>
      </span>
    {/each}
  {:else}
    <span class="rk-skeleton" style:width style:height></span>
  {/if}
</div>

<style>
  .rk-skel {
    display: grid;
    gap: var(--rk-space-sm);
    min-width: 0;
  }

  .rk-skel--row {
    gap: var(--rk-space-lg);
  }

  .rk-skel--tile {
    grid-template-columns: repeat(auto-fill, minmax(min(17.5rem, 100%), 1fr));
    gap: var(--rk-space-lg) var(--rk-space-xl);
  }

  .rk-skel--metric {
    grid-template-columns: repeat(auto-fit, minmax(min(9rem, 100%), 1fr));
  }

  .rk-skel__line {
    height: 0.8rem;
    border-radius: var(--rk-radius-sm);
  }

  .rk-skel__line--sm {
    height: 0.65rem;
  }

  .rk-skel__row,
  .rk-skel__tile {
    display: flex;
    align-items: center;
    gap: var(--rk-space-lg);
    padding: var(--rk-space-xs) var(--rk-space-lg);
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-card);
    background: var(--rk-surface-2);
  }

  .rk-skel__tile {
    padding: var(--rk-space-md) var(--rk-space-lg);
  }

  .rk-skel__thumb {
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    border-radius: var(--rk-radius-cover);
  }

  .rk-skel__cover {
    width: 4.5rem;
    height: 4.5rem;
    flex-shrink: 0;
    border-radius: var(--rk-radius-cover);
  }

  .rk-skel__stack {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: var(--rk-space-sm);
  }

  .rk-skel__metric {
    display: grid;
    gap: var(--rk-space-sm);
    padding: 0.75rem 0.875rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-card);
    background: var(--rk-surface-2);
  }

  .rk-skel__value {
    height: 1.5rem;
    width: 55%;
    border-radius: var(--rk-radius-sm);
  }
</style>
