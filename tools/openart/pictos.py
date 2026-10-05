"""Small illustrated pictograms: improvements, advances and city-screen icons.

Every glyph is drawn on a 32-unit canvas at 4x with a few lit shapes (the
helpers below), reduced, outlined and keyed like the resource icons. They
are original drawings; no game art is read or traced.
"""
import hashlib
import math

from PIL import Image

from . import gfx, iso
from .gfx import KEY, shade_rgb

SZ, SC = 32, 4
INK = (44, 30, 26)

MARB = (234, 228, 210)
STONE = (190, 178, 152)
BRICK = (178, 98, 70)
GOLD = (230, 180, 54)
RED = (192, 62, 50)
BLUE = (66, 110, 178)
SKY = (146, 194, 228)
GREEN = (86, 150, 70)
WOOD = (152, 104, 60)
IRON = (152, 160, 172)
DARK = (62, 56, 60)
WHITE = (248, 246, 240)
PURPLE = (124, 80, 152)
SAND = (214, 190, 120)
PAPER = (238, 222, 184)


# ------------------------------------------------------------ primitives

def rect(cv, x0, y0, x1, y1, col, rim=True):
    """A lit box: lighter left strip, darker right strip."""
    w = x1 - x0
    cv.poly([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], col, shade_rgb(col, 0.55) if rim else None)
    cv.poly([(x0 + .4, y0 + .4), (x0 + w * .3, y0 + .4), (x0 + w * .3, y1 - .4), (x0 + .4, y1 - .4)],
            shade_rgb(col, 1.14))
    cv.poly([(x1 - w * .24, y0 + .4), (x1 - .4, y0 + .4), (x1 - .4, y1 - .4), (x1 - w * .24, y1 - .4)],
            shade_rgb(col, 0.82))


def poly(cv, pts, col, rim=True):
    cv.poly(pts, col, shade_rgb(col, 0.55) if rim else None)


def tri(cv, a, b, c, col):
    poly(cv, [a, b, c], col)


def disc(cv, cx, cy, rx, ry=None, col=GOLD, hi=True):
    ry = rx if ry is None else ry
    cv.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), shade_rgb(col, 0.62))
    cv.ellipse((cx - rx * .9, cy - ry * .9, cx + rx * .86, cy + ry * .86), col)
    if hi:
        cv.ellipse((cx - rx * .62, cy - ry * .72, cx + rx * .05, cy - ry * .08), shade_rgb(col, 1.24))


def arc(cx, cy, rx, ry, a0, a1, n=18):
    return [(cx + rx * math.cos(math.radians(a0 + (a1 - a0) * i / n)),
             cy + ry * math.sin(math.radians(a0 + (a1 - a0) * i / n))) for i in range(n + 1)]


def half(cv, cx, cy, rx, ry, col):
    """A dome: the upper half of an ellipse standing on y = cy."""
    pts = arc(cx, cy, rx, ry, 180, 360)
    poly(cv, pts, col)
    cv.poly([(cx - rx * .7, cy - ry * .45)] + arc(cx, cy, rx * .7, ry * .72, 200, 270, 8) + [(cx - rx * .1, cy - ry * .2)],
            shade_rgb(col, 1.2))


def ring(cv, cx, cy, r, col, w=1.2, a0=0, a1=360, n=28):
    cv.line(arc(cx, cy, r, r, a0, a1, n), col, w)


def stroke(cv, pts, col, w=1.2):
    cv.line(pts, col, w)


def columns(cv, xs, y0, y1, col=MARB, w=2.6):
    for x in xs:
        rect(cv, x, y0, x + w, y1, col)


def arrow_head(cv, tip, ang, col, s=3.0):
    a = math.radians(ang)
    pts = [tip,
           (tip[0] - s * math.cos(a - .5), tip[1] - s * math.sin(a - .5)),
           (tip[0] - s * math.cos(a + .5), tip[1] - s * math.sin(a + .5))]
    poly(cv, pts, col, rim=False)


# --------------------------------------------------------------- buildings

def palace(cv):
    rect(cv, 2, 25, 30, 28.5, STONE)
    rect(cv, 4, 15, 28, 25, MARB)
    for x in (7, 14.5, 22):
        rect(cv, x, 18, x + 3, 25, DARK, rim=False)
    rect(cv, 11, 11, 21, 15, STONE)
    half(cv, 16, 11, 6.5, 6, GOLD)
    stroke(cv, [(16, 5), (16, 1.5)], DARK, .9)
    poly(cv, [(16, 1.5), (21, 2.8), (16, 4)], RED, rim=False)
    rect(cv, 3, 11, 9, 15, STONE)
    rect(cv, 23, 11, 29, 15, STONE)
    tri(cv, (2.5, 11), (6, 7.5), (9.5, 11), BRICK)
    tri(cv, (22.5, 11), (26, 7.5), (29.5, 11), BRICK)


def barracks(cv):
    for sgn in (-1, 1):
        stroke(cv, [(16 - sgn * 11, 3), (16 + sgn * 9, 25)], IRON, 2.2)
        stroke(cv, [(16 - sgn * 10, 3.5), (16 + sgn * 8, 24)], WHITE, .7)
        stroke(cv, [(16 + sgn * 5, 20 - sgn * 0), (16 + sgn * 11, 18)], WOOD, 1.8)
    poly(cv, [(8, 9), (24, 9), (24, 19), (16, 28), (8, 19)], BLUE)
    poly(cv, [(16, 9), (24, 9), (24, 19), (16, 28)], shade_rgb(BLUE, .82), rim=False)
    stroke(cv, [(16, 11), (16, 25)], GOLD, 1.5)
    stroke(cv, [(10, 15), (22, 15)], GOLD, 1.5)


def granary(cv):
    rect(cv, 8, 12, 24, 28, SAND)
    for y in (17, 22):
        stroke(cv, [(8.4, y), (23.6, y)], shade_rgb(SAND, .72), .7)
    poly(cv, [(6, 12.5), (16, 2.5), (26, 12.5)], BRICK)
    poly(cv, [(16, 2.5), (26, 12.5), (16, 12.5)], shade_rgb(BRICK, .82), rim=False)
    rect(cv, 13.5, 20, 18.5, 28, WOOD)
    for dx in (-3.4, 0, 3.4):                         # wheat ears
        stroke(cv, [(16 + dx, 12), (16 + dx * 1.4, 7.5)], GOLD, .7)


def temple(cv, tone=MARB, roof=BRICK):
    rect(cv, 2, 25, 30, 28.5, STONE)
    rect(cv, 4, 11, 28, 14.5, tone)
    columns(cv, (6, 11.4, 16.8, 22.2), 14.5, 25, tone, 3.2)
    poly(cv, [(2, 11), (16, 2.5), (30, 11)], roof)
    poly(cv, [(16, 2.5), (30, 11), (16, 11)], shade_rgb(roof, .84), rim=False)
    disc(cv, 16, 8, 1.9, col=GOLD)


def marketplace(cv):
    rect(cv, 6, 17, 26, 28, WOOD)
    rect(cv, 4, 22, 28, 25, shade_rgb(WOOD, 1.2))
    for i in range(6):                                  # striped awning
        x0 = 3 + i * 4.3
        poly(cv, [(x0, 6), (x0 + 4.3, 6), (x0 + 4.9, 14), (x0 - .6, 14)], RED if i % 2 == 0 else WHITE, rim=False)
    cv.line([(3, 6), (29, 6), (29.6, 14), (2.4, 14), (3, 6)], DARK, .7)
    for x, c in ((9, RED), (13.5, GOLD), (18, GREEN), (22.5, RED)):
        disc(cv, x, 21, 2.0, col=c)


def library(cv):
    books = ((BLUE, 3, 22, 26, 28), (RED, 4.5, 16.5, 25.5, 22), (GREEN, 3.5, 11, 26, 16.5))
    for col, x0, y0, x1, y1 in books:
        rect(cv, x0, y0, x1, y1, col)
        stroke(cv, [(x0 + 1.4, y0 + 1.6), (x0 + 1.4, y1 - 1.6)], GOLD, .8)
        stroke(cv, [(x1 + .2, y0 + 1.2), (x1 + .2, y1 - 1.2)], PAPER, 1.1)
    poly(cv, [(18, 11), (21.5, 3), (24.5, 3.4), (22, 11)], WHITE)         # quill
    stroke(cv, [(20, 11), (19, 13)], DARK, .8)


def courthouse(cv):
    rect(cv, 14.4, 6, 17.6, 27, STONE)
    rect(cv, 8, 25.5, 24, 28.5, STONE)
    stroke(cv, [(5, 8), (27, 8)], GOLD, 1.5)
    disc(cv, 16, 5.6, 2.3, col=GOLD)
    for cx in (6, 26):
        stroke(cv, [(cx, 8), (cx - 3.4, 16)], DARK, .6)
        stroke(cv, [(cx, 8), (cx + 3.4, 16)], DARK, .6)
        poly(cv, arc(cx, 16, 4.6, 3.6, 0, 180, 10), GOLD)


