#!/usr/bin/env bash
# Pacchetti di RE-KORD: un comando per piattaforma e versione, come nella 5.0.
#
#   pnpm pack:linux:server   release/linux/
#                              RE-KORD-Server-<v>-linux-x64.AppImage / .deb   app desktop con l'hub
#                              RE-KORD-Server-<v>-linux-x64-headless.tar.gz   hub senza finestra (systemd)
#   pnpm pack:linux:client   release/linux/
#                              RE-KORD-Client-<v>-linux-x64.AppImage / .deb
#   pnpm pack:win:server     release/windows/
#                              RE-KORD-Server-<v>-windows-x64.zip   cartella con "RE-KORD Server.exe"
#                                                                   (hub incorporato), niente installer
#   pnpm pack:win:client     release/windows/
#                              RE-KORD-Client-<v>-windows-x64.exe   exe unico, si avvia e basta
#   pnpm pack:android        release/android/
#                              RE-KORD-Client-<v>-android-arm64.apk   (build ottimizzata; firmata
#                              con keystore.properties se c'e', altrimenti con la chiave di debug)
#   pnpm pack:all            tutto quanto sopra
#
#   scripts/pack.sh <linux|windows|android|all> [server|client|all] [opzioni]
#     --no-tools   versione server senza yt-dlp, cloudflared e ffmpeg (usa quelli del sistema)
#     --native     niente Docker: serve la toolchain completa su questa macchina
#
# Linux e Windows si compilano in un container Docker (scripts/docker/builder.Dockerfile)
# che ha webkit2gtk, GStreamer e cargo-xwin: su questa macchina basta Docker. Lo si salta
# con --native, oppure da solo su Linux quando webkit2gtk-4.1 e' gia' installato.
# Android usa l'SDK/NDK locale (scripts/lib/android-env.sh).
#
# Ogni esecuzione aggiorna release/<piattaforma>/SHA256SUMS; il log completo e'
# release/<piattaforma>/build.log.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

usage() { awk 'NR>1 && /^#/ { sub(/^# ?/, ""); print; next } NR>1 { exit }' "$0"; }
die() { echo "ERRORE: $*" >&2; exit 1; }
step() { echo; echo "==> $*"; }

PLATFORM=""
FLAVOR=all
TOOLS=1
NATIVE=0
ARGS=("$@")
# Prima parola: piattaforma; seconda (facoltativa): versione. "all" vale per entrambe,
# quindi conta la posizione e non il valore.
POSITIONAL=0
while (( $# )); do
  if [[ "$1" != -* ]]; then
    POSITIONAL=$((POSITIONAL + 1))
    case "$POSITIONAL:$1" in
      1:linux|1:windows|1:android|1:all) PLATFORM="$1" ;;
      1:win) PLATFORM=windows ;;
      2:server|2:client|2:all) FLAVOR="$1" ;;
      1:*) die "piattaforma non riconosciuta: $1 (linux, windows, android, all)" ;;
      2:*) die "versione non riconosciuta: $1 (server, client, all)" ;;
      *) die "argomento in piu': $1" ;;
    esac
    shift
    continue
  fi
  case "$1" in
    --no-tools) TOOLS=0 ;;
    --native) NATIVE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) die "opzione non riconosciuta: $1 (--help per l'elenco)" ;;
  esac
  shift
done
[[ -n "$PLATFORM" ]] || { usage; exit 2; }

passthrough=()
(( TOOLS )) || passthrough+=(--no-tools)
(( NATIVE )) && passthrough+=(--native)

if [[ "$PLATFORM" == all ]]; then
  "$0" linux "$FLAVOR" "${passthrough[@]}"
  "$0" windows "$FLAVOR" "${passthrough[@]}"
  if [[ "$FLAVOR" != server ]]; then "$0" android "${passthrough[@]}"; fi
  exit 0
fi

VERSION="$(node -p 'require("./package.json").version')"
TRIPLE_WIN=x86_64-pc-windows-msvc
TOOLS_PLATFORM=""
case "$PLATFORM" in
  linux) OUT="$ROOT/release/linux"; TOOLS_PLATFORM=linux-x64 ;;
  windows) OUT="$ROOT/release/windows"; TOOLS_PLATFORM=windows-x64 ;;
  android) OUT="$ROOT/release/android"; FLAVOR=client ;;
