#!/usr/bin/env bash
# Scarica ffmpeg (build statica LGPL di https://github.com/BtbN/FFmpeg-Builds) a una
# release fissata, la verifica e ne estrae solo l'eseguibile `ffmpeg` (piu' la
# licenza): all'hub serve per la transcodifica verso Chromecast / Google Home.
#
#   scripts/fetch-ffmpeg.sh [piattaforma] [cartella]
#     piattaforma: linux-x64 | linux-arm64 | windows-x64
#                  (predefinita: quella di questa macchina; su macOS usare Homebrew)
#     cartella:    dove mettere l'eseguibile (predefinita: release/bin/<piattaforma>)
#
#   FFMPEG_TAG=autobuild-AAAA-MM-GG-HH-MM FFMPEG_BUILD=n8.1.3-14-g330caae0c1 \
#     scripts/fetch-ffmpeg.sh                  altra build
#
# La build LGPL basta: decodifica FLAC/OGG/Opus/WAV e codifica MP3 (libmp3lame)
# e AAC (encoder nativo). Il checksum: vedi scripts/lib/fetch-common.sh; BtbN
# pubblica `checksums.sha256` e il campo `digest` di ogni asset nell'API GitHub.
set -euo pipefail
source "$(dirname "$0")/lib/fetch-common.sh"

# Release fissata: aggiornarla e' una modifica voluta (e va provata).
FFMPEG_TAG="${FFMPEG_TAG:-autobuild-2026-10-04-20-51}"
FFMPEG_BUILD="${FFMPEG_BUILD:-n8.1.3-14-g330caae0c1}"
FFMPEG_SERIES="${FFMPEG_SERIES:-8.1}"
PLATFORM="${1:-$(rk_default_platform)}"
DEST="${2:-$RK_FETCH_ROOT/release/bin/$PLATFORM}"

case "$PLATFORM" in
  linux-x64) ASSET="ffmpeg-$FFMPEG_BUILD-linux64-lgpl-$FFMPEG_SERIES.tar.xz"; OUT=ffmpeg ;;
  linux-arm64) ASSET="ffmpeg-$FFMPEG_BUILD-linuxarm64-lgpl-$FFMPEG_SERIES.tar.xz"; OUT=ffmpeg ;;
  windows-x64) ASSET="ffmpeg-$FFMPEG_BUILD-win64-lgpl-$FFMPEG_SERIES.zip"; OUT=ffmpeg.exe ;;
  macos-*) rk_fetch_die "nessuna build statica ufficiale per macOS: brew install ffmpeg" ;;
  *) rk_fetch_die "piattaforma non riconosciuta: $PLATFORM" ;;
esac

rk_need curl awk
case "$ASSET" in
  *.zip) rk_need unzip ;;
  *) rk_need tar xz ;;
esac
BASE="https://github.com/BtbN/FFmpeg-Builds/releases/download/$FFMPEG_TAG"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> ffmpeg $FFMPEG_BUILD ($ASSET) → $DEST/$OUT"
rk_download "$BASE/$ASSET" "$TMP/$ASSET"
UPSTREAM=""
if rk_download "$BASE/checksums.sha256" "$TMP/checksums.sha256" 2>/dev/null; then
  UPSTREAM="$(awk -v a="$ASSET" '$2==a || $2=="*"a {print $1; exit}' "$TMP/checksums.sha256")"
fi
if [[ -z "$UPSTREAM" ]] &&
  rk_download "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/tags/$FFMPEG_TAG" "$TMP/release.json" 2>/dev/null; then
  UPSTREAM="$(rk_json_asset_digest "$TMP/release.json" "$ASSET")"
fi
rk_verify ffmpeg "$FFMPEG_BUILD" "$ASSET" "$TMP/$ASSET" "$UPSTREAM"

mkdir -p "$TMP/x"
case "$ASSET" in
  *.zip) unzip -q "$TMP/$ASSET" -d "$TMP/x" ;;
  *) tar -xJf "$TMP/$ASSET" -C "$TMP/x" ;;
esac
BIN="$(find "$TMP/x" -type f -path "*/bin/$OUT" | head -n1)"
[[ -n "$BIN" ]] || rk_fetch_die "$OUT non trovato dentro $ASSET"
LICENSE="$(find "$TMP/x" -maxdepth 2 -type f -name 'LICENSE*' | head -n1)"

mkdir -p "$DEST"
install -m 0755 "$BIN" "$DEST/$OUT"
[[ -n "$LICENSE" ]] && install -m 0644 "$LICENSE" "$DEST/ffmpeg-LICENSE.txt"
echo "$FFMPEG_BUILD ($FFMPEG_TAG, LGPL)" > "$DEST/ffmpeg.version"
echo "  fatto: $DEST/$OUT"
