#!/usr/bin/env bash
# Installa (o aggiorna) l'hub RE-KORD come servizio systemd da un pacchetto
# rekord-server-<versione>-linux-<arch>.tar.gz gia' estratto: si lancia da dentro
# la cartella estratta, con sudo.
#
#   sudo ./systemd/install.sh            installa in /opt/rekord, dati in /var/lib/rekord
#   PREFIX=/srv/rekord sudo -E ./systemd/install.sh
#
# Un aggiornamento sostituisce i file del programma e lascia stare dati e
# /etc/default/rekord-server.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "Va lanciato con sudo." >&2; exit 1; }

SRC="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX="${PREFIX:-/opt/rekord}"
DATA="${DATA:-/var/lib/rekord}"
UNIT=/etc/systemd/system/rekord-server.service
ENVFILE=/etc/default/rekord-server

[[ -x "$SRC/rekord-server" ]] || { echo "Non trovo $SRC/rekord-server: lancia lo script dalla cartella del pacchetto." >&2; exit 1; }

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

# Programma: si sostituisce per intero (client-ui e admin-ui inclusi).
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
  echo "Creato $ENVFILE: controlla REKORD_MUSIC_ROOT e REKORD_BIND."
fi

systemctl daemon-reload
if (( RESTART )); then
  systemctl start rekord-server
else
  systemctl enable --now rekord-server
fi
systemctl --no-pager --lines=5 status rekord-server || true
echo
echo "Hub su http://<questa-macchina>:7420  (pannello: /admin). Log: journalctl -u rekord-server -f"
