"""
Smaller enclosure for the servo version of the skeleton shaker.

Holds: ESP32 DevKitC (38-pin, running ESPHome) in a corner-post cradle with a USB slot, two
WAGO 221-415 lever nuts (5 V rail, GND rail) in a drop-in cradle, a panel-mount 5.5 x 2.1 mm DC
jack in the front wall, one PG7 gland in the right wall for the servo extension lead, 4x M3 lid
screws, two mounting ears. The trigger comes from Home Assistant over Wi-Fi, so there is no
sensor input; set LEFT_GLAND = True for a wired local trigger.

Reuses the helpers and shared dimensions from skeleton_enclosure.py (same folder).
Run:  python3 servo_enclosure.py  ->  servo_base.stl, servo_lid.stl
"""
import numpy as np
from skeleton_enclosure import (box, cyl_z, cyl_y, union, diff, corner_posts, T, BOSS_R, BOSS_HOLE_R,
                                BOSS_HOLE_DEPTH, LID_T, LIP_H, LIP_T, LIP_CLR, EAR_L, EAR_W, EAR_T, EAR_HOLE_R,
                                ESP_L, ESP_W, ESP_CLR, POST_H, USB_W, USB_Z0, USB_Z1,
                                WAGO_W, WAGO_D, WAGO_CLR, WAGO_WALL, WAGO_WALL_H, WAGO_LIP_H)

W, D, H = 96.0, 64.0, 28.0          # interior cavity (mm)
ESP_X0, ESP_Y0 = 8.0, 2.0           # board corner; long axis along y, USB toward the y=0 wall
WAGO_X0 = 40.0                      # cradle start, against the back wall
FRONT_Z = 15.0                      # centre height of the jack and glands
JACK_X, JACK_R = 60.0, 6.1          # panel-mount DC jack in the front wall, 12 mm hole
GLAND_R = 6.25                      # PG7 = 12.5 mm hole
GLAND_RIGHT_Y = 20.0                # servo lead, right wall (x = W)
LEFT_GLAND = False                  # True adds a second PG7 in the left wall (y = 46) for a wired local trigger
GLAND_LEFT_Y = 46.0

BOSS_XY = [(BOSS_R, BOSS_R), (W - BOSS_R, BOSS_R), (BOSS_R, D - BOSS_R), (W - BOSS_R, D - BOSS_R)]


def cyl_x(cy, cz, x0, x1, r, n=64):
    """Cylinder along x, for holes through the side walls."""
    import trimesh
    c = trimesh.creation.cylinder(radius=r, height=x1 - x0, sections=n)
    c.apply_transform(trimesh.transformations.rotation_matrix(np.pi / 2, [0, 1, 0]))
    c.apply_translation([(x0 + x1) / 2, cy, cz])
    return c


def build_base():
    shell = diff(box(-T, W + T, -T, D + T, -T, H), [box(0, W, 0, D, 0, H + 1)])
    adds = [cyl_z(x, y, 0, H, BOSS_R) for x, y in BOSS_XY]
    adds += corner_posts(ESP_X0, ESP_Y0, ESP_W, ESP_L, ESP_CLR, POST_H)

    pw = WAGO_W + WAGO_CLR
    inner_y0 = D - (WAGO_D + WAGO_CLR + 1.6)
    xs = [WAGO_X0, WAGO_X0 + WAGO_WALL + pw, WAGO_X0 + 2 * (WAGO_WALL + pw)]
    adds += [box(x, x + WAGO_WALL, inner_y0, D, 0, WAGO_WALL_H) for x in xs]
    adds += [box(WAGO_X0, xs[2] + WAGO_WALL, inner_y0 - WAGO_WALL, inner_y0, 0, WAGO_LIP_H)]

    adds += [box(-T - EAR_L, -T, D / 2 - EAR_W / 2, D / 2 + EAR_W / 2, -T, -T + EAR_T),
             box(W + T, W + T + EAR_L, D / 2 - EAR_W / 2, D / 2 + EAR_W / 2, -T, -T + EAR_T)]
    base = union([shell] + adds)

    cuts = [cyl_z(x, y, H - BOSS_HOLE_DEPTH, H + 1, BOSS_HOLE_R) for x, y in BOSS_XY]
    cuts += [cyl_y(JACK_X, FRONT_Z, -T - 1, 1.0, JACK_R)]
    cuts += [cyl_x(GLAND_RIGHT_Y, FRONT_Z, W - 1.0, W + T + 1, GLAND_R)]
    if LEFT_GLAND:
        cuts += [cyl_x(GLAND_LEFT_Y, FRONT_Z, -T - 1, 1.0, GLAND_R)]
    usb_cx = ESP_X0 + ESP_W / 2
    cuts += [box(usb_cx - USB_W / 2, usb_cx + USB_W / 2, -T - 1, ESP_Y0 + 0.5, USB_Z0, USB_Z1)]
    cuts += [cyl_z(-T - EAR_L / 2, D / 2, -T - 1, 1, EAR_HOLE_R),
             cyl_z(W + T + EAR_L / 2, D / 2, -T - 1, 1, EAR_HOLE_R)]
    return diff(base, cuts)


def build_lid():
    plate = box(-T, W + T, -T, D + T, 0, LID_T)
    lip = diff(box(LIP_CLR, W - LIP_CLR, LIP_CLR, D - LIP_CLR, LID_T, LID_T + LIP_H),
               [box(LIP_CLR + LIP_T, W - LIP_CLR - LIP_T, LIP_CLR + LIP_T, D - LIP_CLR - LIP_T,
                    LID_T - 1, LID_T + LIP_H + 1)])
    lid = union([plate, lip])
    cuts = [cyl_z(x, y, LID_T - 0.01, LID_T + LIP_H + 1, BOSS_R + 0.4) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, -1, LID_T + 1, 1.7) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, -1, 1.2, 3.1) for x, y in BOSS_XY]
    return diff(lid, cuts)


if __name__ == "__main__":
    for name, m in [("servo_base.stl", build_base()), ("servo_lid.stl", build_lid())]:
        m.export(name)
        print(f"{name}: watertight={m.is_watertight} volume={m.volume/1000:.1f} cm3 extents={np.round(m.extents, 1)}")
