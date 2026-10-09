"""Scratch: contact sheet of unit sprites over a grass tile."""
from PIL import Image
from forge import units
from forge.paint import render_sprite
from forge.util import write_svg

grass = Image.open("preview/atlas.png").crop((0, 0, 256, 128)).resize((128, 64), Image.Resampling.LANCZOS)
items = [("infantry", units.build_infantry, (160, 160)), ("pioneer", units.build_pioneer, (160, 160)),
         ("cavalry", units.build_cavalry, (160, 160)), ("artillery", units.build_artillery, (160, 160)),
         ("ironclad", units.ironclad, (256, 192))]
sheet = Image.new("RGBA", (160 * 4 * 2 + 256 * 2, 160 * 2), (24, 66, 78, 255))
x = 0
for name, build, (w, h) in items:
    svg = build()
    write_svg(name, svg.render())
    im = render_sprite(svg.render(), w, h, ss=4, seed=2, radius=1, rim=0.6)
    im.save(f"preview/{name}.png")
    tile = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ty = 108 if name != "ironclad" else 123
    tile.alpha_composite(grass.resize((256, 128), Image.Resampling.LANCZOS) if name == "ironclad" else grass.resize((160, 80)), (0 if name != "ironclad" else 0, ty - (40 if name != "ironclad" else 64)))
    if name == "ironclad":
        tile = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    tile.alpha_composite(im)
    sheet.alpha_composite(tile.resize((w * 2, h * 2), Image.Resampling.LANCZOS), (x, 0))
    x += w * 2
sheet.save("preview/units_2x.png")
print(sheet.size)
