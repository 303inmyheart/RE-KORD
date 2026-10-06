# Module: studio

yt-dlp downloads, web Discover (YouTube Music), metadata (Discogs/iTunes/MB/…), cover art and related settings.

Enabled by default in the next hub: the Studio routes are always registered in `rekord-core` (`studio.rs`). The `studio` flag in the manifest remains informational for the module registry.

## Runtime

- `yt-dlp` binary on the PATH or `YTDLP_PATH`
- Optional Netscape cookies: Settings → Library, or env `REKORD_YTDLP_COOKIES`
- Optional Discogs token: Settings or env `REKORD_DISCOGS_TOKEN`
- Disable downloads: `ENABLE_YTDLP=0`
