#!/usr/bin/env bash
# Funzioni comuni a fetch-ytdlp.sh e fetch-cloudflared.sh. Da usare con `source`.
#
# Ogni binario di terze parti si scarica da una release GitHub ufficiale, a una
# versione fissata, e si verifica con SHA-256 prima di metterlo nel pacchetto:
#   1. se scripts/third-party.sha256 ha una riga per (strumento, versione, file),
#      l'hash DEVE combaciare (fiducia fissata nel repo);
#   2. altrimenti si usa il checksum pubblicato dal progetto stesso (yt-dlp:
#      SHA2-256SUMS; cloudflared: campo `digest` dell'asset nell'API GitHub) e si
#      stampa la riga da aggiungere al file, per fissarla dalla volta dopo;
#   3. senza nessun checksum il download si scarta.
# Servono curl e sha256sum (o shasum); per leggere l'API GitHub python3 o node.

RK_FETCH_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RK_PINNED_SUMS="$RK_FETCH_ROOT/scripts/third-party.sha256"

rk_fetch_die() {
  echo "ERRORE: $*" >&2
  exit 1
}

rk_need() {
  local cmd
  for cmd in "$@"; do
    command -v "$cmd" >/dev/null 2>&1 || rk_fetch_die "serve '$cmd' (non trovato nel PATH)"
  done
}

rk_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    rk_fetch_die "serve sha256sum o shasum"
  fi
}

# Piattaforma predefinita: quella della macchina che lancia lo script.
rk_default_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) [[ "$arch" == aarch64 || "$arch" == arm64 ]] && echo linux-arm64 || echo linux-x64 ;;
    Darwin) [[ "$arch" == arm64 ]] && echo macos-arm64 || echo macos-x64 ;;
    MINGW*|MSYS*|CYGWIN*) echo windows-x64 ;;
    *) rk_fetch_die "piattaforma non riconosciuta: $os/$arch" ;;
  esac
}

# rk_pinned_sum <tool> <version> <asset>  → hash o stringa vuota
rk_pinned_sum() {
  [[ -f "$RK_PINNED_SUMS" ]] || return 0
  awk -v t="$1" -v v="$2" -v a="$3" '$1==t && $2==v && $3==a {print $4; exit}' "$RK_PINNED_SUMS"
}

# rk_download <url> <dest>
rk_download() {
  curl -fL --retry 3 --retry-delay 2 --connect-timeout 20 --max-time 900 \
    -H "User-Agent: rekord-pack" -o "$2" "$1"
}

# rk_json_asset_digest <release-json-file> <asset-name>  → sha256 o vuoto
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
    rk_fetch_die "serve python3 o node per leggere l'API GitHub"
  fi
}

# rk_verify <tool> <version> <asset> <file> <upstream-sha-or-empty>
rk_verify() {
  local tool="$1" version="$2" asset="$3" file="$4" upstream="$5"
  local actual pinned
  actual="$(rk_sha256 "$file")"
  pinned="$(rk_pinned_sum "$tool" "$version" "$asset")"
  if [[ -n "$pinned" ]]; then
    [[ "$actual" == "$pinned" ]] || rk_fetch_die "$asset: SHA-256 $actual diverso da quello fissato in third-party.sha256 ($pinned)"
    echo "  sha256 ok (fissato nel repo)"
    return 0
  fi
  [[ -n "$upstream" ]] || rk_fetch_die "$asset: nessun checksum disponibile (ne' fissato ne' pubblicato): scarto il file"
  [[ "$actual" == "$upstream" ]] || rk_fetch_die "$asset: SHA-256 $actual diverso da quello pubblicato ($upstream)"
  echo "  sha256 ok (pubblicato dal progetto). Per fissarlo aggiungi a scripts/third-party.sha256:"
  echo "    $tool $version $asset $actual"
}
