"""Contact sheet of the window chrome: a 9-sliced frame over each surface, as the client draws it."""
from PIL import Image
from forge import chrome


def nine(frame, border, w, h, corner_scale=0.5):
    """Simulate the client's sliced draw: corners at `corner_scale`, sides stretched."""
    f = frame
    fw, fh = f.size
    b = border
    cb = round(b * corner_scale)
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    xs = [0, b, fw - b, fw]
    ys = [0, b, fh - b, fh]
    tx = [0, cb, w - cb, w]
    ty = [0, cb, h - cb, h]
    for j in range(3):
        for i in range(3):
            piece = f.crop((xs[i], ys[j], xs[i + 1], ys[j + 1]))
            size = (tx[i + 1] - tx[i], ty[j + 1] - ty[j])
            if size[0] > 0 and size[1] > 0:
                out.alpha_composite(piece.resize(size, Image.Resampling.LANCZOS), (tx[i], ty[j]))
    return out


def tiled(surface, w, h):
    out = Image.new("RGBA", (w, h))
    sw, sh = surface.size
    for y in range(0, h, sh):
        for x in range(0, w, sw):
            out.paste(surface, (x, y))
    return out


if __name__ == "__main__":
    a = chrome.render_all()
    sheet = Image.new("RGBA", (1500, 900), (30, 80, 110, 255))
    for k, (name, w, h, big) in enumerate((("parchment", 440, 560, True), ("slate", 480, 200, True), ("felt", 300, 90, False))):
        pass
    x = 20
    for name, w, h, big in (("parchment", 440, 560, True), ("slate", 480, 200, True)):
        panel = tiled(a[name], w * 2, h * 2)
        fr = nine(a["frame"], 64, w * 2, h * 2, 1.0)
        panel.alpha_composite(fr)
        sheet.alpha_composite(panel.resize((w, h), Image.Resampling.LANCZOS), (x, 20))
        x += w + 20
    small = tiled(a["felt"], 600, 180)
    small.alpha_composite(nine(a["frame_small"], 40, 600, 180, 1.0))
    sheet.alpha_composite(small.resize((300, 90), Image.Resampling.LANCZOS), (500, 260))
    sheet.alpha_composite(a["cartouche_h"], (500, 380))
    sheet.alpha_composite(a["cartouche_v"], (600, 380))
    sheet.alpha_composite(a["stud"], (700, 380))
    sheet.alpha_composite(a["compass"], (760, 380))
    sheet.alpha_composite(a["rule_end"], (500, 460))
    sheet.save("preview/chrome.png")
    nine(a["frame"], 64, 700, 700, 1.0).save("preview/chrome_big.png")
