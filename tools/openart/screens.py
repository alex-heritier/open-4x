"""Surfaces for the interface sheets: parchment, bevelled plates, wells, rules.

Everything is RGB or RGBA built from noise and gradients; the sheet builders
in `uisheets.py` compose these into the files the game expects.
"""
import random

from PIL import Image, ImageChops, ImageDraw, ImageEnhance, ImageFilter

from . import gfx
from .gfx import mix, shade_rgb

PARCH = [(0.0, (210, 186, 142)), (0.4, (232, 214, 172)), (0.75, (244, 232, 198)), (1.0, (252, 244, 220))]
SLATE = [(0.0, (52, 58, 70)), (0.5, (72, 80, 94)), (1.0, (96, 106, 122))]
LEATHER = [(0.0, (92, 62, 40)), (0.5, (118, 80, 50)), (1.0, (142, 100, 62))]

# plate tones: highlight, top, bottom, outline
TONES = {
    "bronze": ((248, 214, 140), (208, 162, 90), (128, 88, 44), (54, 36, 22)),
    "stone": ((236, 232, 220), (190, 184, 168), (118, 112, 100), (52, 48, 44)),
    "green": ((176, 220, 150), (96, 156, 82), (48, 92, 48), (22, 48, 26)),
    "red": ((248, 170, 150), (196, 80, 62), (116, 36, 30), (54, 18, 16)),
    "blue": ((170, 200, 244), (82, 122, 190), (40, 66, 118), (18, 30, 62)),
    "dark": ((120, 124, 140), (70, 74, 90), (38, 40, 52), (14, 14, 20)),
}


# -------------------------------------------------------------- parchment

