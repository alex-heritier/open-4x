"""Illustrated resource icons: 22 pictures on `resources.pcx`'s 50 px grid.

Each is drawn at 4x on a 48x48 canvas with a few lit shapes (dark base, body
tone, highlight), then reduced, given a dark outline and a ground shadow.
Nothing here derives from any game's art.
"""
import math

from PIL import Image, ImageChops, ImageFilter

from . import gfx, iso
from .gfx import KEY, shade_rgb

SC = 4
SZ = 48
OUTLINE = (44, 30, 26)


def E(cv, cx, cy, rx, ry, col, hi=True):
    """A lit ellipse: shaded base, body, highlight toward the upper left."""
    cv.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), shade_rgb(col, 0.68))
    cv.ellipse((cx - rx * 0.92, cy - ry * 0.92, cx + rx * 0.84, cy + ry * 0.84), col)
    if hi:
        cv.ellipse((cx - rx * 0.66, cy - ry * 0.74, cx + rx * 0.1, cy - ry * 0.05), shade_rgb(col, 1.22))


def Pg(cv, pts, col, rim=True, light=False):
    cv.poly(pts, col, shade_rgb(col, 0.55) if rim else None)


def L(cv, pts, col, w=1.0):
    cv.line(pts, col, w)


# ----------------------------------------------------------------- icons

def horse(cv):
    brown = (150, 98, 58)
    dark = (60, 38, 26)
    for x in (13, 31):                                          # far legs
        L(cv, [(x, 29), (x - 0.5, 41)], shade_rgb(brown, 0.6), 2.6)
    Pg(cv, [(8, 26), (6, 31), (3, 38), (6, 38), (9, 32), (11, 28)], dark)           # tail
    E(cv, 21, 27, 13, 7.5, brown)
    Pg(cv, [(28, 26), (31, 11), (38, 6), (42, 11), (40, 15), (36, 14), (34, 26)], brown)   # neck + head
    Pg(cv, [(26, 24), (28, 9), (31, 10), (30, 24)], dark)       # mane
    Pg(cv, [(37, 5), (38.5, 1.5), (40, 6)], dark)                # ear
    cv.ellipse((38.8, 8.4, 40.6, 10.2), (20, 14, 12))
    for x in (17, 36):                                          # near legs
        L(cv, [(x, 29), (x + 0.2, 41)], brown, 2.8)
        cv.ellipse((x - 1.6, 40, x + 1.8, 42.4), dark)


def cattle(cv):
    hide, dark = (122, 78, 48), (52, 32, 22)
    for x in (13, 33):
        L(cv, [(x, 30), (x, 40)], shade_rgb(hide, 0.6), 3)
    E(cv, 24, 27, 15, 8.5, hide)
    for sx, sy, sr in ((20, 24, 3.2), (29, 30, 3.6), (14, 29, 2.2)):
        cv.ellipse((sx - sr, sy - sr * 0.8, sx + sr, sy + sr * 0.8), (232, 222, 206))
    Pg(cv, [(5, 17), (14, 15), (14, 27), (4, 28), (2, 23)], hide)                 # head
    L(cv, [(4, 17), (1, 12), (5, 11)], (238, 230, 206), 1.6)                      # horns
    L(cv, [(13, 16), (16, 11), (12, 10)], (238, 230, 206), 1.6)
    cv.ellipse((2, 24, 7, 28), (206, 150, 128))
    cv.ellipse((7, 18.4, 9, 20.2), (14, 10, 10))
    for x in (16, 36):
        L(cv, [(x, 30), (x, 41)], hide, 3.2)
        cv.ellipse((x - 1.8, 40, x + 2, 42.4), dark)
    L(cv, [(38, 22), (43, 30), (41, 38)], dark, 1.2)


