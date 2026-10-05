/**
 * Studio failures in the user's language.
 *
 * The hub answers with stable codes (`album_not_found`, `no_audio_format`, …);
 * older hubs send English (or Italian) sentences instead. Both map to the same
 * strings here, so no raw hub text, browser media error or absolute path ever
 * reaches the UI.
 */
import { ApiError } from "../api/http";
import { t } from "../i18n.svelte";
import { redactPaths } from "./ytdlpLogFilter";

/** Code → `locales/studio` key. */
const CODE_KEYS: Record<string, string> = {
  // Library / lookups
  album_not_found: "studio.err.albumNotFound",
  artist_not_found: "studio.err.artistNotFound",
  track_not_found: "studio.err.trackNotFound",
  not_found: "studio.err.notFound",
  no_metadata_found: "studio.err.noMetadata",
  no_match: "studio.err.noMatch",
  no_tracks_found: "studio.preview.noTracks",
  query_too_short: "studio.err.queryTooShort",
  music_root_not_set: "studio.err.musicRootMissing",
  invalid_output_dir: "studio.err.invalidOutputDir",
  invalid_output_directory: "studio.err.invalidOutputDir",
  invalid_path: "studio.err.invalidPath",
  invalid_album_path: "studio.err.invalidPath",
  // URLs
  url_host_not_allowed: "studio.err.urlHostNotAllowed",
  url_not_allowed: "studio.err.urlHostNotAllowed",
  invalid_url: "studio.err.invalidUrl",
  invalid_catalog_url: "studio.err.invalidUrl",
  invalid_releases_url: "studio.dl.errReleasesUrl",
  // Providers
  discogs_rate_limited: "studio.err.discogsRateLimited",
  discogs_not_configured: "studio.err.discogsNotConfigured",
  discogs_unauthorized: "studio.err.discogsUnauthorized",
  discogs_token_invalid: "studio.err.discogsUnauthorized",
  discogs_release_mismatch: "studio.err.discogsMismatch",
  rate_limited: "studio.err.rateLimited",
  upstream_rate_limited: "studio.err.rateLimited",
  upstream_timeout: "studio.err.upstreamTimeout",
  upstream_unavailable: "studio.err.upstream",
  upstream_rejected: "studio.err.upstreamRejected",
  upstream_invalid: "studio.err.upstream",
  upstream_failed: "studio.err.upstream",
  upstream_error: "studio.err.upstream",
  network_error: "studio.err.network",
  // Permissions
  forbidden_remote: "studio.err.forbiddenRemote",
  forbidden_default_account: "studio.err.forbiddenDefault",
  default_account_required: "studio.err.forbiddenDefault",
  // yt-dlp and downloads
  no_audio_format: "studio.err.noAudioFormat",
  http_forbidden: "studio.err.ytdlp403",
  http_403: "studio.err.ytdlp403",
  private_video: "studio.err.privateVideo",
  age_restricted: "studio.err.ageRestricted",
  members_only: "studio.err.membersOnly",
  geo_blocked: "studio.err.geoBlocked",
  sign_in_required: "studio.err.signIn",
  video_unavailable: "studio.err.videoUnavailable",
  postprocess_failed: "studio.err.postprocess",
  already_downloaded: "studio.dl.sum.reasonAlready",
  client_disconnected: "studio.err.clientDisconnected",
  cancelled: "studio.dl.logCancelled",
  ytdlp_disabled: "studio.err.ytdlpDisabled",
  ytdlp_not_found: "studio.err.ytdlpMissing",
  ytdlp_missing: "studio.err.ytdlpMissing",
  ytdlp_timeout: "studio.err.ytdlpTimeout",
  ytdlp_spawn_failed: "studio.err.ytdlpMissing",
  ytdlp_failed: "studio.err.ytdlpFailed",
  download_failed: "studio.err.ytdlpFailed",
  download_id_active: "studio.err.downloadActive",
  download_not_found: "studio.err.downloadNotFound",
  flat_count_failed: "studio.err.countFailed",
  youtube_search_failed: "studio.err.youtubeSearch",
  youtube_releases_failed: "studio.err.youtubeReleases",
  // yt-dlp update
  ytdlp_update_in_progress: "studio.err.ytdlpUpdateBusy",
  ytdlp_platform_unsupported: "studio.err.ytdlpNotUpdatable",
  ytdlp_release_lookup_failed: "studio.err.ytdlpUpdateFailed",
  ytdlp_asset_missing: "studio.err.ytdlpUpdateFailed",
  ytdlp_download_failed: "studio.err.ytdlpUpdateFailed",
  ytdlp_checksum_missing: "studio.err.ytdlpUpdateFailed",
  ytdlp_checksum_mismatch: "studio.err.ytdlpUpdateFailed",
  ytdlp_install_failed: "studio.err.ytdlpUpdateFailed",
  // Preview
  preview_expired: "studio.err.previewExpired",
  preview_timeout: "studio.preview.unavailable",
  preview_upstream_forbidden: "studio.err.ytdlp403",
  preview_upstream_unreachable: "studio.err.upstream",
  preview_resolve_failed: "studio.preview.unavailable",
  preview_failed: "studio.preview.unavailable",
  preview_unavailable: "studio.preview.unavailable",
  // Images
  image_too_large: "studio.err.imageTooLarge",
  file_too_large: "studio.err.imageTooLarge",
  image_invalid: "studio.err.unsupportedImage",
  unsupported_image: "studio.err.unsupportedImage",
  image_fetch_failed: "studio.err.imageFetch",
  file_required: "studio.err.unsupportedImage",
  // Operations without a more precise cause
  album_info_fetch_failed: "studio.err.metaFetch",
  track_info_fetch_failed: "studio.err.metaFetch",
  discogs_search_failed: "studio.err.metaFetch",
  discogs_apply_failed: "studio.err.saveFailed",
  album_info_save_failed: "studio.err.saveFailed",
  track_info_save_failed: "studio.err.saveFailed",
  artwork_apply_failed: "studio.err.saveFailed",
  artwork_upload_failed: "studio.err.saveFailed",
  upload_failed: "studio.err.saveFailed",
  artwork_search_failed: "studio.err.metaFetch",
  entity_info_search_failed: "studio.err.metaFetch",
  entity_info_batch_failed: "studio.err.metaFetch",
  entity_info_save_failed: "studio.err.saveFailed",
  sanitize_failed: "studio.err.saveFailed",
  prune_failed: "studio.err.saveFailed",
  mkdir_failed: "studio.err.mkdirFailed",
  fs_list_failed: "studio.err.fsFailed",
  fs_search_failed: "studio.err.fsFailed",
  discover_failed: "studio.err.discoverFeed",
  feed_unavailable: "studio.err.discoverFeed",
};

