<script lang="ts">
  import { EmptyState } from "@rekord/ui";
  import LibraryListTile from "./LibraryListTile.svelte";
  import {
    virtualList,
    type VirtualListApi,
    type VirtualWindow,
  } from "../lib/virtualList";

  type Item = {
    id: number | string;
    title: string;
    subtitle?: string;
    metaLine?: string;
    /** null / "" = no cover: placeholder, no request. */
    coverSrc?: string | null;
    coverSeed?: string;
    kind?: "artist" | "album";
    favoriteCount?: number;
    albumsMissingMetaCount?: number;
    tracksMissingMetaCount?: number;
    genreMissing?: boolean;
    albumExcluded?: boolean;
    albumsExcludedCount?: number;
    tracksExcludedCount?: number;
    loose?: boolean;
  };

  /** Below this many tiles everything is rendered (dashboard rows, small libraries). */
  const VIRTUALIZE_FROM = 60;
  /** Rows above / below the viewport: Tab must always find the next tile mounted. */
  const OVERSCAN_ROWS = 3;
  /** Tile + gap before the first measurement (~library-list-tile height). */
  const ESTIMATE_ROW_PX = 100;

  let {
    items = [],
    emptyMessage,
    empty,
    kind = "artist" as "artist" | "album",
    dense = true,
    /** Colonne più strette (parity old library-overview-cols--dashboard). */
    dashboard = false,
    onselect,
  }: {
    items?: Item[];
    emptyMessage?: string;
    /** Rich empty state (icon, title, CTA); wins over `emptyMessage`. */
    empty?: import("svelte").Snippet;
    kind?: "artist" | "album";
    dense?: boolean;
    dashboard?: boolean;
    onselect: (id: number | string) => void;
  } = $props();

  let listEl: HTMLDivElement | null = $state(null);
  /** Grid columns at the current width (auto-fill decides them). */
  let cols = $state(1);
  let gridApi: VirtualListApi | null = null;
  let win = $state<VirtualWindow>({
    start: 0,
    end: Math.ceil(VIRTUALIZE_FROM / 2),
    padTop: 0,
    padBottom: 0,
    rowPx: 0,
  });

  const virtualized = $derived(items.length >= VIRTUALIZE_FROM);
  const rowCount = $derived(Math.ceil(items.length / Math.max(1, cols)));
  /** Index in `items` of the first rendered tile. */
  const firstIndex = $derived(virtualized ? win.start * cols : 0);
  const visibleItems = $derived(
    virtualized
      ? items.slice(firstIndex, Math.min(items.length, win.end * cols))
      : items,
  );

  function readColumns(el: HTMLElement): number {
    const tracks = getComputedStyle(el).gridTemplateColumns;
    if (!tracks || tracks === "none") return 1;
    return Math.max(1, tracks.trim().split(/\s+/).length);
  }

  $effect(() => {
    const el = listEl;
    if (!el) return;
    const sync = () => {
      const next = readColumns(el);
      if (next !== cols) cols = next;
    };
    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  });

  function tileAt(index: number): HTMLElement | null {
    if (!listEl) return null;
    const child = listEl.children[index - firstIndex];
    return child instanceof HTMLElement ? child : null;
  }

  function focusTile(index: number) {
    const tryFocus = () => {
      const el = tileAt(index);
      if (!el) return false;
      el.focus({ preventScroll: true });
      el.scrollIntoView({ block: "nearest" });
      return true;
    };
    if (tryFocus()) return;
    // Outside the rendered window: bring its row in, then focus once mounted.
    gridApi?.scrollToIndex(Math.floor(index / cols), "center");
    requestAnimationFrame(() => requestAnimationFrame(() => void tryFocus()));
  }

  /** Arrow keys move between tiles like a grid; Home / End jump to the ends. */
  function onGridKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey || !listEl) return;
    let child = e.target instanceof HTMLElement ? e.target : null;
    while (child && child.parentElement !== listEl) child = child.parentElement;
    if (!child) return;
    const current = firstIndex + Array.prototype.indexOf.call(listEl.children, child);
    const step = Math.max(1, cols);
    let next: number;
    switch (e.key) {
      case "ArrowRight":
        next = current + 1;
        break;
      case "ArrowLeft":
        next = current - 1;
        break;
      case "ArrowDown":
        next = current + step;
        break;
      case "ArrowUp":
        next = current - step;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = items.length - 1;
        break;
      default:
        return;
    }
    if (next < 0 || next >= items.length) return;
    e.preventDefault();
    focusTile(next);
  }
</script>

{#if items.length}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={listEl}
    class="list"
    class:list--cols={dense}
    class:list--dashboard={dashboard}
    style:padding-top={virtualized ? `${win.padTop}px` : null}
    style:padding-bottom={virtualized ? `${win.padBottom}px` : null}
    onkeydown={onGridKey}
    use:virtualList={{
      count: rowCount,
      threshold: virtualized ? 1 : Number.MAX_SAFE_INTEGER,
      estimateRowPx: ESTIMATE_ROW_PX,
      overscan: OVERSCAN_ROWS,
      onwindow: (next) => (win = next),
      onready: (api) => (gridApi = api),
    }}
  >
    {#each visibleItems as item (item.id)}
      <LibraryListTile
        kind={item.kind ?? kind}
        title={item.title}
        subtitle={item.subtitle ?? ""}
        metaLine={item.metaLine ?? ""}
        coverSrc={item.coverSrc || null}
        coverSeed={item.coverSeed ?? item.title}
        favoriteCount={item.favoriteCount ?? 0}
        albumsMissingMetaCount={item.albumsMissingMetaCount ?? 0}
        tracksMissingMetaCount={item.tracksMissingMetaCount ?? 0}
        genreMissing={item.genreMissing ?? false}
        albumExcluded={item.albumExcluded ?? false}
        albumsExcludedCount={item.albumsExcludedCount ?? 0}
        tracksExcludedCount={item.tracksExcludedCount ?? 0}
        loose={item.loose ?? false}
        onclick={() => onselect(item.id)}
      />
    {/each}
  </div>
{:else if empty}
  {@render empty()}
{:else}
  <EmptyState variant="inline" message={emptyMessage} />
{/if}

<style>
  .list {
    display: grid;
    gap: 0.45rem;
    width: 100%;
  }

  .list > :global(*) {
    min-width: 0;
  }

  .list--cols {
    grid-template-columns: repeat(auto-fill, minmax(min(19rem, 100%), 1fr));
    gap: var(--rk-space-lg) var(--rk-space-xl);
  }

  .list--cols.list--dashboard {
    grid-template-columns: repeat(auto-fill, minmax(min(17.5rem, 100%), 1fr));
  }
</style>
