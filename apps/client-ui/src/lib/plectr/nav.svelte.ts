/**
 * Deep links into Plectr from other views (Statistics › Plectr → records).
 *
 * ```ts
 * import { openPlectr } from "../lib/plectr/nav.svelte";
 * openPlectr("records");          // records table
 * ```
 *
 * There is no track picker: the game runs on what the player holds, the
 * track is chosen in the library like everywhere else.
 */
import { session } from "../session.svelte";

export type PlectrSection = "play" | "records";

class PlectrNav {
  /** Section the Plectr view opens on (consumed on mount). */
  section = $state<PlectrSection>("play");

  take(): PlectrSection {
    const out = this.section;
    this.section = "play";
    return out;
  }
}

export const plectrNav = new PlectrNav();

export function openPlectr(section: PlectrSection = "play"): void {
  plectrNav.section = section;
  session.navigate("plectr");
}
