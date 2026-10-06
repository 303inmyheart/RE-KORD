/**
 * Do the client and the hub get along?
 *
 * With 5.0 the UI always came from the hub: once the hub was updated, every
 * client updated on the next reload. The Tauri shells (desktop, Android)
 * instead bundle the UI, so a phone can fall behind
 * the hub — or the hub behind the phone. The hub declares in
 * `/api/v1/health` its version, the API version and the oldest client
 * it accepts; here we decide what to tell the user. Pure function: the tests
 * call it without a browser (`src/lib/semver.test.mjs`).
 */

import { compareSemver, diffLevel, parseSemver, satisfiesMin } from "./semver";

/** `/api/v1` API version this client speaks. */
export const SUPPORTED_API_VERSION = 1;

/** Where updated clients are downloaded from. */
export const DOWNLOAD_URL = "https://re-kord.com";

export type HealthInfo = {
  version?: unknown;
  apiVersion?: unknown;
  minClientVersion?: unknown;
};

export type CompatVerdict =
  | { kind: "ok" }
  /** The client is below the hub's minimum: many things might not work. */
  | { kind: "client-too-old"; hubVersion: string; minClientVersion: string }
  /** A newer client exists (the hub is ahead by at least one minor). */
  | { kind: "hub-newer"; hubVersion: string }
  /** The hub is behind by at least one minor: updating the hub is advisable. */
  | { kind: "hub-older"; hubVersion: string }
  /** Page served by the hub but of another version: stale cache, a reload is enough. */
  | { kind: "reload"; hubVersion: string };

export function evaluateCompat(input: {
  clientVersion: string;
  /** True in the native shells, which carry the UI with them. */
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
    // In the browser the page belongs to the hub: if the versions don't match it is the cache
    // (service worker or browser) still serving the old one.
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
