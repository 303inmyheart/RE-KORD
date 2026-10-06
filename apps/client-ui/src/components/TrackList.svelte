<script lang="ts">
  import { EmptyState, type SelectOption } from "@rekord/ui";
  import type { Track } from "../lib/api";
  import { dragReorder } from "../lib/dragReorder";
  import { t } from "../lib/i18n.svelte";
  import { player } from "../lib/player";
  import { session } from "../lib/session.svelte";
  import { prefsRevision } from "../lib/prefsRevision.svelte";
  import { trackRowStats } from "../lib/trackRowStats.svelte";
  import { virtualList, type VirtualWindow } from "../lib/virtualList";
  import TrackRow from "./TrackRow.svelte";

  const VIRTUAL_FROM = 40;

  let {
    tracks = [],
    favoriteIds = new Set<number>(),
    playlistOptions = [],
    activeTrackId = null as number | null,
    emptyMessage,
    empty,
    numbered = false,
    coverFor,
    showQueueActions = true,
    showPlaylistAction = true,
    showExclude = true,
    onplay,
    ontoggleFavorite,
    onaddToPlaylist,
    onremove,
    onedit,
    onreorder,
  }: {
    tracks?: Track[];
    favoriteIds?: Set<number>;
    playlistOptions?: SelectOption[];
    activeTrackId?: number | null;
    emptyMessage?: string;
    /** Rich empty state (icon, title, CTA); wins over `emptyMessage`. */
    empty?: import("svelte").Snippet;
    /** Album context: rows show the track number instead of the cover. */
    numbered?: boolean;
    /** Cover URL per track; return null when the hub has none (no request). */
    coverFor?: (track: Track) => string | null;
    showQueueActions?: boolean;
    showPlaylistAction?: boolean;
    showExclude?: boolean;
    onplay: (track: Track, list: Track[]) => void;
    ontoggleFavorite: (track: Track) => void;
    onaddToPlaylist?: (playlistId: string, track: Track) => void;
    onremove?: (track: Track) => void;
    onedit?: (track: Track) => void;
    /** Enables drag reordering; indexes refer to `tracks`. */
    onreorder?: (from: number, to: number) => void;
  } = $props();

  /** First window before the action measures anything: about one viewport. */
  let win = $state<VirtualWindow>({
    start: 0,
    end: VIRTUAL_FROM,
    padTop: 0,
    padBottom: 0,
    rowPx: 0,
  });
  /** Rows are recycled on scroll, so the virtualizer stands still while dragging. */
  let dragging = $state(false);

  const virtualized = $derived(tracks.length >= VIRTUAL_FROM);
  /** Album context with more than one disc: rows show "disc·track". */
  const multiDisc = $derived(numbered && tracks.some((t) => (t.disc_number ?? 1) > 1));

  /*
   * Per-row numbers come from one snapshot shared by the whole list, re-read
   * only when play counts / moods / exclusions change (prefsRevision) — not on
   * every player tick, and never one prefs parse per row.
   */
  const counts = $derived(prefsRevision.playCountsMap);
  const moodsMap = $derived(prefsRevision.trackMoodsMap);

  function rowStats(track: Track) {
    return {
      plays: trackRowStats.playsIn(counts, track),
      moods: trackRowStats.moodsFrom(moodsMap, track),
      inQueue: trackRowStats.inQueue(track.id),
      excluded: trackRowStats.excluded(track),
      albumLocked: trackRowStats.albumLocked(track),
    };
  }
  const windowTracks = $derived(
    virtualized ? tracks.slice(win.start, win.end) : tracks,
  );
</script>

{#if tracks.length === 0}
  {#if empty}
    {@render empty()}
  {:else}
    <EmptyState variant="inline" message={emptyMessage ?? t("trackList.empty")} />
  {/if}
{:else}
  <ul
    class="list"
    style:padding-top={virtualized ? `${win.padTop}px` : null}
    style:padding-bottom={virtualized ? `${win.padBottom}px` : null}
    use:dragReorder={{
      onmove: (from, to) => onreorder?.(from, to),
      enabled: Boolean(onreorder) && tracks.length > 1,
      ondragstate: (active) => (dragging = active),
    }}
    use:virtualList={{
      count: tracks.length,
      threshold: VIRTUAL_FROM,
      frozen: dragging,
      onwindow: (next) => (win = next),
    }}
  >
    {#each windowTracks as track, offset (track.id + "-" + (virtualized ? win.start + offset : offset))}
      {@const i = virtualized ? win.start + offset : offset}
      {@const stats = rowStats(track)}
      <TrackRow
        {track}
        index={i}
        plays={stats.plays}
        moods={stats.moods}
        inQueue={stats.inQueue}
        excluded={stats.excluded}
        albumLocked={stats.albumLocked}
        number={numbered ? track.track_number : null}
        disc={multiDisc ? (track.disc_number ?? 1) : null}
        coverSrc={coverFor ? coverFor(track) : undefined}
        reorderIndex={onreorder ? i : null}
        onreorderStep={
          onreorder
            ? (delta) =>
                onreorder(i, Math.max(0, Math.min(tracks.length - 1, i + delta)))
            : undefined
        }
        autoFocusActive={!virtualized}
        favorited={favoriteIds.has(track.id)}
        active={activeTrackId === track.id || session.current?.id === track.id}
        {playlistOptions}
        {showQueueActions}
        {showPlaylistAction}
        onplay={() => onplay(track, tracks)}
        ontoggleFavorite={() => ontoggleFavorite(track)}
        onaddToPlaylist={
          onaddToPlaylist ? (playlistId) => onaddToPlaylist(playlistId, track) : undefined
        }
        onaddToQueue={showQueueActions ? () => player.addToQueue(track) : undefined}
        onremoveFromQueue={
          showQueueActions ? () => player.removeFromQueueById(track.id) : undefined
        }
        ontoggleExclude={showExclude ? () => player.toggleExcludeTrack(track) : undefined}
        onremove={onremove ? () => onremove(track) : undefined}
        onedit={onedit ? () => onedit(track) : () => session.openTrackEdit(track)}
      />
    {/each}
  </ul>
{/if}

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    /* React parity: .list-stack { gap: var(--space-4) } = 0.875rem */
    gap: var(--rk-space-lg);
    /* Rows pick inline actions vs overflow menu from this width (track-row.css). */
    container: track-list / inline-size;
  }
</style>
