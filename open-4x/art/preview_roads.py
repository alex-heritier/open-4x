"""Scratch: build the road and railroad sheets and lay a road network over grass to inspect the joins.

    python preview_roads.py            # writes preview/roads*.png and preview/rails*.png
"""
import sys
import time

import numpy as np
from PIL import Image

from forge import roads, terrain

NEIGHBORS = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)]


def timed(name, fn):
    t = time.time()
    out = fn()
    print(f"{name} {out.size} {time.time() - t:.1f}s")
    return out


def cell(sheet, index):
    row, col = divmod(index, roads.COLUMNS)
    return sheet.crop((col * roads.W, row * roads.H, (col + 1) * roads.W, (row + 1) * roads.H))


def mask_of(has, x, y):
    return sum(1 << i for i, (dx, dy) in enumerate(NEIGHBORS) if has(x + dx, y + dy))


def scatter(x, y):
    return abs((x * 73856093) ^ (y * 19349663))


def network():
    """Roads and railways as a worker's pathfinder leaves them: walks over the eight neighbours."""
    road, rail = set(), set()

    def walk(into, start, moves):
        x, y = start
        into.add((x, y))
        for m in moves.split():
            dx, dy = NEIGHBORS[roads.NAMES.index(m)]
            x, y = x + dx, y + dy
            into.add((x, y))

    walk(road, (1, 9), "NE NE NE NE E E SE SE SE S S")  # a long winding road
    walk(road, (5, 5), "SE SE SE SE SE SE SE SE")  # a straight one (screen: down-right)
    walk(road, (9, 3), "SW SW SW SW SW")  # a straight run in the other grid direction
    walk(road, (9, 3), "NE NE N N")
    walk(road, (3, 12), "E E E E NE NE NE")  # joins the grid's second line
    road.update({(13, 12), (2, 3)})  # lone tiles
    walk(rail, (0, 14), "NE NE E E E E SE SE SE")
    walk(rail, (5, 12), "NE NE N")
    walk(rail, (6, 14), "E E E N N N N")
    return road, rail


def main():
    road_sheet = timed("roads", roads.build_road_sheet)
    rail_sheet = timed("rails", roads.build_rail_sheet)
    road_sheet.save("preview/roads.png")
    rail_sheet.save("preview/rails.png")

    # a contact sheet of the interesting masks over plain grass
    f = terrain.Fields()
    ground = terrain.build_ground(f)
    water = terrain.build_water(f)
    grass = terrain.preview_map(ground, water, np.zeros((16, 16), dtype=int), scale=1.0)
    gcell = grass.crop((grass.width // 2 - roads.W // 2, grass.height // 2 - roads.H, grass.width // 2 + roads.W // 2, grass.height // 2 + roads.H // 2 + 0)).copy()
    picks = [0, 1, 2, 4, 8, 16, 32, 64, 128, 0b00100010, 0b10001000, 0b10100000, 0b00000110, 0b01000100, 0b00101010, 0b11100000, 0b00110110, 0b01010101, 0b11011011, 255]
    for name, sheet in (("roads", road_sheet), ("rails", rail_sheet)):
        cols = 5
        rows = -(-len(picks) // cols)
        canvas = Image.new("RGBA", (cols * roads.W, rows * roads.H), (0, 0, 0, 255))
        for k, m in enumerate(picks):
            tile = Image.new("RGBA", (roads.W, roads.H), (0, 0, 0, 0))
            # grass diamond of the same size from the terrain sheet
            tile.alpha_composite(terrain_tile(ground), (0, 0))
            tile.alpha_composite(cell(sheet, m), (0, 0))
            canvas.alpha_composite(tile, ((k % cols) * roads.W, (k // cols) * roads.H))
        canvas = canvas.resize((canvas.width * 2 // 1, canvas.height * 2 // 1), Image.Resampling.LANCZOS)
        canvas.save(f"preview/{name}_masks.png")

    # a network: placement as in preview_rivers.py
    ny = nx = 16
    base = terrain.preview_map(ground, water, np.zeros((ny, nx), dtype=int), scale=1.0)
    road, rail = network()
    kind = lambda x, y: 2 if (x, y) in rail else 1 if (x, y) in road or (x, y) in rail else 0
    cw, ch = roads.W, roads.H
    out = base.copy()
    for layer, sheet, variants in ((0, road_sheet, roads.ROAD_VARIANTS), (1, rail_sheet, roads.RAIL_VARIANTS)):
        for y in range(ny):
            for x in range(nx):
                own = kind(x, y)
                if (layer == 0 and own == 0) or (layer == 1 and own < 2):
                    continue
                if layer == 0:
                    m = mask_of(lambda a, b: kind(a, b) >= 1 and not (own == 2 and kind(a, b) == 2), x, y)
                    if own == 2 and m == 0:
                        continue
                else:
                    m = mask_of(lambda a, b: kind(a, b) == 2, x, y)
                v = scatter(x, y) % variants if bin(m).count("1") <= 2 else 0
                sx = (x - y) * cw / 2 + ny * cw / 2
                sy = (x + y) * ch / 2 + ch / 2
                out.alpha_composite(cell(sheet, roads.sheet_index(m, v)), (int(sx), int(sy)))
    out.save("preview/road_map.png")
    big = out.crop((out.width // 2 - 700, 0, out.width // 2 + 700, 900))
    big.save("preview/road_map_crop.png")
    print("network", out.size)


def terrain_tile(ground):
    """One grass tile diamond (192 x 96) taken from the ground sheet: the pure grassland cell."""
    col, row = terrain.cell_position(0)
    return ground.crop((col * terrain.W, row * terrain.H, (col + 1) * terrain.W, (row + 1) * terrain.H))


if __name__ == "__main__":
    main()
