<script lang="ts" module>
  export type CoverKind = "track" | "album" | "artist" | "genre";
  export type CoverSize = "xs" | "sm" | "md" | "lg" | "xl" | "tile" | "dock";

  /** Up to two initials from a name ("Bring Me the Horizon" → "BH", "Eminem" → "E"). */
  export function coverInitials(name: string): string {
    const words = name
      .replace(/[^\p{L}\p{N}\s&]/gu, " ")
      .split(/\s+/)
      .filter((w) => w && w !== "&" && !/^(the|a|an|il|lo|la|i|gli|le|di|de|and|e)$/i.test(w));
    if (words.length === 0) return (name.trim().charAt(0) || "").toUpperCase();
    if (words.length === 1) return words[0]!.charAt(0).toUpperCase();
    return (words[0]!.charAt(0) + words[words.length - 1]!.charAt(0)).toUpperCase();
  }
</script>

<script lang="ts">
  /**
   * Cover / avatar square for every entity in the library.
   *
   * - `src` may be null/empty: callers pass `null` when the hub says there is no
   *   cover (`has_cover: false`), so no request is made at all.
   * - A broken image falls back to the placeholder (`onerror`), never to the
   *   browser's broken-image icon.
   * - Placeholder by `kind`: track ♪ note, album disc, artist initials, genre a
   *   mosaic of the `srcs` covers that adapts to how many there are (1, 2, 3, 4).
   */
  let {
    kind = "album",
    title = "",
    src = null,
    srcs = [],
    // Kept for API parity with older callers (stable seed for a tone).
    seed: _seed = "",
    size = "md",
    rounded = true,
    /** Accessible name when the cover is meaningful on its own (default: decorative). */
    alt = "",
    loading = "lazy",
    class: className = "",
  }: {
    kind?: CoverKind;
    title?: string;
    src?: string | null;
    /** Genre mosaic: album cover URLs, best first. Empty entries are skipped. */
    srcs?: (string | null | undefined)[];
    seed?: string;
    size?: CoverSize;
    rounded?: boolean;
    alt?: string;
    loading?: "lazy" | "eager";
    class?: string;
  } = $props();

  let failed = $state(false);
  /** Mosaic tiles that failed to load, by URL. */
  let failedTiles = $state<Record<string, true>>({});

  const showImage = $derived(kind !== "genre" && Boolean(src) && !failed);
  const initials = $derived(coverInitials(title));
  const mosaic = $derived(
    kind === "genre"
      ? srcs.filter((s): s is string => Boolean(s) && !failedTiles[s as string]).slice(0, 4)
      : [],
  );

  $effect(() => {
    void src;
    failed = false;
  });
</script>

<div
  class="rk-cover rk-cover--{size} rk-cover--{kind} {className}"
  class:rounded
  class:is-placeholder={kind === "genre" ? mosaic.length === 0 : !showImage}
  role={alt ? "img" : undefined}
  aria-label={alt || undefined}
  aria-hidden={alt ? undefined : "true"}
>
  {#if kind === "genre" && mosaic.length > 0}
    <span class="mosaic mosaic--{mosaic.length}">
      {#each mosaic as tile (tile)}
        <img
          src={tile}
          alt=""
          {loading}
          decoding="async"
          onerror={() => (failedTiles = { ...failedTiles, [tile]: true })}
        />
      {/each}
    </span>
  {:else if showImage}
    <img src={src ?? ""} alt="" {loading} decoding="async" onerror={() => (failed = true)} />
  {:else if kind === "artist" && initials}
    <span class="initials">{initials}</span>
  {:else if kind === "album"}
    <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.6" />
      <circle cx="12" cy="12" r="2.4" fill="currentColor" />
      <path
        d="M12 6.2a5.8 5.8 0 0 0-5.8 5.8"
        fill="none"
        stroke="currentColor"
        stroke-width="1.4"
        stroke-linecap="round"
        opacity="0.55"
      />
    </svg>
  {:else if kind === "genre"}
    <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="3.5" y="3.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6" />
      <rect x="13.5" y="3.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6" />
      <rect x="3.5" y="13.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6" />
      <rect x="13.5" y="13.5" width="7" height="7" rx="1.5" fill="currentColor" opacity="0.55" />
    </svg>
  {:else}
    <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12 3v10.55A4 4 0 1 0 14 17V7h4V3h-6Z"
      />
    </svg>
  {/if}
</div>

<style>
  .rk-cover {
    position: relative;
    display: grid;
    place-items: center;
    flex-shrink: 0;
    overflow: hidden;
    width: 100%;
    height: 100%;
    background: var(--rk-surface-3);
    color: var(--rk-ink);
    user-select: none;
    border: 1px solid var(--rk-line);
  }

  /* Placeholder: theme-tinted gradient, glyph in a quiet tone. Flat colours on
     purpose — no filters, no shadow per tile (cheap on WebKitGTK). */
  .rk-cover.is-placeholder {
    background: linear-gradient(135deg, var(--rk-album-fb-1), var(--rk-album-fb-2)),
      var(--rk-surface-3);
    border-color: color-mix(in srgb, var(--rk-accent) 18%, var(--rk-line) 82%);
    color: color-mix(in srgb, var(--rk-ink) 62%, transparent);
  }

  .rk-cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .glyph {
    width: 42%;
    height: 42%;
    display: block;
  }

  .initials {
    font-family: var(--rk-font);
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: 1;
    color: var(--rk-ink);
    font-size: var(--cover-initials, 1.1rem);
  }

  .mosaic {
    width: 100%;
    height: 100%;
    display: grid;
    gap: 1px;
    background: var(--rk-line);
  }

  .mosaic img {
    min-width: 0;
    min-height: 0;
  }

  /* 1 cover fills; 2 side by side; 3 = one tall + two stacked; 4 = 2x2. */
  .mosaic--1 {
    grid-template: 1fr / 1fr;
  }

  .mosaic--2 {
    grid-template: 1fr / 1fr 1fr;
  }

  .mosaic--3 {
    grid-template: 1fr 1fr / 1fr 1fr;
  }

  .mosaic--3 img:first-child {
    grid-row: 1 / span 2;
  }

  .mosaic--4 {
    grid-template: 1fr 1fr / 1fr 1fr;
  }

  .rounded {
    border-radius: var(--rk-radius-cover);
  }

  .rk-cover--xs {
    width: 32px;
    height: 32px;
    --cover-initials: 0.75rem;
  }

  .rk-cover--sm {
    width: 40px;
    height: 40px;
    --cover-initials: 0.9375rem;
  }

  .rk-cover--md {
    width: 48px;
    height: 48px;
    --cover-initials: 1.0625rem;
  }

  .rk-cover--dock {
    width: 56px;
    height: 56px;
    --cover-initials: 1.25rem;
  }

  .rk-cover--tile {
    width: 4.5rem;
    height: 4.5rem;
    --cover-initials: 1.5rem;
  }

  .rk-cover--lg {
    width: 120px;
    height: 120px;
    --cover-initials: 2.5rem;
  }

  .rk-cover--xl {
    width: clamp(140px, 18vw, 200px);
    height: clamp(140px, 18vw, 200px);
    --cover-initials: 3rem;
    box-shadow: var(--rk-shadow-2);
  }
</style>
