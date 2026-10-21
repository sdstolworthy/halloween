# Skeleton Shaker

Halloween porch prop: Home Assistant presses **Dance** → ESP32 → L298N → 12 V gear motor shakes a skeleton string.
The press comes from the porch scare automation, triggered by the `porch-sensor` distance sensor (see
[the porch show spec](../docs/specs/2026-10-20-porch-show-design.md)). Full BOM and wiring in
[HANDOFF.md](HANDOFF.md).

## Firmware (ESPHome)

```sh
cp secrets.example.yaml secrets.yaml            # fill in; api_key from `openssl rand -base64 32`
uvx esphome@2026.9.1 run skeleton-shaker.yaml   # first flash over USB; later flashes go over Wi-Fi
uvx esphome@2026.9.1 logs skeleton-shaker.yaml  # serial/network logs (115200 baud)
```

Entities (in Home Assistant or at `http://skeleton-shaker.local`):

- **Dance** button — runs one dance plus cooldown. Presses during the dance or cooldown are ignored, so
  overlapping requests can't stack.
- **Stop** button — aborts and coasts the motor.
- **State** — idle / dancing / cooldown.
- Dance/burst/pause/duty/cooldown min-max sliders, persisted across reboots.

There is no local trigger and no arming switch here; arming lives on the porch sensor. The board reboots after
15 min without Wi-Fi (ESPHome default) to recover, but not when only Home Assistant is down.

Dance logic lives in `shaker.h` so it can be tested on the host:

```sh
g++ -std=c++17 -Wall -Wextra -o /tmp/shaker_test tests/shaker_test.cpp && /tmp/shaker_test
```

## Enclosure

`python3 skeleton_enclosure.py` and `python3 crank_arm.py` regenerate the STLs (deps listed in HANDOFF.md §5).
The PG7 hole (formerly the mat lead) takes a blank PG7 plug.
