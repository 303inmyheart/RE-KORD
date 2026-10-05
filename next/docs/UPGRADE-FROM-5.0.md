# Upgrading from RE-KORD 5.0 to 5.1 (next)

5.1 replaces the Node/Electron/Capacitor stack with a Rust hub (`rekord-server`), a Svelte
client and Tauri 2 shells. Your music files are untouched; your personal data (favourites,
playlists, moods, play counts, settings, accounts) is imported from a 5.0 backup or from the
library's `.kord` folder. Read the whole page once before starting.

## What changes at a glance

| | 5.0 | 5.1 |
|---|---|---|
| Hub port | `3001` | **`7420`** (`REKORD_BIND=0.0.0.0:7420`) |
| Hub program | Node server inside the Electron "Server" app, or `node server/index.mjs` | `rekord-server` (single binary), or the **RE-KORD Server** desktop app (hub embedded) |
| Hub data | Electron `userData` / `REKORD_USER_CONFIG_DIR` (`music-root.config.json`) + `music_root/.kord` | `REKORD_DATA_DIR` (default below): `rekord.db`, `settings.json`, `accounts/`, `modules.manifest.toml` |
| Admin | inside the app | `http://<hub>:7420/admin` |
| Web client | served by the hub | still served by the hub at `/` (now installable as a PWA over HTTPS/localhost) |
| Desktop client | Electron, loaded the UI from the hub | Tauri, **UI bundled in the app** (updates with the app, not with the hub) |
| Android | Capacitor APK, debug-signed | Tauri APK, release-signed → **uninstall the old one first** |
| Docker | `/config` + `/music`, port 3001 | `/data` + `/music`, port 7420 |

Default `REKORD_DATA_DIR` (`dirs::data_dir()/RE-KORD`):

- Linux: `~/.local/share/RE-KORD`
- Windows: `%APPDATA%\RE-KORD` — the same folder the 5.0 desktop app used. 5.1 adds its own
  files (`rekord.db`, `settings.json`, `accounts/`) next to the old ones and does not touch
  `music-root.config.json` & co.
- macOS: `~/Library/Application Support/RE-KORD` — same remark as Windows.
- systemd package: `/var/lib/rekord`; Docker: `/data`.
- RE-KORD Server desktop app: `<app data>/app.rekord.server/hub` (configurable in
  `<app data>/app.rekord.server/hub.json`, see the README).

## 1. Before you start (5.0 still running)

1. In 5.0, download a backup from the settings (backup section): you get a `kordBackup: 2` ZIP with
   configuration, accounts, favourites, playlists, moods and the `.kord` database. Keep it.
2. Note the music folder path (5.0 Settings → Library).
3. Stop 5.0 (quit the Server app / `docker compose down` / stop the Node process) so port
   3001 and the `.kord` database are free.

## 2. Install the 5.1 hub

Pick one:

- **Linux service**: `rekord-server-5.0.0-linux-x64.tar.gz` → `sudo ./systemd/install.sh`
  (see [DEPLOY.md](DEPLOY.md)).
- **Docker**: `cd next && docker compose up -d --build` (see "Docker" below).
- **Windows / macOS**: the `rekord-server-…` archive (`run.cmd` / `run.sh`), or the
  **RE-KORD Server** desktop app, which starts the hub with the window.
- **From source**: `cd next && pnpm install && pnpm build:ui && cargo run -p rekord-server --release`.

Then open `http://<hub>:7420/admin` and set the music folder (same path as in 5.0), or
start the hub with `--music-root /path/to/Music` / `REKORD_MUSIC_ROOT`.

## 3. Bring your data over

Two ways; both are safe to repeat.

**a) From the 5.0 backup ZIP** (recommended: also restores accounts and settings)

```bash
rekord-server --restore-zip ~/rekord-backup-5.0.zip --restore-exit   # then start normally
# or: /admin → Backup → Restore, upload the ZIP
```

The library folder must already exist at the path recorded in the backup (audio files are
not in the ZIP). The restore matches accounts **by display name**: an existing hub account
with the same name keeps its id and gets the backup's data; others are added.

**b) From the library's `.kord` folder** (no backup at hand, same disk)

```bash
rekord-server --music-root /path/to/Music --sync-legacy-meta --sync-legacy-exit
# or, with the hub running: POST /api/v1/library/sync-legacy-meta (from the hub machine)
```

In 5.1 this is a **merge**: studio metadata fills empty fields (genre stubs like `Music` /
`Unknown` are replaced by the richer legacy value), and per-account moods, excludes,
settings, play counts, recent tracks, favourites, playlists and library selection are merged
into what the hub already has. Nothing on the hub is deleted, so running it again after you
started using 5.1 does not lose new data.

After either path, run a library scan from `/admin` (the restore schedules one by itself).

## 4. Clients

- **Browser**: just open `http://<hub>:7420/` (old bookmarks on `:3001` must change). Over
  HTTPS (tunnel / reverse proxy) or on `localhost` it can be installed as an app (PWA).
