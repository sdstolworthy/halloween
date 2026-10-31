#!/usr/bin/env bash
# Pair a Bluetooth speaker with the Pi. Put the speaker in pairing mode next to the Pi, then:
#   ./witch-projector/pair-speaker.sh                    # scan and list nearby devices
#   ./witch-projector/pair-speaker.sh AA:BB:CC:DD:EE:FF  # pair, trust and connect that one
set -euo pipefail

bt() { sudo bluetoothctl "$@"; }

sudo rfkill unblock bluetooth
bt power on >/dev/null
# Pairing only works with a device the adapter has seen recently, so scan first either way.
echo "Scanning for 15 seconds..."
bt --timeout 15 scan on >/dev/null

if [[ $# -eq 0 ]]; then
  bt devices
  echo "Rerun with the speaker's address: $0 AA:BB:CC:DD:EE:FF"
  exit 0
fi

speaker="$1"
bt pair "$speaker"
# Trusted devices may reconnect without asking, so the speaker comes back after a power cycle.
bt trust "$speaker"
bt connect "$speaker"
cat <<MSG

Paired. To send the witch's sound to it, set these in witch-projector/ansible/group_vars/all/vars.yml:
  witch_audio_output: bluetooth
  witch_bluetooth_speaker: "$speaker"
then rerun ./witch-projector/install.sh
MSG
