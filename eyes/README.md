# eyes

A pair of eyes on a 128x64 HUB75 LED panel (ESP32-WROOM-32) that watch visitors walk up the path, using
porch-sensor's distance over MQTT. The browser emulator runs the same renderer, so it shows the exact
pixels the panel will. Design and contracts: [the tracking eyes spec](../docs/specs/2026-10-20-tracking-eyes-design.md).

| Path | What it is |
| --- | --- |
| `eyes-core/` | Geometry, config, payload parsing and the gaze/blink/idle state machine (`no_std`). |
| `eyes-render/` | Draws a pose onto any 128x64 `embedded-graphics` target (`no_std`). |
| `eyes-hal/` | Matrix/Clock/Inbox traits and the device frame loop, tested with mocks (`no_std`). |
| `eyes-web/` | The wasm emulator and its static page. |
| `eyes-emulator/` | Writes a walk-up contact sheet PNG. |
| `firmware/` | ESP-IDF firmware; its own Cargo workspace. |
| `Dockerfile` | Builds the emulator site for Coolify (base directory `/eyes`). |

## Host workspace

`rust-toolchain.toml` pins Rust 1.96.0 with rustfmt, Clippy and the wasm target.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p eyes-emulator            # writes eyes-emulator/out/contact-sheet.png
```

## Emulator

```bash
./eyes-web/build-wasm.sh              # writes eyes-web/web/eyes_web.wasm (not committed)
node eyes-web/web/smoke.mjs           # wasm renders the same pixels as the host
cd eyes-web/web && python3 -m http.server 8080
```

Move the visitor (slider, or drag them in the top-down view), press **Walk up to the door**, untick
**Sensor online** to watch the stale timeout, or slow the simulation down with **Speed**. Distances are sent exactly as porch-sensor sends them (two
decimals, or `none` at and beyond 4.5 m) and parsed by the device's own code. Change the installation
geometry and the page shows the per-eye angles and the full JSON to publish. The whole setup is kept in
the URL, so you can share a link to it.

The panel shows the renderer's pixels, the same ones the firmware draws. **Panel gamma** (on by default)
adds a display stage approximating the light the LEDs give for them: the HUB75 driver's CIE 1931 curve and
8-bit colour depth, re-encoded for your monitor. It doesn't model the brightness setting or how a
particular panel and camera render colour.

`docker build -t porch-eyes .` builds the site. The image runs the smoke test during the build and serves
the page on port 80, with `/healthz` for Coolify and `Cache-Control: no-cache` so a redeploy never mixes
an old script with a new wasm. Base images are pinned by tag and digest.

## Firmware

Prerequisites are the same as flight-rs: `cargo install espup espflash ldproxy`, `espup install`, then
`. ~/export-esp.sh` in each shell.

```bash
cd firmware
cp provision.example.toml provision.toml   # gitignored: Wi-Fi, MQTT broker and account, config defaults
cargo +esp build --release
cargo +esp run --release                   # flash and monitor; append -- --port /dev/ttyUSB0 if needed
```

`build.rs` bakes `provision.toml` into the image and rejects invalid `[eyes]` values. Without the file the
eyes still run, idle and offline. The first build compiles ESP-IDF v5.3.3 into `firmware/.embuild`.

### Wiring

The HUB75 map is flight-rs's WROOM-32 wiring (`firmware/src/board.rs` is authoritative). Use the panel's
**input** connector. On the 2x8 ribbon, position = pin number counted from the red-striped (pin 1) edge;
rainbow wire colours repeat after 10, so count positions rather than trusting colours.

| Pin | Signal | GPIO | Pin | Signal | GPIO |
| --- | --- | --- | --- | --- | --- |
| 1 | R1 | 25 | 9 | A | 23 |
| 2 | G1 | 26 | 10 | B | 19 |
| 3 | B1 | 27 | 11 | C | 5 |
| 4 | GND | GND | 12 | D | 17 |
| 5 | R2 | 14 | 13 | CLK | 16 |
| 6 | G2 | 12 | 14 | LAT (STB) | 4 |
| 7 | B2 | 13 | 15 | OE (BLANK) | 15 |
| 8 | E | 32 | 16 | GND | GND |

- **Pin 8 must be E (GPIO32)**: a 128x64 panel is 1/32 scan and needs it. Wrong here shows mirrored or
  repeated halves.
- **GPIO12 (G2) is a strapping pin** that sets the flash voltage and must be low at reset. If the board
  won't boot or flash with the panel attached, unplug the ribbon to confirm. GPIO5 and GPIO15 are also
  strapping pins, but their defaults are compatible with normal boot.
- Check pin 1's orientation before powering up; a reversed ribbon swaps every signal. A wrong signal wire
  only garbles the image.
- **Never** connect the panel's 5 V to the ESP32's 3V3. Power the panel from its own 5 V supply (a 128x64
  panel can draw several amps at full white; size for 8 A of headroom) and tie all grounds together.
  Brightness defaults to 64/255.

The boot log reports the HUB75 buffer mode (double, or the single-buffer fallback when DMA RAM is short),
free heap after the panel, and every 30 s the frame-time stats, heap and render-loop stack margin; the
network thread logs its stack margin every 5 minutes. If the panel can't be initialised at all the board
restarts after 10 s; a failing network is rebuilt with backoff and never stops the animation.

`build.rs` also rejects a Wi-Fi SSID over 32 bytes or password over 64 (the driver would blank them).

## MQTT

| Topic | Payload |
| --- | --- |
| `halloween/porch/distance` | Metres as text (`1.83`), from porch-sensor ~10 Hz; `none` when it sees nothing within its range. `none` or ≥ `max_range_m` means nobody. |
| `halloween/eyes/config` | Retained JSON with any of the keys below, merged over the provisioned defaults. Invalid messages are rejected whole, with a log line. Publish an empty retained message to go back to defaults. |
| `halloween/eyes/status` | Retained `online`, or `offline` via the last will. |

Tune without reflashing, for example:

```bash
mosquitto_pub -h 10.0.0.84 -u <user> -P <password> -r -t halloween/eyes/config \
  -m '{"eyes_offset_x_m": 0.8, "eyes_height_m": 1.9, "brightness": 48}'