def game(cv):
    fur = (170, 118, 70)
    for x in (16, 32):
        L(cv, [(x, 28), (x - 0.5, 42)], shade_rgb(fur, 0.62), 2.0)
    E(cv, 24, 27, 12.5, 6.2, fur)
    cv.ellipse((12, 27, 36, 32), (236, 224, 200))
    Pg(cv, [(31, 25), (33, 12), (40, 9), (42, 14), (38, 16), (36, 26)], fur)
    cv.ellipse((38.4, 11, 40.2, 12.8), (14, 10, 10))
    for x in (19, 29):
        L(cv, [(x, 28), (x, 42)], fur, 2.2)
    for (x0, y0) in ((35, 10), (39, 8)):                                 # antlers
        L(cv, [(x0, y0), (x0 - 2, y0 - 7), (x0 - 6, y0 - 9)], (230, 214, 178), 1.1)
        L(cv, [(x0 - 2, y0 - 7), (x0 + 1, y0 - 11)], (230, 214, 178), 1.0)
        L(cv, [(x0 - 4, y0 - 8.4), (x0 - 7, y0 - 5)], (230, 214, 178), 1.0)
    Pg(cv, [(12, 25), (8, 21), (10, 28)], (236, 224, 200))


def fish(cv):
    cv.shadow_ellipse(27, 38, 17, 3)
    blue = (70, 140, 190)
    Pg(cv, [(34, 24), (45, 14), (45, 34)], shade_rgb(blue, 0.9))                 # tail
    E(cv, 22, 24, 16, 9, blue)
    cv.ellipse((8, 26, 36, 32), (232, 240, 244))
    Pg(cv, [(18, 16), (26, 8), (30, 17)], shade_rgb(blue, 0.8))                  # fin
    for x in (17, 21, 25):
        L(cv, [(x, 18), (x, 28)], shade_rgb(blue, 1.25), 0.8)
    cv.ellipse((9.6, 21, 12.8, 24.2), (250, 250, 250))
    cv.ellipse((10.4, 21.6, 12.2, 23.6), (16, 20, 28))
    L(cv, [(7.6, 27), (11, 28)], (30, 52, 80), 0.7)


def whales(cv):
    blue = (62, 96, 150)
    L(cv, [(34, 10), (34, 3)], (226, 244, 252), 1.4)                             # spout
    L(cv, [(34, 8), (30, 5)], (226, 244, 252), 1.0)
    L(cv, [(34, 8), (38, 5)], (226, 244, 252), 1.0)
    Pg(cv, [(36, 28), (44, 20), (47, 25), (41, 30), (46, 34), (40, 35)], blue)    # flukes
    E(cv, 21, 28, 17, 9.5, blue)
    cv.ellipse((6, 30, 36, 37), (222, 232, 242))
    Pg(cv, [(15, 33), (21, 41), (23, 33)], shade_rgb(blue, 0.85))               # flipper
    cv.ellipse((9, 25, 11.4, 27.2), (14, 18, 28))
    L(cv, [(5, 33), (14, 34)], (30, 44, 70), 0.8)
    L(cv, [(2, 40), (14, 40)], (140, 190, 224), 1.0)
    L(cv, [(26, 42), (46, 42)], (140, 190, 224), 1.0)


def wheat(cv):
    gold = (222, 178, 66)
    stems = [(15, 4), (24, 2), (33, 4)]
    for sx, tip in stems:
        L(cv, [(24, 42), (sx, 14)], (118, 140, 60), 1.4)
        for k in range(7):
            y = 6 + tip - 2 + k * 3.0
            cv.ellipse((sx - 3.6, y - 2.2, sx - 0.2, y + 1.6), shade_rgb(gold, 0.85 + 0.2 * (k % 2)))
            cv.ellipse((sx + 0.2, y - 2.2, sx + 3.8, y + 1.6), shade_rgb(gold, 1.0 + 0.1 * (k % 2)))
            L(cv, [(sx - 2, y - 2), (sx - 4, y - 6)], (240, 214, 130), 0.5)
            L(cv, [(sx + 2, y - 2), (sx + 4, y - 6)], (240, 214, 130), 0.5)
    Pg(cv, [(18, 34), (30, 34), (28, 38), (20, 38)], (150, 86, 60))                # band


