# Skeleton Shaker

Halloween porch prop: Home Assistant presses **Dance** → ESP32 → MG90S micro servo tugs a shock cord tied to a
string of skeletons. The press comes from the porch scare automation, triggered by the `porch-sensor` distance
sensor (see [the porch show spec](../docs/specs/2026-10-20-porch-show-design.md)). BOM, wiring and design history in
[HANDOFF.md](HANDOFF.md).

## Firmware (ESPHome)

```sh
cp secrets.example.yaml secrets.yaml            # fill in; api_key from `openssl rand -base64 32`
uvx esphome@2026.9.1 run skeleton-shaker.yaml   # first flash over USB; later flashes go over Wi-Fi
uvx esphome@2026.9.1 logs skeleton-shaker.yaml  # serial/network logs (115200 baud)
```

Entities (in Home Assistant or at `http://skeleton-shaker.local`):

- **Dance** button and `esphome.skeleton_shaker_dance` action — one dance plus cooldown. Requests during the
  dance or cooldown are ignored, so overlapping requests can't stack.
- **Stop** button — aborts, centres the horn and detaches the servo.
- **Dancing** binary sensor.
- **Dance length** (3–20 s, default 8) and **Cooldown** (0–60 s, default 4), persisted across reboots.

A dance is random moves to 40–140° with 80–300 ms dwell; then the horn returns to 90° and the servo is detached so
it doesn't buzz or hold current. The sweep and dwell ranges are substitutions at the top of the YAML.

There is no local trigger and no arming switch here; arming lives on the porch sensor. The board reboots after
15 min without Wi-Fi (ESPHome default) to recover, but not when only Home Assistant is down.

Dance logic lives in `shaker.h` so it can be tested on the host:

```sh
g++ -std=c++17 -Wall -Wextra -o /tmp/shaker_test tests/shaker_test.cpp && /tmp/shaker_test
```

## Enclosure

`uv run python servo_enclosure.py` regenerates `servo_base.stl` and `servo_lid.stl` (deps pinned in `uv.lock`). It imports
shared helpers and dimensions from `skeleton_enclosure.py`, so keep both here.

`v1-motor-drive/` holds the v1 gear-motor parts (crank arm and the larger L298N enclosure) for a garland over
~300 g. Regenerate those from inside that folder (both scripts write to the current directory):
`cd v1-motor-drive && uv run python ../skeleton_enclosure.py && uv run python crank_arm.py`. The firmware here would need
the v1 L298N outputs back (see git history before the servo change).