esac
mkdir -p "$OUT"
want() { [[ "$FLAVOR" == all || "$FLAVOR" == "$1" ]]; }

# --- Fuori dal container: preparazione, poi (se serve) si rientra in Docker -------
if [[ -z "${REKORD_BUILDER:-}" ]]; then
  LOG="$OUT/build.log"
  : > "$LOG"
  exec > >(tee -a "$LOG") 2>&1
  echo "RE-KORD $VERSION — $PLATFORM ($FLAVOR) — $(date -Is)"

  step "Controllo versioni"
  node scripts/version.mjs check "$VERSION"

  step "Dipendenze JS (lockfile bloccato)"
  pnpm install --frozen-lockfile

  if [[ "$PLATFORM" != android ]] && want server && (( TOOLS )); then
    step "yt-dlp, cloudflared, ffmpeg per $TOOLS_PLATFORM (download verificato)"
    bash scripts/fetch-tools.sh "$TOOLS_PLATFORM"
  fi

  use_docker=0
  if [[ "$PLATFORM" == windows && "$(uname -s)" == Linux ]] && (( ! NATIVE )); then
    use_docker=1
  elif [[ "$PLATFORM" == linux ]] && (( ! NATIVE )); then
    pkg-config --exists webkit2gtk-4.1 dbus-1 2>/dev/null || use_docker=1
  fi

  if (( use_docker )); then
    command -v docker >/dev/null || die "serve Docker (o --native con la toolchain installata)"
    DOCKERFILE="$ROOT/scripts/docker/builder.Dockerfile"
    # Il tag segue il contenuto del Dockerfile: cambiato quello, l'immagine si rifa'.
    IMAGE="rekord-builder:$(sha256sum "$DOCKERFILE" | cut -c1-12)"
    if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
      step "Immagine di build $IMAGE (solo la prima volta, qualche minuto)"
      docker build -f "$DOCKERFILE" -t "$IMAGE" "$ROOT/scripts/docker"
    fi
    CACHE="${REKORD_BUILDER_CACHE:-$HOME/.cache/rekord-builder}"
    mkdir -p "$CACHE/cargo" "$CACHE/xwin" "$CACHE/home"
    REPO="$ROOT"
    step "Build in Docker ($IMAGE)"
    tty=()
    [[ -t 0 ]] && tty=(-t)
    # Stesso percorso assoluto dentro e fuori: i link di pnpm e i percorsi nei log
    # restano validi. Target separato da quello dell'host (glibc diversa).
    docker run --rm "${tty[@]}" \
      --user "$(id -u):$(id -g)" \
      -v "$REPO:$REPO" -w "$ROOT" \
      -v "$CACHE:/cache" \
      -e CARGO_TARGET_DIR="$ROOT/target/docker" \
      -e CI=true \
      "$IMAGE" bash scripts/pack.sh "$PLATFORM" "$FLAVOR" "${passthrough[@]}"
  else
    REKORD_BUILDER=native bash "$0" "${ARGS[@]}"
  fi

  step "Checksum"
  (
    cd "$OUT"
    find . -maxdepth 1 -type f -name 'RE-KORD-*' -printf '%f\0' | sort -z | xargs -0 -r sha256sum > SHA256SUMS
    cat SHA256SUMS
  )
  echo
  echo "Fatto: $OUT (log: $LOG)"
  exit 0
fi

# --- Dentro il container (o in nativo): la build vera ----------------------------
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
MARKER="$(mktemp)"
trap 'rm -f "$MARKER"' EXIT

tauri() { pnpm --filter @rekord/client-shell exec tauri "$@"; }

# copy_bundles <cartella bundle> <nome finale senza estensione> <estensioni...>
copy_bundles() {
  local dir="$1" base="$2"; shift 2
  local ext f dest found=0
  for ext in "$@"; do
    f="$(find "$dir" -type f -name "*.$ext" -newer "$MARKER" 2>/dev/null | head -n1)"
    [[ -n "$f" ]] || continue
    dest="$OUT/$base.$ext"
    [[ "$ext" == exe ]] && dest="$OUT/$base-setup.exe"
    cp -f "$f" "$dest"
    echo "  $dest"
    found=1
  done
  (( found )) || die "nessun pacchetto prodotto in $dir"
}

