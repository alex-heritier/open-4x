"""Scratch: every city skin at its three sizes (town, city, metropolis), one row per flavor, in preview/."""
import sys

from PIL import Image

from forge import cities
from forge.paint import render_sprite

names = sys.argv[1:] or list(cities.BUILDERS)
sheet = Image.new("RGBA", (3 * 256, len(names) * 224), (92, 112, 52, 255))
for row, name in enumerate(names):
    for col, size in enumerate(cities.SIZES):
        im = render_sprite(cities.BUILDERS[name](size=size).render(), 256, 224, ss=4, seed=3, radius=1)
        sheet.alpha_composite(im, (col * 256, row * 224))
sheet.save(f"preview/city_sizes_{'_'.join(names) if sys.argv[1:] else 'all'}.png")
