#!/usr/bin/env bash
# Installs (or updates) the RE-KORD hub as a systemd service from an already
# extracted rekord-server-<version>-linux-<arch>.tar.gz package: run it from inside
# the extracted folder, with sudo.
#
#   sudo ./systemd/install.sh            installs into /opt/rekord, data in /var/lib/rekord
#   PREFIX=/srv/rekord sudo -E ./systemd/install.sh
#
# An update replaces the program files and leaves the data and
# /etc/default/rekord-server alone.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "Must be run with sudo." >&2; exit 1; }

SRC="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX="${PREFIX:-/opt/rekord}"
DATA="${DATA:-/var/lib/rekord}"
UNIT=/etc/systemd/system/rekord-server.service
ENVFILE=/etc/default/rekord-server

[[ -x "$SRC/rekord-server" ]] || { echo "Cannot find $SRC/rekord-server: run the script from the package folder." >&2; exit 1; }

if ! id rekord >/dev/null 2>&1; then
  useradd --system --home "$DATA" --shell /usr/sbin/nologin rekord
fi
mkdir -p "$PREFIX" "$DATA"
chown rekord:rekord "$DATA"

if systemctl is-active --quiet rekord-server; then
  systemctl stop rekord-server
  RESTART=1
else
  RESTART=0
fi

# Program: replaced entirely (client-ui and admin-ui included).
for item in rekord-server run.sh client-ui admin-ui bin modules.manifest.toml systemd VERSION README.txt; do
  [[ -e "$SRC/$item" ]] || continue
  rm -rf "${PREFIX:?}/$item"
  cp -a "$SRC/$item" "$PREFIX/$item"
done

sed -e "s#/opt/rekord#$PREFIX#g" -e "s#/var/lib/rekord#$DATA#g" \
  "$SRC/systemd/rekord-server.service" > "$UNIT"
if [[ ! -f "$ENVFILE" ]]; then
  sed -e "s#/opt/rekord#$PREFIX#g" -e "s#/var/lib/rekord#$DATA#g" \
    "$SRC/systemd/rekord-server.env" > "$ENVFILE"
  echo "Created $ENVFILE: check REKORD_MUSIC_ROOT and REKORD_BIND."
fi

systemctl daemon-reload
if (( RESTART )); then
  systemctl start rekord-server
else
  systemctl enable --now rekord-server
fi
systemctl --no-pager --lines=5 status rekord-server || true
echo
echo "Hub at http://<this-machine>:7420  (admin panel: /admin). Logs: journalctl -u rekord-server -f"
