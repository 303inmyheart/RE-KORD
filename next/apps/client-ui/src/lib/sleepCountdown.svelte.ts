/**
 * Live countdown of the player's sleep timer, shared by the dock control and
 * the Studio panel. Ticks once a second only while a timer is set; call it
 * during component initialisation (it owns an `$effect`).
 */
import { session } from "./session.svelte";
import { formatSleepRemaining } from "./sleepTimerFormat";

export type SleepCountdown = {
  readonly remainingMs: number;
  readonly active: boolean;
  /** `m:ss` / `h:mm:ss`; empty when no timer runs. */
  readonly label: string;
  /** Re-read the clock now (right after starting a timer). */
  refresh: () => void;
};

export function sleepCountdown(): SleepCountdown {
  let now = $state(Date.now());

  $effect(() => {
    if (!session.sleepTimerEndsAt) return;
    now = Date.now();
    const id = window.setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => window.clearInterval(id);
  });

  const remainingMs = $derived(
    session.sleepTimerEndsAt ? Math.max(0, session.sleepTimerEndsAt - now) : 0,
  );
  const active = $derived(Boolean(session.sleepTimerEndsAt && remainingMs > 0));
  const label = $derived(active ? formatSleepRemaining(remainingMs) : "");

  return {
    get remainingMs() {
      return remainingMs;
    },
    get active() {
      return active;
    },
    get label() {
      return label;
    },
    refresh: () => {
      now = Date.now();
    },
  };
}
