# Porch show — design

Date: 2026-10-20. Status: approved in conversation, pending written-spec review.

## Goal

When someone walks up the narrow front walkway and comes within a set distance of the door, three effects fire
together: the skeleton string dances, the WLED lights switch to a scare preset, and a witch video is projected onto
the door. Home Assistant (HA) orchestrates; each device does one job and knows nothing about the others.

Out of scope for now: the animated eyes on the 128x64 HUB75 matrix (they will later consume the porch sensor's
distance, built on either ESPHome or the flightpanel HUB75 driver), and any time-of-day or presence gating on the
automation (the user will add that in HA after testing).

## Architecture

```
SparkFun HC-SR04 (3.3 V) ── ESP32 "porch-sensor" (ESPHome) ──API──┐
                                                                   ▼
                                                 HA automation "Porch scare"
                                                   ├─ ESPHome → skeleton-shaker: Dance button
                                                   ├─ WLED → preset "Scare", later "Idle"
                                                   └─ MQTT → witch-projector (Pi): Play witch button
```

| Unit | Repo / path | Responsibility |
| --- | --- | --- |
| porch-sensor | `halloween/porch-sensor/` | Measure distance; publish Distance and Visitor to HA. |
| skeleton-shaker | `halloween/skeleton-shaker/` | Dance on request. No local trigger. |
| witch-projector | `halloween/witch-projector/` | Hold a black screen; play the video on request; report Playing. |
| Porch scare automation | `homeassistant/apps/homeassistant/config/automations.yaml` | Fire the effects, restore lights, cool down. |
| WLED controller | existing | User creates presets named `Scare` and `Idle` in the WLED UI. |

The pressure mat from the original skeleton-shaker design is dropped; the distance sensor is the only trigger.

## porch-sensor

### Hardware

- ESP32 DevKitC 38-pin (same board as the shaker).
- SparkFun 4-pin ultrasonic sensor, 3.3 V HC-SR04 variant: VCC → 3V3, GND → GND, Trig → GPIO32, Echo → GPIO35.
  3.3 V logic, so no level shifting. GPIO35 is input-only and not a strapping pin.
  If the board turns out to be the Qwiic version, use its Trig/Echo pads with the same wiring.
- Power: 5 V 1 A USB adapter into the DevKit USB port.
- Mount: sensor ~1 m high, aimed level down the walkway, in a printed hood open toward the walkway and angled
  slightly down to shed water (the transducers are open mesh, not waterproof). ~1 m 4-wire cable from the hood to
  the ESP32 box. The ~15° beam will see railings and plants; Max range below is the tool for ignoring them.

### Firmware

Files: `porch-sensor/porch-sensor.yaml` (board, network, includes the package) and `porch-sensor/sensor.yaml`
(an ESPHome `packages:` file with the sensor, Visitor and tuning entities). Keeping the sensor in a package means it
can later be merged into another device (shaker or eyes) with a one-line include.

- `ultrasonic` sensor polling every 100 ms, median filter over 5 samples. A timeout (no echo) is treated as
  "nobody", i.e. not below any threshold — never as zero distance.
- Two distance sensors:
  - **Distance (raw, internal)** — ~10 Hz, feeds Visitor and (later) the eyes. Not sent to HA.
  - **Distance** (published) — derived from the raw sensor, sent on change > 5 cm or at least every 30 s, so the HA
    recorder isn't flooded.
- **Visitor** binary sensor, built from ESPHome filters (no custom C++):
  - on after distance ≤ *Trigger distance* continuously for 300 ms;
  - off after distance > *Trigger distance* + 0.3 m (or no echo) continuously for 2 s.
- Tuning entities (`entity_category: config`, restored across reboots):
  - **Trigger distance** — number, m, default 2.0, range 0.3–4.0.
  - **Max range** — number, m, default 4.0, range 0.5–4.5; readings beyond it count as "nobody".
  - **Armed** — switch, default on; when off, Visitor never turns on.
- Network, same pattern as the shaker: Wi-Fi with ESPHome's default reboot timeout, fallback AP + captive portal,
  `api` with encryption and `reboot_timeout: 0s`, `ota` with encryption keyed by the API key, `web_server` with
  digest auth and `ota: false`. Secrets via `secrets.yaml` (gitignored) with a committed `secrets.example.yaml`.
- Logger at 115200; log Visitor transitions at INFO.

## skeleton-shaker changes

- Remove the GPIO33 mat `binary_sensor` and the Armed switch.
- Rename the "Test dance" button to **Dance**; it is the automation's entry point. Keep **Stop**.
- Keep the in-script cooldown (`mode: single`) so overlapping requests can't stack dances.
- Restore ESPHome's default Wi-Fi `reboot_timeout` (the prop no longer works offline, so rebooting aids recovery);
  keep `api: reboot_timeout: 0s`.
- Enclosure unchanged; the now-unused PG7 hole gets a blank PG7 plug.
- Update `skeleton-shaker/HANDOFF.md` and `skeleton-shaker/README.md` to describe the network-triggered design.