def spice(cv):
    E(cv, 24, 36, 17, 5, (126, 84, 54), False)
    Pg(cv, [(8, 30), (40, 30), (35, 40), (13, 40)], (150, 98, 60))
    E(cv, 24, 30, 16, 4.6, (214, 100, 36))
    E(cv, 22, 28, 9, 2.8, (240, 150, 52), False)
    for (x, y, a) in ((10, 20, 0.4), (30, 14, -0.5), (24, 22, 0.1), (36, 22, 0.7)):
        pts = [(x, y), (x + 5 * math.cos(a), y + 5 * math.sin(a) - 1), (x + 9 * math.cos(a + 0.2), y + 9 * math.sin(a) + 4), (x + 5, y + 6)]
        Pg(cv, pts, (200, 36, 30))
        L(cv, [(x, y), (x - 1, y - 3)], (60, 110, 40), 1.2)


def incense(cv):
    Pg(cv, [(10, 26), (38, 26), (34, 38), (14, 38)], (158, 124, 66))
    E(cv, 24, 26, 14, 3.8, (190, 150, 82))
    E(cv, 24, 25.4, 9, 2.4, (60, 46, 34), False)
    Pg(cv, [(16, 38), (32, 38), (30, 42), (18, 42)], (120, 92, 48))
    for ox, ph in ((-3, 0), (4, 1.6)):
        pts = [(24 + ox + 3.2 * math.sin(i * 0.55 + ph), 24 - i * 2.0) for i in range(11)]
        L(cv, pts, (228, 226, 232), 2.0)
        L(cv, [(x + 0.8, y) for x, y in pts], (190, 190, 200), 0.8)


def dye(cv):
    for (cx, col, rim) in ((15, (122, 52, 160), (86, 36, 112)), (32, (196, 54, 70), (140, 36, 50))):
        Pg(cv, [(cx - 9, 22), (cx + 9, 22), (cx + 11, 36), (cx + 6, 41), (cx - 6, 41), (cx - 11, 36)], (176, 130, 90))
        E(cv, cx, 22, 9, 3.6, col)
        E(cv, cx - 1.6, 21.6, 5, 1.6, shade_rgb(col, 1.3), False)
        L(cv, [(cx - 9, 27), (cx + 9, 27)], (130, 90, 56), 0.9)
        cv.ellipse((cx + 4, 30, cx + 6, 32.4), col)


def wine(cv):
    Pg(cv, [(11, 40), (14, 40), (16, 18), (14, 10), (11, 10), (9, 18)], (46, 96, 56))
    Pg(cv, [(11, 4), (14, 4), (14, 10), (11, 10)], (150, 30, 40))
    cv.ellipse((10.4, 24, 12, 38), (140, 190, 140))
    for i, (x, y) in enumerate(((28, 22), (34, 22), (25, 27), (31, 27), (37, 27), (28, 32), (34, 32), (31, 37))):
        E(cv, x, y, 3.6, 3.6, (110, 44, 130) if i % 3 else (130, 56, 150))
    L(cv, [(31, 20), (32, 13)], (84, 70, 40), 1.4)
    Pg(cv, [(32, 14), (40, 10), (42, 18), (35, 19)], (70, 130, 52))
    L(cv, [(33, 15), (40, 13)], (140, 190, 100), 0.7)


def ivory(cv):
    cream = (240, 228, 196)
    Pg(cv, [(10, 40), (16, 22), (28, 8), (40, 6), (36, 12), (26, 20), (20, 32), (17, 40)], cream)
    Pg(cv, [(22, 40), (26, 24), (34, 12), (44, 12), (40, 16), (34, 24), (28, 34), (26, 41)], shade_rgb(cream, 0.94))
    L(cv, [(14, 36), (19, 22), (28, 11)], (255, 252, 240), 0.9)
    L(cv, [(25, 38), (29, 24), (36, 15)], (255, 252, 240), 0.8)
    Pg(cv, [(10, 40), (17, 40), (16, 42), (11, 42)], (176, 150, 106))
    Pg(cv, [(22, 40), (27, 41), (26, 43), (23, 43)], (176, 150, 106))