def aqueduct(cv):
    rect(cv, 1, 8, 31, 12, STONE)
    rect(cv, 2, 6.2, 30, 8.6, SKY)
    rect(cv, 1, 12, 31, 28, STONE)
    for cx in (6.5, 16, 25.5):
        poly(cv, [(cx - 3.6, 28)] + arc(cx, 20, 3.6, 8, 180, 360, 10) + [(cx + 3.6, 28)], DARK, rim=False)
    stroke(cv, [(1, 12), (31, 12)], shade_rgb(STONE, .6), .6)


def cathedral(cv):
    rect(cv, 3, 17, 29, 28.5, STONE)
    rect(cv, 11, 8, 21, 28.5, MARB)
    poly(cv, [(10, 8), (16, -.5), (22, 8)], BLUE)
    stroke(cv, [(16, -2), (16, 3.5)], GOLD, .9)
    stroke(cv, [(14.2, .6), (17.8, .6)], GOLD, .9)
    poly(cv, [(3, 17), (7, 13), (11, 13), (11, 17)], BRICK)
    poly(cv, [(29, 17), (25, 13), (21, 13), (21, 17)], BRICK)
    disc(cv, 16, 14, 2.6, col=SKY)
    poly(cv, [(14, 28.5), (14, 21)] + arc(16, 21, 2, 2.4, 180, 360, 6) + [(18, 28.5)], DARK, rim=False)
    for x in (5.5, 24.5):
        poly(cv, [(x - 1, 23), (x - 1, 20)] + arc(x, 20, 1, 1.6, 180, 360, 5) + [(x + 1, 23)], DARK, rim=False)


def university(cv):
    poly(cv, [(16, 5), (30, 11), (16, 17), (2, 11)], DARK)
    poly(cv, [(16, 5), (30, 11), (16, 17)], shade_rgb(DARK, 1.35), rim=False)
    poly(cv, [(8, 14), (16, 18), (24, 14), (24, 21), (16, 25), (8, 21)], shade_rgb(DARK, 1.1))
    stroke(cv, [(27, 11.5), (27, 21)], GOLD, 1)
    disc(cv, 27, 22.5, 1.8, col=GOLD)
    rect(cv, 3, 26, 29, 28.6, STONE)


def bank(cv):
    for i, (x, n) in enumerate(((5.5, 4), (13, 6), (20.5, 3))):
        for k in range(n):
            y = 26.5 - k * 2.5
            disc(cv, x + 3, y, 4.2, 1.7, col=GOLD if k % 2 else shade_rgb(GOLD, 1.12), hi=False)
            cv.line([(x - 1.2, y), (x + 7.2, y)], shade_rgb(GOLD, .6), .5)
    disc(cv, 23, 8, 4.4, col=GOLD)
    stroke(cv, [(23, 5), (23, 11)], shade_rgb(GOLD, .55), 1)


def colosseum(cv):
    for y0, y1, rx, ry, col in ((13, 28, 14.5, 7.5, STONE), (8.5, 22, 12.5, 6.5, shade_rgb(STONE, 1.1))):
        pass
    cv.ellipse((1.5, 14, 30.5, 30), shade_rgb(STONE, .58))
    rect(cv, 1.5, 14.5, 30.5, 24, STONE, rim=False)
    cv.ellipse((1.5, 8, 30.5, 22), shade_rgb(STONE, 1.12))
    cv.ellipse((6, 11, 26, 19), SAND)
    cv.ellipse((9, 13, 23, 18), shade_rgb(SAND, .85))
    for x in (4.5, 8.5, 12.5, 16.5, 20.5, 24.5):
        poly(cv, [(x, 24), (x, 20.5)] + arc(x + 1.1, 20.5, 1.1, 1.6, 180, 360, 5) + [(x + 2.2, 24)], DARK, rim=False)


def factory(cv):
    rect(cv, 2, 15, 30, 28.5, BRICK)
    for i in range(3):
        poly(cv, [(2 + i * 9.3, 15), (2 + i * 9.3, 9), (11.3 + i * 9.3, 15)], shade_rgb(BRICK, 1.12 - i * .06))
    rect(cv, 22, 3, 26, 15, shade_rgb(BRICK, .8))
    rect(cv, 16.5, 5.5, 20, 15, shade_rgb(BRICK, .75))
    for (cx, cy, r) in ((24, 2, 2.4), (27, -.5, 2.2), (18.3, 4, 2)):
        disc(cv, cx, cy, r, col=(176, 176, 180), hi=False)
    for x in (5, 11, 17):
        rect(cv, x, 20, x + 4, 24, GOLD, rim=False)
    rect(cv, 23, 20, 28, 28.5, DARK)


def hospital(cv):
    rect(cv, 3, 8, 29, 28.5, WHITE)
    rect(cv, 12.2, 11, 19.8, 25, RED, rim=False)
    rect(cv, 7, 15.2, 25, 20.8, RED, rim=False)
    rect(cv, 3, 6, 29, 8.6, STONE)


def harbor(cv):
    ring(cv, 16, 7, 2.4, IRON, 1.4)
    stroke(cv, [(16, 9.5), (16, 25)], IRON, 2)
    stroke(cv, [(10.5, 13), (21.5, 13)], IRON, 1.8)
    stroke(cv, arc(16, 19, 10.5, 8, 5, 175, 16), IRON, 2)
    arrow_head(cv, (4.9, 20.5), 120, IRON, 3.2)
    arrow_head(cv, (27.1, 20.5), 60, IRON, 3.2)
    stroke(cv, [(1, 28), (6, 26.4), (11, 28), (16, 26.4), (21, 28), (26, 26.4), (31, 28)], BLUE, 1.5)


def airport(cv):
    poly(cv, [(16, 1.5), (18.4, 9), (18.4, 15)], IRON)
    poly(cv, [(18.4, 13), (30, 20), (30, 22.5), (18.4, 19.5)], shade_rgb(IRON, .85))
    poly(cv, [(13.6, 13), (2, 20), (2, 22.5), (13.6, 19.5)], IRON)
    poly(cv, [(13.6, 9), (13.6, 26), (16, 29.5), (18.4, 26), (18.4, 9), (16, 1.5)], MARB)
    poly(cv, [(18.4, 25), (23, 28), (23, 29.5), (18.4, 28.2)], shade_rgb(IRON, .85))
    poly(cv, [(13.6, 25), (9, 28), (9, 29.5), (13.6, 28.2)], IRON)
    poly(cv, [(14.6, 6), (17.4, 6), (16, 3.6)], BLUE, rim=False)


def badge(cv, col=BLUE, mark=GOLD):
    poly(cv, [(16, 1.5), (28, 6), (28, 16), (16, 30), (4, 16), (4, 6)], col)
    poly(cv, [(16, 1.5), (28, 6), (28, 16), (16, 30)], shade_rgb(col, .82), rim=False)
    star = []
    for i in range(10):
        r = 7 if i % 2 == 0 else 3.1
        a = math.radians(-90 + i * 36)
        star.append((16 + r * math.cos(a), 12.5 + r * math.sin(a)))
    poly(cv, star, mark)


def recycling(cv):
    for k in range(3):
        a0 = -90 + k * 120 + 12
        stroke(cv, arc(16, 16, 9.5, 9.5, a0, a0 + 82, 10), GREEN, 3.4)
        e = math.radians(a0 + 82)
        arrow_head(cv, (16 + 9.5 * math.cos(e + .22), 16 + 9.5 * math.sin(e + .22)), a0 + 82 + 90 + 13, GREEN, 6)


def plant(cv, smoke=(110, 110, 116), fuel=None):
    poly(cv, [(7, 28), (9.5, 8), (18.5, 8), (21, 28)], STONE)
    poly(cv, [(14, 8), (18.5, 8), (21, 28), (14, 28)], shade_rgb(STONE, .8), rim=False)
    stroke(cv, [(8.4, 17), (19.6, 17)], shade_rgb(STONE, .6), .7)
    rect(cv, 22, 15, 30, 28, BRICK)
    for (cx, cy, r) in ((14, 5, 3), (18, 2, 2.6), (11, 2.4, 2.2)):
        disc(cv, cx, cy, r, col=smoke, hi=False)
    if fuel:
        for (x, y) in ((24, 26), (27.6, 26.6), (25.6, 24)):
            disc(cv, x, y, 1.6, col=fuel, hi=False)


def hydro(cv):
    poly(cv, [(2, 6), (12, 6), (17, 28), (2, 28)], STONE)
    poly(cv, [(2, 6), (5, 6), (5, 28), (2, 28)], shade_rgb(STONE, 1.15), rim=False)
    for y in (12, 18, 24):
        stroke(cv, [(3, y), (14 + (y - 6) * .22, y)], shade_rgb(STONE, .62), .6)
    poly(cv, [(12, 12), (17, 12), (21, 28), (17, 28)], SKY, rim=False)
    poly(cv, [(17, 28), (31, 28), (31, 24), (21, 24)], BLUE)
    for y in (26, 27.4):
        stroke(cv, [(18, y), (31, y)], SKY, .8)
    stroke(cv, [(2, 5.6), (12, 5.6)], DARK, .8)
    poly(cv, [(20, 4), (23, 10), (21, 10), (22, 15), (17.6, 8), (19.4, 8)], GOLD)


