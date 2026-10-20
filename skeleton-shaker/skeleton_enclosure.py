"""
Parametric enclosure for the Halloween skeleton shaker controller.

Holds: L298N driver (43x43 mm, 37 mm hole pattern), ESP32 DevKitC (38-pin,
54.4 x 27.9 mm, loose in a corner-post cradle, USB out the gland wall),
LM2596 buck module (43.2 x 20.7 mm, corner-post pocket), 2x PG9 + 1x PG7
cable glands in the "down" wall, 4x M3 lid screws, two wall-mount ears.

Run:  python3 skeleton_enclosure.py   ->  enclosure_base.stl, enclosure_lid.stl
Edit the numbers at the top to fit a different board.
"""
import numpy as np
import trimesh
from trimesh.creation import box as _box, cylinder as _cyl

# ---------------- parameters (mm) ----------------
W, D, H = 120.0, 84.0, 36.0      # interior cavity
T = 2.4                          # wall and floor thickness
BOSS_R, BOSS_HOLE_R, BOSS_HOLE_DEPTH = 3.5, 1.3, 12.0   # M3 thread-forming
LID_T, LIP_H, LIP_T, LIP_CLR = 2.4, 3.5, 1.6, 0.3

# L298N: PCB corner at (LX, LY); 37 mm square hole pattern, holes 3 mm in from edges
LX, LY = 10.0, 12.0
L298_PITCH, STAND_R, STAND_H, STAND_HOLE_R = 37.0, 3.0, 5.0, 1.3

# ESP32 DevKitC (38-pin). For a 30-pin DOIT DevKit V1 use 51.5 x 28.5.
ESP_L, ESP_W = 54.4, 27.9
ESP_X0, ESP_Y0 = 60.8, 2.0        # board corner; USB end faces the y=0 wall
ESP_CLR, POST_T, POST_L, POST_H = 0.4, 1.6, 6.0, 14.0
USB_W, USB_Z0, USB_Z1 = 14.0, 7.5, 17.5   # opening in the y=0 wall

# LM2596 buck module pocket (long axis along y, in the strip right of the ESP32)
BUCK_L, BUCK_W, BUCK_X0, BUCK_Y0, BUCK_POST_H = 43.2, 20.7, 96.0, 10.0, 10.0

# Two WAGO 221-415 lever nuts (12 V rail, GND rail) in a drop-in cradle against the back wall.
# 221-415 body: 20.6 wide x 18.6 deep x 8.3 tall, wires enter the 20.6 mm face. Verify with calipers.
WAGO_W, WAGO_D, WAGO_CLR = 20.6, 18.6, 0.6
WAGO_WALL, WAGO_WALL_H, WAGO_LIP_H, WAGO_X0 = 1.6, 6.0, 2.0, 10.0

# Cable glands in the y=0 wall: (x center, hole radius). PG9 -> 16 mm, PG7 -> 12.5 mm
GLANDS = [(20.0, 8.0), (46.0, 8.0), (102.0, 6.25)]
GLAND_Z = 18.0

# Mounting ears (in the floor plane, on the left/right outer walls)
EAR_L, EAR_W, EAR_T, EAR_HOLE_R = 12.0, 14.0, 3.0, 2.25
# -------------------------------------------------


