"""Scratch: render the ground + water sheets and a sample map with the game's placement math."""
import time

import numpy as np

from forge import terrain
from forge.noise import fbm

t = time.time()
f = terrain.Fields()
ground = terrain.build_ground(f)
water = terrain.build_water(f)
print("sheets", ground.size, f"{time.time() - t:.1f}s")
ground.save("preview/ground.png")
water.save("preview/water.png")

# an archipelago whose land cycles through every climate: kinds 0..3 land, 4 coast, 5 sea, 6 ocean
ny, nx = 18, 26
ys, xs = np.mgrid[0:ny, 0:nx].astype(np.float32)
n = fbm(xs / nx * 0.97, ys / ny * 0.97, 2, 3, seed=7)
n += 0.5 * np.exp(-(((xs - nx / 2) / (nx * 0.45)) ** 2 + ((ys - ny / 2) / (ny * 0.45)) ** 2) * 2) - 0.12
land = n > 0.08
climate = np.clip(((xs + ys * 0.6) / (nx + ny * 0.6) * 4).astype(int), 0, 3)
climate = np.clip(climate + (fbm(xs / nx * 0.97, ys / ny * 0.97, 3, 2, seed=9) * 2.2).astype(int), 0, 3)
kinds = np.where(land, climate, 6)
for y in range(ny):
    for x in range(nx):
        if not land[y, x] and n[y, x] > -0.12:
            kinds[y, x] = 5
prev = terrain.preview_map(ground, water, kinds, scale=0.5)
prev.save("preview/map.png")
print("map", prev.size)
