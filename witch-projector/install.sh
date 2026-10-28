#!/usr/bin/env bash
# Run on the Pi from a checkout of the halloween repo: ./witch-projector/install.sh [video-folder]
# The folder (default: your home directory) holds the <name>.mp4 files listed in ansible/group_vars/all/vars.yml.
set -euo pipefail

export WITCH_VIDEO_SRC_DIR
WITCH_VIDEO_SRC_DIR="$(realpath -m "${1:-$HOME}")"
vault_pass_file="$HOME/.config/halloween/vault-pass"

as_root() { if [[ $EUID -eq 0 ]]; then "$@"; else sudo "$@"; fi; }

if ! command -v ansible >/dev/null || ! command -v ansible-playbook >/dev/null; then
  as_root apt-get update
  as_root apt-get install -y ansible-core
fi

args=(playbook.yml)
if [[ -s "$vault_pass_file" ]]; then
  args+=(--vault-password-file "$vault_pass_file")
else
  args+=(--ask-vault-pass)
fi
if [[ $EUID -ne 0 ]] && ! sudo -n true 2>/dev/null; then
  args+=(--ask-become-pass)
fi

cd "$(dirname "$0")/ansible"
exec ansible-playbook "${args[@]}" "${@:2}"
