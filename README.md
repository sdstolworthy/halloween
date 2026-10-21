# Halloween

Porch props, one directory per device. When someone walks up to the door, Home Assistant's "Porch scare"
automation makes the skeletons dance, switches WLED to a scare preset and projects a witch onto the door. Each
device does one job and knows nothing about the others; see
[the porch show spec](docs/specs/2026-10-20-porch-show-design.md).

ESPHome devices are flashed with `uvx esphome@2026.9.1 run <device>/<device>.yaml` after copying
`secrets.example.yaml` to `secrets.yaml` (gitignored) and filling it in.

| Device | What it does |
| --- | --- |
| [eyes](eyes/) | ESP32 + 128x64 HUB75 panel shows eyes that follow visitors using porch-sensor's distance (MQTT), and turn evil when the house publishes scare mode on `halloween/eyes/mode`; Rust firmware plus a browser emulator. |
| [porch-sensor](porch-sensor/) | ESP32 + 3.3 V HC-SR04 distance sensor; publishes Distance and Visitor to HA, with Trigger distance, Max range and Armed tuning entities, and the raw distance over MQTT for the eyes. |
| [skeleton-shaker](skeleton-shaker/) | ESP32 + gear motor shakes a string of skeletons when HA presses its Dance button. |
| [witch-projector](witch-projector/) | Raspberry Pi + mpv holds a black screen and plays the witch video when HA presses Play witch (MQTT). |
