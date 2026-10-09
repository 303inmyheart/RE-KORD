<script lang="ts">
  import { CoverArt } from "@rekord/ui";
  import MetaBadgeCluster from "./MetaBadgeCluster.svelte";
  import UiIcon from "./icons/UiIcon.svelte";
  let {
    kind = "artist" as "artist" | "album",
    title,
    subtitle = "",
    metaLine = "",
    coverSrc = null,
    coverSeed = "",
    favoriteCount = 0,
    albumsMissingMetaCount = 0,
    tracksMissingMetaCount = 0,
    genreMissing = false,
    albumExcluded = false,
    albumsExcludedCount = 0,
    tracksExcludedCount = 0,
    loose = false,
    showInitialsFallback = true,
    onclick,
  }: {
    kind?: "artist" | "album";
    title: string;
    subtitle?: string;
    metaLine?: string;
    /** Cover URL, or null/"" when there is none (no request, placeholder). */
    coverSrc?: string | null;
    coverSeed?: string;
    favoriteCount?: number;
    albumsMissingMetaCount?: number;
    tracksMissingMetaCount?: number;
    /** True when album studio/sidecar meta is missing (legacy `!hasAlbumMeta`). */
    genreMissing?: boolean;
    albumExcluded?: boolean;
    albumsExcludedCount?: number;
    tracksExcludedCount?: number;
    loose?: boolean;
    showInitialsFallback?: boolean;
    onclick?: () => void;
  } = $props();

  const albumMetaMissing = $derived(kind === "album" ? genreMissing : false);
  const src = $derived(coverSrc || null);
</script>

<button
  type="button"
  class="library-list-tile"
  class:library-list-tile--artist={kind === "artist"}
  class:library-list-tile--album={kind === "album"}
  {onclick}
>
  <div class="library-list-tile__media">
    <!-- Artist without a photo: initials. Album without a cover: disc glyph. -->
    <CoverArt
      kind={kind === "artist" && showInitialsFallback ? "artist" : "album"}
      {title}
      {src}
      seed={coverSeed || title}
      size="tile"
    />
  </div>

  <div class="library-list-tile__body">
    <div class="library-list-tile__title-row">
      <UiIcon name={kind === "artist" ? "person" : "album"} class="library-list-tile__kind-ic" />
      <div class="library-list-tile__title" {title}>{title}</div>
    </div>
    {#if subtitle}
      <div class="library-list-tile__meta" title={subtitle}>{subtitle}</div>
    {/if}
    {#if metaLine}
      <div class="library-list-tile__tracks-meta">
        <UiIcon name="queueMusic" />
        <span>{metaLine}</span>
      </div>
    {/if}
    <MetaBadgeCluster
      variant="foot"
      missingMeta={albumMetaMissing}
      {albumsMissingMetaCount}
      {tracksMissingMetaCount}
      {favoriteCount}
      {albumExcluded}
      {albumsExcludedCount}
      {tracksExcludedCount}
      {loose}
    />
  </div>
</button>
