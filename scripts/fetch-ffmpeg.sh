#!/usr/bin/env bash
# Downloads ffmpeg (LGPL static build from https://github.com/BtbN/FFmpeg-Builds) at a
# pinned release, verifies it and extracts only the `ffmpeg` executable (plus the
# license): the hub needs it to transcode for Chromecast / Google Home.
#
#   scripts/fetch-ffmpeg.sh [platform] [directory]
#     platform:    linux-x64 | linux-arm64 | windows-x64
#                  (default: this machine's; on macOS use Homebrew)
#     directory:   where to put the executable (default: release/bin/<platform>)
#
#   FFMPEG_TAG=autobuild-YYYY-MM-DD-HH-MM FFMPEG_BUILD=n8.1.3-14-g330caae0c1 \
#     scripts/fetch-ffmpeg.sh                  another build
#
# The LGPL build is enough: it decodes FLAC/OGG/Opus/WAV and encodes MP3 (libmp3lame)
# and AAC (native encoder). Checksums: see scripts/lib/fetch-common.sh; BtbN
# publishes `checksums.sha256` and each asset's `digest` field in the GitHub API.
set -euo pipefail
source "$(dirname "$0")/lib/fetch-common.sh"

# Pinned release: bumping it is a deliberate change (and must be tested).
FFMPEG_TAG="${FFMPEG_TAG:-autobuild-2026-10-04-20-51}"
FFMPEG_BUILD="${FFMPEG_BUILD:-n8.1.3-14-g330caae0c1}"
FFMPEG_SERIES="${FFMPEG_SERIES:-8.1}"
PLATFORM="${1:-$(rk_default_platform)}"
DEST="${2:-$RK_FETCH_ROOT/release/bin/$PLATFORM}"

case "$PLATFORM" in
  linux-x64) ASSET="ffmpeg-$FFMPEG_BUILD-linux64-lgpl-$FFMPEG_SERIES.tar.xz"; OUT=ffmpeg ;;
  linux-arm64) ASSET="ffmpeg-$FFMPEG_BUILD-linuxarm64-lgpl-$FFMPEG_SERIES.tar.xz"; OUT=ffmpeg ;;
  windows-x64) ASSET="ffmpeg-$FFMPEG_BUILD-win64-lgpl-$FFMPEG_SERIES.zip"; OUT=ffmpeg.exe ;;
  macos-*) rk_fetch_die "no official static build for macOS: brew install ffmpeg" ;;
  *) rk_fetch_die "unrecognized platform: $PLATFORM" ;;
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
[[ -n "$BIN" ]] || rk_fetch_die "$OUT not found inside $ASSET"
LICENSE="$(find "$TMP/x" -maxdepth 2 -type f -name 'LICENSE*' | head -n1)"

mkdir -p "$DEST"
install -m 0755 "$BIN" "$DEST/$OUT"
[[ -n "$LICENSE" ]] && install -m 0644 "$LICENSE" "$DEST/ffmpeg-LICENSE.txt"
echo "$FFMPEG_BUILD ($FFMPEG_TAG, LGPL)" > "$DEST/ffmpeg.version"
echo "  done: $DEST/$OUT"
