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

Move the visitor (slider, or drag them in the top-down view), press **Walk up to the door**, or untick
**Sensor online** to watch the stale timeout. Change the installation geometry and the page shows the
per-eye angles and the JSON to publish. The whole setup is kept in the URL, so you can share a link to it.

`docker build -t porch-eyes .` builds the site. The image runs the smoke test during the build and serves
the page on port 80, with `/healthz` for Coolify.

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

Wiring is flight-rs's WROOM-32 HUB75 map (`firmware/src/board.rs`). Power the panel from its own 5 V
supply (a 128x64 panel can draw several amps at full white) and share ground with the ESP32. Brightness
defaults to 64/255.

The boot log reports the HUB75 buffer mode (double, or the single-buffer fallback when DMA RAM is short),
free heap after the panel and every 30 s, and frame-time stats.

## MQTT

| Topic | Payload |
| --- | --- |
| `halloween/porch/distance` | Metres as text (`1.83`), from porch-sensor ~10 Hz. `none` or ≥ `max_range_m` means nobody. |
| `halloween/eyes/config` | Retained JSON with any of the keys below, merged over the provisioned defaults. Invalid messages are rejected whole, with a log line. Publish an empty retained message to go back to defaults. |
| `halloween/eyes/status` | Retained `online`, or `offline` via the last will. |

Tune without reflashing, for example:

```bash
mosquitto_pub -h 10.0.0.84 -u <user> -P <password> -r -t halloween/eyes/config \
  -m '{"eyes_offset_x_m": 0.8, "eyes_height_m": 1.9, "brightness": 48}'
```

| Key | Default | Meaning |
| --- | --- | --- |
| `eyes_offset_x_m` | 0.6 | Panel centre's sideways offset from the sensor's aim line. |
| `eyes_offset_y_m` | −0.3 | Offset along the walkway; negative is behind the sensor (towards the door). |
| `eyes_height_m` | 1.5 | Panel centre height above the ground. |
| `panel_yaw_deg` | 0 | 0 faces straight down the walkway; positive turns towards +x. |
| `panel_pitch_deg` | 0 | Positive tilts the panel up. |
| `face_height_m` | 1.6 | Assumed visitor face height. |
| `pixel_pitch_mm` | 3.0 | LED pitch; sets how far apart the eyes physically are. |
| `max_gaze_deg` | 55 | Angle at which the pupil reaches the edge of the eye. |
| `max_range_m` | 4.5 | Readings at or beyond this mean nobody. |
| `stale_timeout_s` | 2.0 | Go idle after this long without a reading. |
| `brightness` | 64 | LED brightness, 1–255. |

The coordinate frame: origin on the ground under the sensor, +y down the walkway towards visitors, +x to
the side, +z up. The broker accounts for `porch-sensor` and the eyes are created in the homeassistant repo.
