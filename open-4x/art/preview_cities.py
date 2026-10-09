"""Scratch: render every city skin onto a contact sheet in preview/ (and one PNG each)."""
import sys

from PIL import Image

from forge import cities
from forge.paint import render_sprite

names = sys.argv[1:] or list(cities.BUILDERS)
tiles = []
for i, name in enumerate(names):
    svg = cities.BUILDERS[name]()
    im = render_sprite(svg.render(), 256, 224, ss=4, seed=3, radius=1)
    im.save(f"preview/city_{name}.png")
    tiles.append((name, im))
cols = 4
rows = (len(tiles) + cols - 1) // cols
sheet = Image.new("RGBA", (cols * 256, rows * 224), (92, 112, 52, 255))
for i, (name, im) in enumerate(tiles):
    sheet.alpha_composite(im, ((i % cols) * 256, (i // cols) * 224))
sheet.save("preview/cities.png")
print(len(tiles), "skins")
