"""
Adds a hanging loop to the crown of the MakerWorld "Flexi skeleton" (model 692453) so it can hang
from fishing line. The source STL is not in the repo (Standard Digital File License: no redistribution).

The skeleton prints face-up with the head toward +Y. The loop stands upright on the bed with its hole
running left-right, so a horizontal line threads through side to side and the skeleton hangs facing
forward. It prints without supports (the hole's top is a short bridge), and a neck buried in the skull
fuses it to the crown, which curves away from the bed.
Run:  python3 hanging_loop.py "Flexi skeleton STL V1.stl" flexi_skeleton_loop.stl
"""
import sys
import numpy as np
import trimesh
from skeleton_enclosure import box, union, diff

LOOP_R, HOLE_R, LOOP_T = 4.5, 1.75, 4.0   # 9 mm ring, 3.5 mm hole, 4 mm thick
NECK_DEPTH = 6.0                          # tab buried in the skull below the crown


def cyl_x(cy, cz, x0, x1, r, n=64):
    c = trimesh.creation.cylinder(radius=r, height=x1 - x0, sections=n)
    c.apply_transform(trimesh.transformations.rotation_matrix(np.pi / 2, [0, 1, 0]))
    c.apply_translation([(x0 + x1) / 2, cy, cz])
    return c


def add_loop(skeleton):
    bodies = skeleton.split(only_watertight=False)
    skull = max(bodies, key=lambda body: body.bounds[1][1])
    others = [body for body in bodies if body is not skull]

    crown = skull.vertices[skull.vertices[:, 1].argmax()]
    cx, crown_y = crown[0], crown[1]
    cy = crown_y + LOOP_R - 1.0   # ring overlaps the crown by 1 mm
    x0, x1, cz = cx - LOOP_T / 2, cx + LOOP_T / 2, LOOP_R
    ring = union([cyl_x(cy, cz, x0, x1, LOOP_R), box(x0, x1, crown_y - NECK_DEPTH, cy, 0, 2 * LOOP_R)])
    loop = diff(ring, [cyl_x(cy, cz, x0 - 1, x1 + 1, HOLE_R)])
    return trimesh.util.concatenate([union([skull, loop])] + others)


if __name__ == "__main__":
    source, target = sys.argv[1], sys.argv[2]
    add_loop(trimesh.load(source)).export(target)
    print(f"wrote {target}")
