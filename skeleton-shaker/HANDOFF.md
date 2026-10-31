# Halloween Skeleton Shaker — project handoff (ESPHome servo node)

Context for Claude Code. Decided in a design conversation 2026-10-17 → 10-20; treat it as the spec. The living
BOM/wiring doc: https://claude.ai/code/artifact/241c37d0-8eb3-49b6-884b-c00b4ba9de08 (sections: How it works, Bill
of materials, Optional extras, Wiring at a glance with schematic, Firmware: ESPHome YAML + HA automation, Build
notes and gotchas).

**Design history:** v1 was a 37 mm 12 V gear motor + L298N + buck + crank arm with a pressure mat wired to the
ESP32. The garland weighs only 100–150 g, so v2 moved to an MG90S servo on 5 V. v3 (current) removes the local
sensor: an ultrasonic distance sensor on a *separate* ESPHome node reports to Home Assistant, and an HA automation
calls an ESPHome action on this box. v1/v2 parts stay in the doc's Optional extras; v1 files are in
`v1-motor-drive/` in this bundle.

> **In this repo:** the firmware is `skeleton-shaker.yaml` + `shaker.h` (not copied from the doc), the sensor node
> is `../porch-sensor/`, and the automation is the porch scare automation in the homeassistant repo (see
> `../docs/specs/2026-10-20-porch-show-design.md`). Where §2 and §4 below differ from those, the repo wins; the
> differences are noted inline.

## 1. What the prop does

A string of light plastic skeletons (100–150 g) hangs on the porch. When the ultrasonic node sees someone within
range, Home Assistant calls `esphome.skeleton_shaker_dance`. This box (ESP32 running ESPHome) sweeps a metal-gear
micro servo through random angles for ~8 s; the horn tugs a shock cord tied to the skeleton string. A `mode: single`
script ignores further calls during the dance plus a cooldown.

## 2. Hardware in this box (~$91)

| Part | Chosen | Notes |
| --- | --- | --- |
| MCU | ESP32 DevKitC, 38-pin (ESP32-WROOM-32E), 54.4 x 27.9 mm, ESPHome | Amazon B09MQJWQN2. For a 30-pin DOIT DevKit V1 set `ESP_L, ESP_W = 51.5, 28.5` in `skeleton_enclosure.py`. |
| Actuator | **MG90S** metal-gear micro servo (~2 kg·cm at 5 V, 180°) | Not the plastic-gear SG90. Stock ~20 mm single-arm horn sweeping 100° ≈ 30 mm stroke; needs ~0.3 kg·cm. Good to ~300 g of garland. |
| Servo lead | 300–500 mm JR/Futaba 3-pin extension | Enters the box through the right-wall PG7 gland. |
| Power | 5 V 3 A wall adapter, 5.5 x 2.1 mm center-positive → panel-mount threaded DC jack (12 mm hole) in the front wall | Adapter indoors / covered outlet. |
| Rails | 2x WAGO 221-415 five-way lever nuts (5 V, GND) in the printed cradle | 24–12 AWG, strip 11 mm. |
| Decoupling | 1000 µF ≥10 V electrolytic across the rails, + on 5 V | Prevents servo-stall brownout of the ESP32. |
| Linkage | 1/8 in shock cord, horn tip → skeleton string | Elastic = bounce; rigid cord pulls hooks out. Hang string ends loosely. |
| Enclosure | `servo_base.stl` + `servo_lid.stl` | §5. One PG7 gland (12.5 mm hole). |
| Wire | 20–22 AWG silicone | |

Not in this box: the ultrasonic sensor node. *(Repo: `../porch-sensor/`, a SparkFun 3.3 V HC-SR04 polled every
100 ms with a median filter; it publishes a debounced `Visitor` binary sensor rather than leaving thresholds to HA.)*

## 3. Wiring (final)

| From | To | Notes |
| --- | --- | --- |
| 5 V adapter | Panel-mount DC jack (front wall) | 5.5 x 2.1 mm, center positive |
| DC jack center / sleeve lug | 5 V rail / GND rail | 20 AWG; 1000 µF across the rails |
| 5 V rail / GND rail | ESP32 **5V (VIN)** / GND | Board LDO makes 3.3 V; USB may stay connected for flashing (DevKitC diode isolates) |
| 5 V rail / GND rail | Servo red / brown | Never from the 3V3 pin |
| **ESP32 GPIO25** | Servo orange (signal) | 50 Hz PWM, 500–2400 µs, ESPHome `ledc` output |
| ESP32 GND | GND rail | common ground |

