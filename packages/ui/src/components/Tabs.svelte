<script lang="ts" module>
  export type TabItem = {
    id: string;
    label: string;
    /** Optional count shown after the label in a quiet pill. */
    count?: number | string | null;
    disabled?: boolean;
  };
</script>

<script lang="ts">
  /**
   * Section tabs: one line, underline marker, scrolls sideways when they do not
   * fit (never wraps). Edges fade where more tabs are hidden.
   *
   * Arrow keys move focus between tabs, Enter/Space (or a click) selects —
   * selecting can load a heavy view, so focus alone never switches.
   *
   * ```svelte
   * <Tabs items={[{ id: "a", label: "Artisti" }, { id: "g", label: "Generi" }]}
   *       active={tab} onselect={(id) => (tab = id)} ariaLabel="Sezioni libreria" />
   * ```
   */
  let {
    items,
    active,
    onselect,
    ariaLabel = "",
    size = "md",
    /** Stretch tabs over the full width (Studio panes). Still scrolls if needed. */
    even = false,
    /** Extra class on each tab button (kept for QA scripts and old CSS hooks). */
    tabClass = "",
    class: className = "",
  }: {
    items: TabItem[];
    active: string;
    onselect: (id: string) => void;
    ariaLabel?: string;
    /** `lg` page-level (title size), `md` under a page title, `sm` inside a panel. */
    size?: "sm" | "md" | "lg";
    even?: boolean;
    tabClass?: string;
    class?: string;
  } = $props();

  let scroller: HTMLDivElement | null = $state(null);
  /** Tab that takes Tab-key focus: the selected one, else the first. */
  const focusId = $derived(
    items.some((t) => t.id === active) ? active : (items.find((t) => !t.disabled)?.id ?? ""),
  );
  let fadeStart = $state(false);
  let fadeEnd = $state(false);

  function syncFade() {
    const el = scroller;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    fadeStart = el.scrollLeft > 2;
    fadeEnd = max - el.scrollLeft > 2;
  }

  $effect(() => {
    const el = scroller;
    if (!el) return;
    syncFade();
    const ro = new ResizeObserver(syncFade);
    ro.observe(el);
    return () => ro.disconnect();
  });

  /** Keep the selected tab in view (e.g. Settings › Sistema on a phone). */
  $effect(() => {
    const el = scroller;
    void active;
    if (!el) return;
    const on = el.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!on) return;
    const left = on.offsetLeft;
    const right = left + on.offsetWidth;
    if (left < el.scrollLeft + 16) el.scrollTo({ left: Math.max(0, left - 24) });
    else if (right > el.scrollLeft + el.clientWidth - 16) {
      el.scrollTo({ left: right - el.clientWidth + 24 });
    }
    syncFade();
  });

  function onKeydown(e: KeyboardEvent) {
    if (!scroller) return;
    const tabs = [...scroller.querySelectorAll<HTMLButtonElement>('[role="tab"]:not(:disabled)')];
    const at = tabs.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    let next = -1;
    if (e.key === "ArrowRight") next = (at + 1) % tabs.length;
    else if (e.key === "ArrowLeft") next = (at - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = tabs.length - 1;
    if (next < 0) return;
    e.preventDefault();
    tabs[next]!.focus();
    tabs[next]!.scrollIntoView({ block: "nearest", inline: "nearest" });
  }
</script>

<div
  class="rk-tabs rk-tabs--{size} {className}"
  class:rk-tabs--even={even}
  class:fade-start={fadeStart}
  class:fade-end={fadeEnd}
>
  <div
    class="rk-tabs__scroller"
    bind:this={scroller}
    role="tablist"
    tabindex="-1"
    aria-label={ariaLabel || undefined}
    onscroll={syncFade}
    onkeydown={onKeydown}
  >
    {#each items as tab (tab.id)}
      {@const on = tab.id === active}
      <button
        type="button"
        role="tab"
        class="rk-tabs__tab {tabClass}"
        class:is-on={on}
        aria-selected={on}
        tabindex={tab.id === focusId ? 0 : -1}
        disabled={tab.disabled}
        onclick={() => {
          if (!on) onselect(tab.id);
        }}
      >
        <span class="rk-tabs__label">{tab.label}</span>
        {#if tab.count != null && tab.count !== ""}
          <span class="rk-tabs__count">{tab.count}</span>
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .rk-tabs {
    position: relative;
    min-width: 0;
    max-width: 100%;
  }

  .rk-tabs__scroller {
    display: flex;
    flex-wrap: nowrap;
    align-items: stretch;
    gap: 1.25rem;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    overscroll-behavior-x: contain;
    /* Room for the underline and the focus ring inside the clip box. */
    padding: 0.2rem 0.15rem 0.3rem;
    margin: -0.2rem -0.15rem -0.3rem;
    outline: none;
  }

  .rk-tabs__scroller::-webkit-scrollbar {
    display: none;
  }

  /* Edge fade only where something is hidden; a static mask, nothing animated. */
  .rk-tabs.fade-end .rk-tabs__scroller {
    --rk-tabs-mask: linear-gradient(90deg, #000 calc(100% - 2.5rem), transparent);
  }

  .rk-tabs.fade-start .rk-tabs__scroller {
    --rk-tabs-mask: linear-gradient(90deg, transparent, #000 2.5rem);
  }

  .rk-tabs.fade-start.fade-end .rk-tabs__scroller {
    --rk-tabs-mask: linear-gradient(
      90deg,
      transparent,
      #000 2.5rem,
      #000 calc(100% - 2.5rem),
      transparent
    );
  }

  .rk-tabs.fade-start .rk-tabs__scroller,
  .rk-tabs.fade-end .rk-tabs__scroller {
    -webkit-mask-image: var(--rk-tabs-mask);
    mask-image: var(--rk-tabs-mask);
  }

  .rk-tabs--even .rk-tabs__scroller {
    justify-content: space-evenly;
  }

  .rk-tabs__tab {
    position: relative;
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0;
    padding: 0.25rem 0;
    border: none;
    background: transparent;
    cursor: pointer;
    font: inherit;
    font-size: var(--rk-fs-4);
    font-weight: 700;
    letter-spacing: -0.015em;
    line-height: var(--rk-lh-tight);
    white-space: nowrap;
    color: color-mix(in srgb, var(--rk-muted) 82%, var(--rk-ink) 18%);
    border-radius: var(--rk-radius-sm);
    transition: color 0.15s ease;
  }

  .rk-tabs--lg .rk-tabs__tab {
    font-size: var(--rk-fs-5);
    font-weight: 800;
    letter-spacing: -0.025em;
  }

  .rk-tabs--sm .rk-tabs__scroller {
    gap: 1rem;
  }

  .rk-tabs--sm .rk-tabs__tab {
    font-size: var(--rk-fs-2);
    font-weight: 650;
    letter-spacing: 0;
  }

  .rk-tabs__tab:hover:not(.is-on):not(:disabled) {
    color: var(--rk-muted-strong);
  }

  .rk-tabs__tab.is-on {
    color: var(--rk-ink);
  }

  .rk-tabs__tab::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -0.15rem;
    height: 2px;
    border-radius: var(--rk-radius-round);
    background: linear-gradient(90deg, var(--rk-accent), var(--rk-accent-2));
    transform: scaleX(0);
    transition: transform 0.18s ease;
  }

  .rk-tabs__tab.is-on::after {
    transform: scaleX(1);
  }

  .rk-tabs__tab:disabled {
    color: color-mix(in srgb, var(--rk-muted) 60%, transparent);
    cursor: not-allowed;
  }

  .rk-tabs__tab:focus-visible {
    outline: 2px solid var(--rk-focus);
    outline-offset: 2px;
  }

  .rk-tabs__count {
    display: inline-grid;
    place-items: center;
    min-width: 1.25rem;
    padding: 0.1rem 0.35rem;
    border-radius: var(--rk-radius-chip);
    background: color-mix(in srgb, var(--rk-ink) 8%, transparent);
    color: var(--rk-muted);
    font-size: var(--rk-fs-1);
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0;
  }

  .rk-tabs__tab.is-on .rk-tabs__count {
    background: var(--rk-accent2-soft);
    color: var(--rk-accent-2);
  }

  /* Phones: one rung smaller so four or five tabs fit before scrolling. */
  @media (max-width: 599.98px) {
    .rk-tabs__scroller {
      gap: 1rem;
    }

    .rk-tabs--md .rk-tabs__tab {
      font-size: var(--rk-fs-3);
    }

    .rk-tabs--lg .rk-tabs__tab {
      font-size: var(--rk-fs-4);
    }
  }

  /* Coarse pointers: a taller hit area without changing the look. */
  @media (pointer: coarse) {
    .rk-tabs__tab {
      min-height: var(--rk-tap-row);
    }
  }
</style>
