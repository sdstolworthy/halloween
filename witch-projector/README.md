# Witch projector

Raspberry Pi + projector aimed at the front door. Between scares it loops `happy.mp4`; when Home Assistant
presses **Random scare** or one of the per-video buttons it cuts to that scare, then goes back to the loop. Design: [porch show spec](../docs/specs/2026-10-20-porch-show-design.md).

- `witch-mpv.service` runs mpv fullscreen on the HDMI output (DRM/KMS, no desktop), JSON IPC at
  `/run/witch/mpv.sock`. mpv itself idles black; the bridge loads the happy loop whenever mpv falls idle (at
  start, after a scare, after Stop). If the happy video fails to load, the bridge stops retrying and the screen
  stays black between scares until the bridge or mpv restarts.
- `witch-bridge.service` runs `witch_bridge`, which connects to Mosquitto and publishes HA MQTT discovery for a
  **Witch projector** device:
  - **Random scare** button — plays one of the videos at random, never the same one twice in a row. The porch
    scare automation presses this one (entity `button.witch_projector_play_witch`, kept from when it was "Play witch").
  - **Play \<name\>** button per video (Play witch1, Play witch2, Play witch3, Play ghost) — plays
    `/opt/witch/videos/<name>.mp4` from the start (restarts it if something is already playing).
  - **Stop** button — ends the scare and returns to the happy loop.
  - **Playing** binary sensor — on only while a scare video is on screen (from mpv's `path` property), so the
    happy loop reads as off.
  - Availability `witch_projector/availability`: `online` only while the bridge is connected to both MQTT and mpv;
    a retained last will marks it `offline` if the bridge dies.

The bridge reconnects to MQTT and to the mpv socket on its own; systemd restarts either service if it exits.

## Install (on the Pi)

Raspberry Pi 4 or 5, Raspberry Pi OS (Bookworm or Trixie), projector on HDMI 0 (the micro-HDMI port next to
USB-C power). On a desktop image the playbook stops the desktop and makes the Pi boot to the console, because the
desktop holds the HDMI output and mpv can't draw; `sudo systemctl set-default graphical.target` undoes that.

```sh
git clone <halloween repo> && cd halloween
scp workstation:{witch1,witch2,witch3,ghost}.mp4 ~/   # videos are not in git
./witch-projector/install.sh ~               # installs ansible-core if needed, then runs the playbook
journalctl -fu witch-bridge -u witch-mpv
```

`install.sh` runs `ansible/playbook.yml` against the Pi itself. It is safe to rerun, and rerunning is how changes
are deployed: `git pull && ./witch-projector/install.sh`. The videos are the names in `witch_videos` (scares) and
`witch_idle_video` (the loop, default `happy`) in `ansible/group_vars/all/vars.yml`, each read from `<folder>/<name>.mp4` (folder defaults to your home directory).
To add or drop one, edit that list and rerun. Missing files are skipped with a warning at the end, and videos no
longer listed are removed from the Pi. Home Assistant keeps a dropped video's button as unavailable until you
delete it there.

The playbook writes `/etc/witch/bridge.env` from `ansible/templates/bridge.env.j2`, so edit
`ansible/group_vars/all/vars.yml` rather than the file on the Pi. The MQTT password is in
`ansible/group_vars/all/vault.yml` (Ansible Vault). `install.sh` asks for the vault password unless
`~/.config/halloween/vault-pass` exists. The broker user is `witch-projector`, defined in the homeassistant repo's
`apps/mosquitto/sealed-secrets.yaml`. To view or rotate the password, run
`ansible-vault edit ansible/group_vars/all/vault.yml`, then re-add the user's hash to that passwordfile.

### Sound

`witch_audio_output` in `ansible/group_vars/all/vars.yml` picks where the sound goes: `hdmi` (default, through
the projector), `jack` (the Pi's 3.5mm socket) or `bluetooth`. The playbook writes it to `/etc/witch/mpv.env`.

For a Bluetooth speaker (BlueALSA, installed by the playbook):

1. Put the speaker in pairing mode next to the Pi and run `./witch-projector/pair-speaker.sh` to list nearby
   devices, then `./witch-projector/pair-speaker.sh AA:BB:CC:DD:EE:FF` to pair, trust and connect it.
2. Set `witch_audio_output: bluetooth` and `witch_bluetooth_speaker: "AA:BB:CC:DD:EE:FF"`, then rerun
   `install.sh`. With an address set, `witch-speaker-reconnect.timer` reconnects the speaker every 30 seconds if
   it drops.

mpv opens the audio device when a video starts. If the speaker was off at that moment, that video plays silently;
the looping idle video stays silent until the next scare or `sudo systemctl restart witch-mpv`. Many speakers
switch themselves off after a few minutes of silence, so leave the idle video's sound on.

## MQTT topics

| Topic | Direction | Payload |
| --- | --- | --- |
| `witch_projector/command` | HA → bridge | `play` (random), `play:<name>`, `stop` |
| `witch_projector/playing` | bridge → HA (retained) | `ON`, `OFF` |
| `witch_projector/availability` | bridge → HA (retained, last will) | `online`, `offline` |
| `homeassistant/<component>/witch_projector/<object>/config` | bridge → HA (retained) | discovery |

## Tests

The core is tested with fakes for MQTT and mpv (no network, filesystem or real mpv):

```sh
uvx --python 3.11 --with 'paho-mqtt>=2,<3' pytest -q -p no:cacheprovider
```
