# Deploying the RE-KORD hub

Four ways to run the hub, from most to least "server-like". All of them expose the client at
`http://<host>:7420/` and the admin panel at `/admin`. Configuration is the same everywhere:
`rekord-server --help` lists every flag and its environment variable.

| Variable | Default | Meaning |
|---|---|---|
| `REKORD_BIND` | `0.0.0.0:7420` | listen address (`127.0.0.1:7420` = this machine only) |
| `REKORD_DATA_DIR` | `dirs::data_dir()/RE-KORD` | database, settings, accounts, module manifest |
| `REKORD_MUSIC_ROOT` | unset (set from `/admin`) | library root (`Artist/Album/track`) |
| `REKORD_CLIENT_UI` / `REKORD_ADMIN_UI` | next to the binary | built `client-ui` / `server-ui` |
| `REKORD_ALLOWED_ORIGINS` | — | extra browser origins for CORS (see API.md) |
| `REKORD_ALLOW_REMOTE_ADMIN` | `0` | machine operations from non-local clients (Default account) |
| `REKORD_PUBLIC_URL` | — | public URL when behind your own reverse proxy |
| `YTDLP_PATH`, `REKORD_CLOUDFLARED_BIN`, `REKORD_FFMPEG` | `PATH` / next to the binary | external tools |

External tools: **ffmpeg** (transcode for cast receivers, previews) — install it from the
distribution; **yt-dlp** (Studio downloads) and **cloudflared** (remote access tunnel) are
optional. `scripts/fetch-ytdlp.sh` and `scripts/fetch-cloudflared.sh` download pinned
releases from GitHub and verify their SHA-256 (pinned in `scripts/third-party.sha256`, or the
checksum the project publishes); `scripts/pack.sh` bundles them (plus ffmpeg, `scripts/fetch-ffmpeg.sh`) in every server package unless `--no-tools` is given.

## Linux, systemd

```bash
# build (or download) the package
cd next && pnpm pack:linux:server
# → release/linux/RE-KORD-Server-5.0.0-linux-x64-headless.tar.gz

tar -xzf RE-KORD-Server-5.0.0-linux-x64-headless.tar.gz
cd RE-KORD-Server-5.0.0-linux-x64-headless
sudo ./systemd/install.sh          # /opt/rekord, data in /var/lib/rekord, user "rekord"
sudoedit /etc/default/rekord-server # REKORD_MUSIC_ROOT, REKORD_BIND, …
sudo systemctl restart rekord-server
journalctl -u rekord-server -f
```

`install.sh` creates the `rekord` system user, copies the program to `/opt/rekord`, installs
`rekord-server.service` and, only the first time, `/etc/default/rekord-server`. Running it
again from a newer package upgrades in place and keeps data and configuration.

The unit is hardened (`ProtectSystem=strict`, `ProtectHome=read-only`, `NoNewPrivileges`,
private `/tmp`). The hub may only write `/var/lib/rekord`: **add your music folder to
`ReadWritePaths=`** (`sudo systemctl edit rekord-server`) if you use Studio, cover
downloads or deletions; for a library under `/home` also set `ProtectHome=false`.
`sudo apt install ffmpeg` for transcoding.

Manual install without the script: the commands are at the top of
`scripts/linux/rekord-server.service`.

## Docker

```bash
cd next
docker compose up -d --build        # image rekord:5.0.0, port 7420
docker compose logs -f
```

Multi-stage `Dockerfile`: UI built with pnpm, `rekord-server` built in release, yt-dlp and
cloudflared fetched at pinned versions with checksum verification, runtime on
`debian:bookworm-slim` with ffmpeg and tini, non-root user (uid 10001), volumes `/data`
and `/music`, `HEALTHCHECK` on `/api/v1/health`. Build args `YTDLP_VERSION` /
`CLOUDFLARED_VERSION` override the pinned versions.

Machine operations need a local client, and in a container nothing is local: run them with
`docker exec rekord curl -X POST http://127.0.0.1:7420/api/v1/library/scan`, or set
`REKORD_ALLOW_REMOTE_ADMIN=1` on a trusted network.

## Windows / macOS (portable)

`pnpm pack:win:server` → `RE-KORD-Server-<v>-windows-x64.zip` (folder with `RE-KORD Server.exe`, no installer), `pnpm pack:macos`
(on a Mac) → `rekord-server-<v>-macos-<arch>.tar.gz` (`run.sh`). Unpack and run; data goes
to `%APPDATA%\RE-KORD` / `~/Library/Application Support/RE-KORD`. Allow the firewall prompt
for private networks so phones can reach port 7420.

## Desktop "server flavor"

The **RE-KORD Server** app is the desktop client with the hub embedded (cargo feature `hub`
of `rekord-client`), like the 5.0 Electron "Server" app:

```bash
pnpm build:client:server-flavor     # tauri build --features hub --config src-tauri/tauri.hub.conf.json
pnpm dev:client:server-flavor
```

- Separate identifier (`app.rekord.server`, product "RE-KORD Server"): installs next to the
  plain client.
- At startup it runs `rekord_core::run_hub` on its own thread/runtime; on exit it signals a
  graceful shutdown (up to 8 s) before the process ends.
- Configuration: `<app data>/app.rekord.server/hub.json`, written on first start:
  `{ "enabled": true, "bind": "0.0.0.0:7420", "dataDir": null }`. Data defaults to
  `<app data>/app.rekord.server/hub`. Environment variables override the file:
  `REKORD_EMBEDDED_HUB=0` (disable), `REKORD_BIND`, `REKORD_DATA_DIR`.
- The admin panel (`server-ui/dist`) is bundled as a resource and served on `/admin`; the
  client UI is served by Tauri to the window only (LAN browsers get `/admin`; other phones
  and desktops use their own app).
- If port 7420 is taken (a standalone hub already running) the embedded hub logs the error
  and the window connects to the existing hub.
- Not available on Android/iOS (the feature and its dependencies are desktop-only).

## Reverse proxy / HTTPS

Any proxy works; keep `Range`, streaming responses and large uploads (restore up to
512 MiB). If the public origin differs from the hub's `Host`, list it in
`REKORD_ALLOWED_ORIGINS`. Requests carrying `X-Forwarded-For` & co. are never "local", so
machine operations from the proxy need `REKORD_ALLOW_REMOTE_ADMIN=1`. Over HTTPS the web
client is installable as a PWA.
