/**
 * Map a failed hub response to a locale key. The hub's own error sentences
 * are Italian-only or bare codes, so the panel recognises the ones it can
 * explain and falls back to the raw text otherwise. No Svelte / DOM here:
 * the node tests import this file directly.
 */

export type HubErrorKey = { key: string; vars?: Record<string, string | number> };

/** Statuses a reverse proxy (Vite dev, Cloudflare) answers for a hub that is down. */
export function isGatewayStatus(status: number): boolean {
  return status === 500 || status === 502 || status === 503 || status === 504;
}

/** True for a bare machine code such as `invalid_account_id`. */
function isBareCode(text: string): boolean {
  return /^[a-z0-9_]+$/.test(text);
}

/**
 * @param status HTTP status of the response
 * @param code   `error` field of the hub envelope, if any
 * @param hasBody whether a JSON body was read at all
 */
export function hubErrorKey(
  status: number,
  code: string | null,
  hasBody: boolean,
): HubErrorKey | null {
  const text = (code ?? "").toLowerCase();

  if (status === 403) {
    if (text.includes("default")) return { key: "errors.forbiddenDefault" };
    if (
      text.includes("computer dell'hub") ||
      text.includes("hub's computer") ||
      text.includes("remot") ||
      text.includes("machine") ||
      text.includes("macchina")
    ) {
      return { key: "errors.forbiddenRemote" };
    }
    if (code && !isBareCode(code)) return { key: "errors.forbiddenDetail", vars: { detail: code } };
    return { key: "errors.forbidden" };
  }

  if (!hasBody && isGatewayStatus(status)) {
    return { key: "errors.unreachableStatus", vars: { status } };
  }
  if (!hasBody && status === 404) return { key: "errors.notAvailable" };

  if (text.includes("already in progress")) return { key: "errors.scanBusy" };
  if (text.includes("music_root not set")) return { key: "errors.musicRootNotSet" };
  if (text.includes("music root is not a directory")) return { key: "errors.musicRootMissing" };
  if (text === "legacy_data_not_found") return { key: "errors.legacyDataNotFound" };
  if (text === "account not found" || text === "account_not_found") {
    return { key: "errors.accountNotFound" };
  }
  if (status === 413) return { key: "errors.tooLarge" };

  return null;
}
