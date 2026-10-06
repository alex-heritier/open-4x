"""Busts: population heads, advisor portraits and leaders.

All heads are `figures.Scene` models seen from the front, so they share the
unit sprites' shading. `render(face, w, h, ...)` returns an RGBA image.
"""
import colorsys
import hashlib
import math

from PIL import Image, ImageDraw

from . import figures as F

SKINS = [(246, 212, 176), (226, 178, 132), (186, 132, 92), (132, 90, 64), (238, 196, 150), (206, 156, 108)]
HAIRS = [(58, 42, 34), (96, 62, 38), (30, 26, 28), (180, 140, 76), (150, 150, 154), (122, 56, 36)]
STEEL = (176, 184, 196)
GOLD = (238, 198, 74)
CLOTH = (236, 232, 222)
INK = (30, 24, 26)
WHITE = (246, 244, 238)


def _h(tag, salt):
    return int.from_bytes(hashlib.sha256((tag + salt).encode()).digest()[:4], "big")


def _hue(tag, salt, s=0.55, v=0.78):
    r, g, b = colorsys.hsv_to_rgb((_h(tag, salt) % 360) / 360.0, s, v)
    return (int(r * 255), int(g * 255), int(b * 255))


class Face:
    def __init__(self, tag, gear=None, era=0, beard=None, glasses=False, skin=None, hair=None,
                 outfit=None, trim=None, long_hair=None):
        self.tag = tag
        self.skin = skin or SKINS[_h(tag, "skin") % len(SKINS)]
        self.hair = hair or HAIRS[_h(tag, "hair") % len(HAIRS)]
        self.outfit = outfit or _hue(tag, "out")
        self.trim = trim or _hue(tag, "trim", 0.5, 0.95)
        self.gear = gear or ("crown", "laurel", "turban", "helmet", "hat", "plume", "feathers", "cap")[
            _h(tag, "gear") % 8]
        self.beard = (_h(tag, "beard") % 3 == 0) if beard is None else beard
        self.long_hair = (_h(tag, "long") % 2 == 0) if long_hair is None else long_hair
        self.glasses = glasses
        self.era = era


def _eyes(sc, face, mood, blink, dx, dz):
    z = 25.6 + dz
    for side in (-1, 1):
        x = side * 2.25 + dx
        if blink:
            sc.limb((x - 1.0, 5.0, z), (x + 1.0, 5.0, z), 0.32, 0.32, (84, 56, 46), bias=0.5)
        else:
            sc.ball((x, 5.0, z), (1.15, 0.5, 0.95), WHITE, bias=0.4)
            sc.ball((x, 5.35, z - 0.05), (0.62, 0.3, 0.62), (62, 92, 132) if face.era % 2 == 0 else (84, 62, 40), bias=0.5)
            sc.ball((x, 5.55, z - 0.05), (0.3, 0.2, 0.3), INK, bias=0.6)
        lo = {"angry": -0.8, "unhappy": 0.6}.get(mood, 0.0)
        hi = {"angry": 0.3, "unhappy": -0.2}.get(mood, 0.0)
        sc.limb((x - side * 1.3, 5.1, z + 1.5 + lo), (x + side * 1.3, 5.1, z + 1.6 + hi),
                0.3, 0.3, face.hair, bias=0.45)


def _mouth(sc, mood, dx, dz):
    z = 21.4 + dz
    if mood == "happy":
        pts = [(-1.9, 5.8, z + 0.9), (-1.0, 6.0, z + 0.2), (0.0, 6.1, z), (1.0, 6.0, z + 0.2), (1.9, 5.8, z + 0.9)]
    elif mood in ("unhappy", "angry"):
        pts = [(-1.7, 5.8, z - 0.3), (-0.8, 6.0, z + 0.3), (0.0, 6.1, z + 0.4), (0.8, 6.0, z + 0.3), (1.7, 5.8, z - 0.3)]
    else:
        pts = [(-1.5, 5.9, z), (1.5, 5.9, z)]
    for a, b in zip(pts, pts[1:]):
        sc.limb((a[0] + dx, a[1], a[2]), (b[0] + dx, b[1], b[2]), 0.36, 0.36, (146, 62, 56), bias=0.5)


