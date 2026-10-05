/**
 * Stato del banner di aggiornamento (`components/UpdateBanner.svelte`).
 *
 * Si alimenta da due strade: la sonda della procedura di connessione
 * (`connect.svelte.ts`, che ha gia' in mano `/api/v1/health`) e un controllo
 * leggero in background — all'avvio, al ritorno in primo piano, ogni tanto —
 * perche' l'hub puo' essere aggiornato mentre il client e' aperto.
 */

import { getServerBaseUrl } from "../config";
import { APP_VERSION } from "../version";
import { evaluateCompat, type CompatVerdict, type HealthInfo } from "./compat";
import { isBundledUi, isTauri } from "./env";

const DISMISS_KEY = "rekord.compatDismissed";
const RECHECK_MS = 30 * 60_000;
const MIN_GAP_MS = 5 * 60_000;
const TIMEOUT_MS = 5_000;

function readDismissed(): string {
  try {
    return localStorage.getItem(DISMISS_KEY) || "";
  } catch {
    return "";
  }
}

function verdictKey(v: CompatVerdict): string {
  return v.kind === "ok" ? "" : `${v.kind}@${v.hubVersion}`;
}

class CompatStore {
  verdict = $state<CompatVerdict>({ kind: "ok" });
  /** Chiave dell'avviso chiuso a mano: lo stesso avviso non torna fino al prossimo cambio. */
  dismissed = $state(readDismissed());

  get visible(): boolean {
    if (this.verdict.kind === "ok") return false;
    // L'avviso bloccante non si chiude per sempre: resta finche' non si aggiorna.
    if (this.verdict.kind === "client-too-old") return true;
    return verdictKey(this.verdict) !== this.dismissed;
  }

  apply(health: HealthInfo | null | undefined) {
    this.verdict = evaluateCompat({
      clientVersion: APP_VERSION,
      bundled: isBundledUi(),
      health,
    });
  }

  dismiss() {
    this.dismissed = verdictKey(this.verdict);
    try {
      localStorage.setItem(DISMISS_KEY, this.dismissed);
    } catch {
      /* storage pieno o negato: l'avviso torna al prossimo avvio, pazienza */
    }
  }
}

export const compat = new CompatStore();

function currentHubBase(): string | null {
  const saved = getServerBaseUrl();
  if (saved) return saved;
  // Senza indirizzo salvato, nel browser l'hub e' l'origine della pagina; nel
  // guscio nativo l'origine e' l'app stessa e non c'e' niente da chiedere.
  if (isTauri() || typeof location === "undefined") return null;
  return location.origin.startsWith("http") ? location.origin : null;
}

let lastCheck = 0;

/** Chiede `/api/v1/health` all'hub corrente e aggiorna il verdetto. Mai un errore. */
export async function checkHubCompat(base: string | null = currentHubBase()): Promise<void> {
  if (!base) return;
  lastCheck = Date.now();
  try {
    const res = await fetch(`${base.replace(/\/+$/, "")}/api/v1/health`, {
      cache: "no-store",
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!res.ok) return;
    const body = (await res.json()) as HealthInfo & { service?: string };
    if (body?.service !== "RE-KORD") return;
    compat.apply(body);
  } catch {
    /* Hub irraggiungibile: ci pensa l'indicatore di connessione, non questo banner. */
  }
}

/** Controlli periodici; torna la funzione che li ferma. */
export function watchHubCompat(): () => void {
  if (typeof window === "undefined") return () => {};
  const maybe = () => {
    if (Date.now() - lastCheck >= MIN_GAP_MS) void checkHubCompat();
  };
  const onVisible = () => {
    if (document.visibilityState === "visible") maybe();
  };
  const timer = window.setInterval(() => void checkHubCompat(), RECHECK_MS);
  document.addEventListener("visibilitychange", onVisible);
  window.addEventListener("online", maybe);
  return () => {
    window.clearInterval(timer);
    document.removeEventListener("visibilitychange", onVisible);
    window.removeEventListener("online", maybe);
  };
}
