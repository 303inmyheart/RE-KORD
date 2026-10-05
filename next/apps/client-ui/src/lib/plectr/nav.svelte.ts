/**
 * Deep links into Plectr from other views (Statistics › Plectr → records).
 *
 * ```ts
 * import { openPlectr } from "../lib/plectr/nav.svelte";
 * openPlectr("records");          // records table
 * openPlectr("play", track);      // pick screen with this track selected
 * ```
 */
import type { Track } from "../api";
import { session } from "../session.svelte";

export type PlectrSection = "play" | "records";

class PlectrNav {
  /** Section the Plectr view opens on (consumed on mount). */
  section = $state<PlectrSection>("play");
  /** Track to preselect on the pick screen (consumed on mount). */
  track = $state<Track | null>(null);

  take(): { section: PlectrSection; track: Track | null } {
    const out = { section: this.section, track: this.track };
    this.section = "play";
    this.track = null;
    return out;
  }
}

export const plectrNav = new PlectrNav();

export function openPlectr(section: PlectrSection = "play", track: Track | null = null): void {
  plectrNav.section = section;
  plectrNav.track = track;
  session.navigate("plectr");
}