def silk(cv):
    Pg(cv, [(6, 28), (26, 22), (40, 28), (22, 36)], (236, 188, 214))                  # unrolled cloth
    Pg(cv, [(6, 28), (22, 36), (22, 40), (6, 32)], (206, 150, 184))
    Pg(cv, [(22, 36), (40, 28), (40, 32), (22, 40)], (176, 122, 156))
    L(cv, [(12, 28), (26, 32)], (252, 228, 240), 1.0)
    L(cv, [(18, 26), (32, 30)], (252, 228, 240), 0.8)
    iso_roll_x, iso_roll_y = 33, 22
    cv.ellipse((iso_roll_x - 7, iso_roll_y - 3.4, iso_roll_x + 7, iso_roll_y + 12), (190, 142, 190))
    cv.ellipse((iso_roll_x - 7, iso_roll_y - 5.4, iso_roll_x + 7, iso_roll_y + 1.6), (246, 214, 236))
    E(cv, iso_roll_x, iso_roll_y - 2, 3, 1.4, (150, 90, 150), False)
    L(cv, [(iso_roll_x - 5, iso_roll_y + 2), (iso_roll_x - 5, iso_roll_y + 9)], (250, 232, 246), 0.9)


def furs(cv):
    fur = (126, 82, 52)
    pts = [(24, 4), (31, 8), (40, 7), (38, 15), (44, 22), (38, 28), (42, 36), (33, 38), (28, 44), (22, 40), (14, 44), (11, 36), (4, 34), (9, 27), (4, 20), (12, 17), (10, 9), (18, 10)]
    Pg(cv, pts, shade_rgb(fur, 0.72))
    inner = [(24 + (x - 24) * 0.82, 24 + (y - 24) * 0.82) for x, y in pts]
    cv.poly(inner, fur)
    inner2 = [(22 + (x - 24) * 0.55, 22 + (y - 24) * 0.55) for x, y in pts]
    cv.poly(inner2, (156, 106, 66))
    for (x, y) in ((16, 18), (28, 14), (32, 26), (20, 30), (26, 22)):
        L(cv, [(x, y), (x + 2, y + 3)], (84, 52, 34), 0.9)


def oil(cv):
    Pg(cv, [(9, 12), (31, 12), (33, 36), (7, 36)], (62, 66, 78))
    E(cv, 20, 12, 11, 3.4, (92, 96, 110))
    E(cv, 20, 36, 13, 3.8, (40, 42, 54), False)
    for y in (18, 30):
        L(cv, [(8, y), (32, y + 0.4)], (30, 32, 40), 1.3)
        L(cv, [(8.8, y - 0.7), (31.2, y - 0.3)], (110, 114, 130), 0.6)
    cv.ellipse((10, 14, 13, 33), (130, 136, 154))
    Pg(cv, [(38, 14), (42, 24), (41, 30), (35, 30), (34, 24)], (22, 20, 26))
    cv.ellipse((36, 18, 38.4, 22), (110, 110, 130))
    E(cv, 38, 40, 7, 2.4, (18, 16, 22), False)


def rubber(cv):
    ring = (50, 52, 58)
    cv.shadow_ellipse(26, 38, 17, 3.6)
    cv.ellipse((6, 10, 42, 40), shade_rgb(ring, 0.8))
    cv.ellipse((8, 11, 40, 37), ring)
    cv.ellipse((8, 11, 36, 32), shade_rgb(ring, 1.4))
    cv.ellipse((14, 17, 34, 31), (152, 150, 156))                       # hub (rim seen through)
    cv.ellipse((17, 19, 31, 28), (190, 188, 194))
    cv.ellipse((21, 21, 27, 25), (90, 90, 96))
    for k in range(12):
        a = math.tau * k / 12
        x, y = 24 + 17 * math.cos(a), 25.5 + 12 * math.sin(a)
        L(cv, [(x, y), (24 + 14 * math.cos(a), 25.5 + 9.5 * math.sin(a))], (28, 28, 32), 1.0)
    Pg(cv, [(38, 6), (44, 8), (42, 14), (37, 12)], (84, 150, 66))        # a leaf


