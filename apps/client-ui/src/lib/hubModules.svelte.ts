/**
 * Optional modules the connected hub has switched on (`/api/v1/health` →
 * `modules`). Learned from the health answers the app already asks for (the
 * connect probe, every refresh): no request of its own, no polling. A module
 * that is off leaves no trace in the UI: no nav entry, no card, no chunk.
 */

class HubModules {
  /** Null until the hub answered once. */
  list = $state<readonly string[] | null>(null);

  /** "Podcast e notizie" is on. */
  get podcasts(): boolean {
    return this.list?.includes("podcasts") ?? false;
  }

  /** Take the `modules` of a health answer (ignored when absent or malformed). */
  apply(health: unknown): void {
    if (!health || typeof health !== "object") return;
    const raw = (health as { modules?: unknown }).modules;
    if (!Array.isArray(raw)) return;
    const next = raw.filter((m): m is string => typeof m === "string").sort();
    const cur = this.list;
    if (cur && cur.length === next.length && cur.every((m, i) => m === next[i])) return;
    this.list = next;
  }

  /** Another hub: forget what the previous one offered. */
  reset(): void {
    this.list = null;
  }
}

export const hubModules = new HubModules();
