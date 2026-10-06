"""2:1 isometric solids for small buildings, walls and huts.

Ground axes: u runs to the lower right (+1, +0.5 on screen) and v to the
lower left (-1, +0.5); z is up. A point (u, v, z) around an origin (cx, cy)
lands at

    x = cx + (u - v),   y = cy + (u + v) / 2 - z

Only the faces a viewer from the south sees are drawn: top, the +v face
(left, lit) and the +u face (right, shaded), so call solids back to front.
Draw on a canvas supersampled by `scale` and reduce afterwards.
"""
import math

from PIL import Image, ImageDraw

from . import gfx

FACE = {"top": 1.12, "left": 0.90, "right": 0.68}


def tone(c, k):
    return gfx.shade_rgb(c, k)


class Canvas:
    """A supersampled RGBA canvas plus a ground-shadow mask."""

    def __init__(self, size, scale=3):
        self.w, self.h = size
        self.s = scale
        self.im = Image.new("RGBA", (self.w * scale, self.h * scale), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.im)
        self.sh = Image.new("L", self.im.size, 0)
        self.sd = ImageDraw.Draw(self.sh)

    def xy(self, x, y):
        return (x * self.s, y * self.s)

    def poly(self, pts, fill, edge=None):
        p = [self.xy(x, y) for x, y in pts]
        self.d.polygon(p, fill=fill + (255,))
        if edge:
            self.d.line(p + [p[0]], fill=edge + (255,), width=max(1, self.s // 2))

    def line(self, pts, fill, width=1.0):
        self.d.line([self.xy(x, y) for x, y in pts], fill=fill + (255,), width=max(1, int(width * self.s)))

    def ellipse(self, box, fill):
        x0, y0, x1, y1 = box
        self.d.ellipse([x0 * self.s, y0 * self.s, x1 * self.s, y1 * self.s], fill=fill + (255,))

    def shadow_ellipse(self, cx, cy, rx, ry):
        self.sd.ellipse([(cx - rx) * self.s, (cy - ry) * self.s, (cx + rx) * self.s, (cy + ry) * self.s], fill=255)

    def done(self, size=None):
        size = size or (self.w, self.h)
        return self.im.resize(size, Image.Resampling.LANCZOS), self.sh.resize(size, Image.Resampling.LANCZOS)


def P(cx, cy, u, v, z):
    return (cx + (u - v), cy + (u + v) * 0.5 - z)


def box(cv, cx, cy, hu, hv, h, colour, z0=0.0, edge=None):
    """An upright block, footprint +-hu by +-hv on the ground, from z0 up h."""
    z1 = z0 + h
    left = [P(cx, cy, -hu, hv, z0), P(cx, cy, hu, hv, z0), P(cx, cy, hu, hv, z1), P(cx, cy, -hu, hv, z1)]
    right = [P(cx, cy, hu, hv, z0), P(cx, cy, hu, -hv, z0), P(cx, cy, hu, -hv, z1), P(cx, cy, hu, hv, z1)]
    top = [P(cx, cy, -hu, -hv, z1), P(cx, cy, hu, -hv, z1), P(cx, cy, hu, hv, z1), P(cx, cy, -hu, hv, z1)]
    cv.poly(left, tone(colour, FACE["left"]), edge)
    cv.poly(right, tone(colour, FACE["right"]), edge)
    cv.poly(top, tone(colour, FACE["top"]), edge)


def gable(cv, cx, cy, hu, hv, z0, rise, colour, ridge="u", overhang=0.0, edge=None):
    """A gable roof over a hu x hv footprint, ridge running along u or v."""
    hu, hv = hu + overhang, hv + overhang
    if ridge == "u":
        a, b = P(cx, cy, -hu, 0, z0 + rise), P(cx, cy, hu, 0, z0 + rise)
        front = [P(cx, cy, -hu, hv, z0), P(cx, cy, hu, hv, z0), b, a]
        end = [P(cx, cy, hu, hv, z0), P(cx, cy, hu, -hv, z0), b]
        cv.poly(front, tone(colour, FACE["left"]), edge)
        cv.poly(end, tone(colour, FACE["right"]), edge)
    else:
        a, b = P(cx, cy, 0, -hv, z0 + rise), P(cx, cy, 0, hv, z0 + rise)
        front = [P(cx, cy, hu, hv, z0), P(cx, cy, hu, -hv, z0), a, b]
        end = [P(cx, cy, -hu, hv, z0), P(cx, cy, hu, hv, z0), b]
        cv.poly(end, tone(colour, FACE["left"]), edge)
        cv.poly(front, tone(colour, FACE["right"]), edge)


def pyramid(cv, cx, cy, hu, hv, z0, rise, colour, edge=None):
    apex = P(cx, cy, 0, 0, z0 + rise)
    cv.poly([P(cx, cy, -hu, hv, z0), P(cx, cy, hu, hv, z0), apex], tone(colour, FACE["left"]), edge)
    cv.poly([P(cx, cy, hu, hv, z0), P(cx, cy, hu, -hv, z0), apex], tone(colour, FACE["right"]), edge)


def cone(cv, cx, cy, r, z0, rise, colour, edge=None, steps=14):
    """A round roof or tepee: ellipse base, apex above the centre."""
    apex = (cx, cy - z0 - rise)
    ry = r * 0.5
    pts = [(cx + r * math.cos(t), cy - z0 + ry * math.sin(t)) for t in [math.pi * i / steps for i in range(steps + 1)]]
    # near half of the base ellipse + apex gives the visible cone body
    cv.poly(pts + [apex], tone(colour, FACE["left"]), edge)
    # shade the right third
    shade = [p for p in pts if p[0] > cx + r * 0.25] + [apex]
    if len(shade) > 2:
        cv.poly([(cx + r * 0.25, cy - z0 + ry * math.sqrt(max(0, 1 - 0.0625)))] + shade, tone(colour, FACE["right"]))


def cylinder(cv, cx, cy, r, z0, h, colour, edge=None, steps=16):
    """An upright round tower, base centre (cx, cy), from z0 up h."""
    ry = r * 0.5
    bot = [(cx + r * math.cos(t), cy - z0 + ry * math.sin(t)) for t in [math.pi * i / steps for i in range(steps + 1)]]
    top = [(x, y - h) for x, y in reversed(bot)]
    cv.poly(bot + top, tone(colour, FACE["left"]), edge)
    shade = [p for p in bot if p[0] > cx + r * 0.3]
    shade_top = [(x, y - h) for x, y in reversed(shade)]
    if shade:
        cv.poly(shade + shade_top, tone(colour, FACE["right"]))
    cv.ellipse((cx - r, cy - z0 - h - ry, cx + r, cy - z0 - h + ry), tone(colour, FACE["top"]))


def flag(cv, x, y, h, colour, pole=(70, 56, 42)):
    cv.line([(x, y), (x, y - h)], pole, 0.9)
    cv.poly([(x, y - h), (x + h * 0.5, y - h + h * 0.18), (x, y - h + h * 0.36)], colour)