def solar(cv):
    disc(cv, 24, 7, 4.6, col=GOLD)
    for k in range(8):
        a = math.radians(k * 45)
        stroke(cv, [(24 + 6.4 * math.cos(a), 7 + 6.4 * math.sin(a)), (24 + 8.6 * math.cos(a), 7 + 8.6 * math.sin(a))], GOLD, .9)
    poly(cv, [(2, 24), (6, 12), (24, 12), (28, 24)], BLUE)
    for i in range(1, 4):
        stroke(cv, [(2 + i * 6.5, 24), (6 + i * 4.5, 12)], SKY, .6)
    for j in (16, 20):
        stroke(cv, [(3.6 + (24 - j) * .35 - .35 * 4, j), (26.4 - (24 - j) * .35 + .35 * 4, j)], SKY, .6)
    rect(cv, 14, 24, 17, 28.5, IRON)
    rect(cv, 9, 27.5, 22, 29.5, IRON)


def atom(cv, col=BLUE):
    for rot in (0, 60, 120):
        pts = []
        for i in range(25):
            t = math.radians(i * 15)
            x, y = 12.5 * math.cos(t), 4.6 * math.sin(t)
            r = math.radians(rot)
            pts.append((16 + x * math.cos(r) - y * math.sin(r), 16 + x * math.sin(r) + y * math.cos(r)))
        stroke(cv, pts, col, 1.3)
    disc(cv, 16, 16, 3.2, col=RED)
    for x, y in ((28, 16), (10, 5.2), (10, 26.8)):
        disc(cv, x, y, 1.8, col=GOLD)


def gear(cv, col=IRON, teeth=9, r0=11, r1=8.4):
    pts = []
    for i in range(teeth * 4):
        r = r0 if i % 4 in (0, 1) else r1
        a = math.radians(i * 360 / (teeth * 4))
        pts.append((16 + r * math.cos(a), 16 + r * math.sin(a)))
    poly(cv, pts, col)
    disc(cv, 16, 16, 7, col=shade_rgb(col, 1.08))
    disc(cv, 16, 16, 3, col=DARK, hi=False)


def flask(cv, col=GREEN):
    poly(cv, [(12.5, 2.5), (19.5, 2.5), (19.5, 11), (28, 26.5), (25.5, 29.5), (6.5, 29.5), (4, 26.5), (12.5, 11)], (212, 232, 238))
    poly(cv, [(7.8, 21), (24.2, 21), (28, 26.5), (25.5, 29.5), (6.5, 29.5), (4, 26.5)], col)
    for x, y, r in ((12, 25, 1.4), (17.5, 23.5, 1.1), (20, 26.5, 1.5)):
        disc(cv, x, y, r, col=shade_rgb(col, 1.35), hi=False)
    rect(cv, 11.5, 1.8, 20.5, 4, WOOD)


def dock(cv):
    rect(cv, 2, 24, 30, 27.5, WOOD)
    for x in (5, 12, 19, 26):
        rect(cv, x, 27, x + 2, 30, shade_rgb(WOOD, .7))
    rect(cv, 4, 6, 7, 24, RED)
    stroke(cv, [(5.5, 7), (24, 7)], RED, 2)
    stroke(cv, [(22, 7), (22, 13)], DARK, .8)
    rect(cv, 19, 13, 26, 20, GOLD)
    rect(cv, 10, 17, 17, 24, BLUE)


def defense(cv):
    poly(cv, [(16, 1.5), (28, 6), (28, 16), (16, 30), (4, 16), (4, 6)], GOLD)
    poly(cv, [(16, 5.5), (24.5, 8.4), (24.5, 15.4), (16, 26), (7.5, 15.4), (7.5, 8.4)], DARK)
    poly(cv, [(16, 9), (21.5, 18.5), (10.5, 18.5)], GOLD, rim=False)
    stroke(cv, [(16, 12), (16, 15.2)], DARK, 1.1)
    disc(cv, 16, 17, .7, col=DARK, hi=False)


def transit(cv):
    rect(cv, 3, 7, 29, 24, BLUE)
    rect(cv, 5, 10, 14, 17, SKY, rim=False)
    rect(cv, 18, 10, 27, 17, SKY, rim=False)
    rect(cv, 3, 19, 29, 21.2, GOLD, rim=False)
    stroke(cv, [(8, 7), (13, 2), (22, 2), (24, 7)], DARK, .9)
    for x in (9, 23):
        disc(cv, x, 26.5, 2.6, col=DARK, hi=False)
    stroke(cv, [(1.5, 29.5), (30.5, 29.5)], IRON, 1.2)


# ----------------------------------------------------------------- wonders

def pyramid(cv):
    poly(cv, [(1, 27), (16, 5), (31, 27)], SAND)
    poly(cv, [(16, 5), (31, 27), (16, 27)], shade_rgb(SAND, .76), rim=False)
    for y in (11.5, 17.5, 23):
        t = (y - 5) / 22
        stroke(cv, [(16 - 15 * t, y), (16 + 15 * t, y)], shade_rgb(SAND, .6), .6)
    disc(cv, 25.5, 6.5, 3, col=GOLD)
    stroke(cv, [(1, 28.5), (31, 28.5)], shade_rgb(SAND, .7), 1)


def statue(cv, torch=True, bolt=False):
    rect(cv, 9, 25, 23, 28.8, STONE)
    poly(cv, [(10, 25), (12.4, 14), (15, 14), (15.4, 25)], BRICK)
    poly(cv, [(17, 25), (16.6, 14), (19.6, 14), (22, 25)], shade_rgb(BRICK, .85))
    rect(cv, 11.5, 7, 20.5, 15, BRICK)
    disc(cv, 16, 4.6, 3, col=shade_rgb(BRICK, 1.15))
    stroke(cv, [(19.5, 9), (24, 5)], BRICK, 2.4)
    if torch:
        poly(cv, [(23, 4.8), (25.6, 5), (26.4, 1), (24, -.4), (22.8, 1.2)], GOLD)
    stroke(cv, [(11.5, 9), (8, 14)], BRICK, 2.4)
    if bolt:
        poly(cv, [(4, 2), (8, 2), (6.4, 6), (9, 6), (4.4, 12), (5.6, 7.4), (3, 7.4)], GOLD)


def lighthouse(cv):
    poly(cv, [(11, 28.5), (13, 9), (19, 9), (21, 28.5)], MARB)
    poly(cv, [(16, 9), (19, 9), (21, 28.5), (16, 28.5)], shade_rgb(MARB, .8), rim=False)
    for y0, y1 in ((14, 18), (22, 26)):
        t0, t1 = (y0 - 9) / 19, (y1 - 9) / 19
        poly(cv, [(13 - 2 * t0, y0), (19 + 2 * t0, y0), (19 + 2 * t1, y1), (13 - 2 * t1, y1)], RED, rim=False)
    rect(cv, 11.5, 7, 20.5, 9.4, STONE)
    rect(cv, 13, 3.4, 19, 7, GOLD)
    tri(cv, (12, 3.4), (16, -.2), (20, 3.4), RED)
    for sgn in (-1, 1):
        poly(cv, [(16 + sgn * 3, 4.4), (16 + sgn * 14, 1), (16 + sgn * 14, 8)], (255, 238, 150), rim=False)
    stroke(cv, [(1, 29.5), (6, 28.2), (11, 29.5)], BLUE, 1.3)
    stroke(cv, [(21, 29.5), (26, 28.2), (31, 29.5)], BLUE, 1.3)


def scroll(cv, col=PAPER):
    poly(cv, [(6, 6), (26, 6), (26, 24), (6, 24)], col)
    for y in (10, 13.5, 17, 20.5):
        stroke(cv, [(9.5, y), (22.5 - (y % 7), y)], shade_rgb(col, .55), .8)
    disc(cv, 6, 6, 2.6, 3, col=shade_rgb(col, .86))
    disc(cv, 26, 24, 2.6, 3, col=shade_rgb(col, .86))
    disc(cv, 6, 24, 2.6, 3, col=shade_rgb(col, .86))
    disc(cv, 26, 6, 2.6, 3, col=shade_rgb(col, .86))


def tripod(cv):
    for x0, x1 in ((16, 7), (16, 25), (16, 16)):
        stroke(cv, [(x0, 15), (x1, 28.5)], BRICK if x1 == 16 else shade_rgb(BRICK, .85), 1.8)
    poly(cv, [(7, 14), (25, 14), (22, 19), (10, 19)], GOLD)
    poly(cv, [(16, -.5), (20.4, 6.4), (19, 12), (16, 14), (13, 12), (11.6, 6.4), (14, 6.6)], RED)
    poly(cv, [(16, 5), (18.2, 9.4), (16, 13), (13.8, 9.4)], (255, 218, 90), rim=False)


