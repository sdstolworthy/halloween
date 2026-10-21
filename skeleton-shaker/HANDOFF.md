# Halloween Skeleton Shaker — project handoff

Context for Claude Code. Everything below was decided in a design conversation on 2026-10-17/18; treat it as the
spec. The living BOM/wiring doc is at https://claude.ai/code/artifact/241c37d0-8eb3-49b6-884b-c00b4ba9de08
(Claude Doc; sections: How it works, Bill of materials, Optional extras, Wiring at a glance with schematic,
Firmware behavior, Build notes and gotchas).

> **Revised 2026-10-20 — network-triggered.** The pressure mat is dropped. The prop now dances only when Home
> Assistant presses its **Dance** button, fired by the porch scare automation from the `porch-sensor` distance
> sensor (see `../docs/specs/2026-10-20-porch-show-design.md`). The GPIO33 mat input and the Armed switch are
> gone (arming lives on the porch sensor); the PG7 mat gland hole gets a blank PG7 plug. Mat-specific text below
> is struck through or marked superseded and kept only for history.

## 1. What the prop does

A string of plastic Halloween skeletons hangs on the porch. When Home Assistant presses the ESP32's **Dance**
button (the porch scare automation does this when the porch sensor sees a visitor), the ESP32 drives a 12 V gear
motor through an H-bridge in short random bursts for a few seconds, and a crank arm on the motor shaft tugs a
shock cord tied to the skeleton string so they bounce and rattle. Then it ignores further Dance presses for a
cooldown so overlapping requests can't stack dances.

## 2. Hardware (final picks)

