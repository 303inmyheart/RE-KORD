/**
 * Il client e l'hub vanno d'accordo?
 *
 * Con la 5.0 l'interfaccia arrivava sempre dall'hub: aggiornato l'hub, ogni
 * client si aggiornava al primo ricaricamento. I gusci Tauri (desktop, Android)
 * invece impacchettano l'interfaccia, quindi un telefono puo' restare indietro
 * rispetto all'hub — o l'hub rispetto al telefono. L'hub dichiara in
 * `/api/v1/health` la sua versione, la versione dell'API e il client piu' vecchio
 * che accetta; qui si decide che cosa dire all'utente. Funzione pura: i test la
 * chiamano senza browser (`src/lib/semver.test.mjs`).
 */

import { compareSemver, diffLevel, parseSemver, satisfiesMin } from "./semver";

/** Versione dell'API `/api/v1` che questo client sa parlare. */
export const SUPPORTED_API_VERSION = 1;

/** Dove si scaricano i client aggiornati. */
export const DOWNLOAD_URL = "https://re-kord.com";

export type HealthInfo = {
  version?: unknown;
  apiVersion?: unknown;
  minClientVersion?: unknown;
};

export type CompatVerdict =
  | { kind: "ok" }
  /** Il client e' sotto il minimo dell'hub: molte cose potrebbero non funzionare. */
  | { kind: "client-too-old"; hubVersion: string; minClientVersion: string }
  /** C'e' un client piu' nuovo (l'hub e' avanti di almeno una minor). */
  | { kind: "hub-newer"; hubVersion: string }
  /** L'hub e' indietro di almeno una minor: conviene aggiornare l'hub. */
  | { kind: "hub-older"; hubVersion: string }
  /** Pagina servita dall'hub ma di un'altra versione: cache vecchia, basta ricaricare. */
  | { kind: "reload"; hubVersion: string };

export function evaluateCompat(input: {
  clientVersion: string;
  /** Vero nei gusci nativi, che portano l'interfaccia con se'. */
  bundled: boolean;
  health: HealthInfo | null | undefined;
}): CompatVerdict {
  const health = input.health ?? {};
  const hubVersion = typeof health.version === "string" ? health.version.trim() : "";
  const minClient =
    typeof health.minClientVersion === "string" ? health.minClientVersion.trim() : "";
  const api = typeof health.apiVersion === "number" ? health.apiVersion : null;

  const apiTooNew = api !== null && api > SUPPORTED_API_VERSION;
  const belowMin = minClient !== "" && !satisfiesMin(input.clientVersion, minClient);

  if (!input.bundled) {
    // Nel browser la pagina e' dell'hub: se le versioni non tornano e' la cache
    // (service worker o browser) che serve ancora quella vecchia.
    if (apiTooNew || belowMin) return { kind: "reload", hubVersion };
    if (parseSemver(hubVersion) && compareSemver(hubVersion, input.clientVersion) === 1) {
      return { kind: "reload", hubVersion };
    }
    return { kind: "ok" };
  }

  if (apiTooNew || belowMin) {
    return {
      kind: "client-too-old",
      hubVersion,
      minClientVersion: minClient || hubVersion,
    };
  }

  const cmp = compareSemver(hubVersion, input.clientVersion);
  const level = diffLevel(hubVersion, input.clientVersion);
  const relevant = level === "major" || level === "minor";
  if (cmp === 1 && relevant) return { kind: "hub-newer", hubVersion };
  if (cmp === -1 && relevant) return { kind: "hub-older", hubVersion };
  return { kind: "ok" };
}
