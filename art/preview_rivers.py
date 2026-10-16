"""Scratch: build the river sheet and tile a few meanders over ground to inspect the joins."""
import time

import numpy as np
from PIL import Image

from forge import rivers, terrain

t = time.time()
sheet = rivers.build_river_sheet()
print("rivers", sheet.size, f"{time.time() - t:.1f}s")
sheet.save("preview/rivers.png")

# ---- a meander over grassland and desert, with a mouth in the sea ----
f = terrain.Fields()
ground = terrain.build_ground(f)
water = terrain.build_water(f)
nx, ny = 14, 12
kinds = np.zeros((ny, nx), dtype=int)
kinds[:, 7:] = 2
kinds[:, :1] = 6
kinds[ny - 2 :, :] = 6
kinds[ny - 3, :] = np.where(kinds[ny - 3] == 6, 6, kinds[ny - 3])
# canonical edges: ("E", x, y) = edge between (x, y) and (x + 1, y); ("S", x, y) = between (x, y) and (x, y + 1)
# a river is a walk over tile corners; corner (x, y) sits between tiles (x, y) .. (x + 1, y + 1)
STEP = {"NE": (0, -1), "SE": (1, 0), "SW": (0, 1), "NW": (-1, 0)}
edges = set()


def walk(start, moves):
    x, y = start
    for m in moves.split():
        dx, dy = STEP[m]
        if m == "NE":
            edges.add(("E", x, y))
        elif m == "SE":
            edges.add(("S", x + 1, y))
        elif m == "SW":
            edges.add(("E", x, y + 1))
        else:
            edges.add(("S", x, y))
        x, y = x + dx, y + dy


walk((3, 1), "SE SW SE SE SW SW SE SE SW SW SE SW")
walk((7, 3), "SE SW SE SE SW SW SE")  # a tributary (joins the main stream when they meet)
walk((2, 6), "SE SE SW SW")


def mask(x, y):
    has = lambda k, a, b: (k, a, b) in edges
    return (
        (1 if has("E", x, y) else 0)
        | (2 if has("S", x + 1, y) else 0)
        | (4 if has("E", x, y + 1) else 0)
        | (8 if has("S", x, y) else 0)
    )


base = terrain.preview_map(ground, water, kinds, scale=1.0, variants=True)
cw, ch = terrain.W, terrain.H
canvas = base.copy()
for y in range(-1, ny):
    for x in range(-1, nx):
        m = mask(x, y)
        if not m:
            continue
        sx = (x - y) * cw / 2 + ny * cw / 2
        sy = (x + y) * ch / 2 + ch
        variant = (x * 7 + y * 3) % rivers.VARIANTS
        col, row = m, variant
        canvas.alpha_composite(sheet.crop((col * cw, row * ch, (col + 1) * cw, (row + 1) * ch)), (int(sx), int(sy)))
canvas.save("preview/river_map.png")
print("river map", canvas.size)