def diamonds(cv):
    cv.shadow_ellipse(24, 40, 14, 3.4)
    top = [(10, 17), (17, 8), (31, 8), (38, 17)]
    Pg(cv, top, (170, 226, 250))
    Pg(cv, [(10, 17), (38, 17), (24, 41)], (92, 170, 226))
    Pg(cv, [(10, 17), (24, 17), (24, 41)], (130, 200, 240))
    Pg(cv, [(17, 8), (24, 17), (10, 17)], (210, 244, 255))
    Pg(cv, [(31, 8), (38, 17), (24, 17)], (148, 214, 244))
    L(cv, [(24, 17), (24, 8.5)], (240, 252, 255), 0.7)
    for (x, y) in ((6, 8), (40, 5), (42, 28)):
        L(cv, [(x, y - 3), (x, y + 3)], (255, 255, 255), 0.9)
        L(cv, [(x - 3, y), (x + 3, y)], (255, 255, 255), 0.9)


def gold(cv):
    cv.shadow_ellipse(24, 41, 18, 3.4)
    g = (236, 188, 40)
    for (cx, cy) in ((13, 36), (35, 36), (24, 36)):
        iso.box(cv, cx, cy, 8.5, 4.2, 6, g)
    iso.box(cv, 18.5, 28, 8.5, 4.2, 6, g)
    iso.box(cv, 30.5, 28, 8.5, 4.2, 6, g)
    iso.box(cv, 24, 20, 8.5, 4.2, 6, g)
    for (x, y) in ((8, 12), (40, 10), (42, 26)):
        L(cv, [(x, y - 3), (x, y + 3)], (255, 252, 200), 1.0)
        L(cv, [(x - 3, y), (x + 3, y)], (255, 252, 200), 1.0)


def iron(cv):
    cv.shadow_ellipse(24, 41, 17, 3.6)
    Pg(cv, [(6, 40), (8, 24), (20, 16), (30, 22), (28, 40)], (120, 118, 124))
    Pg(cv, [(20, 16), (30, 22), (28, 40), (18, 40), (15, 26)], (92, 90, 98))
    Pg(cv, [(24, 40), (26, 28), (38, 22), (44, 32), (42, 40)], (138, 120, 112))
    Pg(cv, [(26, 28), (38, 22), (32, 34)], (170, 100, 76))
    L(cv, [(10, 30), (18, 22)], (180, 178, 188), 0.9)
    L(cv, [(8, 36), (14, 33)], (176, 100, 70), 1.2)
    L(cv, [(32, 28), (40, 31)], (186, 108, 80), 1.0)


def aluminum(cv):
    cv.shadow_ellipse(24, 41, 18, 3.4)
    a = (190, 200, 214)
    iso.box(cv, 15, 36, 9, 4.4, 5.6, a)
    iso.box(cv, 33, 36, 9, 4.4, 5.6, a)
    iso.box(cv, 24, 29, 9, 4.4, 5.6, shade_rgb(a, 1.06))
    iso.box(cv, 15, 23, 9, 4.4, 5.6, a)
    iso.box(cv, 33, 23, 9, 4.4, 5.6, shade_rgb(a, 1.1))
    iso.box(cv, 24, 16, 9, 4.4, 5.6, shade_rgb(a, 1.12))
    L(cv, [(18, 12), (28, 10)], (255, 255, 255), 0.8)


def coal(cv):
    cv.shadow_ellipse(24, 41, 18, 3.8)
    for pts, col in (
            ([(5, 40), (7, 28), (16, 22), (22, 30), (20, 40)], (46, 46, 54)),
            ([(18, 40), (20, 22), (30, 14), (38, 24), (36, 40)], (58, 58, 68)),
            ([(32, 40), (34, 30), (42, 28), (44, 38)], (40, 40, 48))):
        Pg(cv, pts, col)
    L(cv, [(8, 30), (15, 24)], (130, 134, 150), 1.0)
    L(cv, [(22, 24), (30, 17)], (150, 154, 170), 1.0)
    L(cv, [(35, 30), (41, 29)], (120, 124, 140), 0.9)
    Pg(cv, [(24, 22), (30, 16), (34, 24), (28, 28)], (84, 86, 100))