def parchment(size, seed, ramp=PARCH, grain=0.16, vignette=0.14):
    """A mottled sheet (RGB): large clouds, fine fibre and a darker edge."""
    w, h = size
    rnd = random.Random(seed)

    def cloud(div):
        fw, fh = max(2, w // div), max(2, h // div)
        return Image.frombytes("L", (fw, fh), rnd.randbytes(fw * fh)).resize(size, Image.BICUBIC)

    base = Image.blend(cloud(90), cloud(26), 0.45)
    fine = Image.frombytes("L", size, rnd.randbytes(w * h)).filter(ImageFilter.GaussianBlur(0.9))
    fw, fh = max(2, w // 36), max(2, h // 2)
    fibre = Image.frombytes("L", (fw, fh), rnd.randbytes(fw * fh)).resize(size, Image.BICUBIC)
    g = Image.blend(Image.blend(base, fibre, 0.14), fine, grain)
    g = gfx.stretch(g, 3, 97).point([int(128 + (v - 128) * 0.62) for v in range(256)])
    rgb = gfx.colorize(g, ramp)
    if vignette:
        edge = int(255 * (1 - vignette))
        v = Image.new("L", (3, 3))
        v.putdata([edge, edge, edge, edge, 255, edge, edge, edge, edge])
        rgb = ImageChops.multiply(rgb, v.resize(size, Image.BICUBIC).convert("RGB"))
    return rgb


def stone(size, seed, tone=SLATE):
    return parchment(size, seed, tone, grain=0.3, vignette=0.12)


# --------------------------------------------------------------- plates

def _shift(mask, dx, dy):
    return ImageChops.offset(mask, dx, dy)


def plate(w, h, tone="bronze", state=0, radius=5, draw=None, s=4, round_to=None):
    """A bevelled rounded plate (RGBA), `state` 0 idle, 1 rollover, 2 pressed.

    `draw(d, s, box)` may paint on the 4x working image; `box` is the plate's
    rectangle in working pixels.
    """
    hi, top, bot, edge = TONES[tone]
    if state == 1:
        top, bot, hi = shade_rgb(top, 1.12), shade_rgb(bot, 1.18), shade_rgb(hi, 1.05)
    elif state == 2:
        top, bot = shade_rgb(bot, 1.0), shade_rgb(top, 0.9)
    pad = 4 * s
    W, H = w * s, h * s
    size = (W + 2 * pad, H + 2 * pad)
    box = (pad, pad, pad + W - 1, pad + H - 1)
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius * s, fill=255)
    grad = Image.linear_gradient("L").resize(size)
    body = gfx.colorize(grad, [(0.0, top), (1.0, bot)])
    lit, dark = (hi, shade_rgb(bot, 0.6)) if state != 2 else (shade_rgb(bot, 0.8), shade_rgb(hi, 0.9))
    t = int(1.4 * s)
    rim_hi = ImageChops.subtract(m, _shift(m, t, t))
    rim_lo = ImageChops.subtract(m, _shift(m, -t, -t))
    outline = ImageChops.subtract(m, m.filter(ImageFilter.MinFilter(2 * s + 1)))
    img = body.copy()
    img.paste(Image.new("RGB", size, lit), mask=ImageChops.multiply(rim_hi, Image.new("L", size, 190)))
    img.paste(Image.new("RGB", size, dark), mask=ImageChops.multiply(rim_lo, Image.new("L", size, 200)))
    img.paste(Image.new("RGB", size, edge), mask=outline)
    out = img.convert("RGBA")
    out.putalpha(m)
    if draw:
        draw(ImageDraw.Draw(out), s, box)
    out = out.crop((pad, pad, pad + W, pad + H)).resize((w, h), Image.LANCZOS)
    return out


def glyph_x(d, s, box, col=(46, 28, 20), t=2.4, k=0.28):
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    a, b = (x0 + w * k, y0 + h * k), (x1 - w * k, y1 - h * k)
    d.line([a, b], fill=col + (255,), width=int(t * s))
    d.line([(a[0], b[1]), (b[0], a[1])], fill=col + (255,), width=int(t * s))


def glyph_tri(d, s, box, direction, col=(46, 28, 20), k=0.3):
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    r = min(w, h) * k
    pts = {"left": [(cx + r * .7, cy - r), (cx - r * .8, cy), (cx + r * .7, cy + r)],
           "right": [(cx - r * .7, cy - r), (cx + r * .8, cy), (cx - r * .7, cy + r)],
           "up": [(cx - r, cy + r * .7), (cx, cy - r * .8), (cx + r, cy + r * .7)],
           "down": [(cx - r, cy - r * .7), (cx, cy + r * .8), (cx + r, cy - r * .7)]}[direction]
    d.polygon(pts, fill=col + (255,))


def glyph_sign(d, s, box, plus, col=(46, 28, 20), t=2.2, k=0.3):
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    d.line([(x0 + w * k, cy), (x1 - w * k, cy)], fill=col + (255,), width=int(t * s))
    if plus:
        d.line([(cx, y0 + h * k), (cx, y1 - h * k)], fill=col + (255,), width=int(t * s))


# ---------------------------------------------------------------- insets

def well(im, box, dark=0.9, line=(70, 50, 30), light=(250, 240, 214)):
    """Sink a rectangle into a parchment surface: darker fill, shadowed top
    and left, lit bottom and right (modifies `im`)."""
    x0, y0, x1, y1 = box
    region = ImageEnhance.Brightness(im.crop((x0, y0, x1 + 1, y1 + 1))).enhance(dark)
    im.paste(region, (x0, y0))
    d = ImageDraw.Draw(im)
    d.rectangle(box, outline=line)
    inner = mix(line, (150, 120, 84), 0.45)
    d.line([(x0 + 1, y0 + 1), (x1 - 1, y0 + 1)], fill=inner)
    d.line([(x0 + 1, y0 + 1), (x0 + 1, y1 - 1)], fill=inner)
    d.line([(x0 + 1, y1 + 1), (x1 + 1, y1 + 1)], fill=light)
    d.line([(x1 + 1, y0 + 1), (x1 + 1, y1 + 1)], fill=light)


def rule(im, y, h=8, tone="bronze", x0=0, x1=None):
    """A horizontal moulding: outline, bright edge, body gradient, shade."""
    hi, top, bot, edge = TONES[tone]
    x1 = im.width - 1 if x1 is None else x1
    d = ImageDraw.Draw(im)
    for i in range(h):
        t = i / max(1, h - 1)
        if i == 0 or i == h - 1:
            c = edge
        elif i == 1:
            c = hi
        else:
            c = mix(top, bot, (t - 0.12) / 0.8)
        d.line([(x0, y + i), (x1, y + i)], fill=c)


def frame(im, box, tone="bronze", t=4):
    """A bevelled picture frame drawn around `box` (inside it)."""
    x0, y0, x1, y1 = box
    hi, top, bot, edge = TONES[tone]
    d = ImageDraw.Draw(im)
    d.rectangle(box, outline=edge)
    for i in range(1, t):
        f = i / t
        d.line([(x0 + i, y0 + i), (x1 - i, y0 + i)], fill=mix(hi, top, f))
        d.line([(x0 + i, y0 + i), (x0 + i, y1 - i)], fill=mix(hi, top, f))
        d.line([(x0 + i, y1 - i), (x1 - i, y1 - i)], fill=mix(bot, edge, f * .5))
        d.line([(x1 - i, y0 + i), (x1 - i, y1 - i)], fill=mix(bot, edge, f * .5))
    d.rectangle((x0 + t, y0 + t, x1 - t, y1 - t), outline=edge)


def blank(size):
    return Image.new("RGBA", size, (0, 0, 0, 0))


def from_rgb(rgb, hole=None):
    """RGB -> RGBA; `hole` is an optional box made fully transparent."""
    out = rgb.convert("RGBA")
    if hole:
        ImageDraw.Draw(out).rectangle(hole, fill=(0, 0, 0, 0))
    return out


def indexed(rgba):
    """RGBA -> the game's `P` image (index 255 = transparent)."""
    return gfx.indexed_from_rgba(rgba)
