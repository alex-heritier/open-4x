from PIL import Image
from forge import icons, ui
from forge.paint import render_sprite
imgs = icons.render_all()
btn = render_sprite(ui.button_svg().render(), 96, 96, ss=4, radius=0, grain=0.04, strokes=0, grade_kw=None)
names = list(imgs)
cols = 9
sheet = Image.new("RGBA", (cols * 110, ((len(names) + cols - 1) // cols) * 110), (30, 100, 125, 255))
for i, n in enumerate(names):
    x, y = (i % cols) * 110, (i // cols) * 110
    if n.startswith("icon_"):
        sheet.alpha_composite(btn, (x + 7, y + 7))
        sheet.alpha_composite(imgs[n], (x + 23, y + 23))
    else:
        bg = Image.new("RGBA", (96, 96), (16, 26, 32, 255)); sheet.alpha_composite(bg, (x + 7, y + 7)); sheet.alpha_composite(imgs[n], (x + 23, y + 23))
sheet = sheet.resize((sheet.width * 1, sheet.height * 1))
sheet.save("preview/icons_sheet.png"); print(sheet.size, names)
