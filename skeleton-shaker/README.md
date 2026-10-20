# Skeleton Shaker

Halloween porch prop: pressure mat → ESP32 → L298N → 12 V gear motor shakes a skeleton string. Full spec, BOM and
wiring in [HANDOFF.md](HANDOFF.md).

## Firmware (ESPHome)

```sh
cp secrets.example.yaml secrets.yaml   # fill in; api_key from `openssl rand -base64 32`
uvx esphome run skeleton-shaker.yaml   # first flash over USB; later flashes go over Wi-Fi
uvx esphome logs skeleton-shaker.yaml  # serial/network logs (115200 baud)
```

The prop runs fine with no network. Once on Wi-Fi, tune it from `http://skeleton-shaker.local` or Home Assistant:
dance/burst/pause/duty/cooldown min-max sliders (persisted across reboots), an Armed switch, Test dance and Stop
buttons, and a State sensor (idle / dancing / cooldown).

Dance logic lives in `shaker.h` so it can be tested on the host:

```sh
g++ -std=c++17 -Wall -o /tmp/shaker_test tests/shaker_test.cpp && /tmp/shaker_test
```

## Enclosure

`python3 skeleton_enclosure.py` and `python3 crank_arm.py` regenerate the STLs (deps listed in HANDOFF.md §5).
