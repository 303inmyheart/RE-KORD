<script lang="ts">
  import { CoverArt } from "@rekord/ui";
  import { tp } from "../lib/i18n.svelte";
  import UiIcon from "./icons/UiIcon.svelte";

  let {
    title,
    albumCount = 0,
    trackCount = 0,
    coverSlots = [] as string[],
    onclick,
  }: {
    title: string;
    albumCount?: number;
    trackCount?: number;
    /** Album cover URLs of the genre, best first (empty / null skipped). */
    coverSlots?: (string | null | undefined)[];
    onclick?: () => void;
  } = $props();

</script>

<button class="tile" type="button" {onclick}>
  <!-- Adaptive mosaic: 1, 2, 3 or 4 covers fill the square, never empty cells. -->
  <span class="cover"><CoverArt kind="genre" {title} srcs={coverSlots} size="tile" /></span>
  <span class="body">
    <span class="title-row">
      <UiIcon name="style" class="kind" />
      <span class="title">{title}</span>
    </span>
    <span class="sub">{tp("library.albumsCount", albumCount)} · {tp("library.tracksCount", trackCount)}</span>
  </span>
</button>

<style>
  .tile {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.95rem;
    width: 100%;
    min-height: 5.1rem;
    padding: 0.55rem 0.75rem;
    box-sizing: border-box;
    text-align: left;
    border-radius: var(--rk-radius-card);
    border: 1px solid var(--rk-line);
    background: var(--rk-surface-2);
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: border-color 0.16s ease, background 0.16s ease;
  }

  .tile:hover {
    border-color: var(--rk-line-strong);
    background: color-mix(in srgb, var(--rk-surface-3) 55%, var(--rk-surface-2));
  }

  .cover {
    flex-shrink: 0;
    display: block;
    line-height: 0;
  }

  .body {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }

  .title-row :global(.kind) {
    width: 1.1rem;
    height: 1.1rem;
    color: color-mix(in srgb, var(--rk-accent-2) 58%, var(--rk-accent) 42%);
    opacity: 0.88;
    flex-shrink: 0;
  }

  .title {
    font-weight: 700;
    font-size: var(--rk-fs-3);
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub {
    font-size: var(--rk-fs-2);
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--rk-muted) 88%, var(--rk-ink) 12%);
  }
</style>
