#!/usr/bin/env bash
# Scarica e verifica gli strumenti che i pacchetti "server" portano con se':
# yt-dlp (download in Studio), cloudflared (tunnel per l'accesso remoto) e ffmpeg
# (transcodifica per il Cast). Finiscono in release/bin/<piattaforma>/.
#
#   scripts/fetch-tools.sh [piattaforma]     linux-x64 | linux-arm64 | windows-x64
#
# Se un file c'e' gia' alla versione fissata non si riscarica.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/lib/fetch-common.sh"

PLATFORM="${1:-$(rk_default_platform)}"
DEST="$RK_FETCH_ROOT/release/bin/$PLATFORM"
EXT=""
[[ "$PLATFORM" == windows-* ]] && EXT=".exe"

# Le versioni fissate stanno negli script dei singoli strumenti: si leggono da li'
# per sapere se la copia gia' scaricata e' quella giusta.
pinned() { sed -n "s/^$1=\"\${$1:-\(.*\)}\"$/\1/p" "$HERE/$2"; }
fresh() { [[ -x "$DEST/$1$EXT" || -f "$DEST/$1$EXT" ]] && grep -qF "$2" "$DEST/$1.version" 2>/dev/null; }

fresh yt-dlp "$(pinned YTDLP_VERSION fetch-ytdlp.sh)" || bash "$HERE/fetch-ytdlp.sh" "$PLATFORM" "$DEST"
fresh cloudflared "$(pinned CLOUDFLARED_VERSION fetch-cloudflared.sh)" || bash "$HERE/fetch-cloudflared.sh" "$PLATFORM" "$DEST"
fresh ffmpeg "$(pinned FFMPEG_BUILD fetch-ffmpeg.sh)" || bash "$HERE/fetch-ffmpeg.sh" "$PLATFORM" "$DEST"

echo "Strumenti pronti in $DEST:"
ls -la "$DEST"
