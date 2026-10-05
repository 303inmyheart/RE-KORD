import { getServerBaseUrl } from "../config";

/** Same rel_path on two hubs is not the same file: chart cache keys carry the hub. */
export function chartCacheScopePrefix(): string {
  return `${getServerBaseUrl() || "local"}|`;
}

export function chartCacheKey(relPath: string): string {
  return chartCacheScopePrefix() + relPath;
}