No sensor input. Avoid GPIO 0, 2, 12, 15 if adding one (boot straps); a wired local trigger would use GPIO33 with
pull-up and the optional left-wall gland (`LEFT_GLAND = True`).

## 4. Firmware — ESPHome (`skeleton-shaker.yaml`)

Entities exposed to HA: action `dance` (→ `esphome.skeleton_shaker_dance`), button `Dance`, numbers `Dance length`
(3–20 s, default 8) and `Cooldown` (0–60 s, default 4), binary_sensor `Dancing`. Also `web_server` on port 80 so
the Dance button and sliders work from the device page without HA. *(Repo additions: a `Stop` button, digest auth
on the web page, OTA encryption keyed by the API key instead of `ota_password`.)*

Dance script (`mode: single`): publish Dancing ON → loop until `dance_len` elapsed: write random 40–140° to the
servo (level = (deg−90)/90), dwell random 80–300 ms → write 90°, 300 ms, `servo.detach` → Dancing OFF → delay
`cooldown_len`. Servo: `ledc` on GPIO25 at 50 Hz, `min_level: 2.5%`, `max_level: 12.0%`, `transition_length: 0s`,
`auto_detach_time: 0s`.

HA automation: *(Repo: the porch scare automation presses `button.skeleton_shaker_dance` when
`binary_sensor.porch_sensor_visitor` turns on, alongside WLED and the witch projector. The doc's standalone
`numeric_state` automation is not used.)*

Possible follow-ups: add the ±15° double-twitch "rattle" one move in four; a `select` for dance styles; an
`interval:`-based "idle twitch" every few minutes.

## 5. Enclosure (done, parametric)

Files: `servo_enclosure.py` (imports helpers/shared constants from `skeleton_enclosure.py`; keep both in one folder),
`servo_base.stl`, `servo_lid.stl`. Deps: declared in `pyproject.toml`, pinned in `uv.lock`.
`uv run python servo_enclosure.py` regenerates both STLs (watertight).

Geometry (mm; interior origin at the front-left floor corner; y=0 is the front/DC-jack wall; lid is +z):
- Interior 96 x 64 x 28, walls/floor 2.4 → ~101 x 69 x 36 outside assembled.
- ESP32 DevKitC in an L-post cradle, board corner at (8, 2), long axis along y, USB end toward the front wall;
  14 x 10 mm USB slot in the front wall at x 9–23, z 7.5–17.5.
- Panel-mount DC jack: 12.2 mm hole in the front wall at x=60, z=15.
- PG7 gland: 12.5 mm hole in the right wall (x=96) at y=20, z=15 (servo lead). Optional left-wall gland at y=46 via
  `LEFT_GLAND = True`.
- WAGO cradle against the back wall from x=40: two pockets 21.2 x ~20.8 inside, 6 mm walls, 2 mm lip at the
  wire-entry face. From WAGO's published 20.6 x 18.6 x 8.3 mm body + 0.6 mm clearance — measure one first.
- Four M3 corner bosses (2.6 mm holes for thread-forming screws; drill 4 mm for heat-set inserts). Lid: 3.5 mm inner
  lip, M3 clearance + counterbore. Two wall-mount ears (4.5 mm holes, #8 screws) centered on the side walls.
- Print: base open side up, lid lip up, no supports, 0.2 mm layers, 3 walls, 20 % infill, PETG outdoors. Silicone
  bead or foam tape on the lid lip. Mount under the eave, floor flat.
- Servo mounts outside (not waterproof): tabs screwed to trim or a small printed bracket above the garland's
  midpoint, horn down. Possible follow-up: printed MG90S bracket (body 22.5 x 12 x 22.7 mm, tab holes ~28 mm
  apart, M2 screws).

## 6. Open items / verify

- Confirm the ESP32 is the 38-pin DevKitC (else change `ESP_L, ESP_W`).
- Measure a WAGO 221-415 before printing; adjust `WAGO_W / WAGO_D / WAGO_CLR` in `skeleton_enclosure.py`.
- Confirm the DC jack's panel hole (most threaded 5.5 x 2.1 jacks are 12 mm) → `JACK_R`.
- Check Wi-Fi RSSI at the mounting spot from the ESPHome web page; give the node a DHCP reservation.
- Amazon links in the BOM are pre-filtered searches, not verified listings.
- Garland over ~300 g → v1 motor drive (doc: Optional extras) and the v1 enclosure.
