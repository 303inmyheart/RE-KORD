# RE-KORD

Nuova implementazione modulare (staging in `next/`). Stesso nome prodotto; sostituirà l’app attuale quando pronta.

## Architettura

| App | Ruolo |
|-----|--------|
| **rekord-server** | Backend API + SQLite + scan + stream media + SPA client (stesso origin) |
| **client-ui** | Frontend completo (player, libreria, preferiti, playlist) via API |
| **client-shell** | Shell Tauri 2 (desktop + Android) che impacchetta `client-ui`; con la feature `hub` diventa "RE-KORD Server" (hub incorporato) |
| **@rekord/ui** | Componenti grafici condivisi (Button, Panel, Field, …) |

In produzione / accesso remoto il hub serve `client-ui` su `/` (same-origin con `/api/v1` e `/media`), come il server legacy. In dev puoi ancora usare Vite su `:7422` con proxy.

### UI components

Tutti gli elementi grafici passano da componenti in `packages/ui` (condivisi) o da componenti di app in `apps/*/src/components`. Le view restano sottili e orchestrano solo layout + stato.

## Requisiti

- Rust (stable recente; la CI usa `stable`)
- Node 20+ / pnpm 9 (`corepack enable`)
- ffmpeg nel PATH sull'hub (transcodifica per Cast, anteprime)
- Per il client desktop: dipendenze Tauri 2 ([docs](https://v2.tauri.app/start/prerequisites/)).
  Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
  libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libssl-dev patchelf file`
- Per Android: Android SDK + NDK e JDK 17+ (vedi [docs/ANDROID.md](docs/ANDROID.md))

## Avvio rapido (dev)

```bash
cd next
pnpm install

# Terminale 1 — server API
cargo run -p rekord-server

# Terminale 2 — pannello hub (opzionale in dev: http://127.0.0.1:7421/admin/)
pnpm dev:server-ui

# Terminale 3 — client UI nel browser
pnpm dev:client-ui
# oppure shell Tauri:
pnpm --filter @rekord/client-shell tauri dev
```

Default hub: `http://127.0.0.1:7420` (bind `0.0.0.0:7420` → raggiungibile in LAN).  
Per solo localhost: `REKORD_BIND=127.0.0.1:7420`.

1. Apri il client su `http://127.0.0.1:7420/` (UI servita dal hub se `apps/client-ui/dist` è presente) oppure Vite (`pnpm dev:client-ui`)
2. Il pannello hub è su `http://127.0.0.1:7420/admin`: cartella musica, struttura libreria, scansioni, job, diagnostica, log, backup, account, integrazioni e rete
3. Imposta la libreria dal pannello hub (o `PUT /api/v1/library/path`), avvia lo scan e ascolta
4. Accesso remoto: pannello hub → Rete (URL in LAN / tunnel Cloudflare) — apri l’URL dal telefono; API e UI sullo stesso origin
5. Le operazioni che toccano la macchina (cartella musica, scansioni, credenziali, ripristini, tunnel) richiedono l’account Default e una richiesta locale; per abilitarle da remoto usa l’interruttore in Rete → Operazioni di macchina

## Build e pacchetti

Un comando per piattaforma e versione, come nella 5.0. I pacchetti finiscono in
`release/<piattaforma>/` con `SHA256SUMS` e `build.log`.

| Comando | Cosa produce |
|---------|--------------|
| `pnpm pack:linux:server` | `RE-KORD-Server-<v>-linux-x64.AppImage` / `.deb` (app con hub incorporato) + `…-headless.tar.gz` (hub senza finestra, systemd) |
| `pnpm pack:linux:client` | `RE-KORD-Client-<v>-linux-x64.AppImage` / `.deb` |
| `pnpm pack:win:server` | `RE-KORD-Server-<v>-windows-x64.zip`: cartella con `RE-KORD Server.exe` (hub incorporato), si estrae e si avvia, nessun installer |
| `pnpm pack:win:client` | `RE-KORD-Client-<v>-windows-x64.exe`: exe unico, si avvia e basta |
| `pnpm pack:android` | `RE-KORD-Client-<v>-android-arm64.apk` (ottimizzato; firmato con `keystore.properties` se presente, altrimenti con la chiave di debug) |
| `pnpm pack:all` | tutto quanto sopra |
| `pnpm pack:macos` | solo su un Mac: tarball hub + `.dmg` |

- Le versioni **server** includono yt-dlp, cloudflared e ffmpeg (LGPL), scaricati dalle
  release ufficiali a versioni fissate e verificati con gli hash di
  `scripts/third-party.sha256`. `-- --no-tools` li toglie (l'hub usa quelli del PATH).
- **Linux e Windows** si compilano in un container Docker
  (`scripts/docker/builder.Dockerfile`, Ubuntu 24.04: webkit2gtk, GStreamer, cargo-xwin): sulla macchina
  basta Docker. La prima volta l'immagine richiede qualche minuto e cargo-xwin scarica
  la CRT/Windows SDK di Microsoft (accettandone la licenza); le cache stanno in
  `~/.cache/rekord-builder`. Con `-- --native` si usa la toolchain locale.
- **Android** usa SDK/NDK locali ([docs/ANDROID.md](docs/ANDROID.md)).

Build singole per lo sviluppo:

```bash
pnpm build:ui
pnpm build:server
pnpm build:client                 # client desktop Tauri (bundle in target/release/bundle)
pnpm build:client:server-flavor   # "RE-KORD Server": client + hub incorporato
pnpm android:build --install      # APK debug arm64 sul telefono collegato
docker compose up -d --build      # hub in container (porta 7420, volumi /data e /music)
```

Dettagli su systemd, Docker, server flavor e reverse proxy in [docs/DEPLOY.md](docs/DEPLOY.md).

### Versione

Una sola versione per tutto (package.json, Cargo workspace, Cargo.lock, `version.ts`):

```bash
pnpm version:sync 5.2.0   # scrive ovunque
pnpm version:check        # verifica (anche in CI)
```

`tauri.conf.json` non porta una versione: Tauri la eredita da `Cargo.toml` (e da li'
anche il versionCode Android). L'hub annuncia in `/api/v1/health` `version`,
`apiVersion` e `minClientVersion`; i client impacchettati (desktop, Android) mostrano un
avviso quando sono troppo vecchi per l'hub o quando c'e' una versione piu' nuova.

### Client desktop: note di piattaforma

- CSP attiva (`tauri.conf.json`): script solo dal bundle, `connect-src`/`img-src`/
  `media-src` aperti a `http:`/`https:` per raggiungere qualunque hub in LAN o remoto.
- Link esterni e `window.open` verso altri siti si aprono nel browser di sistema
  (plugin opener); i file generati nella pagina (backup, profilo, tema) si salvano con
  una finestra «Salva con nome» (desktop) o in Download (Android).
- Una sola istanza: il secondo avvio porta in primo piano la finestra esistente.
- Hub in chiaro (`http://192.168.x.x:7420`) da UI `tauri://localhost`: su WebKitGTK/macOS
  c'e' un rischio di blocco "mixed content". Procedura di prova in
  [docs/UPGRADE-FROM-5.0.md](docs/UPGRADE-FROM-5.0.md#7-known-risk-desktop-apps-and-plain-http-hubs-mixed-content).

### Client web (PWA)

Servito dall'hub su `/`, e' installabile come app (manifest + service worker che mette in
cache solo il guscio dell'app; API, audio e copertine sempre dalla rete). I service worker
esistono solo in contesti sicuri: HTTPS (tunnel, reverse proxy) o `localhost`; su
`http://<ip-lan>:7420` il client funziona uguale ma non si installa. I font sono quelli di
sistema (nessuna richiesta a Google Fonts).

## CI

`.github/workflows/next.yml` (push/PR che toccano `next/`): versioni, `cargo fmt --check`,
`clippy -D warnings`, `cargo test`, `pnpm -r check|test`, build UI, bundle Tauri Linux,
APK Android debug, build dell'immagine Docker.

## Dalla 5.0

Porta 3001 → 7420, cartella dati, restore del backup, reinstallazione dei client (Android:
disinstallare prima, cambia la chiave di firma), volumi Docker: vedi
[docs/UPGRADE-FROM-5.0.md](docs/UPGRADE-FROM-5.0.md).

Il progetto Android nativo è versionato in `apps/client-shell/src-tauri/gen/android`:
dettagli e firma di release in [docs/ANDROID.md](docs/ANDROID.md).

## Moduli opzionali

Vedi [docs/MODULES.md](docs/MODULES.md). Tutti disabilitati nell’MVP.

## API

Vedi [docs/API.md](docs/API.md).