def _gear(sc, face, dx, dz):
    g = face.gear
    hair, trim, outfit = face.hair, face.trim, face.outfit
    z = 22.0 + dz
    if g == "crown":
        sc.ball((dx, -1.4, z + 4.0), (5.9, 5.0, 4.2), hair)
        sc.disc((dx, 0.4, z + 5.6), 5.4, (0, 0, 1), GOLD, thick=1.6)
        for k in range(7):
            a = math.radians(-90 + k * 30)
            px, py = dx + 5.2 * math.sin(a), 0.4 + 4.6 * math.cos(a)
            sc.limb((px, py, z + 5.8), (px, py, z + 8.6), 0.9, 0.25, GOLD)
        sc.ball((dx, 4.2, z + 5.8), (0.9, 0.5, 0.9), (210, 40, 56), bias=0.4)
    elif g == "laurel":
        sc.ball((dx, -1.4, z + 4.0), (5.9, 5.0, 4.4), hair)
        for k in range(9):
            a = math.radians(-100 + k * 25)
            sc.ball((dx + 5.5 * math.sin(a), 0.6 + 3.9 * math.cos(a) * 0.9 - 2.4, z + 5.6 + 0.4 * math.cos(a)),
                    (1.5, 0.9, 0.6), (92, 150, 64), bias=0.3)
    elif g == "turban":
        sc.ball((dx, 0.2, z + 4.6), (6.7, 6.4, 4.6), face.trim)
        sc.ball((dx, 0.3, z + 3.4), (6.9, 6.6, 3.0), F.tone(face.trim, 0.82))
        sc.ball((dx, 4.6, z + 6.2), (1.0, 0.6, 1.0), (214, 40, 56), bias=0.4)
        sc.limb((dx, 4.6, z + 6.6), (dx, 3.6, z + 9.6), 0.5, 0.15, (230, 230, 240))
    elif g == "helmet":
        sc.ball((dx, 0.1, z + 4.0), (6.2, 6.0, 5.2), STEEL)
        sc.limb((dx, -4.0, z + 9.0), (dx, 4.4, z + 8.6), 1.1, 1.1, face.trim)
        sc.limb((dx, 4.9, z + 6.0), (dx, 5.2, z + 1.2), 0.6, 0.6, F.tone(STEEL, 0.8))
    elif g == "hat":
        sc.ball((dx, -1.4, z + 3.6), (5.9, 5.0, 4.0), hair)
        sc.disc((dx, 0.4, z + 5.6), 8.4, (0, 0, 1), (46, 40, 48), thick=0.8)
        sc.ball((dx, 0.4, z + 7.8), (4.8, 4.8, 3.2), (46, 40, 48))
        sc.limb((dx - 4.8, 0.4, z + 6.4), (dx + 4.8, 0.4, z + 6.4), 0.6, 0.6, face.trim)
    elif g == "plume":
        sc.ball((dx, -0.1, z + 4.2), (6.2, 6.0, 4.8), GOLD)
        sc.limb((dx + 3.0, -0.5, z + 8.4), (dx + 6.2, -4.0, z + 13.0), 1.2, 0.3, face.trim)
        sc.limb((dx + 2.6, -0.5, z + 8.4), (dx + 3.4, -4.4, z + 12.4), 1.0, 0.3, F.tone(face.trim, 1.2))
    elif g == "feathers":
        sc.ball((dx, -1.4, z + 4.0), (5.9, 5.0, 4.2), hair)
        sc.limb((dx - 5.6, 0.4, z + 5.8), (dx + 5.6, 0.4, z + 5.8), 1.2, 1.2, face.trim)
        for k in range(7):
            a = math.radians(-60 + k * 20)
            sc.limb((dx + 4.6 * math.sin(a), 0.0, z + 6.2),
                    (dx + 9.0 * math.sin(a), -0.6, z + 6.0 + 8.4 * math.cos(a)), 1.1, 0.45,
                    face.trim if k % 2 else (236, 236, 228), bias=-0.3)
    elif g == "none":
        sc.ball((dx, -1.4, z + 3.8), (5.9, 5.0, 4.4), hair)
        sc.ball((dx, 1.4, z + 5.4), (5.2, 3.6, 2.4), hair)
    elif g == "jester":
        for side in (-1, 1):
            sc.limb((dx + side * 3.0, 0.2, z + 5.0), (dx + side * 7.6, 0.0, z + 9.0), 2.0, 1.2, trim if side < 0 else outfit)
            sc.limb((dx + side * 7.6, 0.0, z + 9.0), (dx + side * 9.6, 0.0, z + 5.6), 1.2, 0.8, trim if side < 0 else outfit)
            sc.ball((dx + side * 9.8, 0.0, z + 5.0), (1.0, 1.0, 1.0), GOLD)
        sc.ball((dx, -0.6, z + 4.6), (5.8, 5.3, 3.4), trim)
    elif g == "visor":
        sc.ball((dx, -1.0, z + 4.2), (5.9, 5.2, 4.0), (60, 130, 78))
        sc.disc((dx, 3.6, z + 3.4), 4.6, (0, 0, 1), (46, 104, 62), thick=0.5)
    else:                                                    # cap
        sc.ball((dx, -1.4, z + 3.8), (5.9, 5.0, 4.2), hair)
        sc.ball((dx, 0.8, z + 4.8), (6.3, 5.9, 3.0), face.outfit)
        sc.ball((dx, 4.2, z + 4.4), (3.4, 1.8, 0.8), F.tone(face.outfit, 0.8))