| Part | Chosen | Notes |
| --- | --- | --- |
| MCU | ESP32 DevKitC, 38-pin (ESP32-WROOM-32E), 54.4 x 27.9 mm | Amazon B09MQJWQN2 (Espressif's own). If a 30-pin DOIT DevKit V1 is used instead, enclosure needs `ESP_L, ESP_W = 51.5, 28.5`. |
| ~~Trigger~~ (dropped 2026-10-20) | ~~Alarm-type pressure mat, normally-open, ~720 x 390 mm, 3 mm PVC~~ | ~~Contact rated 25 V DC / 0.25 A, IP64 (not waterproof), trips at ~25 kg over a 50 mm disc. Must stay flat, never rolled. 4-wire mats: use only the N.O. pair; the tamper pair stays unconnected.~~ Replaced by the network-triggered Dance button. |
| Motor | Greartisan DC 12 V, 100 RPM, 37 mm gearbox, **eccentric** output shaft, 6 mm D-shaft | Eccentric chosen over centric: standard 37D form factor, fits common brackets, fewer gear stages (less backlash on direction reversals). 60 RPM also fine. Shaft side-load limit ~1 kg, so keep the crank arm tight to the gearbox face. |
| Motor driver | L298N dual H-bridge module | Takes 3.3 V logic. Jumpers: **5V-EN ON** (onboard regulator powers logic from 12 V), **ENA jumper OFF** (so GPIO27 PWMs speed). DRV8871 is a smaller/cooler alternative if desired. |
| Power | 12 V 3 A wall adapter, 5.5 x 2.1 mm barrel → female barrel-jack-to-screw-terminal adapter | Adapter stays indoors / covered outlet. |
| 5 V | LM2596 adjustable buck module | Set to 5.0 V with a meter BEFORE connecting to the ESP32 5V/VIN pin. Never feed 12 V to the ESP32. |
| Rails | 2x WAGO 221-415 five-way lever nuts (one = 12 V rail, one = GND rail) | 24–12 AWG, strip 11 mm. Seated in a cradle printed into the enclosure floor. |
| Decoupling | 1000 µF 25 V electrolytic across the 12 V / GND rails | + to 12 V. |
| Crank hub | Pololu Universal Aluminum Mounting Hub, 6 mm shaft, M3 holes, 2-pack, item #1999 ($12.95) | 25.4 mm dia x 9.2 mm, two M3 set screws, six tapped M3 holes. Runner-up: goBILDA 1314 steel set-screw hub, 6 mm bore. |
| Crank arm | 3D-printed `crank_arm.stl` | Bolts to the hub face with 2x M3 x 8; slotted holes fit the hub's bolt circle (r 7–11 mm) without measuring. M4 x 20 bolt + nylock nut as the crank pin, holes at 20 / 30 / 40 mm throw. Start at 30. |
| Motor mount | 37 mm gear-motor L-bracket (6x M3 pattern) | Mount motor above the string's midpoint. |
| Linkage | 1/8 in shock cord from crank pin to skeleton string | Elastic gives a bounce; rigid cord yanks hooks out. |
| Enclosure | 3D-printed `enclosure_base.stl` + `enclosure_lid.stl` | See §5. Glands: 2x PG9 (16 mm holes: power, motor), 1x PG7 (12.5 mm: formerly the mat lead, now a blank PG7 plug). |
| Wire | 18–20 AWG silicone for 12 V and motor runs | (The 22 AWG mat lead and its 100 nF GPIO33 filter cap are no longer needed.) |

Approximate cost: ~$154 all-in as originally specced, ~$134 without the mat (ESP32 $9, mat $20, motor $18, L298N $7, PSU $12, buck $5, hub $13, bracket $6,
cord $6, enclosure consumables, glands, wire, WAGOs, screw-terminal breakout, etc.). Optional extras in the doc:
DFPlayer Mini + speaker for sound, WS2812B eyes, HC-SR501 PIR as second trigger, wiper-motor + BTS7960 upgrade
for heavy skeletons.

## 3. Wiring (pin assignments are final)

Topology: one 12 V rail and one GND rail (the two WAGOs). The L298N takes 12 V straight off the rails; the buck
takes 12 V in and feeds the ESP32 5V pin; all grounds common.

| From | To | Notes |
| --- | --- | --- |
| Adapter + / − (barrel-jack terminal) | 12 V rail / GND rail | 18 AWG |
| 12 V rail / GND rail | L298N +12V / GND | 1000 µF cap also across these rails |
| 12 V rail / GND rail | Buck IN+ / IN− | |
| Buck OUT+ / OUT− | ESP32 5V (VIN) / GND | |
| **ESP32 GPIO25** | L298N IN1 | direction A |
| **ESP32 GPIO26** | L298N IN2 | direction B |
| **ESP32 GPIO27** | L298N ENA | PWM speed (LEDC), ENA jumper removed |
| ~~ESP32 GPIO33~~ | ~~Pressure mat lead 1~~ | Removed 2026-10-20; GPIO33 is unused. |
| ESP32 GND | GND rail | common ground |
| L298N OUT1 / OUT2 | Motor | polarity irrelevant; swap to flip direction |

Avoid GPIO 0, 2, 12, 15 for any future input (boot strapping pins). USB cutout in the enclosure lets you flash without
opening the box.

## 4. Firmware — done (`skeleton-shaker.yaml` + `shaker.h`)

Current behavior (supersedes items 1, 4 and 5's "no network" wording below):

- **Dance** button (HA / web UI) runs the dance script; it is the porch scare automation's entry point. **Stop**
  aborts and coasts the motor. A State text sensor reports idle / dancing / cooldown.
- The cooldown lives inside the `mode: single` dance script, so a Dance press during the dance or the cooldown is
  logged and ignored.
- The prop needs the network now, so Wi-Fi keeps ESPHome's default reboot timeout (reboot after 15 min without
  Wi-Fi aids recovery); `api: reboot_timeout: 0s` stays so Home Assistant being down doesn't reboot it.
- Coasting: on the L298N, IN1 = IN2 with ENA high is a hard *brake*. The firmware coasts by dropping ENA (PWM) to
  0 and then setting IN1 = IN2 = LOW. This supersedes "IN1=IN2=LOW to coast" in items 2 and 3 below.

Original brief, kept for history:

Target: Arduino-ESP32 sketch (PlatformIO or Arduino IDE), or an ESPHome YAML. Behavior spec:

1. ~~`GPIO33` with internal pull-up, 50 ms debounce; a falling edge (mat pressed) is a step.~~ Superseded by the Dance button.
2. On trigger, run a **dance** for 6–10 s (random): loop of bursts 150–500 ms long; before each burst randomly
   pick direction (flip IN1/IN2) and PWM duty 50–100 % on ENA (`ledcSetup`/`ledcWrite`, 1–5 kHz, 8-bit);
   pause 50–200 ms between bursts (IN1=IN2=LOW to coast).
3. Stop with IN1 and IN2 both LOW (coast, not brake).
4. Ignore triggers during the dance and for a **cooldown** of 3–5 s after it.
5. Nice-to-have: Wi-Fi + ArduinoOTA (or ESPHome's OTA + web dashboard) so dance length, burst range, duty
   range and cooldown can be tuned without opening the box. Keep Wi-Fi optional/non-blocking; the prop must
   work with no network.
6. Serial logging of trigger/dance/cooldown events at 115200 for bench testing.

ESPHome equivalent: `binary_sensor` (gpio, `inverted: true`, `mode: {input: true, pullup: true}`) →
`script` doing the random bursts on an `hbridge` fan (`pin_a: GPIO25`, `pin_b: GPIO26`, `enable_pin: GPIO27`
as a `ledc` output), plus `ota:` and `web_server:`.

Bench test order: set buck to 5.0 V unloaded → wire everything with the motor unloaded → press Dance from the web
UI and verify dance/cooldown on serial → add the crank and string.

## 5. Enclosure (done, parametric)

Files: `skeleton_enclosure.py` (generator), `enclosure_base.stl`, `enclosure_lid.stl`, `crank_arm.py`,
`crank_arm.stl`. Python deps: `trimesh manifold3d shapely mapbox_earcut numpy` (+ `matplotlib` for previews).
`python3 skeleton_enclosure.py` regenerates both STLs; both are watertight.

Geometry (mm, interior origin at the front-left floor corner; y=0 is the gland wall, lid is +z):
- Interior cavity 120 x 84 x 36; walls/floor 2.4; assembled ~125 x 89 x 44 outside.
- L298N on four M3 standoffs (37 mm pattern), board corner at (10, 12).
- ESP32 DevKitC in an L-post cradle, board corner at (60.8, 2.0), USB end facing the gland wall; 14 x 10 mm USB
  cutout in that wall at z 7.5–17.5.
- LM2596 buck in an L-post cradle, board corner at (96, 10), long axis along y (right-hand strip).
- WAGO cradle against the back wall (y=84) behind the L298N: two pockets 21.2 x ~20 inside, 6 mm walls, 2 mm
  lip at the wire-entry face, starting x=10. Sized from WAGO's 20.6 x 18.6 x 8.3 mm body + 0.6 mm clearance —
  unverified against a physical part; measure one.
- Gland wall (y=0): PG9 holes (r 8) at x=20 and x=46, PG7 hole (r 6.25) at x=102 (blank plug since the mat was dropped), all at z=18.
- Four M3 corner bosses (2.6 mm holes for thread-forming screws; drill 4 mm for heat-set inserts). Lid has a
  3.5 mm lip that seats inside the walls, M3 clearance + counterbore.
- Two wall-mount ears on the left/right outer walls, 4.5 mm holes for #8 screws. Mount floor flat, glands down.
- Print: base open side up, lid lip up, no supports, 0.2 mm layers, 3 walls, 20 % infill, PETG for outdoors.
  Crank arm flat, 100 % infill. Silicone bead or foam tape on the lid lip for rain.

## 6. Open items / things to verify

- Confirm the ESP32 board is the 38-pin DevKitC (else change `ESP_L, ESP_W`).
- Confirm the L298N has the standard 37 mm mounting-hole spacing (nearly all do).
- Measure a WAGO 221-415 before printing the base; adjust `WAGO_W / WAGO_D / WAGO_CLR`.
- Pololu hub bolt-circle radius was not confirmed (drawing PDF unreadable); the crank-arm slots cover r 7–11 mm.
- Amazon listings in the BOM are mostly pre-filtered searches, not verified ASINs (Amazon blocks automated reads).
- Motor sizing assumes a light plastic garland. For 2–3 ft skeletons use the wiper-motor + BTS7960 upgrade.