def saltpetre(cv):
    cv.shadow_ellipse(24, 41, 17, 3.4)
    Pg(cv, [(5, 41), (8, 30), (20, 26), (36, 28), (43, 41)], (136, 112, 88))
    for (x, y, h, c) in ((12, 34, 14, (232, 242, 250)), (20, 32, 18, (214, 232, 246)), (28, 33, 15, (240, 248, 252)), (35, 35, 11, (206, 226, 244)), (16, 36, 9, (246, 250, 255))):
        Pg(cv, [(x - 3.4, y), (x - 2.4, y - h + 3), (x, y - h), (x + 2.4, y - h + 3), (x + 3.4, y)], c)
        L(cv, [(x - 1.2, y - 1), (x - 1, y - h + 4)], (255, 255, 255), 0.6)
        L(cv, [(x + 1.6, y - 1), (x + 1.4, y - h + 5)], shade_rgb(c, 0.8), 0.6)


def uranium(cv):
    cv.shadow_ellipse(24, 41, 15, 3.4)
    Pg(cv, [(8, 41), (10, 22), (14, 14), (18, 22), (18, 41)], (96, 190, 64))
    Pg(cv, [(16, 41), (20, 12), (26, 4), (31, 14), (30, 41)], (140, 236, 84))
    Pg(cv, [(28, 41), (30, 24), (36, 18), (41, 26), (40, 41)], (84, 168, 60))
    L(cv, [(21, 38), (22, 14)], (230, 255, 190), 1.0)
    L(cv, [(12, 38), (13, 24)], (190, 244, 150), 0.8)
    for k in range(8):
        a = math.tau * k / 8
        L(cv, [(24 + 17 * math.cos(a), 24 + 17 * math.sin(a)), (24 + 21 * math.cos(a), 24 + 21 * math.sin(a))], (200, 255, 120), 0.9)


ICONS = {
    "horse": horse, "diamonds": diamonds, "saltpetre": saltpetre, "coal": coal, "oil": oil, "iron": iron,
    "aluminum": aluminum, "uranium": uranium, "wine": wine, "furs": furs, "dye": dye, "incense": incense,
    "spice": spice, "ivory": ivory, "silk": silk, "rubber": rubber, "whales": whales, "game": game,
    "fish": fish, "cattle": cattle, "wheat": wheat, "gold": gold,
}


def icon(name):
    """One 48x48 icon: (RGB, alpha, shadow)."""
    cv = iso.Canvas((SZ, SZ), SC)
    cv.shadow_ellipse(24, 41, 15, 3.0)
    ICONS[name](cv)
    img, sh = cv.done()
    img = gfx.outline(img, OUTLINE, 1)
    alpha = img.getchannel("A").point([255 if i >= 110 else 0 for i in range(256)])
    sh = sh.filter(ImageFilter.GaussianBlur(0.9))
    sh = ImageChops.multiply(sh, ImageChops.invert(alpha))
    rgb = Image.new("RGB", (SZ, SZ), KEY)
    rgb.paste(img.convert("RGB"), mask=alpha)
    return rgb, alpha, sh


def resources(order):
    """`resources.pcx`: 6 columns of 50 px cells holding the 48 px icons."""
    size = (300, 300)
    rgb = Image.new("RGB", size, KEY)
    alpha = Image.new("L", size, 0)
    shadow = Image.new("L", size, 0)
    for i, name in enumerate(order):
        c, r = i % 6, i // 6
        a, m, s = icon(name)
        rgb.paste(a, (c * 50 + 1, r * 50 + 1), m)
        alpha.paste(m, (c * 50 + 1, r * 50 + 1))
        shadow.paste(s, (c * 50 + 1, r * 50 + 1))
    return rgb, alpha, shadow