def _hair_back(sc, face, dx, dz):
    z = 22.0 + dz
    if face.long_hair and face.gear not in ("turban", "helmet"):
        sc.ball((dx, -2.4, z - 3.0), (6.2, 3.6, 8.4), face.hair, bias=-3.0)


def _body(sc, face, bob):
    out, trim = face.outfit, face.trim
    era = face.era
    sc.ball((0, -0.3, 6.5 + bob), (10.5, 5.2, 8.4), F.tone(out, 0.9))               # torso
    for side in (-1, 1):
        sc.ball((side * 8.4, -0.2, 11.4 + bob), (4.2, 4.0, 3.6), out)                 # shoulders
    sc.limb((0, 0.2, 13 + bob), (0, 0.5, 17.4 + bob), 2.4, 2.1, F.tone(face.skin, 0.9))   # neck
    # collar / chest trim
    if era == 0:                                                                     # toga: clasp + drape
        sc.ball((-6.8, 2.6, 11.6 + bob), (2.6, 2.0, 2.0), trim)
        sc.ball((-6.0, 4.6, 10.4 + bob), (0.9, 0.6, 0.9), GOLD, bias=0.4)
    elif era == 1:                                                                   # tunic + fur collar
        sc.ball((0, 0.8, 13.4 + bob), (7.4, 4.6, 2.2), (232, 228, 216))
        sc.poly([(-1.0, 4.9, 12.4 + bob), (1.0, 4.9, 12.4 + bob), (0.6, 5.5, 4.0 + bob), (-0.6, 5.5, 4.0 + bob)], trim, bias=0.3)
    elif era == 2:                                                                   # coat + cravat
        sc.poly([(-3.4, 5.0, 14.8 + bob), (3.4, 5.0, 14.8 + bob), (0.0, 5.7, 5.2 + bob)], WHITE, bias=0.3)
        sc.poly([(-1.1, 5.5, 14.2 + bob), (1.1, 5.5, 14.2 + bob), (0, 6.0, 9.2 + bob)], trim, bias=0.5)
        for side in (-1, 1):
            sc.poly([(side * 3.5, 5.0, 14.8 + bob), (side * 7.5, 4.6, 12.0 + bob), (side * 0.4, 5.7, 5.0 + bob)],
                    F.tone(out, 1.25), bias=0.2)
    else:                                                                            # suit and tie
        sc.poly([(-3.0, 5.0, 14.8 + bob), (3.0, 5.0, 14.8 + bob), (0.0, 5.8, 4.6 + bob)], WHITE, bias=0.3)
        sc.poly([(-0.9, 5.6, 14.0 + bob), (0.9, 5.6, 14.0 + bob), (0.4, 6.0, 6.0 + bob), (-0.4, 6.0, 6.0 + bob)],
                trim, bias=0.5)
        for side in (-1, 1):
            sc.poly([(side * 3.1, 5.0, 14.8 + bob), (side * 6.6, 4.5, 11.0 + bob), (side * 0.6, 5.8, 4.4 + bob)],
                    F.tone(out, 1.18), bias=0.2)


