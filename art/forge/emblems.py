"""Painted emblems (`ui/chrome/ico_*.png`): the coloured icons of the windows, 96 x 96 and shown at
24 to 36 logical px. Unlike the flat white glyphs of `icons.py` these carry their own colour: a gold
coin, a blue flask, a wheat ear, a steel shield, the brass instruments of the minimap.
"""

import math

from .chrome import BRONZE_LO, GOLD, GOLD_HI, GOLD_LO, OUTLINE
from .svgkit import Svg
from .util import downsample, rasterize

SIZE = 96


def gold(s: Svg, fx=0.35, fy=0.3):
    return s.rad([(0, "#fff0b8"), (0.3, GOLD_HI), (0.7, GOLD), (1, GOLD_LO)], 0.5, 0.5, 0.6, fx, fy)


def coin(s: Svg, cx, cy, r, rim=OUTLINE):
    s.circle(cx, cy + r * 0.06, r + 1, "#000000", op=0.25, blur=s.blur(1.4))
    s.circle(cx, cy, r + 2.2, rim)
    s.circle(cx, cy, r, gold(s))
    s.circle(cx, cy, r * 0.76, "none", stroke=GOLD_LO, sw=max(1.6, r * 0.07))
    s.circle(cx, cy, r * 0.76, "none", stroke="#fff3c4", sw=max(0.8, r * 0.03), op=0.5)
    # a small mark in the middle
    s.poly([(cx, cy - r * 0.34), (cx + r * 0.22, cy), (cx, cy + r * 0.34), (cx - r * 0.22, cy)], GOLD_LO, op=0.8)
    s.circle(cx - r * 0.35, cy - r * 0.4, r * 0.1, "#ffffff", op=0.55)


def e_coin(s):
    coin(s, 48, 48, 40)


def e_coins(s):
    """Three coins stacked, the top one flat."""
    for k, y in enumerate((72, 58, 44)):
        s.ellipse(48, y + 9, 33, 14, OUTLINE)
        s.rect(15, y - 3, 66, 12, OUTLINE)
        side = s.lin([(0, GOLD), (0.5, GOLD_LO), (1, "#5b3b0e")], 0, 0, 1, 0)
        s.ellipse(48, y + 7, 31, 12, side)
        s.rect(17, y - 2, 62, 9, side)
        s.ellipse(48, y - 2, 31, 12, OUTLINE)
        s.ellipse(48, y - 2.5, 29.4, 10.6, gold(s, 0.4, 0.25))
        s.ellipse(48, y - 2.5, 20, 6.6, "none", stroke=GOLD_LO, sw=1.4)
    s.ellipse(40, 33, 9, 2.6, "#ffffff", op=0.5)


def e_ingots(s):
    """Three bars of gold, two below and one across the top."""

    def bar(x, y, w, h, tilt=8):
        top = [(x + tilt, y), (x + w - tilt, y), (x + w, y + h * 0.45), (x, y + h * 0.45)]
        front = [(x, y + h * 0.45), (x + w, y + h * 0.45), (x + w + 2, y + h), (x - 2, y + h)]
        s.poly([(a, b) for a, b in [(x + tilt - 1.6, y - 1.6), (x + w - tilt + 1.6, y - 1.6), (x + w + 1.6, y + h * 0.45), (x + w + 3.6, y + h + 1.6), (x - 3.6, y + h + 1.6), (x - 1.6, y + h * 0.45)]], OUTLINE)
        s.poly(front, s.lin([(0, GOLD), (1, GOLD_LO)], 0, 0, 0, 1))
        s.poly(top, s.lin([(0, "#fff0b8"), (1, GOLD_HI)], 0, 0, 1, 1))
        s.line((x + tilt + 3, y + 3), (x + w - tilt - 6, y + 3), "#ffffff", 1.4, 0.55)

    bar(8, 56, 42, 26)
    bar(46, 56, 42, 26)
    bar(26, 32, 44, 26)


def e_flask(s):
    glass = "#cfe6f2"
    body = "M40,12 L56,12 L56,36 C56,40 60,44 66,54 C76,70 76,86 62,88 L34,88 C20,86 20,70 30,54 C36,44 40,40 40,36 Z"
    s.path(body, "#e9f4fa", OUTLINE, 5.6, op=0.95)
    s.path("M26,62 C32,56 64,56 70,62 C78,76 76,88 62,88 L34,88 C20,86 20,74 26,62 Z", s.lin([(0, "#7fb8ee"), (0.6, "#3f78c4"), (1, "#27529a")], 0, 0, 0, 1))
    s.path("M26,62 C32,56 64,56 70,62", "none", "#c8e4ff", 2.2, op=0.8)
    s.path(body, "none", "#16273d", 3.0)
    s.rect(38, 7, 20, 9, "#a9744a", rx=2, stroke=OUTLINE, sw=2.6)
    s.rect(40, 9, 8, 3, "#d3a070", rx=1.5, op=0.8)
    s.path("M43,18 L43,38 C43,44 37,50 33,58", "none", "#ffffff", 2.4, op=0.7)
    for cx, cy, r in ((50, 74, 3.4), (58, 68, 2.2), (44, 80, 2.0)):
        s.circle(cx, cy, r, "#bfe0ff", op=0.7)


