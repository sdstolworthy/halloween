# Tracking eyes — design

Date: 2026-10-20. Status: built; not yet bench-flashed. This is the "Eyes" piece deferred in the
[porch show spec](2026-10-20-porch-show-design.md).

## Goal

A pair of animated eyes on a 128x64 RGB HUB75 panel (ESP32-WROOM-32, no PSRAM) that look at a visitor
walking up the front path. The visitor's distance comes from porch-sensor over MQTT. A browser emulator
built from the same renderer shows exactly what the panel will show for any distance and geometry, and is
deployed on Coolify.

## Architecture

```
porch-sensor (ESPHome) ──MQTT halloween/porch/distance (~10 Hz)──▶ Mosquitto ──▶ eyes firmware
HA / mosquitto_pub ──────MQTT halloween/eyes/config (retained)───▶           ──▶ eyes firmware
eyes firmware ───────────MQTT halloween/eyes/status (retained, LWT)──▶
```

The Rust code mirrors flight-rs: a host Cargo workspace of hardware-free crates plus a separate firmware
workspace.

| Crate | Role |
| --- | --- |
| `eyes-core` | `no_std` + alloc. Geometry, config model (validation, JSON merge), payload parsing, MQTT topics, the track/idle/blink state machine. Time is a parameter; randomness an injected `Rng`. |
| `eyes-render` | `no_std`. Draws a `Pose` on any 128x64 `DrawTarget` whose colour converts from `Rgb888`. |
| `eyes-hal` | `no_std`. `Matrix`, `Clock`, `Inbox` traits, the double/single buffer ladder, mocks, and `App`, the device frame loop (drain messages, tick, draw, flush), tested on the host. |
| `eyes-web` | Raw-ABI wasm emulator (no wasm-bindgen) and the static page. |
| `eyes-emulator` | Native binary writing a walk-up contact sheet PNG. |
| `firmware/` | ESP-IDF std binary: HUB75 matrix, clock, hardware RNG seed, Wi-Fi + MQTT thread. Own workspace. |

### Reuse of flight-rs

flight-rs (`sdstolworthy/flight-rs`) is a **private** repository and this one is public, so a Cargo git
dependency on it would make this repo unbuildable for anyone without access, including a Coolify build.
Beyond that, `flight-hal` depends on `flight-core` and carries flight-panel storage keys and a Wi-Fi portal
the eyes do not need. So instead:

- The `Matrix` trait and the double-buffer → single-buffer ladder are re-stated in `eyes-hal` (≈30
  lines), credited to flight-hal at rev `2ebde02`.
- The C HUB75 shim is copied to `firmware/components/matrix_shim/` with a `PROVENANCE.md` (source repo,
  rev, the one change). The underlying driver (mrcodetastic ESP32-HUB75-MatrixPanel-DMA 3.0.11) is still
  fetched by the IDF component manager exactly as flight-rs does.
- `firmware/src/matrix.rs` and the WROOM-32 pin map are adapted from flight-rs's with attribution.

## Geometry

