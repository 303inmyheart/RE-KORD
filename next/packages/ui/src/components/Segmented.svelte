<script lang="ts" module>
  export type SegmentedOption = {
    value: string;
    label: string;
    /** Hide the text and keep it as the accessible name (icon-only segment). */
    iconOnly?: boolean;
    disabled?: boolean;
    title?: string;
  };
</script>

<script lang="ts">
  /**
   * Segmented control: 2–5 mutually exclusive options in one pill (sort by,
   * view mode, "Almeno uno / Tutti"). One size everywhere; it never wraps.
   *
   * ```svelte
   * <Segmented ariaLabel="Ordina" value={sort} onchange={(v) => (sort = v)}
   *   options={[{ value: "name", label: "Nome" }, { value: "plays", label: "Ascolti" }]}>
   *   {#snippet icon(opt)}<UiIcon name={opt.value === "name" ? "sortAz" : "chart"} />{/snippet}
   * </Segmented>
   * ```
   */
  let {
    options,
    value,
    onchange,
    ariaLabel = "",
    /** Stretch segments to fill the width. */
    block = false,
    icon,
    class: className = "",
  }: {
    options: SegmentedOption[];
    value: string;
    onchange: (value: string) => void;
    ariaLabel?: string;
    block?: boolean;
    icon?: import("svelte").Snippet<[SegmentedOption]>;
    class?: string;
  } = $props();

  let root: HTMLDivElement | null = $state(null);
  const focusValue = $derived(
    options.some((o) => o.value === value) ? value : (options[0]?.value ?? ""),
  );

  function onKeydown(e: KeyboardEvent) {
    if (!root) return;
    const btns = [...root.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const at = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    let next = -1;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") next = (at + 1) % btns.length;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = (at - 1 + btns.length) % btns.length;
    if (next < 0) return;
    e.preventDefault();
    btns[next]!.focus();
    btns[next]!.click();
  }
</script>

<div
  bind:this={root}
  class="rk-seg {className}"
  class:rk-seg--block={block}
  role="radiogroup"
  aria-label={ariaLabel || undefined}
  tabindex="-1"
  onkeydown={onKeydown}
>
  {#each options as opt (opt.value)}
    {@const on = opt.value === value}
    <button
      type="button"
      role="radio"
      class="rk-seg__opt"
      class:is-on={on}
      aria-checked={on}
      aria-label={opt.iconOnly ? opt.label : undefined}
      title={opt.title ?? (opt.iconOnly ? opt.label : undefined)}
      tabindex={opt.value === focusValue ? 0 : -1}
      disabled={opt.disabled}
      onclick={() => {
        if (!on) onchange(opt.value);
      }}
    >
      {#if icon}<span class="rk-seg__ic" aria-hidden="true">{@render icon(opt)}</span>{/if}
      {#if !opt.iconOnly}<span class="rk-seg__label">{opt.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .rk-seg {
    display: inline-flex;
    flex-wrap: nowrap;
    align-items: stretch;
    gap: 2px;
    max-width: 100%;
    padding: 2px;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-control);
    background: color-mix(in srgb, var(--rk-surface-3) 60%, var(--rk-surface-2));
    outline: none;
  }

  .rk-seg--block {
    display: flex;
    width: 100%;
  }

  .rk-seg--block .rk-seg__opt {
    flex: 1 1 0;
  }

  .rk-seg__opt {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    min-width: 0;
    min-height: calc(var(--rk-control-h-sm) - 6px);
    padding: 0.25rem 0.7rem;
    border: 0;
    border-radius: calc(var(--rk-radius-control) - 2px);
    background: transparent;
    color: var(--rk-muted);
    font: inherit;
    font-size: var(--rk-fs-2);
    font-weight: 650;
    line-height: var(--rk-lh-tight);
    white-space: nowrap;
    cursor: pointer;
    transition:
      color 0.12s ease,
      background 0.12s ease;
  }

  .rk-seg__opt:hover:not(.is-on):not(:disabled) {
    color: var(--rk-ink);
    background: color-mix(in srgb, var(--rk-ink) 6%, transparent);
  }

  .rk-seg__opt.is-on {
    background: var(--rk-surface);
    color: var(--rk-ink);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--rk-accent) 35%, var(--rk-line)),
      0 1px 2px color-mix(in srgb, var(--rk-bg) 40%, transparent);
  }

  .rk-seg__opt:disabled {
    color: color-mix(in srgb, var(--rk-muted) 55%, transparent);
    cursor: not-allowed;
  }

  .rk-seg__label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rk-seg__ic {
    display: inline-flex;
    flex-shrink: 0;
  }

  .rk-seg__ic :global(svg) {
    width: 1rem;
    height: 1rem;
  }

  .rk-seg__opt.is-on .rk-seg__ic {
    color: var(--rk-accent-2);
  }

  @media (pointer: coarse) {
    .rk-seg__opt {
      min-height: calc(var(--rk-tap-row) - 6px);
    }
  }
</style>