## witch-projector

### Runtime

- Raspberry Pi 4 or 5, Raspberry Pi OS Lite (Bookworm), HDMI to the projector, reachable over SSH.
- **mpv** as a long-running systemd service, fullscreen, no desktop session:
  `--idle --force-window --fullscreen --keep-open=no` (the window background defaults to black; exact flags
  verified against the installed mpv version), JSON IPC socket at `/run/witch/mpv.sock`.
  Idle shows black; at end of file mpv returns to idle black.
- Audio: HDMI to the projector by default (one mpv option to change).
- Video at `/opt/witch/witch.mp4` (supplied by the user, not in git).

### witch-bridge (Python)

- Connects to the existing Mosquitto broker (`homeassistant/apps/mosquitto`); broker host, port and credentials
  from an environment file (`/etc/witch/bridge.env`), never from the repo.
- Publishes HA MQTT discovery for one device, "Witch projector", with:
  - **Play witch** — button; loads the video from the start (restarts if already playing).
  - **Stop** — button; stops playback, back to black.
  - **Playing** — binary sensor; driven by mpv's idle/playback events, not by assumption.
  - Availability topic with an MQTT last-will, so HA shows the device unavailable if the bridge dies.
- Structure: the MQTT client and the mpv IPC connection are injected behind small interfaces; the core maps
  commands → mpv requests and mpv events → state publishes. Unit tests use fakes for both (no network, no
  filesystem, no real mpv).
- Reconnects to MQTT and to the mpv socket on loss without exiting; systemd restarts it if it does exit.

### Deployment

- `witch-projector/systemd/`: `witch-mpv.service`, `witch-bridge.service` (both `Restart=always`, enabled at boot).
- `witch-projector/install.sh`: run on the Pi; installs mpv via apt, the bridge into a virtualenv, the units, and
  a template `bridge.env`.

## Porch scare automation

Lives in the `homeassistant` repo and deploys through its normal flow (push to Gitea, which mirrors to GitHub;
ArgoCD syncs the ConfigMap). Entity IDs below are placeholders resolved against the real HA instance at
implementation time.

```yaml
- id: porch_scare
  alias: Porch scare
  mode: single
  triggers:
    - trigger: state
      entity_id: binary_sensor.porch_sensor_visitor
      from: "off"
      to: "on"
  actions:
    - parallel:
        - action: button.press
          target: { entity_id: button.skeleton_shaker_dance }
          continue_on_error: true
        - action: select.select_option
          target: { entity_id: select.<wled>_preset }
          data: { option: Scare }
          continue_on_error: true
        - action: button.press
          target: { entity_id: button.witch_projector_play_witch }
          continue_on_error: true
    - wait_for_trigger:
        - trigger: state
          entity_id: binary_sensor.witch_projector_playing
          to: "on"
      timeout: "00:00:10"
    - wait_for_trigger:
        - trigger: state
          entity_id: binary_sensor.witch_projector_playing
          to: "off"
      timeout: "00:05:00"
    - action: select.select_option
      target: { entity_id: select.<wled>_preset }
      data: { option: Idle }
    - delay: "00:00:30"
```

- `mode: single` plus the trailing delay makes the cooldown: triggers during the show or cooldown are dropped.
- Timeouts mean a dead Pi still ends with the lights back on Idle.
- No conditions or gates; the user adds those in HA after testing.

## Power

One GFCI outdoor outlet in a weatherproof in-use box; each device gets its own adapter.

| Device | Supply |
| --- | --- |
| skeleton-shaker | existing 12 V 3 A adapter (LM2596 makes the ESP32's 5 V) |
| porch-sensor | 5 V 1 A USB adapter |
| witch-projector | official Pi supply (5.1 V 3 A for Pi 4, 27 W for Pi 5) |
| WLED | its own supply sized to the LED count, WLED current limiter on |

## Repo layout

```
halloween/
├── README.md
├── docs/specs/2026-10-20-porch-show-design.md
├── skeleton-shaker/
├── porch-sensor/
│   ├── porch-sensor.yaml
│   ├── sensor.yaml
│   └── secrets.example.yaml
└── witch-projector/
    ├── witch_bridge/
    ├── tests/
    ├── systemd/
    └── install.sh
```

## Testing and acceptance

- skeleton-shaker: `shaker.h` host tests pass; `esphome config` and `esphome compile` succeed.
- porch-sensor: `esphome config` and `esphome compile` succeed.
- witch-bridge: pytest suite passes (table-driven where it fits; fakes for MQTT and mpv).
- Automation: HA config check passes.
- Bench/porch acceptance: walking toward the sensor fires all three effects exactly once; lingering does not
  retrigger; the lights return to Idle when the video ends; a second approach after the cooldown fires again.

## Assumptions to verify during implementation

- HA's MQTT integration is already connected to the Mosquitto broker.
- The Pi's model and OS match the above.
- The WLED controller is already in HA via the WLED integration.
