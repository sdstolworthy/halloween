"""
3D-printed crank arm that bolts onto a Pololu Universal Aluminum Mounting Hub
(6 mm shaft, M3 holes, item #1999) with two M3 x 8 mm screws.

Two radial slots accept M3 screws anywhere from r = 7 to 11 mm, so the arm
fits the hub's bolt circle without measuring it. Three crank-pin holes give a
20 / 30 / 40 mm throw; use an M4 x 20 bolt + nylock nut as the pin.

Run: python3 crank_arm.py  ->  crank_arm.stl
"""
import numpy as np
import trimesh
from trimesh.creation import box as _box, cylinder as _cyl

LEN, WID, THK = 50.0, 14.0, 5.0        # arm body
HUB_CLR_R = 3.6                        # clears the 6 mm shaft end (7.2 mm hole)
SLOT_W, SLOT_R0, SLOT_R1 = 3.4, 7.0, 11.0
PIN_R = 2.2                            # M4 clearance
PIN_THROWS = [20.0, 30.0, 40.0]
BOSS_R = 14.5                          # round pad over the hub


def cyl(cx, cy, z0, z1, r, n=64):
    c = _cyl(radius=r, height=z1 - z0, sections=n)
    c.apply_translation([cx, cy, (z0 + z1) / 2])
    return c


def slot(r0, r1, w, z0, z1, angle):
    body = _box(extents=[r1 - r0, w, z1 - z0])
    body.apply_translation([(r0 + r1) / 2, 0, (z0 + z1) / 2])
    s = trimesh.boolean.union([body, cyl(r0, 0, z0, z1, w / 2), cyl(r1, 0, z0, z1, w / 2)], engine="manifold")
    s.apply_transform(trimesh.transformations.rotation_matrix(angle, [0, 0, 1]))
    return s


arm = _box(extents=[LEN, WID, THK]); arm.apply_translation([LEN / 2 - 6, 0, THK / 2])
tip = cyl(LEN - 6, 0, 0, THK, WID / 2)
pad = cyl(0, 0, 0, THK, BOSS_R)
body = trimesh.boolean.union([arm, tip, pad], engine="manifold")

cuts = [cyl(0, 0, -1, THK + 1, HUB_CLR_R),
        slot(SLOT_R0, SLOT_R1, SLOT_W, -1, THK + 1, np.pi / 2),
        slot(SLOT_R0, SLOT_R1, SLOT_W, -1, THK + 1, -np.pi / 2)]
cuts += [cyl(t, 0, -1, THK + 1, PIN_R) for t in PIN_THROWS]
m = trimesh.boolean.difference([body] + cuts, engine="manifold")
m.export("crank_arm.stl")
print(f"crank_arm.stl watertight={m.is_watertight} extents={np.round(m.extents, 1)}")
