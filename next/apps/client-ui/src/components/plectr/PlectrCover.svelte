<script lang="ts">
  /**
   * Album cover for Plectr lists and cards: the shared `CoverArt` with
   * `coverUrlFor` (no request when the hub says the album has no cover, note
   * placeholder on errors — no broken image icons).
   */
  import { CoverArt } from "@rekord/ui";
  import { coverUrlFor, type CoverEntity, type Track } from "../../lib/api";

  type CoverTrack = Pick<Track, "album_id" | "rel_path"> & Partial<Track>;

  let {
    track,
    size = 128,
    class: className = "",
  }: {
    track: CoverTrack | null | undefined;
    size?: 128 | 256;
    class?: string;
  } = $props();

  const src = $derived(track ? coverUrlFor(track as unknown as CoverEntity, size) : null);
</script>

<span class="plectr-cover {className}" aria-hidden="true">
  <CoverArt kind="track" {src} title={track?.title ?? ""} class="plectr-cover__art" />
</span>