/** Legacy hub sentences (no code yet) → code. Order matters: first match wins. */
const TEXT_CODES: Array<[RegExp, string]> = [
  [/album not found/i, "album_not_found"],
  [/artist not found/i, "artist_not_found"],
  [/track not found/i, "track_not_found"],
  [/no (metadata|results?) found/i, "no_metadata_found"],
  [/query too short/i, "query_too_short"],
  [/url host not allowed|host not allowed/i, "url_host_not_allowed"],
  [/discogs.*(rate.?limit|429)|rate.?limit.*discogs/i, "discogs_rate_limited"],
  [/discogs.*(token|not configured)/i, "discogs_not_configured"],
  [/rate.?limit|too many requests|\b429\b/i, "rate_limited"],
  [/requested format is not available|no audio format/i, "no_audio_format"],
  [/http error 403|403: forbidden/i, "http_forbidden"],
  [/yt-dlp disabled|ENABLE_YTDLP=0/i, "ytdlp_disabled"],
  [/music_?root (not set|missing|not configured)/i, "music_root_not_set"],
  [/invalid output directory/i, "invalid_output_directory"],
  [/invalid path/i, "invalid_path"],
  [/downloadId already active/i, "download_id_active"],
  [/preview expired/i, "preview_expired"],
  [/Solo l'account Default|only the default account/i, "forbidden_default_account"],
  [/computer dell'hub|only from the hub|remote admin/i, "forbidden_remote"],
];

/** Stable code for an error, whatever the hub version said. */
export function studioErrorCode(e: unknown): string | null {
  if (e instanceof ApiError) {
    const code = (e.code ?? "").trim();
    if (code && /^[a-z0-9_]+$/.test(code)) return code;
    const text = `${code} ${e.message}`;
    for (const [re, c] of TEXT_CODES) if (re.test(text)) return c;
    return null;
  }
  const text = e instanceof Error ? e.message : typeof e === "string" ? e : "";
  for (const [re, c] of TEXT_CODES) if (re.test(text)) return c;
  return null;
}

/** Message for a bare code (from a stream event or a per-source report). */
export function studioCodeText(code: string | null | undefined, fallback?: string | null): string {
  const key = code ? CODE_KEYS[code] : undefined;
  if (key) return t(key);
  if (fallback?.trim()) {
    for (const [re, c] of TEXT_CODES) {
      if (re.test(fallback)) return t(CODE_KEYS[c] ?? "studio.err.generic");
    }
    return redactPaths(fallback.trim());
  }
  return code ? t("studio.err.code", { code }) : t("studio.err.generic");
}

/** One-line, translated description of any Studio failure. */
export function studioErrorText(e: unknown): string {
  // <audio> / media failures: never show the browser's English sentence.
  if (typeof DOMException !== "undefined" && e instanceof DOMException) {
    if (e.name === "NotSupportedError" || e.name === "NotAllowedError") {
      return t(e.name === "NotAllowedError" ? "studio.err.autoplayBlocked" : "studio.preview.unavailable");
    }
  }
  if (e instanceof ApiError) {
    if (e.kind !== "http") return e.message; // network/timeout: already translated
    const code = studioErrorCode(e);
    if (code && CODE_KEYS[code]) return t(CODE_KEYS[code]);
    if (e.status === 403) return e.message; // shared 403 translation (http.ts)
    if (e.status === 502 || e.status === 504) return t("studio.err.upstream");
    if (e.code && /^[a-z0-9_]+$/.test(e.code)) return t("studio.err.code", { code: e.code });
    return redactPaths(e.message || t("studio.err.generic"));
  }
  const code = studioErrorCode(e);
  if (code && CODE_KEYS[code]) return t(CODE_KEYS[code]);
  const msg = e instanceof Error ? e.message : String(e ?? "");
  return redactPaths(msg || t("studio.err.generic"));
}
