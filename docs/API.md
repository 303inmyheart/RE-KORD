# RE-KORD API (`/api/v1`)

Default base URL: `http://127.0.0.1:7420`. The hub listens on `0.0.0.0:7420`, so the same API is reachable on the LAN and through the remote-access tunnel.

Envelope (JSON):

```json
{ "ok": true, "data": {} }
{ "ok": false, "error": "snake_case_code", "message": "optional English detail" }
```

`error` is a stable code the client translates (see [Error codes](#error-codes)); `message`, when present, is an untranslated detail for logs and tooltips.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/health` | Service health → `{ ok, service: "RE-KORD", version, apiVersion: 1, minClientVersion: "5.0.0", transcode, modules, scanning }` (see [Versioning](#versioning-and-client-compatibility)) |
| GET | `/api/v1/library` | List tracks (`limit`, `offset`) |
| GET | `/api/v1/library/stats` | Counts for the requesting account's library selection (`track_count`, `album_count`, `artist_count`, `albums_without_cover`, `albums_without_meta`, `tracks_without_meta`, `loose_album_count`, `genre_count`), `catalog_track_count` (whole catalog), `index_epoch` (bumped by every scan / folder rescan), last scan, optional `disk_total_bytes` / `disk_available_bytes` |
| GET | `/api/v1/library/search?q=` | Full-text track search (FTS over title, artist, album, genre; accent-insensitive; every word is a prefix; file paths and track-number prefixes never match). `limit` default 100, max 500. `scope=all` → `{ tracks, albums, artists }`: albums / artists matching by name (album title, folder, artist) or by genre of the album or its tracks. Without `scope` the bare track list (compat) |
| GET | `/api/v1/library/genres` | Canonical genres of the account's library → `[{ key, label, track_count, album_count, artist_count }]`, most common first |
| GET | `/api/v1/library/albums` | List albums |
| GET | `/api/v1/library/albums/{id}` | Album by id |
| GET | `/api/v1/library/albums/{id}/tracks` | Album tracks |
| GET | `/api/v1/library/artists` | List artists |
| GET | `/api/v1/library/artists/{id}` | Artist by id |
| GET | `/api/v1/library/artists/{id}/albums` | Albums of artist |
| GET | `/api/v1/library/tracks/{id}` | Track by id |
| GET/PUT | `/api/v1/library/path` | Music root path |
| POST | `/api/v1/library/scan?mode=incremental\|full` | Scan library (default incremental: upsert by path, skip unchanged size+mtime, prune missing files/albums/artists; `full` re-reads every tag). Curated values (see below) are never replaced by tags. After the scan: the one-time legacy import when pending (see [Legacy import](#legacy-import)), else sidecar values for fields nobody curated yet. Personal data is never re-imported by scans. Report includes `indexEpoch` |
| POST | `/api/v1/library/sync-legacy-meta` | Explicit merge: curated metadata from `music_root/.kord/rekord.db` (wins over tags, never over what a person typed in RE-KORD 5) then sidecars; merge personal moods/excludes/settings/playCounts/recent, Plectr records, favorites, playlists (matched by legacy playlist id), library selection + accounts registry from `.kord` into what the hub already has (nothing on the hub is deleted) |
| GET | `/api/v1/library/tracks-page` | Paginated personal library (`limit`, `offset`) → `{ items, total, revision }` |
| GET | `/api/v1/library/artists-page` | Paginated artists (`limit`, `offset`) → `{ items, total }` |
| GET | `/api/v1/library/changes?revision=` | Delta since a revision: `{ revision, updated, removed, full }` (`full: true` → page again) |
| POST | `/api/v1/library/probe` | Analyse folder structure and suggest a layout |
| GET/PUT | `/api/v1/library/layout` | Read / write `music_root/.kord/library-layout.json` |
| GET/PUT | `/api/v1/library/watch` | Filesystem watcher status / enable-disable (debounced incremental scan) |
| POST | `/api/v1/library/thumbnails` | Start the cover thumbnail backfill as a job |
| GET/DELETE | `/api/v1/jobs` | Background jobs (scan, thumbnails, restore, sync-legacy) with progress / drop finished entries |
| GET | `/api/v1/jobs/{id}` | One job → `{ "ok": true, "data": Job }`, 404 when unknown |
| POST | `/api/v1/jobs/{id}/cancel` | Cancel a cancellable job |
| GET/DELETE | `/api/v1/diagnostics/errors` | Recent WARN/ERROR ring buffer (`limit`) / clear it |
| GET | `/api/v1/network/public-ip` | Public IP (best effort, `null` when offline) |
| GET/PUT | `/api/v1/system/machine-access` | Machine-operation rights; `PUT { "enabled" }` toggles remote admin (local only) |
| GET/POST | `/api/v1/favorites` | List / add favorite (`{ "track_id" }`) — per account |
| DELETE | `/api/v1/favorites/{id}` | Remove favorite — per account |
| GET/POST | `/api/v1/playlists` | List / create (`{ "name" }`) — per account |
| GET/PUT/DELETE | `/api/v1/playlists/{id}` | Tracks / rename / delete — per account |
| POST | `/api/v1/playlists/{id}/tracks` | Add track (`{ "track_id" }`), appended last |
| PUT | `/api/v1/playlists/{id}/tracks` | Reorder (`{ "trackIds": [...] }`); 409 unless the ids are exactly the current ones |
| DELETE | `/api/v1/playlists/{id}/tracks?track_id=` | Remove track |
| GET/PATCH | `/api/v1/my-library-selection` | Per-account library selection |
| GET | `/api/v1/catalog` | Global FS catalog (unfiltered) |
| GET/POST | `/api/v1/accounts` | List / create local accounts (alias `/api/accounts`) |
| PUT/DELETE | `/api/v1/accounts/{id}` | Rename / delete (default `default` locked) |
| GET | `/api/v1/accounts/{id}/export` | Export profile ZIP (selection + favorites + playlists) |
| GET | `/api/v1/modules` | Optional module registry |
| GET | `/api/v1/covers/album/{id}` | Album cover image (folder cover.jpg…, else the legacy `.kord/artwork` registry). `?size=128\|256` serves a cached thumbnail; `?v=<cover_version>` is ignored server-side (cache busting) and makes the response `immutable` for a year, otherwise `max-age=300`. `ETag` + `If-None-Match` → 304. Missing cover → 404 with `Cache-Control: public, max-age=3600` |
| GET | `/api/v1/covers/artist/{id}` | Artist cover (first album with cover), same `?size=` |
| GET | `/api/v1/backup/kord-data` | Download hub backup ZIP (`kordBackup: 3`) |
| POST | `/api/v1/backup/kord-restore` | Restore ZIP (`multipart` field `file`; v2/v3; body limit 512 MiB) |
| GET/POST/DELETE | `/api/v1/config` (+ youtube-cookies, discogs-token) | Hub settings / Studio integrations |
| GET/POST | `/api/v1/fs/list`, `mkdir`, `search-dirs` | Local folders under music_root |
| POST | `/api/v1/fs/delete-audio-relpaths` | Delete audio files (`{ "relPaths": [...] }`) → `{ deleted, skipped, affectedAlbums }`. Only plain audio files inside the library; a path that resolves elsewhere (`..`, symlink) or is already gone lands in `skipped` |
| POST | `/api/v1/fs/delete-album-folder` | Delete an album folder whole (`{ "albumPath" }`, at least `Artist/Album` deep and holding audio) → `{ deleted, deletedFolder, affectedAlbums }` |
| POST | `/api/v1/download` | yt-dlp NDJSON stream (`downloadId`, `url`, `downloadKind`, `outputDir`, `background`) — see [Downloads](#downloads) |
| GET | `/api/v1/download/active` | Running and recently finished downloads (`progress`, `logTail`, `canCancel`, `done`); `?downloadId=…&stream=1` re-attaches an NDJSON stream |
| POST | `/api/v1/download-cancel` | Cancel (`{ downloadId }`) → `{ found, status }` |
| POST | `/api/v1/download-flat-count` | Playlist size → `{ count, known }`; `count: null, known: false` (plus `error`) when yt-dlp cannot tell — never 0 |
| POST | `/api/v1/tools/ytdlp/update` | Install the latest official yt-dlp (machine operation) — see [Tools](#tools-yt-dlp-ffmpeg-cloudflared) |
| POST | `/api/v1/youtube-explore-search`, `youtube-releases-list` | YT Music explore / releases |
| GET | `/api/v1/catalog-web-discover` | New releases not in the library (Innertube): random 36 albums + 36 singles; `hl` / `gl` (or `locale`, else `Accept-Language`, else `it`/`IT`). Singles are recovered from the albums feed (`Single • …` items) when the singles feed fails (`singlesRecovered: true`). Feed failures are listed in `errors: [{ feed, code, message }]`; `error` is set only when a section could not be filled. "Already in library" = exact accent/case-insensitive artist + album match |
| GET | `/api/v1/catalog-web-tracks` | Track list of a release page (`url`): Innertube browse, yt-dlp flat playlist as fallback |
| GET | `/api/v1/catalog-web-preview` | Resolve a ~30s audio-only audition for a watch `url` → `{ playUrl, expiresInSecs }` (token valid **300 s**; 403 `ytdlp_disabled` with `ENABLE_YTDLP=0`; 422 `no_audio_format` / `video_unavailable` / `preview_resolve_failed`, 503 `ytdlp_not_found`, 504 `preview_timeout`) |
| GET | `/api/v1/catalog-web-preview/stream` | Proxy the audition audio for a token (`t`); honours `Range`, else serves the first `REKORD_PREVIEW_INITIAL_RANGE_BYTES` (default 512 KiB). Upstream failures never pass through as empty bodies: 410 `preview_expired` (unknown / expired token, dead upstream URL), 502 `preview_upstream_forbidden` (upstream 403), 502 `preview_upstream_error`, 429 `upstream_rate_limited` |
| GET/POST | `/api/v1/artwork/search`, `apply`, `upload` | Cover search → `{ results, errors }` (`errors`: sources that failed, results partial) / apply / upload → `{ saved, albumPath, coverRelPath, coverVersion }` |
| POST | `/api/v1/album-info/fetch` | `{ albumPath \| albumId, artist?, album?, overwriteTitle? }` (`overwriteTitle`: replace the album name with the fetched title, never over a name a person typed) |
| POST | `/api/v1/album-info/*`, `track-info/*`, `track-info/prune-orphans`, `studio/sanitize-track-titles`, `track-lyrics/fetch`, `discogs/*` | Metadata / prune / sanitize titles / LRCLIB / Discogs |
| GET/POST | `/api/v1/entity-info`, `search`, `save` | Curiosità read / multi-source search (`{ artist, album?, lang?, folderArtist?, folderAlbum? }` → `{ candidates, errors, page, lang }`, saved ones flagged) / persist (`add`, `removeIds`, `edit`, `imageUrl`) |
| POST | `/api/v1/entity-info/batch-targets` | Targets of a curiosità batch: `{ artist, scope: "artist" \| "albums", albums? }` → `{ targets: [{ key: "artist" \| "Artist/Album", album, label }] }` |
| POST | `/api/v1/entity-info/batch-search` | Search every target (nothing written): `{ artist, scope, albums?, lang?, stream? }` → `{ rows: [{ key, album, label, candidates, errors, saved, error? }] }`; `stream: true` → NDJSON `{type:"progress",done,total,key}` lines then `{type:"done", ok, data: { rows } \| error, message}` |
| POST | `/api/v1/entity-info/batch-save` | Save reviewed items (library write): `{ artist, rows: [{ album?, add, removeIds, edit: [{ id, text?, title? }], imageUrl? }] }` → `{ results: [{ key, album, saved, duplicates, total, image?, error? }] }` |
| POST | `/api/v1/entity-info/batch-auto` (alias `batch`) | Search and save every new candidate (library write); same body / streaming as `batch-search` → `{ results }` |
| GET/HEAD | `/media/{rel_path}`, `/api/v1/media/{rel_path}` | Audio stream: HTTP Range, `If-Range`, `ETag` / `Last-Modified` → 304, 416 with `Content-Range: bytes */len`. `.m4a` is `audio/mp4`; `Cache-Control: private, max-age=31536000` |
| GET/HEAD | `/api/v1/transcode/{rel_path}?format=mp3\|aac` | On-the-fly transcode for cast receivers (see below) |
| GET/PUT/PATCH | `/api/v1/user-state` | Per-account prefs (playCounts, recent, moods, excludes) + `revision`. PUT/PATCH accept `expectedRevision`; on mismatch → 409 `{ "ok": false, "error": "revision_conflict", "current": <state> }` |
| GET/POST/DELETE | `/api/v1/user-state/custom-theme-bg` | Serve / upload (`multipart` field `file`) / clear custom theme background (JPEG/PNG/WebP/GIF, max 32 MiB) |
| GET | `/api/v1/diagnostics` | Version, uptime, DB counts, scanning, jobs, watcher, recent errors, disk space, library layout, and `binaries.{ytdlp,ffmpeg,ffprobe,cloudflared}`: `{ available, path, source, version, candidates[] }` (+ `releaseDate`, `ageDays`, `stale` — older than 60 days — for yt-dlp). Versions are cached 10 min (`versionCacheTtlSecs`) |
| GET | `/api/v1/activity-log` | Activity JSONL entries (`ts`, `kind`, `message`, optional `accountId` / `accountName`). Query: `day=YYYY-MM-DD` (Default only, local calendar day), `since` (RFC3339), `scope=all\|system\|user` + `filterAccountId` (Default only; ignored for others → `scope=all`), `limit` (max 2000). Non-default callers are always clamped to the last 24h server-side (`canSelectDay: false`). Response includes `window`, `scope`, `canSelectDay`. |
| GET | `/api/v1/remote-access` | LAN URL, tunnel status (`stopped` / `starting` / `running` / `error`), `publicUrl`, `error` (English detail) and `errorCode` (`cloudflared_not_found`, `tunnel_start_timeout`, `tunnel_exited_early`, `tunnel_exited`, `tunnel_failed`), Cloudflare login flag, `cloudflaredAvailable` (cached, never runs a subprocess; LAN addresses cached 10 s) |
| POST | `/api/v1/remote-access/start` | Start temporary cloudflared quick tunnel (or use `REKORD_PUBLIC_URL`) |
| POST | `/api/v1/remote-access/stop` | Stop tunnel / clear public URL |
| POST | `/api/v1/remote-access/login` | Mark Cloudflare login + return dashboard URL |
| POST | `/api/v1/remote-access/logout` | Logout flag + stop tunnel |
| GET | `/api/v1/backup/theme-export` | Shareable theme ZIP (`rekord-theme/`) for the current account |

Track/Album JSON may include `genre`, `release_date`, `lyrics` (tracks) and `genre`/`label`/`expected_track_count` (albums) from tags or Studio meta.

**Display model** (all additive, existing fields keep their names):

- Tracks (`/library`, `tracks-page`, `search`, `changes`, album / playlist / favorite tracks, `tracks/{id}`): `title` is the display title (tag title; when missing or just the file name, the file name cleaned like legacy `sanitizeLocalTrackTitleDisplay`: no `01 - ` prefix, no `[…]` / `(Official Video)` / `(Remaster)` cruft; musical versions such as `(Remix)`, `(Live)`, `(Acoustic)`, `(feat. …)` stay). `track_number` falls back to the file name (`07 - x`, `1-07 x`). New: `file_name`, `disc_number`, `genres` (array of canonical labels: `genre` split on `;` `/` `,` `|`, numeric / stub tokens dropped, one label per case/space/hyphen-insensitive key, e.g. `["Hip Hop", "Pop Rap"]`), `has_cover` / `cover_version` (of the album; skip cover requests when `has_cover` is false, append `?v=cover_version`), `added_at`, `updated_at` (RFC3339), `user_edited` (a person edited it), `curated_fields` (fields whose curated value wins over tags: `title`, `release_date`, `genre`, `track_number`, `disc_number`).
- Albums: `name` is the display title (curated title, else the most common album tag, else the folder name; full-width `？` `：` → `?` `:`, `Album - ` prefix dropped). New: `folder_name` (on disk), `genres`, `cover_version`, `added_at`, `updated_at` (title / dates / genre / cover / track list changed: order "recently updated" by it), `user_edited`, `curated_fields`.
- Dates are stored as precise as the source: `YYYY-MM-DD`, `YYYY-MM` or `YYYY` (`YYYYMMDD` and timestamps are normalised). A curated date (sidecar, legacy, Studio) wins over tag dates (e.g. yt-dlp upload dates); a bare curated year never replaces a full tag date of the same year.
- Curated values: sidecars (`kord-albuminfo.json` / `kord-trackinfo.json`), the legacy library DB, Studio saves and metadata fetches. Rescans never replace them with tag values; values a person typed (`user_edited`) are not replaced by fetches or imports either.

### Legacy import

On the first scan of a library that has a legacy `.kord` folder (and on the first start of a hub that already indexed one) the hub imports it **once**: curated metadata (titles, full dates, genres, track/disc numbers, `user_edited`, added/updated times), the accounts registry (legacy names, `default` stays "Default"), and per account settings, favorites, playlists (keyed by legacy playlist id, legacy order), library selections, play counts, recents, moods, exclusions, theme backgrounds and Plectr records (`plectrBests` → `settings.plectr`). Accounts already used on the new hub (favorites, playlists or settings) are left alone; folders of accounts missing from the legacy registry are skipped (no state files). The outcome is recorded in `<data_dir>/legacy-import.json`; afterwards scans and restarts never import again, so data cleared after the import stays cleared. `REKORD_SKIP_LEGACY_IMPORT=1` disables it; `POST /library/sync-legacy-meta` stays available as an explicit merge.

Responses with JSON / JS / CSS / HTML / SVG bodies over 1 KiB are compressed (br / gzip per `Accept-Encoding`); media and images never are. Served UIs: hashed `/assets/*` get `Cache-Control: public, max-age=31536000, immutable`, `index.html` (SPA fallbacks) and `sw.js` get `no-cache`. Albums may also expose read-only Discogs fields when present: `discogs_release_id`, `discogs_uri`, and `discogs_extra` (`formatSummary`, `catalogNo`, `discogsUri`, `masterId` — camelCase, legacy parity).

Account resolution: query `accountId`, or headers `X-KORD-Account-Id` / `X-REKORD-Account-Id`, else default account `default` (named “Default”, as in legacy; a name the user chose is never rewritten).

Account registry writes are serialised and atomic (concurrent creates never lose entries). Deleting an account removes `accounts/<id>/`, `accounts/<id>_info/` (user-state, theme background) and its favorites / playlists rows. Errors: `account_not_found` (404), `cannot_delete_default_account`, `last_account` (403), `invalid_account_id`, `account_create_failed`, `account_update_failed`, `account_delete_failed`.

Every response carries `x-request-id` (echoed from the request when provided) to correlate client actions with hub logs.

**Account ids** match `[A-Za-z0-9_-]{1,64}`. An unknown id gets 404 `account_not_found` (it no longer falls back to Default).

### Library and machine operations

Two levels (parity legacy `requestAccess.mjs`, where a loopback request could run every server mutation):

**Library operations** — writes to the library from Studio. **Any account** may run them from the hub machine (local request); a remote client needs `allow_remote_admin` (any account).

- files on disk: `POST …/fs/mkdir`, `POST …/fs/delete-audio-relpaths|delete-album-folder`
- Studio: `POST …/download`, `…/download-cancel`, `GET …/download/active`, artwork `apply` / `upload`, album/track info fetch and save, `track-info/prune-orphans`, `track-lyrics/fetch`, `studio/sanitize-track-titles`, Discogs apply, `entity-info/save|batch-save|batch-auto|batch`
- account create / rename / delete

**Machine operations** — act on the host itself. They require the **Default** account *and* a local client (or `allow_remote_admin`).

- library: `PUT …/library/path`, `POST …/library/scan|thumbnails|sync-legacy-meta`, `PUT …/library/layout|watch`
- integrations: `POST/DELETE …/config/youtube-cookies`, `PUT/DELETE …/config/discogs-token`
- tools: `POST …/tools/ytdlp/update`
- backup: `GET …/backup/kord-data` (download) and `POST …/backup/kord-restore` (full restore; a non-admin may only restore a **theme package** up to 32 MiB)
- jobs and diagnostics: job cancel / clear, `DELETE …/diagnostics/errors`
- remote access: `POST …/remote-access/start|stop|login|logout`, `PUT …/system/machine-access`

Refusals are `403` with `error: "forbidden_remote"` (not on the hub machine and remote admin off) or `error: "forbidden_default_account"` (machine operation from another account).

A request counts as local when the peer address is loopback, `Host` is `localhost`/`127.0.0.1`/`::1`, no proxy headers (`cf-connecting-ip`, `cf-ray`, `x-forwarded-for`, `x-forwarded-host`, `x-real-ip`) are present and its `Origin`, if any, is allowed (see CORS) — so a Cloudflare tunnel is remote even though `cloudflared` connects from `127.0.0.1`, and a foreign web page open on the hub machine is never local. `PUT /api/v1/system/machine-access { "enabled": true }` (or `REKORD_ALLOW_REMOTE_ADMIN=1`), callable only from the hub machine, lifts the local requirement. `GET /api/v1/system/machine-access`, `GET /api/v1/config` and `GET /api/v1/remote-access` expose `machineAccess` so clients can gate controls instead of failing on submit:

```json
{ "isDefaultAccount": false, "local": true, "allowRemoteAdmin": false,
  "canManageLibrary": true, "canManageMachine": false,
  "libraryDeniedReason": null, "machineDeniedReason": "forbidden_default_account" }
```

`GET /api/v1/config` also reports `youtubeCookiesWritable` / `discogsWritable` as `false` when the caller cannot manage the machine.

In **Docker** every request reaches the hub from the Docker network, never from loopback: run machine operations with `docker exec rekord curl -X POST http://127.0.0.1:7420/…`, or set `REKORD_ALLOW_REMOTE_ADMIN=1` on a trusted network. The desktop **server flavor** (hub embedded in the Tauri app) is local: its window talks to `127.0.0.1:7420` from an allowed origin.

### CORS

The hub listens on the LAN and on loopback, so any web page open on a machine of the network could otherwise script it. Allowed browser origins:

- the same origin (`Origin` host equals `Host` / `X-Forwarded-Host`): the hub-served SPA and admin panel, also through the Cloudflare tunnel;
- the Tauri shells: `tauri://localhost` (macOS, Linux), `http://tauri.localhost` (Windows, Android), `https://tauri.localhost`;
- the Vite dev servers: `http://localhost:7421|7422`, `http://127.0.0.1:7421|7422`;
- anything listed in `REKORD_ALLOWED_ORIGINS` (comma separated, e.g. `https://music.example.org`).

A foreign origin gets `403 cross_origin_forbidden` on mutating methods and no `Access-Control-Allow-Origin` on reads (requests without `Origin` — curl, the native media service fetching artwork — are unaffected). Every response also carries `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` and a restrictive `Permissions-Policy`.

**Body limits:** 2 MiB by default; restore 512 MiB, theme background 33 MiB, artwork upload 16 MiB, YouTube cookies 3 MiB.

### Versioning and client compatibility

`GET /api/v1/health` declares:

- `version` — the hub version (workspace version, e.g. `5.0.0`);
- `apiVersion` — the `/api/v1` contract revision (`api::API_VERSION`, currently `1`); it only grows on a breaking change;
- `minClientVersion` — the oldest client the hub accepts (`api::MIN_CLIENT_VERSION`);
- `transcode` — whether `/api/v1/transcode` is usable (ffmpeg found).

Native clients (Tauri desktop / Android) bundle the UI, so they no longer update with the hub. The client (`src/lib/platform/compat.ts`) compares those fields with its own version and shows a banner: blocking warning when it is below `minClientVersion` or `apiVersion` is newer than it knows, a dismissible notice when the hub is ahead (or behind) by a minor/major version. The browser client served by the hub only offers a reload when the page it runs is older than the hub (stale cache / service worker).

### Transcode

`GET /api/v1/transcode/{relPath}?format=mp3|aac|flac[&bitrate=64..320]` — on-the-fly transcode for cast receivers (Chromecast / Google Home) that can't decode FLAC/OGG/Opus/WAV, and cached FLAC copies for players that can't decode WMA/AIFF/ALAC.

- `relPath`: same library-relative path as `/api/v1/media/{relPath}`, same validation (no `..`, reserved or hidden segments, must resolve inside the music root).
- `format`: `mp3` (default, 320 kbps, `audio/mpeg`) or `aac` (256 kbps ADTS, `audio/aac`) for live streams; `flac` for a lossless copy (see below).
- Response: 200 chunked stream of ffmpeg stdout; no `Content-Length`, `Accept-Ranges: none`, `Cache-Control: no-store`. `HEAD` returns the headers only. ffmpeg is killed when the client disconnects.
- Errors: 404 track not found / invalid path; 400 `unsupported_format`; 503 `ffmpeg_unavailable` (ffmpeg not installed); 503 `transcode_busy` (4 transcodes already running).
- Check `GET /api/v1/health` → `"transcode": true` before offering it.

ffmpeg is located like the other tools (see below).

### Tools (yt-dlp, ffmpeg, cloudflared)

Each tool is looked up in these places, in order; when several exist the **newest `--version` wins** (ties go to the earlier one):

1. yt-dlp only: config `ytdlp_path` (settings), `YTDLP_PATH`; then the copy installed by the update below, `<data_dir>/tools/yt-dlp`
2. ffmpeg: `REKORD_FFMPEG`, `FFMPEG_PATH`, `REKORD_FFMPEG_BIN`; ffprobe: `REKORD_FFPROBE`, `FFPROBE_PATH`; cloudflared: `REKORD_CLOUDFLARED_BIN`
3. bundled: `<exe dir>/`, `<exe dir>/bin/`, `<exe dir>/../resources/bin/`, `<exe dir>/resources/bin/`, `<exe dir>/../Resources/bin/` (macOS), `REKORD_TOOLS_DIR`; in debug builds `release/bin/<platform>/` of the repository
4. `PATH`

Probes run off the async workers and are cached for 10 minutes (an update invalidates the cache). `GET /api/v1/diagnostics` shows the choice and every candidate.

`POST /api/v1/tools/ytdlp/update` (`{ "force"?: bool }`, machine operation) downloads the latest official release asset for the platform (`yt-dlp_linux`, `yt-dlp_linux_aarch64`, `yt-dlp.exe`, `yt-dlp_macos`) from GitHub (`REKORD_YTDLP_RELEASE_API` overrides the release JSON URL), verifies it against the release's `SHA2-256SUMS`, checks that it runs, and installs it atomically (temp file + rename, `chmod 755`) as `<data_dir>/tools/yt-dlp`. Response: `{ updated, upToDate, latestVersion, previousVersion, version, source, asset }` — when the yt-dlp in use is already the latest, nothing is downloaded (`upToDate: true`) unless `force`. Errors: 409 `ytdlp_update_in_progress`, 501 `ytdlp_platform_unsupported`, 502 `ytdlp_release_lookup_failed` / `ytdlp_asset_missing` / `ytdlp_download_failed` / `ytdlp_checksum_missing` / `ytdlp_checksum_mismatch`, 500 `ytdlp_install_failed`.

### Downloads

`POST /api/v1/download` `{ url, downloadId (UUID), downloadKind?, outputDir?, background? }` (library operation) answers with an NDJSON stream:

- `started` `{ downloadId, background, ytdlp: { version, source }, ffmpeg, jsRuntime }`
- `progress` `{ progress: { current, total } }` ("Downloading item N of M")
- `item` `{ item: { id, index, title, status: pending|downloaded|skipped|failed, format?, reason?, code?, files? } }` as each item changes
- `log` `{ stream, line }` (yt-dlp output without per-percent progress lines), `keepalive` every 5 s
- `indexing` (success only) while the output folder is re-indexed
- `items` (legacy) then `done`:

```json
{ "type": "done", "downloadId": "…", "ok": true, "partial": true, "cancelled": false,
  "cancelReason": null, "error": null, "code": 1,
  "summary": { "downloaded": 11, "skipped": 0, "failed": 1, "total": 12 },
  "downloadedItems": ["Artist/Album/01 - Title.m4a"],
  "skippedItems": [{ "label": "…", "reason": "already downloaded" }],
  "failedItems": [{ "id": "dQw4w9WgXcQ", "index": 7, "title": "…", "label": "…", "reason": "Requested format is not available…", "code": "no_audio_format" }],
  "items": [], "formats": ["140"], "progress": { "current": 12, "total": 12 },
  "stdout": "…", "stderr": "…", "logTruncated": false, "stdoutTotalChars": 0, "stderrTotalChars": 0,
  "command": "yt-dlp -f … (no absolute paths, no URL)", "outputDir": "Artist/Album",
  "rescanned": true, "rescanError": null, "indexEpoch": 42 }
```

- Items are attributed from the whole output stream (the `[rekord-item]` marker printed by yt-dlp before format selection, `Downloading item N of M`, `[youtube] <id>:` lines), not from the truncated preview.
- Formats are audio only (`bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio`): an item without an audio-only format fails with `no_audio_format`. yt-dlp gets `--ffmpeg-location` (resolved ffmpeg) and `--js-runtimes deno:<abs>` / `node:<abs>` only when one is on `PATH` (`REKORD_YTDLP_JS_RUNTIME` overrides, `none` disables).
- `ok`: something was written or already present (or a clean exit without failures). Only then the **output folder** is re-indexed (whole library when `outputDir` is empty) *before* `done`, so `indexEpoch` (same counter as `/library/stats` → `index_epoch`) already covers the new files. After a failure or a cancel nothing is rescanned.
- `done.error` when not ok: `cancelled`, `client_disconnected`, `ytdlp_not_found`, `ytdlp_spawn_failed`, `ytdlp_failed`, or the common item code (`no_audio_format`, `video_unavailable`, …).
- When every stream attached to a download is gone (tab closed, fetch aborted), yt-dlp is stopped (`cancelReason: "client_disconnected"`), unless the download was started with `background: true`.
- `GET /api/v1/download/active` lists this hub's running and finished (kept 15 min) downloads: `{ downloadId, kind, outputDir, accountId, background, status: running|indexing|done|failed|cancelled, startedAt, finishedAt, progress, counts, items, logTail (last 80 lines), canCancel, attachedStreams, cancelReason, done }`. `?downloadId=…` returns one; with `&stream=1` the response is an NDJSON stream starting with `{ type: "snapshot", download: {…} }` followed by live events up to `done` (immediately when already finished) — a client returning to the pane re-attaches this way (or polls).
- Start errors: 403 `ytdlp_disabled`, 400 `url_not_allowed` / `invalid_download_id` / `invalid_output_dir` / `music_root_not_set`, 409 `download_id_active`.

Library scan is **folder-first**: `Music/Artist/Album/track` (other layouts are detected, see [supported-formats.md](supported-formats.md#library-layout)). Embedded tags supply title, album name, genres, dates, track/disc numbers, BPM and lyrics; curated values always win over them.

### Backup / restore

- **v3 (RE-KORD 5):** ZIP includes `config/manifest.json` (`kordBackup: 3`), `config/settings.json`, `config/accounts.json`, `hub/accounts/{id}/favorites.json|playlists.json|library-selection.json|user-state.json` (+ optional `theme-bg.jpg`), library sidecars under `libraries/shared/`, and `kord-db/` (mirror of `music_root/.kord`). Also `config/youtube-cookies.txt` / activity when present.
- **v2 (legacy):** ZIP from the React hub. Restore reads `config/music-root.config.json` + `config/manifest.json`, extracts `kord-db/` → `music_root/.kord`, imports registry from `kord-db/global_info/accounts.json` (or manifest `accounts`), and for each `{id}_info/user-state.json` migrates favorites/playlists into SQLite **and** full prefs into `{data_dir}/accounts/{id}_info/user-state.json` (playCounts, recent, moods, excludes, settings, optional `legacyQueue`). After the library scan, album/track studio metadata is merged from restored sidecars (`kord-albuminfo.json` / `kord-trackinfo.json`) and from `music_root/.kord/rekord.db` into the hub DB (fill-empty). Audio files are **not** in the ZIP — `libraryRoot` must already exist on disk.
- **Account overwrite-by-name:** before writing personal data, restore matches backup accounts to existing hub accounts with the same display name (case-insensitive). Matching accounts keep the hub id and have favorites/playlists/selection/user-state/theme overwritten; unmatched backup accounts are added with their backup id. `default` always maps to `default`.
- **Theme package:** ZIP with `rekord-theme/rekord-theme.json` (`kind: "rekord-theme"`) + optional background image. `POST …/kord-restore` detects it and applies only theme settings (preset/custom, glass, background) to the current account — no user data. `GET …/theme-export` builds the same format.
- CLI: `rekord-server --restore-zip /path/to.zip [--restore-exit]` restores without HTTP multipart.
- CLI: `rekord-server --sync-legacy-meta [--sync-legacy-exit]` merges studio metadata + full personal data (moods, excludes, settings, favorites, playlists, selection, accounts) from `music_root/.kord` into the hub (same as `POST /api/v1/library/sync-legacy-meta`).

The **server** serves the built **client SPA** from `--client-ui` / `REKORD_CLIENT_UI` at `/`, so LAN and Cloudflare tunnel URLs are same-origin for UI + API, and the **admin panel** from `--admin-ui` / `REKORD_ADMIN_UI` at `/admin` (also on `/` when no client bundle is present). Native shells may still bundle the UI and point `Server URL` at the hub.

### Error codes

Studio, downloads, tools and permissions answer `{ ok: false, error: "<code>", message? }`:

- permissions: `forbidden_remote`, `forbidden_default_account`, `cross_origin_forbidden`, `invalid_account_id`, `account_not_found`
- config: `music_root_not_set`, `file_required`, `file_too_large`, `upload_failed`, `cookies_invalid`, `cookies_locked_by_env`, `discogs_token_invalid`, `discogs_token_locked_by_env`
- files: `invalid_path`, `fs_list_failed`, `fs_search_failed`, `mkdir_failed`, `rel_paths_required`, `delete_failed`, `delete_album_failed`
- downloads: `ytdlp_disabled`, `url_not_allowed`, `invalid_download_id`, `invalid_output_dir`, `download_id_active`, `download_not_found`, `download_id_required`, `flat_count_failed`, `ytdlp_not_found`, `ytdlp_timeout`, `ytdlp_failed`, `ytdlp_spawn_failed`, `download_failed`, `cancelled`, `client_disconnected`
- per-item (`failedItems[].code`, preview): `no_audio_format`, `video_unavailable`, `private_video`, `age_restricted`, `members_only`, `geo_blocked`, `sign_in_required`, `http_forbidden`, `postprocess_failed`, `network_error`, `unknown` (skipped: `already_downloaded`)
- YouTube / catalog: `query_too_short`, `youtube_search_failed`, `invalid_releases_url`, `youtube_releases_failed`, `invalid_catalog_url`, `no_tracks_found`, `track_list_failed`, `preview_resolve_failed`, `preview_timeout`, `preview_expired`, `preview_failed`, `preview_upstream_unreachable`, `preview_upstream_forbidden`, `preview_upstream_error`, `preview_range_invalid`, discover feeds `upstream_rejected` / `upstream_unavailable` / `upstream_timeout` / `upstream_failed`
- metadata (codes of `metadata::error::classify` pass through with their status: `album_not_found`, `no_match` (404, track fetch without a close match), `no_metadata_found`, `discogs_rate_limited`, `discogs_unauthorized`, `upstream_unavailable`, …), otherwise: `album_ref_required`, `invalid_album_path`, `track_ref_required`, `track_not_found`, `invalid_patch`, `db_error`, `artwork_search_failed`, `artwork_apply_failed`, `artwork_upload_failed`, `album_info_fetch_failed`, `album_info_save_failed`, `track_info_fetch_failed`, `track_info_save_failed`, `lyrics_missing_artist`, `lyrics_not_found`, `lyrics_fetch_failed`, `prune_failed`, `sanitize_failed`, `discogs_search_failed`, `discogs_apply_failed`, `entity_info_search_failed`, `entity_info_save_failed`, `entity_info_batch_failed`, `invalid_scope`, `upstream_rate_limited`, `upstream_timeout`
- remote access (`errorCode` of `GET /remote-access`): `cloudflared_not_found`, `tunnel_start_timeout`, `tunnel_exited_early`, `tunnel_exited`, `tunnel_failed`
- tools: `ytdlp_update_in_progress`, `ytdlp_platform_unsupported`, `ytdlp_release_lookup_failed`, `ytdlp_asset_missing`, `ytdlp_download_failed`, `ytdlp_checksum_missing`, `ytdlp_checksum_mismatch`, `ytdlp_install_failed`, `cloudflared_not_found`
- accounts: `account_create_failed`, `account_update_failed`, `account_delete_failed`, `cannot_delete_default_account`, `last_account`