def gardens(cv):
    for i, (x0, x1, y) in enumerate(((3, 29, 22), (6, 26, 15.5), (9, 23, 9))):
        rect(cv, x0, y, x1, y + 6.6, STONE)
        stroke(cv, [(x0 + .5, y + .5), (x1 - .5, y + .5)], GREEN, 1.8)
    for x, y in ((5, 20), (11, 13), (17, 6.4), (24, 13), (26.5, 19.8), (13, 20)):
        disc(cv, x, y - 1.5, 2.4, col=GREEN)
        stroke(cv, [(x, y - .4), (x, y + .6)], WOOD, 1.1)


def sunzi(cv):
    scroll(cv)
    stroke(cv, [(3, 28), (28, 3)], IRON, 2.2)
    stroke(cv, [(5.4, 25), (8.4, 28)], WOOD, 2)
    stroke(cv, [(4.4, 26.6), (7, 24)], GOLD, 1.6)


def dome(cv, col=GOLD, cross=True):
    rect(cv, 4, 22, 28, 28.6, STONE)
    rect(cv, 8, 14, 24, 22, MARB)
    for x in (10, 15, 20):
        rect(cv, x, 15.5, x + 2.4, 21, DARK, rim=False)
    half(cv, 16, 14, 10, 9, col)
    if cross:
        stroke(cv, [(16, -1), (16, 5.2)], GOLD, 1)
        stroke(cv, [(13.8, 1.4), (18.2, 1.4)], GOLD, 1)


def organ(cv):
    for i, h in enumerate((12, 17, 22, 26, 22, 17, 12)):
        x = 3 + i * 3.7
        rect(cv, x, 28.5 - h, x + 3, 28.5, GOLD if i != 3 else shade_rgb(GOLD, 1.14))
        cv.ellipse((x + .5, 28.5 - h - .6, x + 2.5, 28.5 - h + 1.2), DARK)
    rect(cv, 2, 26, 30, 29.5, WOOD)


def masks(cv):
    for cx, col, smile in ((11, GOLD, True), (21, SKY, False)):
        disc(cv, cx, 15, 8.6, 10.5, col=col)
        cv.ellipse((cx - 4.6, 11, cx - 1.8, 13.4), INK)
        cv.ellipse((cx + 1.8, 11, cx + 4.6, 13.4), INK)
        stroke(cv, arc(cx, 18.6 if smile else 22.4, 4.2, 3, 15 if smile else 195, 165 if smile else 345, 10), INK, 1.1)


def observatory(cv):
    rect(cv, 6, 17, 26, 28.6, STONE)
    half(cv, 16, 17, 10.5, 10, MARB)
    poly(cv, [(15, 7.6), (17, 7.6), (17.6, 17), (14.4, 17)], DARK, rim=False)
    stroke(cv, [(16.5, 10), (27, 1.5)], GOLD, 2.6)
    disc(cv, 28, 1.2, 1.6, col=SKY)
    rect(cv, 13, 21, 19, 28.6, DARK)


def apple(cv):
    disc(cv, 16, 18, 11, 10.4, col=RED)
    disc(cv, 12, 18, 6, 9, col=shade_rgb(RED, 1.1), hi=False)
    stroke(cv, [(16, 8.5), (16.6, 3.4)], WOOD, 1.4)
    poly(cv, [(17.4, 5.6), (23.4, 1.6), (25, 6), (19, 8)], GREEN)
    poly(cv, [(7, 27.4), (25, 27.4), (23, 30.6), (9, 30.6)], BLUE)


def ship(cv):
    poly(cv, [(3, 20), (29, 20), (25, 27), (7, 27)], WOOD)
    stroke(cv, [(4.4, 22.6), (27.6, 22.6)], shade_rgb(WOOD, .6), .6)
    stroke(cv, [(16, 3), (16, 20)], DARK, 1.2)
    poly(cv, [(15, 4), (4.6, 17.5), (15, 17.5)], WHITE)
    poly(cv, [(17.5, 6), (26.6, 17.5), (17.5, 17.5)], shade_rgb(WHITE, .88))
    poly(cv, [(16, 3), (21, 4.4), (16, 5.8)], RED, rim=False)
    stroke(cv, [(1, 29.5), (6, 28.3), (11, 29.5), (16, 28.3), (21, 29.5), (26, 28.3), (31, 29.5)], BLUE, 1.4)


def pouch(cv):
    poly(cv, [(11, 6), (21, 6), (19, 11), (25, 20), (25, 26), (23, 29), (9, 29), (7, 26), (7, 20), (13, 11)], WOOD)
    poly(cv, [(11, 6), (21, 6), (19, 11), (13, 11)], shade_rgb(WOOD, .8))
    stroke(cv, [(12.2, 11.6), (19.8, 11.6)], GOLD, 1.6)
    disc(cv, 16, 21, 4.2, col=GOLD)
    stroke(cv, [(16, 18), (16, 24)], shade_rgb(GOLD, .55), 1)


def ballot(cv):
    rect(cv, 5, 12, 27, 28.6, BLUE)
    stroke(cv, [(10, 12), (22, 12)], DARK, 2.6)
    poly(cv, [(10, 2), (22, 2), (22, 14), (10, 14)], WHITE)
    stroke(cv, [(12, 6), (20, 6)], shade_rgb(WHITE, .5), .7)
    stroke(cv, [(13, 9), (15, 11), (20, 5)], GREEN, 1.1)
    disc(cv, 16, 20, 3.6, col=GOLD)


def helix(cv):
    a, b = [], []
    for i in range(25):
        y = 2 + i * 1.1
        s = math.sin(i / 24 * math.pi * 3.2)
        a.append((16 + 8.5 * s, y))
        b.append((16 - 8.5 * s, y))
    for i in range(1, 25, 3):
        stroke(cv, [a[i], b[i]], PAPER, 1)
    stroke(cv, a, RED, 2)
    stroke(cv, b, BLUE, 2)


def globe(cv, grid=True):
    disc(cv, 16, 16, 12.5, col=BLUE)
    poly(cv, [(8, 10), (14, 6), (18, 11), (14, 15), (11, 20), (8, 17)], GREEN, rim=False)
    poly(cv, [(20, 15), (26, 14), (26, 21), (21, 24)], GREEN, rim=False)
    if grid:
        stroke(cv, arc(16, 16, 12.5, 12.5, 0, 360, 24), shade_rgb(BLUE, .6), .6)
        stroke(cv, arc(16, 16, 5.6, 12.5, 0, 360, 24), shade_rgb(BLUE, .6), .6)


def un(cv):
    globe(cv, grid=False)
    stroke(cv, arc(16, 16, 14.6, 14.6, 120, 240, 8), GREEN, 1.8)
    stroke(cv, arc(16, 16, 14.6, 14.6, -60, 60, 8), GREEN, 1.8)


def mushroom(cv):
    poly(cv, [(13, 28.5), (14.4, 16), (18.6, 16), (20, 28.5)], (120, 108, 100))
    disc(cv, 16, 11, 11.5, 7.5, col=(226, 112, 52))
    disc(cv, 8.4, 15.4, 4.6, 3.6, col=(210, 94, 44))
    disc(cv, 23.6, 15.4, 4.6, 3.6, col=(210, 94, 44))
    disc(cv, 16, 7.8, 5, 3.2, col=(255, 214, 108), hi=False)
    stroke(cv, [(2, 29), (30, 29)], DARK, 1.4)


def dish(cv):
    poly(cv, [(4, 6), (20, 2), (27, 17), (11, 22)], WHITE)
    poly(cv, [(4, 6), (20, 2), (27, 17), (11, 22)], WHITE)
    poly(cv, [(11, 22), (27, 17), (24, 12), (9, 14)], shade_rgb(WHITE, .76), rim=False)
    stroke(cv, [(15, 13), (24, 4.4)], IRON, 1.2)
    disc(cv, 24.4, 4, 1.6, col=RED)
    stroke(cv, [(16, 19), (13, 29)], IRON, 2)
    stroke(cv, [(16, 19), (21, 29)], IRON, 2)
    stroke(cv, [(10, 29), (24, 29)], IRON, 1.6)


def monitor(cv):
    rect(cv, 3, 4, 29, 22, DARK)
    poly(cv, [(5.5, 6.5), (26.5, 6.5), (26.5, 19.5), (5.5, 19.5)], SKY, rim=False)
    globe(Sub(cv, 16, 13, .43), grid=True)
    rect(cv, 13, 22, 19, 26, IRON)
    rect(cv, 8, 26, 24, 29, IRON)


class Sub:
    """A canvas view that draws a glyph scaled by `k` about (16, 16) at (cx, cy)."""

    def __init__(self, cv, cx, cy, k):
        self.cv, self.cx, self.cy, self.k = cv, cx, cy, k

    def _p(self, p):
        return (self.cx + (p[0] - 16) * self.k, self.cy + (p[1] - 16) * self.k)

    def poly(self, pts, fill, edge=None):
        self.cv.poly([self._p(p) for p in pts], fill, edge)

    def line(self, pts, fill, width=1.0):
        self.cv.line([self._p(p) for p in pts], fill, max(.5, width * self.k))

    def ellipse(self, box, fill):
        a, b = self._p(box[:2]), self._p(box[2:])
        self.cv.ellipse((a[0], a[1], b[0], b[1]), fill)