```

| Key | Default | Meaning |
| --- | --- | --- |
| `eyes_offset_x_m` | 0.6 | Panel centre's sideways offset from the sensor's aim line; positive is to the **right** standing at the sensor looking down the walkway (the approaching visitor's left). |
| `eyes_offset_y_m` | −0.3 | Offset along the walkway; negative is behind the sensor (towards the door). |
| `eyes_height_m` | 1.5 | Panel centre height above the ground. |
| `panel_yaw_deg` | 0 | 0 faces straight down the walkway; positive turns the panel's face to the **right** as seen standing at the sensor looking down the walkway (towards +x). |
| `panel_pitch_deg` | 0 | Positive tilts the panel up. |
| `face_height_m` | 1.6 | Assumed visitor face height. |
| `pixel_pitch_mm` | 3.0 | LED pitch; sets how far apart the eyes physically are. |
| `max_gaze_deg` | 55 | Angle at which the pupil reaches the edge of the eye. |
| `max_range_m` | 4.5 | Readings at or beyond this mean nobody. At most 4.5, the sensor's own limit. |
| `stale_timeout_s` | 2.0 | Go idle after this long without a reading. |
| `brightness` | 64 | LED brightness, 1–255. |

The coordinate frame: origin on the ground under the sensor, +y down the walkway towards the street (where
visitors come from), +z up, and +x to the **right** when standing at the sensor looking down the walkway,
which is the approaching visitor's **left**. The emulator's top-down view labels +x and both ends.

Blinks ride the eye movements: when idle, each glance of 10° or more carries a blink that closes just
after the eyes start moving and reopens just before they land (smaller glances don't blink, and the eyes
never go 6 s without one). While tracking, routine blinks wait until the gaze is still, and the startled
blink on noticing someone rides the move towards them. The emulator's **Speed** control (down to 0.1×)
slows the simulation so you can watch this.

Readings are debounced: the eyes notice someone after two consecutive in-range readings and let go after
half a second of `none` (or the stale timeout with no readings at all). The broker accounts for `porch-sensor` and the eyes are created in the homeassistant repo.