- **Desktop**: uninstall the 5.0 Electron client, install the 5.1 RE-KORD app. It finds a
  hub on the same machine by itself; otherwise type the address or scan the QR from
  `/admin`. Desktop and mobile apps now carry their own UI: when the hub is updated to a
  newer minor version they show an "update available" banner (and a blocking warning if the
  hub requires a newer client, `minClientVersion` in `/api/v1/health`). Get updates from
  [re-kord.com](https://re-kord.com).
- **Android**: the 5.0 APK was signed with a debug key, the 5.1 APK with the release key,
  and Android refuses to update across keys. **Uninstall RE-KORD 5.0 first**, then install
  the new APK. Nothing important is lost — everything lives on the hub; the app asks for the
  hub address again (or scan the QR). Google Cast from the Android app is not available in
  5.1: cast from Chrome using the web client (see [ANDROID.md](ANDROID.md)).

## 5. Docker

```yaml
# 5.0                                   # 5.1 (next/docker-compose.yml)
ports: ["3001:3001"]                    ports: ["7420:7420"]
volumes:                                volumes:
  - ./docker-data/config:/config          - ./docker-data/data:/data
  - ./docker-data/music:/music            - ./docker-data/music:/music
```

- The image runs as uid **10001**: `sudo chown -R 10001:10001 docker-data` (or set `user:`
  in the compose file).
- The old `/config` volume is **not** read by 5.1. Restore your data with the backup ZIP
  (copy it into the data volume, then
  `docker compose run --rm rekord --restore-zip /data/backup.zip --restore-exit`), or with
  `--sync-legacy-meta` if `/music/.kord` is in the music volume.
- Environment: `PORT` / `REKORD_LISTEN_HOST` → `REKORD_BIND`; `REKORD_USER_CONFIG_DIR` →
  `REKORD_DATA_DIR`; `MUSIC_ROOT` / `REKORD_DOCKER_MUSIC_DIR` → `REKORD_MUSIC_ROOT`.
- Machine operations (scan, library path, restore, tunnel) need a local client, and from
  inside Docker nothing is local: use `docker exec rekord curl -X POST
  http://127.0.0.1:7420/api/v1/library/scan`, or `REKORD_ALLOW_REMOTE_ADMIN=1` on a trusted
  network. See [API.md](API.md#machine-operations).

## 6. Security changes you may notice

- CORS is no longer `*`: only the hub's own pages, the Tauri apps, the dev servers and
  `REKORD_ALLOWED_ORIGINS` may call the API from a browser. A custom web page or reverse
  proxy on another origin must be listed there.
- Machine operations (library path, scans, deletions, Studio downloads, restores, tunnel,
  backup download…) require the Default account on the hub machine, or the *remote admin*
  switch. Personal data (favourites, playlists, moods) works from every client as before.

## 7. Known risk: desktop apps and plain-HTTP hubs (mixed content)

The Tauri UI runs from a secure origin (`tauri://localhost` on macOS/Linux,
`http://tauri.localhost` on Windows) and talks to a hub on `http://192.168.x.x:7420`. WebView2
(Windows) and the Android WebView allow it. **WebKitGTK (Linux) and WKWebView (macOS) may
treat `tauri://localhost` as a secure context and block or "upgrade" plain-HTTP requests
(mixed content)**, especially `fetch` from a secure origin to a private-network address and
`<audio src="http://…">`. Behaviour depends on the WebKit version.

Test procedure on each desktop platform before a release:

1. Start a hub on another machine of the LAN (`REKORD_BIND=0.0.0.0:7420`).
2. Start the desktop app, type `http://<lan-ip>:7420`, pick an account.
3. Check: library loads (fetch), album covers show (img), a FLAC and an MP3 play and seek
   (audio + Range), Settings → Backup download saves a file (blob → save dialog).
4. Open the devtools (debug build, or right click → Inspect) and look for `Mixed Content`
   / `blocked` / `CORS` messages in the console.
5. Repeat with the hub on the same machine (`http://127.0.0.1:7420`, loopback is always
   potentially trustworthy) and through an HTTPS tunnel URL.

If step 3 fails on a platform, the workarounds are: reach the hub through HTTPS (tunnel or
reverse proxy with a certificate), or use the web client in a browser for that platform.
Record the WebKitGTK / macOS versions tested in the release notes.

## 8. Rolling back

5.1 never touches `music-root.config.json`; `--sync-legacy-meta` only reads `.kord`, while a
restore of a 5.0 ZIP writes the backup's `kord-db/` into `music_root/.kord` (the same data
5.0 had when you exported it). Copy `music_root/.kord` aside before restoring if you want a
bit-for-bit way back. To go back: stop the 5.1 hub, start 5.0 again on port 3001, reinstall the 5.0 clients
(on Android, uninstall 5.1 first — again a key change).
