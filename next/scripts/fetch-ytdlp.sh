#!/usr/bin/env bash
# Scarica yt-dlp (binario standalone, nessun Python richiesto) da
# https://github.com/yt-dlp/yt-dlp/releases a una versione fissata e lo verifica.
#
#   scripts/fetch-ytdlp.sh [piattaforma] [cartella]
#     piattaforma: linux-x64 | linux-arm64 | windows-x64 | macos-x64 | macos-arm64
#                  (predefinita: quella di questa macchina)
#     cartella:    dove mettere l'eseguibile (predefinita: release/bin/<piattaforma>)
#
#   YTDLP_VERSION=2025.09.26 scripts/fetch-ytdlp.sh   altra versione
#
# Il checksum: vedi scripts/lib/fetch-common.sh. yt-dlp pubblica SHA2-256SUMS
# accanto ai binari di ogni release.
set -euo pipefail
source "$(dirname "$0")/lib/fetch-common.sh"

# Versione fissata: aggiornarla e' una modifica voluta (e va provata).
YTDLP_VERSION="${YTDLP_VERSION:-2026.08.19}"
PLATFORM="${1:-$(rk_default_platform)}"
DEST="${2:-$RK_FETCH_ROOT/release/bin/$PLATFORM}"

case "$PLATFORM" in
  linux-x64) ASSET=yt-dlp_linux; OUT=yt-dlp ;;
  linux-arm64) ASSET=yt-dlp_linux_aarch64; OUT=yt-dlp ;;
  windows-x64) ASSET=yt-dlp.exe; OUT=yt-dlp.exe ;;
  macos-x64|macos-arm64) ASSET=yt-dlp_macos; OUT=yt-dlp ;;
  *) rk_fetch_die "piattaforma non riconosciuta: $PLATFORM" ;;
esac

rk_need curl awk
BASE="https://github.com/yt-dlp/yt-dlp/releases/download/$YTDLP_VERSION"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> yt-dlp $YTDLP_VERSION ($ASSET) → $DEST/$OUT"
rk_download "$BASE/$ASSET" "$TMP/$ASSET"
UPSTREAM=""
if rk_download "$BASE/SHA2-256SUMS" "$TMP/SHA2-256SUMS" 2>/dev/null; then
  UPSTREAM="$(awk -v a="$ASSET" '$2==a || $2=="*"a {print $1; exit}' "$TMP/SHA2-256SUMS")"
fi
rk_verify yt-dlp "$YTDLP_VERSION" "$ASSET" "$TMP/$ASSET" "$UPSTREAM"

mkdir -p "$DEST"
install -m 0755 "$TMP/$ASSET" "$DEST/$OUT"
echo "$YTDLP_VERSION" > "$DEST/yt-dlp.version"
echo "  fatto: $DEST/$OUT"