server_flavor_args() {
  printf '%s\n' --features hub --config src-tauri/tauri.hub.conf.json
  if (( TOOLS )); then
    # Gli strumenti finiscono tra le risorse dell'app, in bin/ (embedded_hub.rs).
    printf '%s\n' --config \
      "{\"bundle\":{\"resources\":{\"../../../release/bin/$TOOLS_PLATFORM/\":\"bin/\"}}}"
  fi
}

stage_headless_tools() {
  local dest="$1"
  (( TOOLS )) || return 0
  mkdir -p "$dest/bin"
  cp -a "$ROOT/release/bin/$TOOLS_PLATFORM/." "$dest/bin/"
}

linux_client() {
  step "Client desktop Linux (deb + AppImage)"
  touch "$MARKER"
  tauri build --bundles deb,appimage
  copy_bundles "$TARGET_DIR/release/bundle" "RE-KORD-Client-$VERSION-linux-x64" AppImage deb
}

linux_server() {
  step "Server desktop Linux, hub incorporato (deb + AppImage)"
  touch "$MARKER"
  local flavor
  mapfile -t flavor < <(server_flavor_args)
  tauri build --bundles deb,appimage "${flavor[@]}"
  copy_bundles "$TARGET_DIR/release/bundle" "RE-KORD-Server-$VERSION-linux-x64" AppImage deb

  step "Hub headless Linux (tarball + systemd)"
  pnpm build:ui
  cargo build -p rekord-server --release --locked
  local name="RE-KORD-Server-$VERSION-linux-x64-headless"
  # Si prepara fuori dalla cartella condivisa: con Docker Desktop i permessi dei
  # file sul volume montato non si possono cambiare (chmod rifiutato).
  local stage_root
  stage_root="$(mktemp -d)"
  local stage="$stage_root/$name"
  mkdir -p "$stage/systemd"
  install -m 0755 "$TARGET_DIR/release/rekord-server" "$stage/rekord-server"
  cp -a apps/client-ui/dist "$stage/client-ui"
  cp -a apps/server-ui/dist "$stage/admin-ui"
  cp -f modules.manifest.toml "$stage/modules.manifest.toml"
  cp -f scripts/linux/rekord-server.service scripts/linux/rekord-server.env scripts/linux/install.sh "$stage/systemd/"
  stage_headless_tools "$stage"
  echo "$VERSION" > "$stage/VERSION"
  cat > "$stage/run.sh" <<'EOF'
#!/usr/bin/env bash
# Avvia l'hub RE-KORD con le interfacce di questo pacchetto.
# Opzioni e variabili: ./rekord-server --help (REKORD_BIND, REKORD_DATA_DIR, ...).
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
# yt-dlp, cloudflared e ffmpeg del pacchetto, se ci sono.
if [[ -z "${YTDLP_PATH:-}" && -x "$DIR/bin/yt-dlp" ]]; then export YTDLP_PATH="$DIR/bin/yt-dlp"; fi
if [[ -z "${REKORD_CLOUDFLARED_BIN:-}" && -x "$DIR/bin/cloudflared" ]]; then export REKORD_CLOUDFLARED_BIN="$DIR/bin/cloudflared"; fi
if [[ -z "${REKORD_FFMPEG:-}" && -x "$DIR/bin/ffmpeg" ]]; then export REKORD_FFMPEG="$DIR/bin/ffmpeg"; fi
exec "$DIR/rekord-server" \
  --client-ui "${REKORD_CLIENT_UI:-$DIR/client-ui}" \
  --admin-ui "${REKORD_ADMIN_UI:-$DIR/admin-ui}" \
  "$@"
EOF
  chmod +x "$stage/run.sh"
  cat > "$stage/README.txt" <<EOF
RE-KORD hub $VERSION (Linux x64, senza finestra)

Prova al volo:
  ./run.sh                       http://<questa-macchina>:7420  (pannello: /admin)
  REKORD_BIND=127.0.0.1:7420 ./run.sh

Come servizio (utente dedicato, avvio automatico, dati in /var/lib/rekord):
  sudo ./systemd/install.sh

bin/ contiene yt-dlp, cloudflared e ffmpeg (LGPL, licenza in bin/ffmpeg-LICENSE.txt);
senza, l'hub usa quelli nel PATH. Per l'app con finestra usa RE-KORD-Server-*.AppImage.
EOF
  tar -C "$stage_root" -czf "$OUT/$name.tar.gz" "$name"
  rm -rf "$stage_root"
  echo "  $OUT/$name.tar.gz"
}

