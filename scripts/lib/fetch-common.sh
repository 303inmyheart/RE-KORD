#!/usr/bin/env bash
# Functions shared by fetch-ytdlp.sh and fetch-cloudflared.sh. Meant to be used with `source`.
#
# Every third-party binary is downloaded from an official GitHub release, at a
# pinned version, and verified with SHA-256 before it goes into the package:
#   1. if scripts/third-party.sha256 has a line for (tool, version, file),
#      the hash MUST match (trust pinned in the repo);
#   2. otherwise the checksum published by the project itself is used (yt-dlp:
#      SHA2-256SUMS; cloudflared: the asset's `digest` field in the GitHub API) and
#      the line to add to the file is printed, to pin it from the next time on;
#   3. with no checksum at all, the download is discarded.
# Requires curl and sha256sum (or shasum); python3 or node to read the GitHub API.

RK_FETCH_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RK_PINNED_SUMS="$RK_FETCH_ROOT/scripts/third-party.sha256"

rk_fetch_die() {
  echo "ERROR: $*" >&2
  exit 1
}

rk_need() {
  local cmd
  for cmd in "$@"; do
    command -v "$cmd" >/dev/null 2>&1 || rk_fetch_die "'$cmd' is required (not found in PATH)"
  done
}

rk_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    rk_fetch_die "sha256sum or shasum is required"
  fi
}

# Default platform: that of the machine running the script.
rk_default_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) [[ "$arch" == aarch64 || "$arch" == arm64 ]] && echo linux-arm64 || echo linux-x64 ;;
    Darwin) [[ "$arch" == arm64 ]] && echo macos-arm64 || echo macos-x64 ;;
    MINGW*|MSYS*|CYGWIN*) echo windows-x64 ;;
    *) rk_fetch_die "unrecognized platform: $os/$arch" ;;
  esac
}

# rk_pinned_sum <tool> <version> <asset>  → hash or empty string
rk_pinned_sum() {
  [[ -f "$RK_PINNED_SUMS" ]] || return 0
  awk -v t="$1" -v v="$2" -v a="$3" '$1==t && $2==v && $3==a {print $4; exit}' "$RK_PINNED_SUMS"
}

# rk_download <url> <dest>
rk_download() {
  curl -fL --retry 3 --retry-delay 2 --connect-timeout 20 --max-time 900 \
    -H "User-Agent: rekord-pack" -o "$2" "$1"
}

# rk_json_asset_digest <release-json-file> <asset-name>  → sha256 or empty
rk_json_asset_digest() {
  local script='
import json,sys
data=json.load(open(sys.argv[1]))
for a in data.get("assets",[]):
    if a.get("name")==sys.argv[2]:
        d=a.get("digest") or ""
        print(d.split(":",1)[1] if d.startswith("sha256:") else "")
        break
'
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "$script" "$1" "$2"
  elif command -v node >/dev/null 2>&1; then
    node -e '
const fs=require("fs");const [f,n]=process.argv.slice(1);
const a=(JSON.parse(fs.readFileSync(f,"utf8")).assets||[]).find(x=>x.name===n);
const d=(a&&a.digest)||"";console.log(d.startsWith("sha256:")?d.slice(7):"");' "$1" "$2"
  else
    rk_fetch_die "python3 or node is required to read the GitHub API"
  fi
}

# rk_verify <tool> <version> <asset> <file> <upstream-sha-or-empty>
rk_verify() {
  local tool="$1" version="$2" asset="$3" file="$4" upstream="$5"
  local actual pinned
  actual="$(rk_sha256 "$file")"
  pinned="$(rk_pinned_sum "$tool" "$version" "$asset")"
  if [[ -n "$pinned" ]]; then
    [[ "$actual" == "$pinned" ]] || rk_fetch_die "$asset: SHA-256 $actual differs from the one pinned in third-party.sha256 ($pinned)"
    echo "  sha256 ok (pinned in the repo)"
    return 0
  fi
  [[ -n "$upstream" ]] || rk_fetch_die "$asset: no checksum available (neither pinned nor published): discarding the file"
  [[ "$actual" == "$upstream" ]] || rk_fetch_die "$asset: SHA-256 $actual differs from the published one ($upstream)"
  echo "  sha256 ok (published by the project). To pin it, add to scripts/third-party.sha256:"
  echo "    $tool $version $asset $actual"
}
