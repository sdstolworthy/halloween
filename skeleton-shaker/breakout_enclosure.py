"""
Enclosure v3 for the servo skeleton shaker: ESP32 on a screw-terminal breakout board.

Holds: 77 x 63 mm breakout board (holes on a 72 x 57 mm pattern, M3) with the ESP32 seated on
top (21 mm stack), LM2596 buck module (5 V rail -> 3V3 to the breakout), two WAGO 221-415 lever
nuts (5 V rail, GND rail), a panel-mount 5.5 x 2.1 mm DC jack in the front wall, and the MG90S
servo nose-mounted through the right wall so the horn sweeps outside (its own screws, pilot
holes through the wall; a pedestal under the body takes the torque). 4x M3 lid screws, two
mounting ears on the front/back walls. No cable glands: the only moving linkage is the horn.

Reuses the helpers and shared dimensions from skeleton_enclosure.py (same folder).
Run:  python3 breakout_enclosure.py  ->  breakout_base.stl, breakout_lid.stl
"""
import numpy as np
from skeleton_enclosure import (box, cyl_z, cyl_y, union, diff, corner_posts, T, BOSS_R, BOSS_HOLE_R,
                                BOSS_HOLE_DEPTH, LID_T, LIP_H, LIP_T, LIP_CLR, EAR_L, EAR_W, EAR_T, EAR_HOLE_R,
                                WAGO_W, WAGO_D, WAGO_CLR, WAGO_WALL, WAGO_WALL_H, WAGO_LIP_H)
from servo_enclosure import cyl_x

W, D, H = 140.0, 84.0, 30.0            # interior cavity (mm)

# Breakout board, long axis along x. Stack (board + seated ESP32) is 21 mm; screws from the top.
BK_L, BK_W, BK_X0, BK_Y0 = 77.0, 63.0, 4.0, 13.0
BK_INSET_X, BK_INSET_Y = 2.5, 3.0      # hole centres: 72 x 57 pattern inside the 77 x 63 outline
STAND_R, STAND_H, STAND_HOLE_R = 3.0, 5.0, 1.3   # M3 thread-forming

# LM2596 buck module, long axis along y, in the strip right of the breakout.
BUCK_L, BUCK_W, BUCK_X0, BUCK_Y0, BUCK_CLR, BUCK_POST_H = 43.2, 20.7, 85.0, 19.0, 0.4, 10.0

# WAGO cradle against the back wall, right of the breakout.
WAGO_X0 = 85.0

# MG90S through the right wall (x = W): nose + shaft out, tabs against the inner face, fixed by
# the servo's own screws through wall pilot holes. Cross-section facing the wall: 22.9 x 12.2.
SRV_CY, SRV_CZ = 30.0, 16.0
SRV_CUT_W, SRV_CUT_H = 23.3, 12.6      # body 22.9 x 12.2 + clearance
SRV_SCREW_DY, SRV_PILOT_R = 13.9, 0.8  # tab holes 27.8 mm apart; pilots for the ~2.2 mm screws
SRV_BODY_IN = 24.0                     # body reach inward from the inner wall face
PED_W = 20.0                           # pedestal under the body takes the horn torque

JACK_X, JACK_Z, JACK_R = 120.0, 15.0, 6.1   # panel-mount DC jack, 12 mm hole, front wall

BOSS_XY = [(BOSS_R, BOSS_R), (W - BOSS_R, BOSS_R), (BOSS_R, D - BOSS_R), (W - BOSS_R, D - BOSS_R)]


def build_base():
    shell = diff(box(-T, W + T, -T, D + T, -T, H), [box(0, W, 0, D, 0, H + 1)])
    adds = [cyl_z(x, y, 0, H, BOSS_R) for x, y in BOSS_XY]

    # breakout standoffs
    hx = [BK_X0 + BK_INSET_X, BK_X0 + BK_L - BK_INSET_X]
    hy = [BK_Y0 + BK_INSET_Y, BK_Y0 + BK_W - BK_INSET_Y]
    stand_xy = [(x, y) for x in hx for y in hy]
    adds += [cyl_z(x, y, 0, STAND_H, STAND_R) for x, y in stand_xy]

    adds += corner_posts(BUCK_X0, BUCK_Y0, BUCK_W, BUCK_L, BUCK_CLR, BUCK_POST_H)

    # WAGO cradle (back wall = enclosure wall)
    pw = WAGO_W + WAGO_CLR
    inner_y0 = D - (WAGO_D + WAGO_CLR + 1.6)
    xs = [WAGO_X0, WAGO_X0 + WAGO_WALL + pw, WAGO_X0 + 2 * (WAGO_WALL + pw)]
    adds += [box(x, x + WAGO_WALL, inner_y0, D, 0, WAGO_WALL_H) for x in xs]
    adds += [box(WAGO_X0, xs[2] + WAGO_WALL, inner_y0 - WAGO_WALL, inner_y0, 0, WAGO_LIP_H)]

    # servo pedestal: top at the body's underside (z = SRV_CZ - 6.1 - 0.1)
    adds += [box(W - SRV_BODY_IN, W - 2.0, SRV_CY - PED_W / 2, SRV_CY + PED_W / 2, 0, SRV_CZ - 6.2)]

    # mounting ears, front/back walls
    adds += [box(W / 2 - EAR_W / 2, W / 2 + EAR_W / 2, -T - EAR_L, -T, -T, -T + EAR_T),
             box(W / 2 - EAR_W / 2, W / 2 + EAR_W / 2, D + T, D + T + EAR_L, -T, -T + EAR_T)]
    base = union([shell] + adds)

    cuts = [cyl_z(x, y, H - BOSS_HOLE_DEPTH, H + 1, BOSS_HOLE_R) for x, y in BOSS_XY]
    cuts += [cyl_z(x, y, 0.6, STAND_H + 1, STAND_HOLE_R) for x, y in stand_xy]
    cuts += [cyl_y(JACK_X, JACK_Z, -T - 1, 1.0, JACK_R)]
    # servo: body cutout and screw pilots through the right wall
    cuts += [box(W - 1.0, W + T + 1, SRV_CY - SRV_CUT_W / 2, SRV_CY + SRV_CUT_W / 2,
                 SRV_CZ - SRV_CUT_H / 2, SRV_CZ + SRV_CUT_H / 2)]
    cuts += [cyl_x(SRV_CY - SRV_SCREW_DY, SRV_CZ, W - 1.0, W + T + 1, SRV_PILOT_R),
             cyl_x(SRV_CY + SRV_SCREW_DY, SRV_CZ, W - 1.0, W + T + 1, SRV_PILOT_R)]
    cuts += [cyl_z(W / 2, -T - EAR_L / 2, -T - 1, 1, EAR_HOLE_R),
             cyl_z(W / 2, D + T + EAR_L / 2, -T - 1, 1, EAR_HOLE_R)]
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
    for name, m in [("breakout_base.stl", build_base()), ("breakout_lid.stl", build_lid())]:
        m.export(name)
        print(f"{name}: watertight={m.is_watertight} volume={m.volume/1000:.1f} cm3 extents={np.round(m.extents, 1)}")
