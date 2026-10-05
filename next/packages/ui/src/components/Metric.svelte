<script lang="ts">
  /**
   * One KPI: a label in sentence case over a large tabular number.
   *
   * ```svelte
   * <Metric label="Brani" value={stats.track_count} />
   * <Metric label="Avvisi qualità" value={48} tone="warning" hint="12 senza copertina" />
   * <Metric label="Album" loading />
   * ```
   */
  let {
    label,
    value = "",
    hint = "",
    tone = "default",
    loading = false,
    onclick,
    icon,
    class: className = "",
  }: {
    label: string;
    value?: string | number | null;
    /** One short line under the value. */
    hint?: string;
    tone?: "default" | "accent" | "success" | "warning" | "danger";
    /** Shows a skeleton bar instead of a misleading 0 while data loads. */
    loading?: boolean;
    /** Makes the whole tile a button (drill-down). */
    onclick?: () => void;
    icon?: import("svelte").Snippet;
    class?: string;
  } = $props();

  const shown = $derived(value == null || value === "" ? "—" : value);
</script>

<svelte:element
  this={onclick ? "button" : "div"}
  type={onclick ? "button" : undefined}
  class="rk-metric rk-metric--{tone} {className}"
  class:rk-metric--action={!!onclick}
  role={onclick ? undefined : "group"}
  aria-busy={loading || undefined}
  {onclick}
>
  <span class="rk-metric__label">
    {#if icon}<span class="rk-metric__ic" aria-hidden="true">{@render icon()}</span>{/if}
    <span class="rk-metric__label-text">{label}</span>
  </span>
  {#if loading}
    <span class="rk-skeleton rk-metric__skel" aria-hidden="true"></span>
  {:else}
    <strong class="rk-metric__value">{shown}</strong>
  {/if}
  {#if hint}
    <span class="rk-metric__hint">{hint}</span>
  {/if}
</svelte:element>

<style>
  .rk-metric {
    --metric-c: var(--rk-ink);
    display: grid;
    align-content: start;
    gap: 0.25rem;
    min-width: 0;
    margin: 0;
    padding: 0.75rem 0.875rem;
    border-radius: var(--rk-radius-card);
    border: 1px solid var(--rk-line);
    background: color-mix(in srgb, var(--rk-surface-3) 55%, var(--rk-surface-2) 45%);
    color: inherit;
    font: inherit;
    text-align: left;
  }

  .rk-metric--action {
    cursor: pointer;
    transition: border-color 0.15s ease;
  }

  .rk-metric--action:hover {
    border-color: var(--rk-line-strong);
  }

  .rk-metric--accent {
    --metric-c: var(--rk-accent);
  }

  .rk-metric--success {
    --metric-c: var(--rk-success);
  }

  .rk-metric--warning {
    --metric-c: var(--rk-warning);
    border-color: color-mix(in srgb, var(--rk-warning) 40%, var(--rk-line));
  }

  .rk-metric--danger {
    --metric-c: var(--rk-danger);
    border-color: color-mix(in srgb, var(--rk-danger) 40%, var(--rk-line));
  }

  .rk-metric__label {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
    font-size: var(--rk-fs-2);
    font-weight: 600;
    line-height: var(--rk-lh-snug);
    color: var(--rk-muted);
  }

  .rk-metric__label-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rk-metric__ic {
    display: inline-flex;
    color: var(--rk-accent-2);
  }

  .rk-metric__ic :global(svg) {
    width: 1rem;
    height: 1rem;
  }

  .rk-metric__value {
    font-size: var(--rk-fs-6);
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: var(--rk-lh-tight);
    font-variant-numeric: tabular-nums;
    color: var(--metric-c);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rk-metric__skel {
    height: 1.5rem;
    width: 55%;
    border-radius: var(--rk-radius-sm);
  }

  .rk-metric__hint {
    font-size: var(--rk-fs-1);
    color: var(--rk-muted);
  }
</style>
