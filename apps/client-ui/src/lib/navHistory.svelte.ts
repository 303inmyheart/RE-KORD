/**
 * In-app navigation ↔ browser history.
 *
 * The client has no URL routing: the visible section lives in `session.view`
 * (plus the library drill-down level). On Android the hardware Back key only
 * does something useful when there are history entries to go back to, so every
 * section change leaves one, and Back / popstate walks the app back through
 * them. Overlays (Modal, sheets) sit on top through `pushBackLayer` from
 * `@rekord/ui`, which owns `popstate` and hands us only the pops they don't
 * claim.
 *
 * Loop guards: we only push when the visible location differs from the entry
 * we're on; a pop first records the target as "current", so the change it
 * causes is not pushed again; an in-app "back" to the previous location steps
 * history back silently instead of piling up entries.
 */

import { untrack } from "svelte";
import {
  enableBackStack,
  historyBackSilently,
  onHistoryPop,
  pushHistoryEntry,
  replaceHistoryEntry,
} from "@rekord/ui";
import { session, type ViewId } from "./session.svelte";

const VIEWS: ReadonlySet<string> = new Set<ViewId>([
  "dashboard",
  "studio",
  "library",
  "plectr",
  "favorites",
  "playlists",
  "queue",
  "recent",
  "statistics",
  "achievements",
  "settings",
  "podcasts",
]);

function currentKey(): string {
  return session.view === "library" ? `library:${session.libraryLevel}` : session.view;
}

function entry(key: string, idx: number): Record<string, unknown> {
  const prev = history.state && typeof history.state === "object" ? history.state : {};
  return { ...prev, rkNav: key, rkIdx: idx, rkLayer: undefined };
}

/** Call once from the shell's script (needs an effect context). */
export function bindNavHistory(): () => void {
  const teardown = enableBackStack();

  /** Keys by history index, as far as this page load knows them. */
  const keys: string[] = [];
  const initialState = history.state as Record<string, unknown> | null;
  let idx = typeof initialState?.rkIdx === "number" ? initialState.rkIdx : 0;
  keys[idx] = currentKey();
  replaceHistoryEntry(entry(keys[idx]!, idx));

  function apply(target: string, forward: boolean) {
    const [view, level] = target.split(":");
    if (!view || !VIEWS.has(view)) return;
    if (view === "library" && session.view === "library") {
      // Library drill-down: only "up" can be rebuilt without the item ids.
      if (!forward && level !== session.libraryLevel) void session.backLibrary();
      return;
    }
    if (session.view !== view) session.navigate(view as ViewId);
  }

  const offPop = onHistoryPop((state) => {
    const target = typeof state?.rkNav === "string" ? state.rkNav : null;
    if (!target) return;
    const nextIdx = typeof state?.rkIdx === "number" ? state.rkIdx : 0;
    const forward = nextIdx > idx;
    idx = nextIdx;
    keys[idx] = target;
    if (target !== currentKey()) apply(target, forward);
    const now = currentKey();
    if (now !== target) {
      // Couldn't land exactly there: describe where we really are.
      keys[idx] = now;
      replaceHistoryEntry(entry(now, idx));
    }
  });

  $effect(() => {
    const key = currentKey();
    untrack(() => {
      if (keys[idx] === key) return;
      if (idx > 0 && keys[idx - 1] === key) {
        // In-app "back" (library ‹, same place as before): reuse the entry.
        idx -= 1;
        historyBackSilently();
        return;
      }
      idx += 1;
      keys.length = idx;
      keys[idx] = key;
      pushHistoryEntry(entry(key, idx));
    });
  });

  return () => {
    offPop();
    teardown();
  };
}