Right-handed world frame in metres: origin on the ground directly below the sensor; **+y** along the
walkway from the sensor towards the approaching visitor (the sensor's aim axis); **+x** lateral; **+z** up.
The walkway is narrow, so the visitor's face is assumed on the sensor axis at `(0, d, face_height_m)`,
where `d` is the reported distance treated as horizontal (the slant from the sensor's ~1 m mounting height
is ignored).

The panel centre is at `(eyes_offset_x_m, eyes_offset_y_m, eyes_height_m)`. Its outward normal is
`(sin yaw · cos pitch, cos yaw · cos pitch, sin pitch)`: `panel_yaw_deg = 0` faces down the walkway (+y),
positive yaw turns it towards +x, positive `panel_pitch_deg` tilts it up. "Screen right" (increasing pixel
x as seen by someone facing the panel) is `(−cos yaw, sin yaw, 0)`; "screen up" is normal × right.

The eyes are centred at pixels (32, 32) and (96, 32), so their physical centres are the panel centre
±32 px along screen-right, at `pixel_pitch_mm` per pixel (64 px × 3 mm = 0.192 m apart on a P3 panel).
For each eye, the vector to the face is resolved into the panel frame:

```
forward = v · normal,   right = v · screen_right,   up = v · screen_up
yaw   = atan2(right, forward)
pitch = atan2(up, hypot(forward, right))
```

Each eye aims from its own position, so vergence emerges: at 0.3 m on-axis each eye turns ~17.7° towards
the nose; at 2 m, ~2.7°.

Angles map to the pupil like a rotating eyeball: offset ∝ sin θ, normalised so `max_gaze_deg` reaches the
edge of travel, each axis clamped to ±`max_gaze_deg` (so a face behind the panel, |yaw| > 90°, pins the
pupil to the edge instead of wrapping), and the 2-D result clamped to the unit disc.

## Behaviour

- **Tracking** when the latest reading is a distance below `max_range_m` and no older than
  `stale_timeout_s`. Each eye's gaze follows its target through a critically damped spring
  (ω = 14 rad/s, ~90% of a step in 0.28 s), solved in closed form so it is exact for any frame time and
  never overshoots. This hides the sensor's 10 Hz steps and jitter.
- **Idle** otherwise (nobody, out of range, or the sensor/broker went quiet): stare where the visitor was
  for 1.5 s, then wander to a random point every 1–3.5 s (25% of glances return to centre) on a lazier
  spring (ω = 6).
- **Blinks** in both modes: 240 ms (close 70, hold 50, open 120), every 2.5–6 s idle or 2–5 s tracking,
  15% double blinks, plus a startled blink the moment a visitor is noticed.
- **Pupils** dilate as the visitor approaches (0 at max range, 1 at 0.3 m; idle 0.35), with a 0.35 s lag.
- **Suspicion**: inside 1 m the lids narrow (up to 40% closed at 0.4 m) and tilt into a frown.
- Frame times are clamped to 250 ms so a stall resumes timers instead of skipping them; a clock that steps
  backwards freezes the eyes instead of panicking.
- Randomness only comes from the injected RNG (SplitMix64; the firmware seeds it from `esp_random`, the
  emulator from the URL's `seed`), so identical inputs give identical frames.

## Rendering

Every pixel of the 128x64 frame is shaded analytically — almond sclera with a spherical rim falloff,
amber-to-ember iris with a dark limbus, black pupil with a glint, lids that follow the sclera outline when
open and meet 6 px below centre when shut, a shadow under the upper lid — and written in one
`fill_contiguous` pass. A frame is therefore always a full repaint: no clear is needed, and in the
single-buffer fallback the only artefact is a brief tear while the frame is written.

The sclera is a dim bone colour, not white: two white eyeballs would light ~40% of the panel at full
current. At the default `brightness` 64/255 the frame draws roughly a quarter of what the same frame would
at 255.

**Pixel parity.** The renderer uses only IEEE-754 single-precision arithmetic and `libm`, with no
platform-dependent code paths, so the same pose produces the same pixels on x86, wasm32 and Xtensa. This
is checked, not assumed, for host vs wasm: `eyes-web/web/smoke.mjs` replays a scripted 3-second run through
the release wasm and requires the frame's FNV-1a hash to equal the one the host test pins. The Docker build
runs it, so an image whose pixels differ cannot be built. Xtensa parity relies on the same argument and is
not tested (the firmware cannot run here).

## Messaging contract

| Topic | Direction | Payload | QoS | Retained |
| --- | --- | --- | --- | --- |
| `halloween/porch/distance` | porch-sensor → eyes | Decimal metres as plain text, e.g. `1.83`. `none`, or any value ≥ `max_range_m`, means nobody. | 0 | no |
| `halloween/eyes/config` | you → eyes | JSON object with any subset of the config keys below. | 1 | **yes** |
| `halloween/eyes/status` | eyes → broker | `online` on every connect; `offline` as the MQTT last will. | 1 | yes |

- porch-sensor already reports "no echo" and "beyond its Max range" as 4.5 m, and publishes `%.2f`, so
  "nobody" arrives as `4.50`; the eyes' default `max_range_m` of 4.5 treats it as nobody.
- Distance payloads that are not UTF-8, not a number, negative or non-finite are ignored with a log line.
- Each config message is merged over the **provisioned defaults**, not over the previous message, so the
  retained payload alone describes the tuning. It is all or nothing: an unknown key, a non-number or an
  out-of-range value rejects the whole message with a log line naming the key and range, and the current
  config stays. Publishing an empty retained message clears it and restores the defaults.
- The eyes subscribe on every (re)connect, because clean sessions forget subscriptions.

### Config keys

| Key | Default | Range | Meaning |
| --- | --- | --- | --- |
| `eyes_offset_x_m` | 0.6 | −10..10 | Panel centre's lateral offset from the sensor axis. |
| `eyes_offset_y_m` | −0.3 | −10..10 | Offset along the walkway; negative is behind the sensor (towards the door). |
| `eyes_height_m` | 1.5 | 0..5 | Panel centre height above the ground. |
| `panel_yaw_deg` | 0 | −180..180 | Facing; 0 is down the walkway, positive turns towards +x. |
| `panel_pitch_deg` | 0 | −60..60 | Positive tilts the panel up. |
| `face_height_m` | 1.6 | 0.3..2.5 | Assumed visitor face height. |
| `pixel_pitch_mm` | 3.0 | 1..10 | LED pitch; sets the eyes' physical spacing (P3 128x64 = 3 mm). |
| `max_gaze_deg` | 55 | 10..85 | Gaze angle at which the pupil reaches the edge of its travel. |
| `max_range_m` | 4.5 | 0.5..10 | Readings at or beyond this mean nobody. |
| `stale_timeout_s` | 2.0 | 0.2..60 | Go idle after this long without a reading. |
| `brightness` | 64 | 1..255, integer | LED brightness cap. A 128x64 panel at full white draws several amps. |

## Firmware

- **Provisioning** follows flight-rs: a gitignored `firmware/provision.toml` (template
  `provision.example.toml`) is baked into the image by `build.rs` as compile-time environment variables:
  Wi-Fi SSID/password, MQTT URL/username/password, and an `[eyes]` table of config defaults. `build.rs`
  validates `[eyes]` with `eyes-core`'s own rules and fails the build on a bad value or unknown key. No
  NVS settings and no captive portal: runtime tuning is the retained config topic. Without a
  `provision.toml` the eyes run idle and offline.
- **Start-up order:** the HUB75 matrix is initialised first, while internal RAM is unfragmented, walking
  the double → single buffer ladder with a warning on fallback; then the network thread starts.
- **Render loop** on the main task, pinned to core 1 (`CONFIG_ESP_MAIN_TASK_AFFINITY_CPU1`) so Wi-Fi and
  lwIP on core 0 never steal frame time, at **30 fps** with a 1 kHz FreeRTOS tick so the 33 ms sleep is
  accurate. The I2S DMA driver refreshes the panel on its own, independent of the frame rate, so the frame
  rate only sets motion smoothness; 30 fps is smooth for spring-damped motion and leaves headroom for the
  8192 per-pixel FFI writes each frame. The loop logs mean/worst frame time, overruns and heap every 30 s
  so 60 fps can be evaluated on the bench.
- **Network thread** (8 KiB stack): Wi-Fi station with exponential-backoff rejoin (2 s → 60 s) and a link
  check every 2 s; esp-mqtt with its own 5 s reconnect, 15 s keepalive, the status last will, and a 1 KiB
  buffer (larger payloads are ignored). Received messages go through a bounded channel (32) to the render
  loop, which drains at most 16 per frame; overflow is dropped and logged. Nothing in this path can block
  or panic the render loop.
- **RAM levers** for the PSRAM-less WROOM-32: matrix first; trimmed Wi-Fi RX buffers and AMPDU off in
  `sdkconfig.defaults`; no TLS, HTTP server or NVS settings.

## porch-sensor change

`porch-sensor.yaml` gains an `mqtt:` block (broker, username and password from `secrets.yaml`) next to the
native API, which ESPHome supports ("enabling `api` and `mqtt`", esphome.io/components/mqtt). Implications
and choices:

- `discovery: false`: HA already has every entity over the API, and MQTT discovery would duplicate them.
- `reboot_timeout: 0s`: MQTT's own reboot timer (default 15 min) is disabled, so a broker outage never
  reboots the sensor, matching `api: reboot_timeout: 0s`.
- `log_topic: null`: logs stay on serial/API rather than streaming to the broker.
- The default birth/will on `porch-sensor/status` and per-entity state topics under `porch-sensor/` remain;
  harmless, and internal sensors (like the raw distance) are not published there.
- The raw ~10 Hz distance (after the nobody mapping and median filter) is published from its `on_value`,
  guarded by `mqtt.connected`, via `sensor: - id: !extend porch_distance_raw` in `porch-sensor.yaml`. The
  reusable `sensor.yaml` package stays MQTT-free.
- The broker account (`porch-sensor`) is created separately in the homeassistant repo.

## Emulator

`eyes-web` exposes the real `Eyes` state machine and renderer. The page plays the sensor (a reading every
100 ms with optional seeded noise, a "nobody" toggle, and a "sensor online" toggle to watch the stale
timeout), drives frames from `requestAnimationFrame`, and shows: the LED panel; per-eye yaw/pitch; top-down
and side diagrams of sensor, panel, normal and gaze lines (drag the visitor to set distance); controls for
every config key, built from the core's param table so ranges match the firmware; a "walk up" animation
(from beyond max range to 0.3 m at 0.8 m/s); and the JSON to publish to the config topic, plus a box that
validates a payload with the device's merge rules. Everything is mirrored in the URL query string.

Deployment: `eyes/Dockerfile` (build context `eyes/`) builds the wasm in `rust:1.96.0-slim`, runs the
smoke test in `node:22-alpine`, and serves the page from `nginx:alpine` on port 80 with `/healthz` and a
Docker `HEALTHCHECK`. No compiled wasm is committed.

## Testing

- `eyes-core`: table-driven tests for geometry (panel axes, gaze angles for representative installs),
  config validation and JSON merge, payload parsing, springs, and state-machine behaviour (mode, staleness,
  smoothing, vergence, blinks, wander, linger, dilation, squint, determinism, clock steps).
- `eyes-render`: region assertions (pupil/iris/sclera placement, gaze direction, vergence, blink, squint,
  dilation, Rgb565, non-finite poses) and golden FNV hashes at representative distances.
- `eyes-hal`: the frame loop with mocks (routing, bad payloads, merge-over-defaults, rejection, clearing,
  brightness writes, flood limiting, staleness) and the buffer ladder.
- `eyes-web`: ABI-level behaviour and the pinned scripted-run hash; `smoke.mjs` checks the wasm against it.
- Firmware: `cargo +esp build --release` must succeed. Not runnable here; bench acceptance below.

## Bench and porch acceptance (to do)

- Boot log shows the HUB75 buffer mode, heap after HUB75, Wi-Fi join, MQTT connect; the 30 s stats line
  shows mean frame time well under 33 ms and zero overruns.
- `halloween/eyes/status` reads `online`; pulling power flips it to `offline`.
- Walking up the path, the eyes pick the visitor up and follow; at the door they converge and narrow.
- Killing the broker or the sensor: the eyes go idle within the stale timeout and recover by themselves.