def shield_cross(cv):
    poly(cv, [(5, 3), (27, 3), (27, 17), (16, 30), (5, 17)], WHITE)
    poly(cv, [(16, 3), (27, 3), (27, 17), (16, 30)], shade_rgb(WHITE, .84), rim=False)
    rect(cv, 13.4, 5, 18.6, 25, RED, rim=False)
    rect(cv, 7.4, 10, 24.6, 15.2, RED, rim=False)


def tomb(cv):
    rect(cv, 3, 24, 29, 28.6, STONE)
    rect(cv, 6, 20, 26, 24, shade_rgb(STONE, 1.1))
    columns(cv, (7.5, 12, 16.5, 21), 12, 20, MARB, 2.6)
    rect(cv, 6, 9.5, 26, 12.5, MARB)
    poly(cv, [(7, 9.5), (16, 0.5), (25, 9.5)], SAND)
    poly(cv, [(16, .5), (25, 9.5), (16, 9.5)], shade_rgb(SAND, .78), rim=False)


def bow(cv):
    ring(cv, 14, 16, 12.5, WOOD, 2.2, a0=-70, a1=70, n=14)
    stroke(cv, [(14 + 12.5 * math.cos(math.radians(-70)), 16 + 12.5 * math.sin(math.radians(-70))),
                (14 + 12.5 * math.cos(math.radians(70)), 16 + 12.5 * math.sin(math.radians(70)))], PAPER, .7)
    stroke(cv, [(6, 16), (29, 16)], WOOD, 1.2)
    arrow_head(cv, (30.5, 16), 0, IRON, 4)
    poly(cv, [(6, 16), (3, 13), (4.4, 16), (3, 19)], RED, rim=False)
    disc(cv, 6, 5, 3.4, col=(240, 236, 210))
    disc(cv, 7.4, 4.2, 2.8, col=(22, 20, 28), hi=False)


def pagoda(cv):
    rect(cv, 10, 22, 22, 28.6, BRICK)
    for i, (y, w) in enumerate(((20, 14), (13, 11.5), (6.4, 9))):
        poly(cv, [(16 - w, y + 1.6), (16 - w + 2, y - .8), (16 + w - 2, y - .8), (16 + w, y + 1.6),
                  (16 + w - 3, y + 3), (16 - w + 3, y + 3)], RED if i != 1 else shade_rgb(RED, 1.12))
        if i < 2:
            rect(cv, 16 - w + 4, y + 3, 16 + w - 4, y + 7, MARB)
    stroke(cv, [(16, 1), (16, 5)], GOLD, 1)
    rect(cv, 13.4, 23.5, 18.6, 28.6, DARK)


def laurel(cv, star=True):
    for sgn in (-1, 1):
        for i in range(7):
            a = math.radians(100 + i * 20) if sgn < 0 else math.radians(80 - i * 20)
            cx, cy = 16 + 11.5 * math.cos(a), 15 + 12.5 * math.sin(a)
            poly(cv, [(cx - 2.2 * sgn, cy - 2.4), (cx + 2.6 * sgn, cy - .8), (cx - .4 * sgn, cy + 2.4)], GREEN)
    if star:
        pts = []
        for i in range(10):
            r = 6.6 if i % 2 == 0 else 2.9
            a = math.radians(-90 + i * 36)
            pts.append((16 + r * math.cos(a), 15 + r * math.sin(a)))
        poly(cv, pts, GOLD)


def academy(cv):
    poly(cv, [(10, 16), (16, 29), (22, 16)], RED)
    poly(cv, [(10, 16), (6, 29), (13, 24.5)], BLUE)
    poly(cv, [(22, 16), (26, 29), (19, 24.5)], BLUE)
    disc(cv, 16, 11.5, 9.4, col=GOLD)
    pts = []
    for i in range(10):
        r = 6.2 if i % 2 == 0 else 2.8
        a = math.radians(-90 + i * 36)
        pts.append((16 + r * math.cos(a), 11.8 + r * math.sin(a)))
    poly(cv, pts, WHITE, rim=False)


def magnifier(cv):
    stroke(cv, [(20, 20), (28.6, 28.6)], WOOD, 3.6)
    disc(cv, 13, 13, 9.6, col=(214, 232, 240))
    ring(cv, 13, 13, 9.4, IRON, 2.2)
    cv.ellipse((7, 7, 12, 10.4), WHITE)
    disc(cv, 14, 14, 3.2, col=BLUE, hi=False)
    disc(cv, 14, 14, 1.4, col=DARK, hi=False)


def rocket(cv):
    poly(cv, [(16, 1), (21, 9), (21, 22), (11, 22), (11, 9)], WHITE)
    poly(cv, [(16, 1), (21, 9), (21, 22), (16, 22)], shade_rgb(WHITE, .82), rim=False)
    poly(cv, [(16, 1), (20.4, 8), (11.6, 8)], RED)
    disc(cv, 16, 13, 2.8, col=SKY)
    poly(cv, [(11, 16), (6, 24), (11, 22)], RED)
    poly(cv, [(21, 16), (26, 24), (21, 22)], shade_rgb(RED, .85))
    poly(cv, [(12, 22), (20, 22), (18, 30.6), (14, 30.6)], GOLD)
    poly(cv, [(14, 22), (18, 22), (16, 28)], (255, 240, 170), rim=False)


def chart(cv):
    for i, h in enumerate((7, 11, 9, 16, 20)):
        x = 3 + i * 5.4
        rect(cv, x, 29 - h, x + 4, 29, GREEN if i != 4 else GOLD)
    stroke(cv, [(3, 18), (10, 13), (15, 15), (22, 7), (27, 4)], RED, 1.7)
    arrow_head(cv, (30, 2.4), -35, RED, 4.4)


def anvil(cv):
    poly(cv, [(2, 8), (28, 8), (22, 14), (20, 16), (20, 20), (24, 24), (24, 27), (8, 27), (8, 24), (12, 20), (12, 16), (9, 14), (4, 12)], DARK)
    poly(cv, [(2, 8), (28, 8), (24, 11), (10, 11)], IRON)
    poly(cv, [(28, 8), (31, 6), (28, 11)], DARK, rim=False)
    for x, y in ((6, 2), (10, 1), (4, 5)):
        stroke(cv, [(x, y), (x - 1.5, y - 1.5)], GOLD, 1)


def sdi(cv):
    stroke(cv, arc(16, 22, 14, 18, 190, 350, 14), SKY, 1.8)
    rocket_k = Sub(cv, 16, 21, .6)
    rocket(rocket_k)
    stroke(cv, [(1, 28.5), (31, 28.5)], DARK, 1.5)


def crown(cv):
    poly(cv, [(4, 24), (3, 10), (10, 17), (16, 6), (22, 17), (29, 10), (28, 24)], GOLD)
    poly(cv, [(16, 6), (22, 17), (29, 10), (28, 24), (16, 24)], shade_rgb(GOLD, .84), rim=False)
    rect(cv, 4, 24, 28, 28, shade_rgb(GOLD, .9))
    for x, c in ((10, RED), (16, BLUE), (22, GREEN)):
        disc(cv, x, 21, 1.9, col=c, hi=False)


# ---------------------------------------------------------- advances (extra)

def wheel(cv):
    ring(cv, 16, 16, 11.5, WOOD, 3.2)
    for k in range(8):
        a = math.radians(k * 45)
        stroke(cv, [(16, 16), (16 + 11 * math.cos(a), 16 + 11 * math.sin(a))], shade_rgb(WOOD, 1.1), 1.4)
    disc(cv, 16, 16, 3.4, col=shade_rgb(WOOD, 1.2))
    disc(cv, 16, 16, 1.2, col=DARK, hi=False)


def pot(cv):
    poly(cv, [(11, 4), (21, 4), (20, 9), (26, 16), (24, 26), (20, 29.5), (12, 29.5), (8, 26), (6, 16), (12, 9)], BRICK)
    poly(cv, [(16, 4), (21, 4), (20, 9), (26, 16), (24, 26), (20, 29.5), (16, 29.5)], shade_rgb(BRICK, .82), rim=False)
    stroke(cv, [(8, 17), (24, 17)], GOLD, 1.3)
    stroke(cv, [(8.4, 22), (23.6, 22)], GOLD, 1.3)
    cv.ellipse((10.8, 2.4, 21.2, 5.6), shade_rgb(BRICK, .5))


def ingot(cv, col=(204, 134, 70)):
    for x, y in ((3, 18), (14, 18), (8.5, 8.5)):
        poly(cv, [(x, y + 8), (x + 2, y), (x + 12, y), (x + 14, y + 8)], col)
        poly(cv, [(x + 2, y), (x + 12, y), (x + 11, y + 2.2), (x + 3, y + 2.2)], shade_rgb(col, 1.3), rim=False)


