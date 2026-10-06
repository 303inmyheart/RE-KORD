# Changelog

All notable changes to RE-KORD. Versions follow [semantic versioning](https://semver.org);
one version number covers the hub, the clients and the packages.

## 5.0.0 — RE-KORD 5

RE-KORD 5 is a ground-up rewrite. The legacy React / Node / Electron / Capacitor app is
replaced by a Rust hub, a Svelte client and Tauri 2 shells. Every feature of the legacy app
is carried over, your data is imported automatically, and a lot is new or fixed.

The legacy app, last released as 5.0, is preserved at the tag `legacy-5.0`. Read
[Upgrading from legacy RE-KORD](docs/upgrading-from-legacy.md) before you switch.

### Breaking changes

- The hub listens on port **7420** (was 3001).
- The hub keeps its data in its own folder (`REKORD_DATA_DIR`) instead of the Electron
  config folder; see the upgrade guide for every platform.
- Docker: the `/config` volume is replaced by `/data`, and environment variables were
  renamed (`REKORD_BIND`, `REKORD_DATA_DIR`, `REKORD_MUSIC_ROOT`).
- Android: the app is signed with a new key. Uninstall the legacy app before installing.
- The desktop and Android apps now bundle their own UI instead of loading it from the hub,
  so they are updated by installing a new version.
- The API moved to `/api/v1` with a `{ ok, data }` envelope and stable error codes.
- Cross-origin requests are restricted, and host-level operations require the hub computer
  or an explicit remote-admin switch (see [SECURITY.md](SECURITY.md)).

### New architecture

- **Hub in Rust** (`rekord-server`, `crates/core`): one self-contained binary built on
  axum, Tokio and SQLite. Low memory use, fast start-up, graceful shutdown that closes the
  database cleanly.
- **Client in Svelte 5**: one codebase for the browser, desktop and Android, with
  lazy-loaded views and languages.
- **Tauri 2 shells** for Linux, Windows and Android. The **RE-KORD Server** app embeds the
  hub in the desktop app, like the legacy Electron "Server" app, and serves the web client
  and admin panel to the rest of the network.
- **Admin panel** at `/admin`: music folder, library structure, scans and scan reports,
  jobs, diagnostics, activity log, backups, accounts, integrations and network.
- **Packaging**: one command per platform and flavor (`scripts/pack.sh`), built in a Docker
  image so the host only needs Docker. Portable Windows downloads (single exe for the
  client, folder for the server). A headless Linux package with a hardened systemd unit.
  yt-dlp, cloudflared and ffmpeg are bundled at pinned versions and verified with SHA-256.
- **Docker image**: multi-stage build, non-root user, health check, amd64 and arm64.
- **CI** on every push and pull request: version consistency, rustfmt, clippy, Rust and
  JavaScript tests, type checks, the desktop bundle, an Android APK and the Docker image.

### Data and migration

- **Automatic one-time import** of a legacy library: the first scan of a music folder that
  contains `.kord` imports curated metadata, accounts, settings, favorites, playlists (in
  legacy order), library selections, play counts, history, moods, shuffle exclusions, theme
  backgrounds and Plectr records. Legacy credentials (Discogs token, YouTube cookies) are
  imported once at start-up. Later scans never resurrect data you deleted.
- Explicit **merge** from `.kord` at any time (admin panel, client or
  `--sync-legacy-meta`) that never overwrites newer data.
- Legacy (v2) **backup ZIPs** restore directly; the new v3 backup adds hub settings and
  per-account state. Accounts are matched by name on restore.

### Library

- **Embedded tags are imported on scan**: title, album, multiple genres, full release dates,
  track and disc numbers, BPM and lyrics (inspired by PR #95 by @knoellix).
- **Exact durations for long VBR MP3s** without a Xing header: frames are counted on scan
  and a synthetic seek header is served with the file, so long DJ sets show the right length
  and seek accurately (inspired by PR #95 by @knoellix).
- **WMA, AIFF and ALAC play everywhere**: the hub converts them once to a cached FLAC copy
  that seeks like any file.
- Display titles and album names are separate from file names, cleaned of numbering and
  video noise; musical versions such as "(Live)" or "(Remix)" are kept.
- Curated and hand-edited values are protected across rescans, imports and metadata fetches.
- Normalised multi-genre support, full dates (`YYYY-MM-DD`, `YYYY-MM`, `YYYY`) and
  track/disc numbers inferred from file names when tags lack them.
- Full-text, accent-insensitive search over titles, artists, albums and genres.
- **Safe rescans**: an incremental scan refuses to drop a large part of the library at
  once (for example when a disk is not mounted) and reports what is missing; favorites and
  playlist entries reconnect when a file returns.
- Per-account library statistics, added/updated timestamps ("recently updated" albums), and
  a filesystem watcher that coalesces bursts of changes into one update.

### Listening

- Per-account queue synced to the hub, so it follows you between devices.
- Near-gapless playback with crossfade off; crossfade of 3 or 5 seconds.
- Smart shuffle and Smart Radio by moods, genres and history; instant playlists from genres
  and moods on the dashboard.
- Automatic recovery when the hub goes away and comes back; failing tracks are skipped.
- **Google Cast** from Chrome and, natively, from the Android app, with transcoding for
  formats receivers cannot play.
- **Android**: media notification and lock-screen controls with the screen off, audio focus
  (pause on calls and when headphones are unplugged), Back sends the app to the background
  without stopping music, portrait on phones, files saved to Downloads, QR pairing.
- Sonic Nebula, DiscoWall, karaoke and eight visualizers.

### Studio

- Rewritten Studio with **Listen**, **Discover**, **Download**, **Metadata** and **Covers**.
- Downloads run as hub jobs that survive a closed tab: re-attach to see progress, per-item
  results (downloaded, already present, failed with a reason), and the folder is re-indexed
  before completion.
- **Update yt-dlp** from the app: the hub fetches the latest official release and verifies
  its checksum.
- Discover › Web: new releases from YouTube Music with 30-second previews.
- Metadata matching uses the track list first and similarity thresholds, so wrong matches are
  rejected instead of written.
- **Curiosità** (trivia) rebuilt: multi-source search (Wikipedia, Wikiquote, Last.fm,
  Discogs, TheAudioDB), per-language entries, editing before saving, artist photos.

### Plectr

- Redesigned game with a portrait 9:16 stage, real pause, results screen, per-difficulty
  records synced to the account, latency calibration, key remapping, light stage for slower
  devices and a challenge mode.

### Interface

- New design system: self-hosted fonts (no Google Fonts requests), consistent type scale,
  accessible dialogs, skeletons and empty states.
- History-based navigation: Back closes dialogs first, then returns to the previous view.
- Faster on Linux (WebKitGTK): no continuous animations, capped canvases, no glass blur.
- **German translation** by @knoellix (PR #93), alongside Italian and English, with tests
  that keep the three languages in sync.
- Update banners when the app and the hub are out of step.

### Security

- Path traversal protection on every file route.
- Browser origin policy: only the hub's own pages, the Tauri apps, the development servers
  and configured origins may call the API.
- Library and machine operations restricted to the hub computer (and the Default account
  for machine operations) unless remote administration is turned on.
- Validated account ids; artwork downloads protected against server-side request forgery.
- Security headers on every response.

## Earlier versions

The legacy app's history (1.x to 4.4 and the legacy 5.0) is on
[GitHub Releases](https://github.com/Creiv/RE-KORD/releases) and in the repository tags.
Highlights:

- **Legacy 5.0**: structural refactor, graceful shutdown, job queue, diagnostics, CI and
  end-to-end tests, offline PWA shell.
- **4.4**: server-side cover thumbnails.
- **4.3**: Sonic Nebula, Smart Radio on the dashboard, Android background resilience.
- **4.2**: adaptive library scan (layout detection), Discogs integration.
- **4.1**: SQLite library core, artwork cache, Cast to Google Home, sleep timer, Pro
  Workspace UI.
- **4.0**: Android client with QR pairing, theme sharing, adjustable glass.