def e_wheat(s):
    stem = "M30,90 C36,64 46,42 68,10"
    s.path(stem, "none", OUTLINE, 6.4)
    s.path(stem, "none", "#b98a2e", 3.2)
    for k in range(6):
        t = 0.28 + k * 0.13
        x = 30 + (68 - 30) * t + 6 * math.sin(t * 4)
        y = 90 - 80 * t
        for side in (-1, 1):
            ang = -62 + side * 34
            ex, ey = 8.5, 3.8
            cx = x + side * 7.5
            cy = y - 1.5 - side * 2.4
            s.ellipse(cx, cy, ex, ey, OUTLINE, rot=ang)
            s.ellipse(cx, cy, ex - 1.5, ey - 1.4, s.lin([(0, GOLD_HI), (1, GOLD)], 0, 0, 1, 1), rot=ang)
    s.ellipse(69, 10, 4, 7.4, OUTLINE, rot=-62)
    s.ellipse(69, 10, 2.6, 5.8, GOLD_HI, rot=-62)
    # leaves
    s.path("M31,88 C18,80 12,64 14,50 C24,60 32,72 32,88 Z", "#7c9a3a", OUTLINE, 2.6)


def e_shield(s):
    outline = "M48,6 L84,16 C84,50 74,74 48,92 C22,74 12,50 12,16 Z"
    s.path(outline, "#cdd3da", OUTLINE, 6)
    s.path("M48,12 L78,20 C78,50 70,70 48,85 C26,70 18,50 18,20 Z", s.lin([(0, "#3f5879"), (1, "#1b2a42")], 0, 0, 0.6, 1))
    s.path("M48,12 L48,85 C26,70 18,50 18,20 Z", "#ffffff", op=0.08)
    s.path("M48,12 L78,20 C78,50 70,70 48,85", "none", "#0d1624", 1.6, op=0.6)
    s.path("M48,6 L84,16 C84,50 74,74 48,92 C22,74 12,50 12,16 Z", "none", "#f0f3f6", 1.4, op=0.55, extra='transform="translate(-0.8,-0.8)"')
    s.circle(48, 44, 7, "#aab4c0", stroke=OUTLINE, sw=2)
    s.circle(46, 42, 2.4, "#ffffff", op=0.8)


def e_globe(s):
    c = 48
    s.circle(c, c, 41, OUTLINE)
    s.circle(c, c, 38.5, "#d2a548")
    s.circle(c, c, 33, BRONZE_LO)
    for rx in (12, 24):
        s.ellipse(c, c, rx, 33, "none", stroke=GOLD_HI, sw=2.4)
    for dy in (-17, 0, 17):
        half = math.sqrt(33 * 33 - dy * dy)
        s.line((c - half, c + dy), (c + half, c + dy), GOLD_HI, 2.4)
    s.circle(c, c, 33, "none", stroke=GOLD, sw=3.0)
    s.circle(c, c, 38.5, "none", stroke="#fff0b8", sw=1.4, op=0.5)


def e_ship(s):
    # hull
    hull = "M12,66 L84,66 C80,78 70,84 58,84 L38,84 C26,84 16,76 12,66 Z"
    s.path(hull, GOLD, OUTLINE, 4)
    s.path("M16,70 L80,70", "none", GOLD_LO, 2.0)
    s.path("M12,66 L84,66", "none", GOLD_HI, 2.0)
    s.line((46, 66), (46, 10), OUTLINE, 6)
    s.line((46, 66), (46, 10), GOLD_LO, 2.6)
    s.path("M50,14 C70,30 76,48 78,60 L50,60 Z", GOLD_HI, OUTLINE, 3.2)
    s.path("M42,20 C26,32 20,46 16,60 L42,60 Z", GOLD, OUTLINE, 3.2)
    s.path("M54,28 C64,38 68,46 70,54", "none", "#ffffff", 1.6, op=0.55)
    s.path("M8,84 C20,90 28,86 38,90 C48,94 60,88 72,92 C78,93 84,90 90,88", "none", GOLD_LO, 3, op=0.55)


def e_lens(s):
    s.circle(40, 40, 29, OUTLINE)
    s.circle(40, 40, 26, GOLD)
    s.circle(40, 40, 20, "#1d2a38")
    s.circle(40, 40, 20, "#a8d4ee", op=0.25)
    s.path("M26,34 C28,26 34,22 42,22", "none", "#ffffff", 3.0, op=0.7)
    s.circle(40, 40, 26, "none", stroke=GOLD_HI, sw=1.4, op=0.8)
    s.line((61, 61), (86, 86), OUTLINE, 14)
    s.line((61, 61), (86, 86), "#8a5a24", 9)
    s.line((60, 59), (84, 83), "#c8964a", 3, 0.8)
    s.line((58, 58), (68, 68), GOLD, 11)


ICONS = {
    "coin": e_coin,
    "coins": e_coins,
    "ingots": e_ingots,
    "flask": e_flask,
    "wheat": e_wheat,
    "shield": e_shield,
    "globe": e_globe,
    "ship": e_ship,
    "lens": e_lens,
}


def render_all():
    out = {}
    for name, draw in ICONS.items():
        s = Svg(SIZE, SIZE)
        draw(s)
        out[f"ico_{name}"] = downsample(rasterize(s.render(), SIZE * 3, SIZE * 3), SIZE, SIZE)
    return out