def letter(cv, ch="A"):
    poly(cv, [(4, 3), (28, 3), (28, 29), (4, 29)], PAPER)
    f = {"A": [[(9, 25), (16, 7), (23, 25)], [(11.6, 19), (20.4, 19)]],
         "T": [[(9, 8), (23, 8)], [(16, 8), (16, 25)]],
         "W": [[(8, 8), (11.5, 25), (16, 13), (20.5, 25), (24, 8)]]}[ch]
    for line in f:
        stroke(cv, line, DARK, 2.3)


def quill_scroll(cv):
    scroll(cv)
    poly(cv, [(18, 29), (30, 4), (32, 6), (23, 22)], WHITE)
    stroke(cv, [(18, 29.5), (22, 22)], DARK, 1)


def star_eye(cv):
    pts = []
    for i in range(16):
        r = 14 if i % 2 == 0 else 5.6
        a = math.radians(-90 + i * 22.5)
        pts.append((16 + r * math.cos(a), 16 + r * math.sin(a)))
    poly(cv, pts, PURPLE)
    disc(cv, 16, 16, 6.4, 4.6, col=WHITE)
    disc(cv, 16, 16, 3, col=BLUE, hi=False)
    disc(cv, 16, 16, 1.3, col=DARK, hi=False)


def bricks(cv):
    for r in range(4):
        for c in range(3):
            x = 2 + c * 9.6 + (4.8 if r % 2 else 0)
            if x + 9 > 31:
                continue
            rect(cv, x, 4 + r * 6.6, x + 8.6, 4 + r * 6.6 + 5.8, shade_rgb(STONE, 1.12 - .06 * ((r + c) % 3)))
    rect(cv, 2, 4 + 6.6, 6.5, 4 + 6.6 + 5.8, shade_rgb(STONE, 1.0))


def axe(cv):
    stroke(cv, [(7, 29), (23, 5)], WOOD, 2.8)
    poly(cv, [(17, 3), (27, 3), (30, 10), (22, 14), (19, 10)], IRON)
    poly(cv, [(23, 3.4), (27, 3.4), (30, 10), (24, 13)], shade_rgb(IRON, .8), rim=False)


def urn(cv):
    pot(cv)
    for dx in (-1, 1):
        stroke(cv, arc(16 + dx * 10, 14, 4, 5, 90 if dx > 0 else 270, 270 if dx > 0 else 450, 8), BRICK, 1.6)


def horse_head(cv):
    poly(cv, [(8, 29), (10, 14), (13, 6), (20, 2), (22, 6), (20, 8), (26, 15), (28, 21), (24, 23), (20, 18), (18, 29)], BRICK)
    poly(cv, [(8, 29), (10, 14), (13, 6), (16, 5), (16, 29)], DARK, rim=False)
    poly(cv, [(20, 2), (22.4, 0), (22.8, 5)], BRICK)
    disc(cv, 19, 9, 1.1, col=WHITE, hi=False)


def map_scroll(cv):
    poly(cv, [(3, 6), (11, 4), (21, 7), (29, 5), (29, 26), (21, 28), (11, 25), (3, 27)], PAPER)
    stroke(cv, [(11, 4), (11, 25)], shade_rgb(PAPER, .6), .6)
    stroke(cv, [(21, 7), (21, 28)], shade_rgb(PAPER, .6), .6)
    stroke(cv, [(5, 20), (9, 16), (14, 19), (19, 11), (25, 14)], RED, 1.2)
    poly(cv, [(23, 20), (26, 22), (23, 24), (20, 22)], BLUE, rim=False)


def abacus(cv):
    rect(cv, 3, 4, 29, 28, WOOD)
    rect(cv, 5, 6, 27, 26, PAPER, rim=False)
    for r in range(4):
        y = 9 + r * 5.2
        stroke(cv, [(5, y), (27, y)], DARK, .8)
        for k in range(4):
            x = 8 + k * 3.6 + (9 if (r + k) % 2 else 0)
            disc(cv, x, y, 1.9, col=RED if r % 2 else BLUE, hi=False)


def head_bust(cv):
    poly(cv, [(7, 29), (9, 20), (14, 18), (18, 18), (23, 20), (25, 29)], MARB)
    poly(cv, [(12, 8), (14, 3), (19, 3), (21, 8), (21, 15), (18, 18), (14, 18), (12, 15)], MARB)
    poly(cv, [(19, 3), (21, 8), (21, 15), (18, 18), (18, 3)], shade_rgb(MARB, .8), rim=False)
    stroke(cv, [(13.4, 8.4), (16.4, 8.4)], DARK, .9)
    stroke(cv, [(15, 11), (15, 14)], shade_rgb(MARB, .5), .9)


def lance(cv):
    stroke(cv, [(4, 29), (24, 5)], WOOD, 2.2)
    poly(cv, [(22, 9), (30, 0), (28, 10)], IRON)
    poly(cv, [(8, 14), (14, 12), (13, 6), (7, 8)], RED)
    poly(cv, [(8, 14), (14, 12), (14, 18), (8, 20)], WHITE)


def helmet(cv):
    half(cv, 16, 18, 11, 12, IRON)
    rect(cv, 5, 18, 27, 25, IRON)
    stroke(cv, [(16, 18), (16, 25)], DARK, 1.4)
    stroke(cv, [(9, 21.4), (23, 21.4)], DARK, 1.6)
    poly(cv, [(14, 7), (18, 7), (22, 0), (10, 0)], RED)


def barrel(cv):
    poly(cv, [(8, 5), (24, 5), (27, 17), (24, 28), (8, 28), (5, 17)], WOOD)
    poly(cv, [(16, 5), (24, 5), (27, 17), (24, 28), (16, 28)], shade_rgb(WOOD, .8), rim=False)
    for y in (9, 24):
        stroke(cv, [(6.6, y), (25.4, y)], IRON, 1.5)
    poly(cv, [(14, 2), (18, 2), (18, 5), (14, 5)], DARK, rim=False)
    stroke(cv, [(16, 2), (20, -1), (24, 1)], GOLD, 1.1)
    disc(cv, 25, 1.4, 1.8, col=RED, hi=False)


def bulb(cv):
    disc(cv, 16, 12, 9.4, col=(255, 236, 130))
    poly(cv, [(11, 19), (21, 19), (19.4, 24), (12.6, 24)], (255, 236, 130))
    rect(cv, 12, 24, 20, 29, IRON)
    stroke(cv, [(12.6, 24), (14, 15), (16, 18), (18, 15), (19.4, 24)], DARK, .8)
    for a in (-150, -110, -70, -30):
        r = math.radians(a)
        stroke(cv, [(16 + 12 * math.cos(r), 12 + 12 * math.sin(r)), (16 + 15 * math.cos(r), 12 + 15 * math.sin(r))], GOLD, .9)


def locomotive(cv):
    rect(cv, 3, 12, 20, 24, DARK)
    poly(cv, [(3, 14), (20, 14), (20, 17), (3, 17)], RED, rim=False)
    rect(cv, 16, 4, 28, 24, BLUE)
    rect(cv, 18.4, 7, 25.6, 13, SKY, rim=False)
    rect(cv, 4, 6, 8, 12, DARK)
    disc(cv, 6, 3.4, 2.6, col=(176, 176, 180), hi=False)
    for x, r in ((7, 3), (14, 3), (23, 3.4)):
        disc(cv, x, 25.2, r, col=IRON)
    stroke(cv, [(1, 29), (31, 29)], DARK, 1.2)


def tank(cv):
    poly(cv, [(2, 22), (4, 17), (24, 17), (28, 22), (26, 27), (4, 27)], GREEN)
    poly(cv, [(8, 17), (10, 10), (20, 10), (22, 17)], shade_rgb(GREEN, 1.12))
    stroke(cv, [(20, 13.5), (31, 13.5)], DARK, 2)
    for x in (7, 12, 17, 22):
        disc(cv, x, 24.4, 2.3, col=DARK, hi=False)


def plane(cv):
    airport(cv)


def radio(cv):
    rect(cv, 4, 12, 28, 28, WOOD)
    disc(cv, 11, 20, 5.2, col=PAPER)
    disc(cv, 11, 20, 2.2, col=DARK, hi=False)
    for y in (16, 19.4, 22.8):
        stroke(cv, [(18, y), (26, y)], DARK, .9)
    stroke(cv, [(22, 12), (27, 2)], IRON, 1.1)
    for r in (3, 5.6):
        stroke(cv, arc(27, 2, r, r, 20, 150, 6), GOLD, .9)


def chip(cv):
    rect(cv, 8, 8, 24, 24, DARK)
    rect(cv, 11, 11, 21, 21, shade_rgb(DARK, 1.6))
    for i in range(4):
        v = 9.4 + i * 4.2
        stroke(cv, [(v, 8), (v, 3.6)], GOLD, 1.5)
        stroke(cv, [(v, 24), (v, 28.4)], GOLD, 1.5)
        stroke(cv, [(8, v), (3.6, v)], GOLD, 1.5)
        stroke(cv, [(24, v), (28.4, v)], GOLD, 1.5)


