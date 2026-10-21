# Witch projector

Raspberry Pi + projector aimed at the front door. mpv holds a black screen and plays the witch video when Home
Assistant presses **Play witch**. Design: [porch show spec](../docs/specs/2026-10-20-porch-show-design.md).

- `witch-mpv.service` runs mpv fullscreen on the HDMI output (DRM/KMS, no desktop), idle black, JSON IPC at
  `/run/witch/mpv.sock`. At end of file mpv goes back to idle black.
- `witch-bridge.service` runs `witch_bridge`, which connects to Mosquitto and publishes HA MQTT discovery for a
  **Witch projector** device:
  - **Play witch** button — plays `/opt/witch/witch.mp4` from the start (restarts it if already playing).
  - **Stop** button — back to black.
  - **Playing** binary sensor — from mpv's `idle-active` property, not assumed.
  - Availability `witch_projector/availability`: `online` only while the bridge is connected to both MQTT and mpv;
    a retained last will marks it `offline` if the bridge dies.

The bridge reconnects to MQTT and to the mpv socket on its own; systemd restarts either service if it exits.

## Install (on the Pi)

Raspberry Pi 4 or 5, Raspberry Pi OS Lite (Bookworm), projector on HDMI 0.

```sh
git clone <halloween repo> && cd halloween
scp workstation:witch.mp4 ~/witch.mp4        # the video is not in git
./witch-projector/install.sh ~/witch.mp4     # installs ansible-core if needed, then runs the playbook
journalctl -fu witch-bridge -u witch-mpv
```

`install.sh` runs `ansible/playbook.yml` against the Pi itself. It is safe to rerun, and rerunning is how changes
are deployed: `git pull && ./witch-projector/install.sh ~/witch.mp4`. Without a video it still sets everything
up, leaves the screen black, and says so at the end.

The playbook writes `/etc/witch/bridge.env` from `ansible/templates/bridge.env.j2`, so edit
`ansible/group_vars/all/vars.yml` rather than the file on the Pi. The MQTT password is in
`ansible/group_vars/all/vault.yml` (Ansible Vault). `install.sh` asks for the vault password unless
`~/.config/halloween/vault-pass` exists. The broker user is `witch-projector`, defined in the homeassistant repo's
`apps/mosquitto/sealed-secrets.yaml`. To view or rotate the password, run
`ansible-vault edit ansible/group_vars/all/vault.yml`, then re-add the user's hash to that passwordfile.

Audio goes to HDMI 0 by default (`--audio-device` in `systemd/witch-mpv.service`; list devices with
`mpv --audio-device=help`).

## MQTT topics

| Topic | Direction | Payload |
| --- | --- | --- |
| `witch_projector/command` | HA → bridge | `play`, `stop` |
| `witch_projector/playing` | bridge → HA (retained) | `ON`, `OFF` |
| `witch_projector/availability` | bridge → HA (retained, last will) | `online`, `offline` |
| `homeassistant/<component>/witch_projector/<object>/config` | bridge → HA (retained) | discovery |

## Tests

The core is tested with fakes for MQTT and mpv (no network, filesystem or real mpv):

```sh
uvx --python 3.11 --with 'paho-mqtt>=2,<3' pytest -q -p no:cacheprovider
```