def box(x0, x1, y0, y1, z0, z1):
    b = _box(extents=[x1 - x0, y1 - y0, z1 - z0])
    b.apply_translation([(x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2])
    return b


def cyl_z(cx, cy, z0, z1, r, n=64):
    c = _cyl(radius=r, height=z1 - z0, sections=n)
    c.apply_translation([cx, cy, (z0 + z1) / 2])
    return c


def cyl_y(cx, cz, y0, y1, r, n=64):
    c = _cyl(radius=r, height=y1 - y0, sections=n)
    c.apply_transform(trimesh.transformations.rotation_matrix(np.pi / 2, [1, 0, 0]))
    c.apply_translation([cx, (y0 + y1) / 2, cz])
    return c


def union(parts):
    return trimesh.boolean.union(parts, engine="manifold")


def diff(a, parts):
    return trimesh.boolean.difference([a] + parts, engine="manifold")


def l_post(cx, cy, sx, sy, h):
    """L-shaped corner post hugging a board corner at (cx, cy).
    sx, sy = +1/-1: direction the board extends from this corner."""
    bar_x = box(cx - (POST_T if sx > 0 else 0), cx + (POST_T if sx < 0 else 0) + 0,
                cy - (POST_T if sy > 0 else 0), cy + (POST_T if sy < 0 else 0), 0, h)
    # along x (lies against the board's y edge)
    ax = box(min(cx, cx + sx * POST_L), max(cx, cx + sx * POST_L),
             cy - POST_T if sy > 0 else cy, cy if sy > 0 else cy + POST_T, 0, h)
    # along y (lies against the board's x edge)
    ay = box(cx - POST_T if sx > 0 else cx, cx if sx > 0 else cx + POST_T,
             min(cy, cy + sy * POST_L), max(cy, cy + sy * POST_L), 0, h)
    return union([bar_x, ax, ay])


def corner_posts(x0, y0, L, Wd, clr, h):
    x1, y1 = x0 + L, y0 + Wd
    c = clr / 2
    return [l_post(x0 - c, y0 - c, +1, +1, h), l_post(x1 + c, y0 - c, -1, +1, h),
            l_post(x0 - c, y1 + c, +1, -1, h), l_post(x1 + c, y1 + c, -1, -1, h)]


BOSS_XY = [(BOSS_R, BOSS_R), (W - BOSS_R, BOSS_R), (BOSS_R, D - BOSS_R), (W - BOSS_R, D - BOSS_R)]


def build_base():
    shell = box(-T, W + T, -T, D + T, -T, H)
    shell = diff(shell, [box(0, W, 0, D, 0, H + 1)])

    adds = [cyl_z(x, y, 0, H, BOSS_R) for x, y in BOSS_XY]

    # L298N standoffs
    hx = [LX + 3.0, LX + 3.0 + L298_PITCH]
    hy = [LY + 3.0, LY + 3.0 + L298_PITCH]
    stand_xy = [(x, y) for x in hx for y in hy]
    adds += [cyl_z(x, y, 0, STAND_H, STAND_R) for x, y in stand_xy]

    # ESP32 cradle and buck pocket
    adds += corner_posts(ESP_X0, ESP_Y0, ESP_W, ESP_L, ESP_CLR, POST_H)   # long axis along y
    adds += corner_posts(BUCK_X0, BUCK_Y0, BUCK_W, BUCK_L, ESP_CLR, BUCK_POST_H)   # long axis along y

    # WAGO cradle: three full-height walls per pocket (back wall = enclosure wall), low lip at the wire-entry face
    pw = WAGO_W + WAGO_CLR
    inner_y0 = D - (WAGO_D + WAGO_CLR + 1.6)
    xs = [WAGO_X0, WAGO_X0 + WAGO_WALL + pw, WAGO_X0 + 2 * (WAGO_WALL + pw)]
    adds += [box(x, x + WAGO_WALL, inner_y0, D, 0, WAGO_WALL_H) for x in xs]
    adds += [box(WAGO_X0, xs[2] + WAGO_WALL, inner_y0 - WAGO_WALL, inner_y0, 0, WAGO_LIP_H)]

    # mounting ears
    ears = [box(-T - EAR_L, -T, D / 2 - EAR_W / 2, D / 2 + EAR_W / 2, -T, -T + EAR_T),
            box(W + T, W + T + EAR_L, D / 2 - EAR_W / 2, D / 2 + EAR_W / 2, -T, -T + EAR_T)]
    adds += ears

    base = union([shell] + adds)

    cuts = [cyl_z(x, y, H - BOSS_HOLE_DEPTH, H + 1, BOSS_HOLE_R) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, 0.6, STAND_H + 1, STAND_HOLE_R) for x, y in stand_xy]
    cuts += [cyl_y(x, GLAND_Z, -T - 1, 1.0, r) for x, r in GLANDS]
    usb_cx = ESP_X0 + ESP_W / 2
    cuts += [box(usb_cx - USB_W / 2, usb_cx + USB_W / 2, -T - 1, ESP_Y0 + 0.5, USB_Z0, USB_Z1)]
    cuts += [cyl_z(-T - EAR_L / 2, D / 2, -T - 1, 1, EAR_HOLE_R),
             cyl_z(W + T + EAR_L / 2, D / 2, -T - 1, 1, EAR_HOLE_R)]
    return diff(base, cuts)


def build_lid():
    plate = box(-T, W + T, -T, D + T, 0, LID_T)
    lip_outer = box(LIP_CLR, W - LIP_CLR, LIP_CLR, D - LIP_CLR, LID_T, LID_T + LIP_H)
    lip_inner = box(LIP_CLR + LIP_T, W - LIP_CLR - LIP_T, LIP_CLR + LIP_T, D - LIP_CLR - LIP_T,
                    LID_T - 1, LID_T + LIP_H + 1)
    lip = diff(lip_outer, [lip_inner])
    lid = union([plate, lip])
    cuts = [cyl_z(x, y, LID_T - 0.01, LID_T + LIP_H + 1, BOSS_R + 0.4) for x, y in BOSS_XY]   # clear the bosses
    cuts += [cyl_z(x, y, -1, LID_T + 1, 1.7) for x, y in BOSS_XY]                             # M3 clearance
    cuts += [cyl_z(x, y, -1, 1.2, 3.1) for x, y in BOSS_XY]                                   # counterbore, outer face
    return diff(lid, cuts)


if __name__ == "__main__":
    base = build_base()
    lid = build_lid()
    for name, m in [("enclosure_base.stl", base), ("enclosure_lid.stl", lid)]:
        m.export(name)
        print(f"{name}: watertight={m.is_watertight} volume={m.volume/1000:.1f} cm3 "
              f"extents={np.round(m.extents, 1)}")