def leaf(cv):
    poly(cv, [(5, 27), (6, 12), (16, 3), (27, 5), (26, 17), (16, 26)], GREEN)
    poly(cv, [(16, 3), (27, 5), (26, 17), (16, 26), (14, 14)], shade_rgb(GREEN, .82), rim=False)
    stroke(cv, [(5, 28), (24, 7)], shade_rgb(GREEN, .45), 1.2)


def cross_tech(cv):
    rect(cv, 13, 3, 19, 29, MARB)
    rect(cv, 6, 10, 26, 15.5, MARB)
    disc(cv, 16, 12.6, 2, col=GOLD)


def castle(cv):
    rect(cv, 5, 12, 27, 28.6, STONE)
    for x in (5, 11, 17, 23):
        rect(cv, x, 8, x + 4, 12, STONE)
    rect(cv, 2, 6, 9, 28.6, shade_rgb(STONE, 1.08))
    rect(cv, 23, 6, 30, 28.6, shade_rgb(STONE, 1.08))
    for x in (2, 5.6):
        rect(cv, x, 3, x + 2.6, 6, STONE)
    for x in (23, 26.6):
        rect(cv, x, 3, x + 2.6, 6, STONE)
    poly(cv, [(12.4, 28.6), (12.4, 21)] + arc(16, 21, 3.6, 4.4, 180, 360, 8) + [(19.6, 28.6)], DARK, rim=False)


def goblet(cv):
    poly(cv, [(8, 4), (24, 4), (22, 13), (19, 17), (13, 17), (10, 13)], GOLD)
    poly(cv, [(16, 4), (24, 4), (22, 13), (19, 17), (16, 17)], shade_rgb(GOLD, .8), rim=False)
    rect(cv, 14.4, 17, 17.6, 24, GOLD)
    rect(cv, 9, 24, 23, 28, GOLD)
    poly(cv, [(10, 5.6), (22, 5.6), (21.2, 9), (10.8, 9)], RED, rim=False)


def pill_cross(cv):
    rect(cv, 4, 6, 28, 28, WHITE)
    rect(cv, 13, 9, 19, 25, GREEN, rim=False)
    rect(cv, 7, 14.5, 25, 20.5, GREEN, rim=False)


def sewer(cv):
    disc(cv, 16, 16, 12.4, col=IRON)
    ring(cv, 16, 16, 9.4, DARK, 1)
    for k in range(4):
        a = math.radians(k * 45)
        stroke(cv, [(16 - 9 * math.cos(a), 16 - 9 * math.sin(a)), (16 + 9 * math.cos(a), 16 + 9 * math.sin(a))], DARK, 1.2)
    poly(cv, [(16, 6), (19, 12), (16, 14), (13, 12)], SKY, rim=False)


def bolt(cv):
    poly(cv, [(19, 1), (6, 17), (14, 17), (11, 31), (26, 12), (17, 12)], GOLD)
    poly(cv, [(19, 1), (17, 12), (26, 12), (11, 31)], shade_rgb(GOLD, .82), rim=False)


def satellite(cv):
    rect(cv, 12, 12, 20, 20, GOLD)
    for sgn in (-1, 1):
        rect(cv, 16 + sgn * 11 - 4.4, 11, 16 + sgn * 11 + 4.4, 21, BLUE)
        stroke(cv, [(16 + sgn * 4, 16), (16 + sgn * 6.6, 16)], IRON, 1)
    stroke(cv, [(16, 12), (16, 6)], IRON, 1)
    disc(cv, 16, 5, 2, col=RED, hi=False)


def fabric(cv):
    for k, c in enumerate((PURPLE, SKY, PAPER)):
        poly(cv, [(4 + k * 3, 6 + k * 3), (26 + k * 3, 4 + k * 3), (28 + k * 3, 14 + k * 3), (6 + k * 3, 17 + k * 3)], c)
    stroke(cv, [(10, 21), (30, 18)], shade_rgb(PURPLE, .6), .9)


def stealth(cv):
    poly(cv, [(16, 2), (30, 22), (22, 20), (16, 27), (10, 20), (2, 22)], DARK)
    poly(cv, [(16, 2), (30, 22), (22, 20), (16, 27)], shade_rgb(DARK, 1.5), rim=False)
    stroke(cv, [(16, 2), (16, 22)], shade_rgb(DARK, 2.1), .8)


def money(cv):
    disc(cv, 16, 16, 12.4, col=GOLD)
    ring(cv, 16, 16, 9.2, shade_rgb(GOLD, .62), .9)
    stroke(cv, [(19.6, 10.8), (13.4, 10.8), (13.4, 15.6), (18.6, 15.6), (18.6, 21), (12, 21)], shade_rgb(GOLD, .5), 1.5)
    stroke(cv, [(16, 8.6), (16, 23.6)], shade_rgb(GOLD, .5), 1.1)


def compass(cv):
    disc(cv, 16, 16, 13, col=PAPER)
    ring(cv, 16, 16, 12.6, WOOD, 1.8)
    poly(cv, [(16, 4), (19, 16), (16, 28), (13, 16)], WHITE, rim=True)
    poly(cv, [(16, 4), (19, 16), (13, 16)], RED)
    poly(cv, [(4, 16), (16, 13), (28, 16), (16, 19)], shade_rgb(PAPER, .8), rim=True)
    disc(cv, 16, 16, 1.6, col=GOLD, hi=False)


# -------------------------------------------------------------- city icons
# Icons for `CityIcons.png` (30 px cells).

def i_coin(cv):
    disc(cv, 16, 16, 13, col=GOLD)
    ring(cv, 16, 16, 9.6, shade_rgb(GOLD, .62), 1)
    stroke(cv, [(20.4, 11.6), (13.6, 11.6), (12.6, 16), (19, 16), (18.2, 20.6), (11.4, 20.6)], shade_rgb(GOLD, .5), 1.6)
    stroke(cv, [(16, 8.4), (16, 24)], shade_rgb(GOLD, .5), 1.2)


def i_shield(cv, col=(176, 70, 52)):
    poly(cv, [(5, 4), (27, 4), (27, 16), (16, 29.5), (5, 16)], col)
    poly(cv, [(16, 4), (27, 4), (27, 16), (16, 29.5)], shade_rgb(col, .8), rim=False)
    poly(cv, [(8.4, 7.4), (15, 7.4), (15, 15), (8.4, 15)], shade_rgb(col, 1.22), rim=False)
    stroke(cv, [(16, 4), (16, 29)], GOLD, 1.5)
    stroke(cv, [(5.5, 11), (26.5, 11)], GOLD, 1.5)


def i_shield_box(cv):
    poly(cv, [(5, 4), (27, 4), (27, 16), (16, 29.5), (5, 16)], (186, 184, 190))
    poly(cv, [(8, 7), (24, 7), (24, 15), (16, 25.4), (8, 15)], (112, 110, 118), rim=False)


def i_food(cv):
    stroke(cv, [(16, 29), (16, 12)], (96, 140, 60), 1.8)
    for k in range(4):
        y = 12 - k * 2.4
        poly(cv, [(16, y + 2.6), (11, y - .6), (14.2, y - 2.6)], GOLD)
        poly(cv, [(16, y + 2.6), (21, y - .6), (17.8, y - 2.6)], shade_rgb(GOLD, .86))
    poly(cv, [(16, 3.4), (14.6, 7.4), (17.4, 7.4)], GOLD)
    poly(cv, [(16, 24), (9, 19), (12, 19)], GREEN)
    poly(cv, [(16, 24), (23, 19), (20, 19)], GREEN)


def i_food_box(cv):
    rect(cv, 3, 3, 29, 29, (186, 160, 112))
    rect(cv, 5.4, 5.4, 26.6, 26.6, (126, 100, 62), rim=False)
    Sub_ = Sub(cv, 16, 16, .72)
    i_food(Sub_)


def i_box(cv):
    rect(cv, 3, 3, 29, 29, (186, 160, 112))
    rect(cv, 5.4, 5.4, 26.6, 26.6, (112, 88, 54), rim=False)


def i_flask(cv):
    flask(cv, (88, 150, 226))


def i_upkeep(cv):
    i_coin(Sub(cv, 14, 13, .8))
    rect(cv, 17, 22, 30, 27.5, RED)


def i_culture(cv):
    columns(cv, (6, 11.4, 16.8, 22.2), 11, 25, MARB, 3.4)
    rect(cv, 3, 25, 29, 28.6, STONE)
    rect(cv, 4, 7.6, 28, 11, MARB)
    poly(cv, [(2, 7.6), (16, 1), (30, 7.6)], PURPLE)


def face(cv, kind):
    cols = {"happy": (252, 214, 70), "content": (226, 200, 150), "unhappy": (130, 168, 220), "angry": (214, 76, 62),
            "star": (252, 214, 70)}
    disc(cv, 16, 16, 13, col=cols[kind])
    cv.ellipse((10, 10.4, 13.6, 14.6), INK)
    cv.ellipse((18.4, 10.4, 22, 14.6), INK)
    if kind == "happy":
        stroke(cv, arc(16, 15.5, 7, 7.6, 20, 160, 10), INK, 1.5)
    elif kind == "content":
        stroke(cv, [(10.4, 21), (21.6, 21)], INK, 1.5)
    elif kind in ("unhappy", "angry"):
        stroke(cv, arc(16, 25, 6.4, 6, 200, 340, 10), INK, 1.5)
        if kind == "angry":
            stroke(cv, [(8.6, 8.2), (13.6, 10.4)], INK, 1.3)
            stroke(cv, [(23.4, 8.2), (18.4, 10.4)], INK, 1.3)


