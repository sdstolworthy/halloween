#!/usr/bin/env bash
# Run on the Pi from a checkout of witch-projector/: sudo ./install.sh
set -euo pipefail
cd "$(dirname "$0")"

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo." >&2
  exit 1
fi

apt-get update
apt-get install -y mpv python3-venv

id witch &>/dev/null || useradd --system --home-dir /opt/witch --shell /usr/sbin/nologin witch
usermod -aG video,render,audio witch

install -d -m 0755 /opt/witch
python3 -m venv /opt/witch/venv
/opt/witch/venv/bin/pip install --upgrade .

install -d -m 0750 -g witch /etc/witch
if [[ ! -e /etc/witch/bridge.env ]]; then
  install -m 0640 -g witch bridge.env.example /etc/witch/bridge.env
fi

install -m 0644 systemd/witch-mpv.service systemd/witch-bridge.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable witch-mpv.service witch-bridge.service
systemctl restart witch-mpv.service witch-bridge.service

[[ -e /opt/witch/witch.mp4 ]] || echo "Copy the video to /opt/witch/witch.mp4."
echo "Fill in /etc/witch/bridge.env, then: sudo systemctl restart witch-bridge"
