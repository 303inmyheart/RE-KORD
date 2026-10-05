<script lang="ts">
  /**
   * Status / count pill. Render it only when it carries a value — never a
   * ghost placeholder at low opacity.
   *
   * ```svelte
   * {#if favCount > 0}
   *   <Badge tone="favorite" title="3 preferiti">{#snippet icon()}<UiIcon name="favorite" />{/snippet}3</Badge>
   * {/if}
   * ```
   */
  let {
    tone = "neutral",
    title = "",
    /** Icon-only round pill (needs `title` for its accessible name). */
    iconOnly = false,
    /** Tint by an arbitrary colour (mood colours). Overrides `tone`. */
    color = "",
    icon,
    children,
    class: className = "",
  }: {
    tone?: "neutral" | "accent" | "accent2" | "success" | "warning" | "danger" | "favorite";
    title?: string;
    iconOnly?: boolean;
    color?: string;
    icon?: import("svelte").Snippet;
    children?: import("svelte").Snippet;
    class?: string;
  } = $props();
</script>

<span
  class="rk-pill {tone !== 'neutral' ? `rk-pill--${tone}` : ''} {className}"
  class:rk-pill--icon={iconOnly}
  style:--pill-c={color || undefined}
  title={title || undefined}
  role={iconOnly && title ? "img" : undefined}
  aria-label={iconOnly && title ? title : undefined}
>
  {#if icon}{@render icon()}{/if}
  {#if children}{@render children()}{/if}
</span>