# Windows senza installer, come la 5.0: l'exe si avvia e basta. WebView2 c'e' gia' su
# Windows 10/11 aggiornati; il loader e' collegato staticamente nell'exe.
windows_tauri() {
  tauri build --runner cargo-xwin --target "$TRIPLE_WIN" --no-bundle "$@"
}

windows_client() {
  step "Client desktop Windows (exe portatile)"
  windows_tauri
  local dest="$OUT/RE-KORD-Client-$VERSION-windows-x64.exe"
  cp -f "$TARGET_DIR/$TRIPLE_WIN/release/rekord-client.exe" "$dest"
  echo "  $dest"
}

windows_server() {
  step "Server desktop Windows, hub incorporato (cartella portatile in zip)"
  local flavor
  mapfile -t flavor < <(server_flavor_args)
  windows_tauri "${flavor[@]}"
  pnpm build:server-ui
  # L'app cerca interfacce e strumenti accanto all'exe (resource_dir su Windows).
  local name="RE-KORD-Server-$VERSION-windows-x64"
  # Si prepara fuori dalla cartella condivisa: con Docker Desktop i permessi dei
  # file sul volume montato non si possono cambiare (chmod rifiutato).
  local stage_root
  stage_root="$(mktemp -d)"
  local stage="$stage_root/RE-KORD Server"
  mkdir -p "$stage"
  cp -f "$TARGET_DIR/$TRIPLE_WIN/release/rekord-server-app.exe" "$stage/RE-KORD Server.exe"
  cp -a apps/server-ui/dist "$stage/admin-ui"
  cp -a apps/client-ui/dist "$stage/client-ui"
  stage_headless_tools "$stage"
  printf '%s\r\n' \
    "RE-KORD Server $VERSION (Windows x64)" \
    '' \
    'Avvio: doppio clic su "RE-KORD Server.exe". La finestra e'' il client; l''hub parte' \
    'insieme e resta raggiungibile dagli altri dispositivi su http://<IP-di-questo-PC>:7420' \
    '(pannello: /admin). Al primo avvio Windows chiede di consentire l''accesso alla rete:' \
    'rispondi si'' per le reti private.' \
    '' \
    'Tieni insieme l''exe e le cartelle admin-ui, client-ui e bin: si puo'' spostare tutta' \
    'la cartella dove vuoi. bin\ contiene yt-dlp, cloudflared e ffmpeg (LGPL, licenza in' \
    'bin\ffmpeg-LICENSE.txt). Dati: %APPDATA%\app.rekord.server.' \
    > "$stage/LEGGIMI.txt"
  rm -f "$OUT/$name.zip"
  (cd "$stage_root" && zip -qr "$OUT/$name.zip" "RE-KORD Server")
  rm -rf "$stage_root"
  echo "  $OUT/$name.zip"
}

android_client() {
  step "Client Android (APK arm64, build ottimizzata)"
  touch "$MARKER"
  bash scripts/android-build.sh --release
  local apk
  apk="$(find apps/client-shell/src-tauri/gen/android/app/build/outputs/apk -type f -name '*.apk' -newer "$MARKER" | head -n1)"
  [[ -n "$apk" ]] || die "nessun APK prodotto"
  cp -f "$apk" "$OUT/RE-KORD-Client-$VERSION-android-arm64.apk"
  echo "  $OUT/RE-KORD-Client-$VERSION-android-arm64.apk"
}

case "$PLATFORM" in
  linux)
    if want client; then linux_client; fi
    if want server; then linux_server; fi
    ;;
  windows)
    if want client; then windows_client; fi
    if want server; then windows_server; fi
    ;;
  android) android_client ;;
esac
