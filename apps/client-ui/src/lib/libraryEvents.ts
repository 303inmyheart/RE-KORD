/**
 * "Library changed" events: a track or an album was edited (title, genre,
 * lyrics, album name, cover…).
 *
 * The session applies every change itself (`session.applyLibraryChange`); this
 * module only carries it further:
 * - to the other tabs of this client (BroadcastChannel), which apply the same
 *   patch so their player bar and lists follow without a reload;
 * - to anything in this tab that keeps its own copy of library data, through a
 *   `rekord:library-changed` window event.
 *
 * The hub has no push channel for library edits: other clients pick them up
 * from the catalog delta (`/library/changes`) on their next refresh.
 */
import type { Album, Track } from "./api";

export type LibraryChange = {
  /** Track fields by `rel_path` (only the fields that changed). */
  tracks?: Array<Partial<Track> & { rel_path: string }>;
  /** Album fields by `id`; `name` / `cover_version` / `has_cover` reach its tracks. */
  albums?: Array<Partial<Album> & { id: number }>;
};

export const LIBRARY_CHANGED_EVENT = "rekord:library-changed";
const CHANNEL = "rekord.library";

let channel: BroadcastChannel | null | undefined;

function getChannel(): BroadcastChannel | null {
  if (channel !== undefined) return channel;
  channel =
    typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel(CHANNEL);
  return channel;
}

/** Plain JSON copy: what crosses tabs must be cloneable (no `$state` proxies). */
function plain(change: LibraryChange): LibraryChange {
  return JSON.parse(JSON.stringify(change)) as LibraryChange;
}

export function broadcastLibraryChange(change: LibraryChange) {
  const data = plain(change);
  try {
    getChannel()?.postMessage(data);
  } catch {
    /* closed channel / exotic WebView: other tabs catch up on refresh */
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent<LibraryChange>(LIBRARY_CHANGED_EVENT, { detail: data }));
  }
}

/** Changes made in other tabs of this client. */
export function watchLibraryChanges(fn: (change: LibraryChange) => void): () => void {
  const ch = getChannel();
  if (!ch) return () => {};
  const onMessage = (e: MessageEvent) => {
    const data = e.data as LibraryChange | null;
    if (data && (Array.isArray(data.tracks) || Array.isArray(data.albums))) fn(data);
  };
  ch.addEventListener("message", onMessage);
  return () => ch.removeEventListener("message", onMessage);
}
