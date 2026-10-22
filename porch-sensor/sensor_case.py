"""
Door-mounted battery case for the porch ultrasonic sensor node.

A vertical box stuck flat on the door with two Command strips (recesses in the back face).
Top band: classic HC-SR04 (45.2 x 20.2 PCB) held against the front lid, cans out through two
17.2 mm holes. Middle: ESP32 DevKitC (38-pin) in a corner-post cradle, USB slot in the left
wall for flashing in place. Bottom: 4x AA holder pocket (NiMH 4.8 V or alkaline 6 V, both fine
into the DevKit's 5V pin) with a KCD11 rocker switch cutout in the right wall. The front lid
carries the sensor and its holes, centred in x so it still fits after the flip; 4x M3 screws.

Reuses the helpers from skeleton-shaker/skeleton_enclosure.py.
Run:  python3 sensor_case.py  ->  sensor_case_base.stl, sensor_case_lid.stl
"""
import sys, os
sys.path.append(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "skeleton-shaker"))

import numpy as np
from skeleton_enclosure import (box, cyl_z, cyl_y, union, diff, corner_posts, T, BOSS_R, BOSS_HOLE_R,
                                BOSS_HOLE_DEPTH, LID_T, LIP_H, LIP_T, LIP_CLR,
                                ESP_L, ESP_W, ESP_CLR, POST_H, USB_W, USB_Z0, USB_Z1)
from servo_enclosure import cyl_x

# Interior: x = width across the door, y = vertical (y=0 at the bottom), z = depth off the door.
W, D, H = 76.0, 122.0, 24.0

# 4x AA holder (2x2 flat, ~71.5 x 63.5 x 19.5) in a walled pocket at the bottom.
AA_L, AA_W, AA_H, AA_CLR, AA_WALL, AA_WALL_H = 71.5, 63.5, 19.5, 0.8, 1.6, 10.0
AA_X0, AA_Y0 = (W - AA_L - AA_CLR) / 2, 4.0

# ESP32 DevKitC, long axis along x, USB toward the left wall (x = 0).
ESP_X0, ESP_Y0 = 9.0, 72.0

# HC-SR04 against the lid, top band: PCB 45.2 x 20.2, cans 16.2 dia on 26.5 mm centres.
SR_L, SR_W = 45.2, 20.2
SR_CY = 110.0                       # can/PCB centreline (y)
SR_HOLE_R, SR_PITCH = 8.6, 26.5     # 17.2 mm holes
SR_POST_H = 6.0

# KCD11 rocker switch snap-in cutout in the right wall, battery band.
SW_CY, SW_CZ, SW_W, SW_H = 30.0, 12.0, 13.5, 9.1

# Command strip recesses in the back face (medium strips are 15.9 mm wide).
STRIP_W, STRIP_Y0, STRIP_Y1, STRIP_DEPTH = 16.5, 14.0, 106.0, 0.8
STRIP_X = [(6.0, 6.0 + STRIP_W), (W - 6.0 - STRIP_W, W - 6.0)]

BOSS_XY = [(BOSS_R, BOSS_R), (W - BOSS_R, BOSS_R), (BOSS_R, D - BOSS_R), (W - BOSS_R, D - BOSS_R)]


def build_base():
    shell = diff(box(-T, W + T, -T, D + T, -T, H), [box(0, W, 0, D, 0, H + 1)])
    adds = [cyl_z(x, y, 0, H, BOSS_R) for x, y in BOSS_XY]

    # ESP32 cradle (long axis along x)
    adds += corner_posts(ESP_X0, ESP_Y0, ESP_L, ESP_W, ESP_CLR, POST_H)

    # AA holder pocket: three walls (the fourth is lid pressure via the pack's own bulk)
    x0, x1 = AA_X0 - AA_WALL, AA_X0 + AA_L + AA_CLR + AA_WALL
    y1 = AA_Y0 + AA_W + AA_CLR
    adds += [box(x0, AA_X0, AA_Y0 - AA_WALL, y1 + AA_WALL, 0, AA_WALL_H),
             box(AA_X0 + AA_L + AA_CLR, x1, AA_Y0 - AA_WALL, y1 + AA_WALL, 0, AA_WALL_H),
             box(x0, x1, y1, y1 + AA_WALL, 0, AA_WALL_H),
             box(x0, x1, AA_Y0 - AA_WALL, AA_Y0, 0, AA_WALL_H)]
    base = union([shell] + adds)

    cuts = [cyl_z(x, y, H - BOSS_HOLE_DEPTH, H + 1, BOSS_HOLE_R) for x, y in BOSS_XY]
    # USB slot in the left wall at the DevKit band
    usb_cy = ESP_Y0 + ESP_W / 2
    cuts += [box(-T - 1, 1.0, usb_cy - USB_W / 2, usb_cy + USB_W / 2, USB_Z0, USB_Z1)]
    # rocker switch in the right wall
    cuts += [box(W - 1.0, W + T + 1, SW_CY - SW_W / 2, SW_CY + SW_W / 2, SW_CZ - SW_H / 2, SW_CZ + SW_H / 2)]
    # Command strip recesses in the back (outer floor face)
    cuts += [box(xa, xb, STRIP_Y0, STRIP_Y1, -T - 1, -T + STRIP_DEPTH) for xa, xb in STRIP_X]
    return diff(base, cuts)


def build_lid():
    plate = box(-T, W + T, -T, D + T, 0, LID_T)
    lip = diff(box(LIP_CLR, W - LIP_CLR, LIP_CLR, D - LIP_CLR, LID_T, LID_T + LIP_H),
               [box(LIP_CLR + LIP_T, W - LIP_CLR - LIP_T, LIP_CLR + LIP_T, D - LIP_CLR - LIP_T,
                    LID_T - 1, LID_T + LIP_H + 1)])
    # corner posts on the inner face hold the HC-SR04 PCB over its holes (centred in x,
    # so the lid still fits after being flipped onto the box)
    posts = corner_posts(W / 2 - SR_L / 2, SR_CY - SR_W / 2, SR_L, SR_W, 0.4, LID_T + SR_POST_H)
    posts = [diff(p, [box(-T, W + T, -T, D + T, -1, LID_T)]) for p in posts]  # keep posts above the plate
    lid = union([plate, lip] + posts)
    cuts = [cyl_z(x, y, LID_T - 0.01, LID_T + LIP_H + 1, BOSS_R + 0.4) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, -1, LID_T + 1, 1.7) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, -1, 1.2, 3.1) for x, y in BOSS_XY]
    cuts += [cyl_z(W / 2 - SR_PITCH / 2, SR_CY, -1, LID_T + 1, SR_HOLE_R),
             cyl_z(W / 2 + SR_PITCH / 2, SR_CY, -1, LID_T + 1, SR_HOLE_R)]
    return diff(lid, cuts)


if __name__ == "__main__":
    for name, m in [("sensor_case_base.stl", build_base()), ("sensor_case_lid.stl", build_lid())]:
        m.export(name)
        print(f"{name}: watertight={m.is_watertight} volume={m.volume/1000:.1f} cm3 extents={np.round(m.extents, 1)}")
