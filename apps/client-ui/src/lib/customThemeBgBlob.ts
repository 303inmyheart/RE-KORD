import { requestBlob } from "./api/http";
import { t } from "./i18n.svelte";

/**
 * The stored custom theme background as a Blob, read through the hub
 * transport (account header + query, timeout, error kinds).
 *
 * Not a bare `fetch(url, { credentials: "include" })`: the hub authenticates
 * by header and its CORS answer carries no `Access-Control-Allow-Credentials`,
 * so a credentialed cross-origin read (Tauri app, dev server, any client not
 * served by the hub itself) is blocked by the browser before the image arrives.
 */
export async function fetchCustomThemeBgBlob(rev?: number | null): Promise<Blob> {
  const q = rev != null && Number.isFinite(rev) ? `?v=${Math.floor(rev)}` : "";
  const { blob } = await requestBlob(`/api/v1/user-state/custom-theme-bg${q}`, {
    fallbackError: t("themePicker.customBgExtractErr"),
    fallbackName: "theme-bg",
  });
  return blob;
}