def i_happy(cv):
    face(cv, "happy")


def i_content(cv):
    face(cv, "content")


def i_unhappy(cv):
    face(cv, "unhappy")


def i_angry(cv):
    face(cv, "angry")


def i_treasury(cv):
    pouch(cv)


def i_trade(cv):
    stroke(cv, [(3, 11), (24, 11)], BLUE, 3)
    arrow_head(cv, (30, 11), 0, BLUE, 7)
    stroke(cv, [(29, 21), (8, 21)], GOLD, 3)
    arrow_head(cv, (2, 21), 180, GOLD, 7)


def i_pollution(cv):
    for (cx, cy, rx, ry) in ((10, 20, 8, 6), (21, 18, 8.4, 7), (16, 11, 7.6, 6.6)):
        disc(cv, cx, cy, rx, ry, col=(104, 100, 96))
    for x in (9, 16, 23):
        stroke(cv, [(x, 24), (x, 29)], (84, 80, 76), 1.4)


def i_corrupt(cv):
    i_coin(cv)
    stroke(cv, [(10, 4), (14, 12), (11, 17), (17, 22), (14, 30)], DARK, 1.5)


def i_sword(cv):
    barracks(cv)


def i_celebrate(cv):
    pts = []
    for i in range(10):
        r = 14 if i % 2 == 0 else 6
        a = math.radians(-90 + i * 36)
        pts.append((16 + r * math.cos(a), 16.5 + r * math.sin(a)))
    poly(cv, pts, GOLD)
    disc(cv, 16, 16.5, 4.4, col=RED)


def i_disorder(cv):
    disc(cv, 16, 16, 13, col=RED)
    stroke(cv, [(9, 9), (23, 23)], WHITE, 3.4)
    stroke(cv, [(9, 23), (23, 9)], WHITE, 3.4)


def i_hungry(cv):
    i_food(cv)
    stroke(cv, [(3, 28), (29, 4)], RED, 2.6)


def i_lux(cv):
    goblet(cv)


def i_science(cv):
    scroll(cv)


# ------------------------------------------------------------------ lookup

BUILDINGS = {
    "Palace": palace, "Barracks": barracks, "Granary": granary, "Temple": temple, "Marketplace": marketplace,
    "Library": library, "Courthouse": courthouse, "Aqueduct": aqueduct, "Cathedral": cathedral,
    "University": university, "Bank": bank, "Colosseum": colosseum, "Factory": factory, "Hospital": hospital,
    "Harbor": harbor, "Airport": airport, "Police_Station": badge, "Recycling_Center": recycling,
    "Coal_Plant": lambda cv: plant(cv, fuel=DARK), "Hydro_Plant": hydro, "Solar_Plant": solar,
    "Nuclear_Plant": atom, "Manufacturing_Plant": gear, "Research_Lab": flask, "Commercial_Dock": dock,
    "Civil_Defense": defense, "Mass_Transit": transit,
    "Pyramids": pyramid, "Colossus": statue, "Great_Lighthouse": lighthouse,
    "Great_Library": lambda cv: temple(cv, (226, 232, 246), BLUE), "Oracle": tripod,
    "Hanging_Gardens": gardens, "Sun_Tzus_Art_of_War": sunzi, "Leonardos_Workshop": gear,
    "Sistine_Chapel": dome, "JS_Bachs_Cathedral": organ, "Shakespeares_Theater": masks,
    "Copernicus_Observatory": observatory, "Newton_University": apple, "Magellans_Voyage": ship,
    "Smiths_Trading_Company": pouch, "Universal_Suffrage": ballot, "Theory_of_Evolution": helix,
    "Hoover_Dam": hydro, "United_Nations": un, "Manhattan_Project": mushroom, "SETI_Program": dish,
    "Internet": monitor, "Statue_of_Zeus": lambda cv: statue(cv, torch=False, bolt=True),
    "Knights_Templar": shield_cross, "Mausoleum_of_Mausollos": tomb, "Temple_of_Artemis": bow,
    "Forbidden_Palace": pagoda, "Heroic_Epic": laurel, "Military_Academy": academy,
    "Intelligence_Agency": magnifier, "Apollo_Program": rocket, "Wall_Street": chart,
    "Iron_Works": anvil, "Strategic_Missile_Defense": sdi,
}

TECHS = {
    "Bronze_Working": ingot, "Alphabet": lambda cv: letter(cv, "A"), "Pottery": pot, "The_Wheel": wheel, "Wheel": wheel,
    "Warrior_Code": axe, "Ceremonial_Burial": urn, "Masonry": bricks, "Writing": quill_scroll, "Mysticism": star_eye,
    "Iron_Working": anvil, "Mathematics": abacus, "Philosophy": head_bust, "Code_of_Laws": courthouse,
    "Literature": library, "Map_Making": map_scroll, "Horseback_Riding": horse_head, "Polytheism": statue,
    "Currency": money, "Construction": aqueduct, "Monarchy": crown, "Republic": laurel, "The_Republic": laurel,
    "Feudalism": castle, "Monotheism": cross_tech, "Engineering": gear, "Theology": dome, "Chivalry": lance,
    "Invention": bulb, "Gunpowder": barrel, "Education": university, "Banking": bank, "Astronomy": observatory,
    "Chemistry": flask, "Printing_Press": library, "Democracy": ballot, "Economics": chart, "Navigation": compass,
    "Metallurgy": helmet, "Military_Tradition": academy, "Physics": apple, "Theory_of_Gravity": apple,
    "Magnetism": compass, "Steam_Power": locomotive, "Nationalism": shield_cross, "Industrialization": factory,
    "Electricity": bolt, "Medicine": pill_cross, "Communism": lambda cv: badge(cv, RED), "Sanitation": sewer,
    "Replaceable_Parts": gear, "Scientific_Method": flask, "The_Corporation": bank, "Corporation": bank,
    "Refining": lambda cv: plant(cv, fuel=DARK), "Steel": ingot, "Combustion": plant, "Flight": plane,
    "Mass_Production": factory, "Motorized_Transportation": tank, "Advanced_Flight": plane, "Amphibious_War": ship,
    "Radio": radio, "Atomic_Theory": atom, "Electronics": chip, "Computers": monitor, "Ecology": leaf,
    "Fission": mushroom, "Nuclear_Power": atom, "Rocketry": rocket, "Space_Flight": rocket, "Satellites": satellite,
    "Synthetic_Fibers": fabric, "Superconductor": bolt, "Stealth": stealth, "Miniaturization": chip, "Robotics": gear,
    "Recycling": recycling, "Integrated_Defense": sdi, "Future_Tech": star_eye,
}

# CityIcons cell -> drawing (the engine uses 2, 4, 6, 8, 9, 16-19, 23, 24).
CITY_ICONS = [i_content, i_unhappy, i_coin, i_trade, i_shield, i_sword, i_food, i_hungry, i_shield_box, i_box,
              i_pollution, i_corrupt, i_angry, i_lux, i_science, i_celebrate, i_flask, i_upkeep, i_culture,
              i_happy, i_disorder, i_unhappy, i_science, i_food_box, i_treasury]


def rgba(fn, size=SZ):
    """One glyph as an RGBA image of `size` px: outlined, hard alpha. The
    canvas is drawn at a scale that keeps the outline about one icon-pixel."""
    scale = max(SC, math.ceil(size / SZ * 2))
    cv = iso.Canvas((SZ, SZ), scale)
    fn(cv)
    img, _ = cv.done((size, size))
    img = gfx.outline(img, INK, max(1, round(size / SZ / 1.6)))
    img.putalpha(img.getchannel("A").point([255 if i >= 128 else 0 for i in range(256)]))
    return img


def render(fn, size=SZ):
    """(RGB on the key colour, alpha) of one glyph, for the indexed sheets."""
    img = rgba(fn, size)
    alpha = img.getchannel("A")
    rgb = Image.new("RGB", img.size, KEY)
    rgb.paste(img.convert("RGB"), mask=alpha)
    return rgb, alpha


def fallback(tag):
    pool = [gear, flask, scroll, star_eye, bricks, wheel, compass, crown, globe, laurel]
    return pool[int.from_bytes(hashlib.sha256(tag.encode()).digest()[:2], "big") % len(pool)]


def for_building(entry):
    key = entry[5:] if entry.startswith("BLDG_") else entry
    key = key[4:] if key.startswith("The_") else key
    return BUILDINGS.get(key, fallback(entry))


def for_tech(entry):
    key = entry[5:] if entry.startswith("TECH_") else entry
    return TECHS.get(key, fallback(entry))