def build(sc, face, mood="content", blink=False, bob=0.0, dx=0.0, dz=0.0):
    _hair_back(sc, face, dx, dz)
    _body(sc, face, bob)
    z = 22.0 + dz + bob
    sc.ball((dx - 5.5, 0.6, z), (1.0, 1.0, 1.7), F.tone(face.skin, 0.92))                # ears
    sc.ball((dx + 5.5, 0.6, z), (1.0, 1.0, 1.7), F.tone(face.skin, 0.92))
    sc.ball((dx, 0.6, z), (5.6, 5.4, 6.4), face.skin)
    sc.ball((dx, 5.4, z + 2.1), (1.15, 1.3, 1.5), F.tone(face.skin, 1.06), bias=0.3)     # nose
    if face.beard:
        sc.ball((dx, 3.4, z - 4.4), (4.4, 3.2, 3.4), face.hair, bias=0.2)
        sc.limb((dx - 1.8, 5.8, z - 1.7), (dx + 1.8, 5.8, z - 1.7), 0.7, 0.7, face.hair, bias=0.5)
    _gear(sc, face, dx, dz + bob)
    _eyes(sc, face, mood, blink, dx, dz + bob)
    _mouth(sc, mood, dx, dz + bob)
    if face.glasses:
        for side in (-1, 1):
            x = dx + side * 2.25
            ring = [(x - 1.5, 5.4, z + 3.5), (x + 1.5, 5.4, z + 3.5), (x + 1.5, 5.4, z + 1.5), (x - 1.5, 5.4, z + 1.5)]
            for a, b in zip(ring, ring[1:] + ring[:1]):
                sc.limb(a, b, 0.28, 0.28, INK, bias=0.7)
        sc.limb((dx - 0.8, 5.4, z + 3.0), (dx + 0.8, 5.4, z + 3.0), 0.25, 0.25, INK, bias=0.7)


def render(face, w, h, k, mood="content", blink=False, bob=0.0, dx=0.0, dz=0.0, gy=None, outline=(34, 24, 24)):
    """The bust as RGBA; ground line `gy` (px) is the z = 0 of the model."""
    sc = F.Scene(w, h, 0, gy=h - 2 if gy is None else gy, k=k)
    build(sc, face, mood, blink, bob, dx, dz)
    sc.render()
    return sc.rgba(outline)


def backdrop(size, tag, ramp=None):
    """A soft studio background (RGB) tinted from the tag."""
    w, h = size
    base = _hue(tag, "bg", 0.35, 0.55)
    top = F.tone(base, 1.35)
    bot = F.tone(base, 0.55)
    img = Image.new("RGB", size)
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)))
    glow = Image.new("L", (3, 3))
    glow.putdata([40, 70, 40, 70, 150, 70, 40, 70, 40])
    glow = glow.resize(size, Image.BICUBIC)
    img.paste(Image.new("RGB", size, F.tone(base, 1.8)), mask=glow)
    return img
