<script lang="ts">
  let {
    label,
    active = false,
    emphasis = false,
    /** Light ghost background (topbar sync/search, like old). */
    surface = false,
    /** Glyph only: no chip/border on hover/active. */
    bare = false,
    /** Active/hover colour in bare mode (`danger` = favourite/exclude). */
    tone = "default" as "default" | "danger",
    disabled = false,
    onclick,
    children,
  }: {
    label: string;
    active?: boolean;
    emphasis?: boolean;
    surface?: boolean;
    bare?: boolean;
    tone?: "default" | "danger";
    disabled?: boolean;
    onclick?: () => void;
    children: import("svelte").Snippet;
  } = $props();
</script>

<button
  class="rk-icon"
  class:active={active}
  class:emphasis={emphasis}
  class:surface={surface}
  class:bare={bare}
  class:danger={tone === "danger"}
  type="button"
  aria-label={label}
  title={label}
  {disabled}
  {onclick}
>
  {@render children()}
</button>

<style>
  .rk-icon {
    width: 2.35rem;
    height: 2.35rem;
    border: 0;
    border-radius: var(--rk-radius);
    background: transparent;
    color: var(--rk-muted);
    cursor: pointer;
    font: inherit;
    display: inline-grid;
    place-items: center;
  }

  .rk-icon.surface {
    width: 2.5rem;
    height: 2.5rem;
    min-width: 2.5rem;
    min-height: 2.5rem;
    border: 1px solid var(--rk-line);
    background: color-mix(in srgb, var(--rk-surface-3) 72%, var(--rk-surface-2));
    color: var(--rk-ink);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--rk-bg) 52%, transparent);
  }

  .rk-icon:hover:not(:disabled) {
    color: var(--rk-ink);
    background: var(--rk-surface-3);
  }

  .rk-icon.surface:hover:not(:disabled) {
    border-color: var(--rk-line-strong);
    background: color-mix(in srgb, var(--rk-surface-3) 88%, var(--rk-accent-2) 6%);
    color: var(--rk-ink);
  }

  .rk-icon:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .rk-icon.active {
    color: var(--rk-accent-2);
    background: var(--rk-accent2-soft);
  }

  .rk-icon.bare,
  .rk-icon.bare:hover:not(:disabled),
  .rk-icon.bare.active,
  .rk-icon.bare.active:hover:not(:disabled) {
    background: transparent;
    border: 0;
    box-shadow: none;
  }

  .rk-icon:focus-visible {
    outline: 2px solid var(--rk-focus);
    outline-offset: 2px;
  }

  .rk-icon:disabled {
    opacity: 1;
    color: color-mix(in srgb, var(--rk-muted) 55%, transparent);
  }

  .rk-icon.bare:hover:not(:disabled) {
    color: var(--rk-ink);
  }

  .rk-icon.bare.active,
  .rk-icon.bare.active:hover:not(:disabled) {
    color: var(--rk-accent-2);
  }

  .rk-icon.bare.danger:hover:not(:disabled),
  .rk-icon.bare.danger.active,
  .rk-icon.bare.danger.active:hover:not(:disabled) {
    color: var(--rk-danger);
  }

  .rk-icon.emphasis {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: var(--rk-radius);
    background: var(--rk-cta);
    color: var(--rk-accent-ink);
    box-shadow: var(--rk-shadow);
    opacity: 1;
  }

  .rk-icon.emphasis:hover:not(:disabled) {
    filter: brightness(1.06);
  }

  /* With a finger the square grows to the minimum target: the glyph keeps its
     size, only the area that responds to touch changes. */
  @media (pointer: coarse) {
    .rk-icon,
    .rk-icon.surface,
    .rk-icon.emphasis {
      width: var(--rk-tap-min);
      height: var(--rk-tap-min);
      min-width: var(--rk-tap-min);
      min-height: var(--rk-tap-min);
    }
  }
</style>
