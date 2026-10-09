"""Scratch: render the western city skin for inspection (preview_cities.py renders them all)."""
from PIL import Image
from forge import cities
from forge.paint import render_sprite
from forge.util import write_svg

svg = cities.western()
write_svg("city_western", svg.render())
im = render_sprite(svg.render(), 256, 224, ss=4, seed=3, radius=1)
im.save("preview/city.png")
bg = Image.new("RGBA", (im.width * 3, im.height * 3), (92, 112, 52, 255))
bg.alpha_composite(im.resize((im.width * 3, im.height * 3), Image.Resampling.LANCZOS))
bg.save("preview/city_3x.png")
print(im.size)
