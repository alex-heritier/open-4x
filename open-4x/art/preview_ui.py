"""Scratch: contact sheet of UI chrome."""
from PIL import Image
from forge import ui
from forge.paint import render_sprite

def r(svg, w, h, **kw):
    kw.setdefault("grade_kw", None)
    kw.setdefault("radius", 0)
    kw.setdefault("grain", 0.04)
    kw.setdefault("strokes", 0.0)
    return render_sprite(svg.render(), w, h, ss=4, **kw)

def stretch9(im, inset, w, h, vertical=True):
    """Nine-slice stretch for preview (vertical=False slices horizontally only)."""
    W, H = im.size
    out = Image.new("RGBA", (w, h))
    iy = inset if vertical else 0
    xs, ys = [0, inset, W - inset, W], [0, iy, H - iy, H]
    xd, yd = [0, inset, w - inset, w], [0, iy, h - iy, h]
    for j in range(3):
        for i in range(3):
            if xs[i + 1] <= xs[i] or ys[j + 1] <= ys[j]:
                continue
            tile = im.crop((xs[i], ys[j], xs[i + 1], ys[j + 1])).resize((max(1, xd[i + 1] - xd[i]), max(1, yd[j + 1] - yd[j])), Image.Resampling.LANCZOS)
            out.alpha_composite(tile, (xd[i], yd[j]))
    return out

navy = ui.textured_panel("navy")
parch = ui.textured_panel("parchment")
plate = r(ui.nameplate_svg(), 288, 56)
btn = r(ui.button_svg(), 96, 96)
btnp = r(ui.button_svg(pressed=True), 96, 96)
dark = r(ui.dark_button_svg(), 192, 72)
bar = r(ui.bar_frame_svg(), 256, 32)
fill = r(ui.bar_fill_svg("green"), 64, 32)
sel = r(ui.select_svg(), 256, 128)
fd = r(ui.flag_svg("dawn"), 96, 64)
fl = r(ui.flag_svg("league"), 96, 64)
bt = r(ui.battle_svg(), 160, 160, radius=1, grain=0.1)
canvas = Image.new("RGBA", (1500, 900), (30, 100, 125, 255))
canvas.alpha_composite(stretch9(navy, 44, 560, 300), (20, 20))
canvas.alpha_composite(stretch9(parch, 44, 560, 250), (20, 340))
canvas.alpha_composite(stretch9(plate, 56, 420, 56, False), (620, 20))
canvas.alpha_composite(stretch9(plate, 56, 300, 56, False), (620, 90))
canvas.alpha_composite(btn, (620, 170)); canvas.alpha_composite(btnp, (730, 170))
canvas.alpha_composite(dark, (850, 170))
canvas.alpha_composite(stretch9(bar, 14, 400, 32, False), (620, 290))
f = fill.resize((260, 20)); canvas.alpha_composite(f, (630, 296))
canvas.alpha_composite(sel, (620, 350)); canvas.alpha_composite(fd, (900, 350)); canvas.alpha_composite(fl, (1010, 350))
canvas.alpha_composite(bt, (1130, 330))
canvas.save("preview/ui_sheet.png")
navy.save("preview/panel_navy.png"); parch.save("preview/panel_parchment.png")
