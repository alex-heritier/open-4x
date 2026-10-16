"""Scratch: render every overlay on its natural ground and lay them out in a contact sheet."""
import sys
import time

from PIL import Image

import build
from forge import nature, relief, terrain
from forge.paint import paint, render_sprite
from forge.util import downsample

f = terrain.Fields()
ground = terrain.build_ground(f)
W, H = terrain.W, terrain.H


def pure(digit):
    col, row = divmod(0, 1)
    index = digit * 85
    r, c = divmod(index, terrain.GRID)
    return ground.crop((c * W, r * H, (c + 1) * W, (r + 1) * H)).resize((256, 128), Image.Resampling.LANCZOS)


def sp(builder, **kw):
    svg = builder()
    return render_sprite(svg.render(), 256, 224, ss=4, radius=1, **kw)


def rel(layer):
    img = paint(layer, radius=1, grain=0.10, strokes=0.05, grade_kw={"sat": 1.1, "contrast": 1.08, "key": 0.10})
    return downsample(img, 256, 224)


jobs = {
    "forest": (0, lambda: sp(nature.build_forest_svg, seed=4)),
    "pine": (3, lambda: sp(nature.build_pine_svg, seed=4)),
    "jungle": (0, lambda: sp(nature.build_jungle_svg, seed=5)),
    "marsh": (0, lambda: sp(nature.build_marsh_svg, seed=6)),
    "hills": (0, lambda: rel(relief.build_hills_layer(21, "temperate"))),
    "hills_dry": (1, lambda: rel(relief.build_hills_layer(22, "arid"))),
    "hills_cold": (3, lambda: rel(relief.build_hills_layer(23, "arctic"))),
    "mountain": (0, lambda: rel(relief.build_mountain_layer(3, "temperate"))),
    "mountain_dry": (2, lambda: rel(relief.build_mountain_layer(4, "arid"))),
    "mountain_cold": (3, lambda: rel(relief.build_mountain_layer(5, "arctic"))),
}
only = sys.argv[1:] or list(jobs)
tag = str(int(time.time()))
cols = 5
sheet = Image.new("RGBA", (256 * cols, 224 * ((len(only) + cols - 1) // cols)), (22, 30, 36, 255))
for i, name in enumerate(only):
    t = time.time()
    digit, make = jobs[name]
    im = make()
    im.save(f"preview/ov_{name}.png")
    tile = Image.new("RGBA", (256, 224), (0, 0, 0, 0))
    tile.alpha_composite(pure(digit), (0, 96))
    tile.alpha_composite(im)
    sheet.alpha_composite(tile, ((i % cols) * 256, (i // cols) * 224))
    print(name, f"{time.time() - t:.1f}s", flush=True)
sheet.save(f"preview/overlays_{tag}.png")
print(f"preview/overlays_{tag}.png")
