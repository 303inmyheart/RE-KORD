#!/usr/bin/env bash
# Downloads cloudflared (tunnel for remote access) from
# https://github.com/cloudflare/cloudflared/releases at a pinned version and
# verifies it.
#
#   scripts/fetch-cloudflared.sh [platform] [directory]
#     platform:    linux-x64 | linux-arm64 | windows-x64 | macos-x64 | macos-arm64
#     directory:   default release/bin/<platform>
#
#   CLOUDFLARED_VERSION=2025.8.1 scripts/fetch-cloudflared.sh   another version
#
# The published checksum is read from the assets' `digest` field in the GitHub API
# (GITHUB_TOKEN, if set, avoids the anonymous rate limit). See
# scripts/lib/fetch-common.sh. The hub finds cloudflared next to its own
# executable (or in bin/), or via REKORD_CLOUDFLARED_BIN.
set -euo pipefail
source "$(dirname "$0")/lib/fetch-common.sh"

CLOUDFLARED_VERSION="${CLOUDFLARED_VERSION:-2026.9.3}"
PLATFORM="${1:-$(rk_default_platform)}"
DEST="${2:-$RK_FETCH_ROOT/release/bin/$PLATFORM}"

ARCHIVE=0
case "$PLATFORM" in
  linux-x64) ASSET=cloudflared-linux-amd64; OUT=cloudflared ;;
  linux-arm64) ASSET=cloudflared-linux-arm64; OUT=cloudflared ;;
  windows-x64) ASSET=cloudflared-windows-amd64.exe; OUT=cloudflared.exe ;;
  macos-x64) ASSET=cloudflared-darwin-amd64.tgz; OUT=cloudflared; ARCHIVE=1 ;;
  macos-arm64) ASSET=cloudflared-darwin-arm64.tgz; OUT=cloudflared; ARCHIVE=1 ;;
  *) rk_fetch_die "unrecognized platform: $PLATFORM" ;;
esac

rk_need curl awk
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> cloudflared $CLOUDFLARED_VERSION ($ASSET) → $DEST/$OUT"
rk_download "https://github.com/cloudflare/cloudflared/releases/download/$CLOUDFLARED_VERSION/$ASSET" "$TMP/$ASSET"

UPSTREAM=""
API="https://api.github.com/repos/cloudflare/cloudflared/releases/tags/$CLOUDFLARED_VERSION"
AUTH=()
[[ -n "${GITHUB_TOKEN:-}" ]] && AUTH=(-H "Authorization: Bearer $GITHUB_TOKEN")
if curl -fsSL --retry 3 -H "Accept: application/vnd.github+json" -H "User-Agent: rekord-pack" \
  ${AUTH[@]+"${AUTH[@]}"} -o "$TMP/release.json" "$API"; then
  UPSTREAM="$(rk_json_asset_digest "$TMP/release.json" "$ASSET")"
fi
rk_verify cloudflared "$CLOUDFLARED_VERSION" "$ASSET" "$TMP/$ASSET" "$UPSTREAM"

mkdir -p "$DEST"
if (( ARCHIVE )); then
  tar -xzf "$TMP/$ASSET" -C "$TMP"
  install -m 0755 "$TMP/cloudflared" "$DEST/$OUT"
else
  install -m 0755 "$TMP/$ASSET" "$DEST/$OUT"
fi
echo "$CLOUDFLARED_VERSION" > "$DEST/cloudflared.version"
echo "  done: $DEST/$OUT"
