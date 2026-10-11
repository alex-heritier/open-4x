"""Scratch: contact sheet of the UI glyphs, tinted the way the client tints them."""
from PIL import Image
from forge import icons

imgs = icons.render_all()
names = list(imgs)
cols = 9
cell = 96
sheet = Image.new("RGBA", (cols * cell, ((len(names) + cols - 1) // cols) * cell), (246, 242, 231, 255))
for i, n in enumerate(names):
    x, y = (i % cols) * cell, (i // cols) * cell
    ink = Image.new("RGBA", imgs[n].size, (28, 38, 48, 255))
    ink.putalpha(imgs[n].getchannel("A"))
    sheet.alpha_composite(ink, (x + 16, y + 16))
sheet.save("preview/icons_sheet.png")
print(sheet.size, names)
