"""Window chrome (`ui/chrome/`): the ornate frames, paper and slate surfaces, studs and cartouches
that dress every panel of the interface, drawn like the painted references (a gilt bronze frame with
bosses and scrolls at the corners, aged parchment, dark slate).

Frames are 9-slice pictures authored at 2x (the client draws their corners at half size, see
`theme::dress`); surfaces are seamless tiles. All light comes from the upper left, as everywhere.
"""

import math

import numpy as np
from PIL import Image

from .svgkit import Svg, hexc, mixc, shade
from .util import downsample, rasterize, to_array, to_image

OUTLINE = "#1c1109"
BRONZE = "#7a5326"
BRONZE_HI = "#b98a45"
BRONZE_LO = "#46300f"
GOLD = "#d8aa4c"
GOLD_HI = "#f4dc8e"
GOLD_LO = "#8a5f1f"
DARK = "#2e1e0e"
SHADOW = "#5a3612"


# --- seamless tiles ----------------------------------------------------------------------------------
def periodic_noise(size, sigma, seed):
    """Gaussian-filtered white noise on a torus, scaled to unit standard deviation: it tiles."""
    rng = np.random.default_rng(seed)
    white = rng.standard_normal((size, size))
    fy = np.fft.fftfreq(size)[:, None]
    fx = np.fft.fftfreq(size)[None, :]
    kernel = np.exp(-2.0 * (math.pi * sigma) ** 2 * (fx * fx + fy * fy))
    out = np.fft.ifft2(np.fft.fft2(white) * kernel).real
    return out / (out.std() + 1e-9)


def tile(base, size=256, seed=1, blotch=0.05, mottle=0.03, fibre=0.025, grain=0.012, warm=0.0):
    """A surface of colour `base` (0-255 triple) with large blotches, mottling, fibres and grain."""
    n = lambda sigma, s: periodic_noise(size, sigma, seed * 17 + s)
    field = blotch * n(46, 1) + mottle * n(14, 2) + grain * n(0.9, 3)
    # Paper fibres: noise stretched along one axis.
    rng = np.random.default_rng(seed * 31)
    fibres = np.zeros((size, size))
    for _ in range(420):
        x, y = rng.integers(0, size, 2)
        length = int(rng.integers(6, 22))
        angle = rng.uniform(-0.5, 0.5) + (math.pi / 2 if rng.random() < 0.5 else 0)
        shade_ = rng.normal(0, 1)
        for t in range(length):
            xx = int(x + math.cos(angle) * t) % size
            yy = int(y + math.sin(angle) * t) % size
            fibres[yy, xx] += shade_
    field = field + fibre * fibres
    rgb = np.array(base, dtype=np.float64)[None, None, :] / 255.0
    tint = np.array([1.0 + warm, 1.0, 1.0 - warm])[None, None, :]
    out = rgb * (1.0 + field[..., None] * tint)
    out = np.dstack([np.clip(out, 0, 1), np.ones((size, size))])
    return to_image(out)


def parchment():
    img = tile((229, 209, 166), seed=3, blotch=0.02, mottle=0.012, fibre=0.012, grain=0.008, warm=0.12)
    # A few faint foxing stains.
    arr = to_array(img)
    rng = np.random.default_rng(9)
    size = arr.shape[0]
    yy, xx = np.mgrid[0:size, 0:size]
    for _ in range(9):
        cx, cy, r = rng.integers(0, size), rng.integers(0, size), rng.uniform(10, 28)
        for ox in (-size, 0, size):
            for oy in (-size, 0, size):
                d = np.hypot(xx - cx - ox, yy - cy - oy) / r
                arr[..., :3] *= 1.0 - np.exp(-d * d)[..., None] * np.array([0.05, 0.06, 0.09])
    return to_image(arr)


def slate():
    return tile((38, 41, 46), seed=5, blotch=0.07, mottle=0.05, fibre=0.0, grain=0.03)


def felt():
    return tile((31, 66, 52), seed=7, blotch=0.06, mottle=0.04, fibre=0.0, grain=0.035)


# --- frames ------------------------------------------------------------------------------------------
def ring(s: Svg, a, b, fill, size=160, op=1.0):
    """The band between the rectangles inset `a` and `b` from the edge."""
    w = size
    s.raw(
        f'<path fill-rule="evenodd" fill="{fill}" opacity="{op}" d="M{a},{a} H{w - a} V{w - a} H{a} Z '
        f'M{b},{b} H{w - b} V{w - b} H{b} Z"/>'
    )


def stroke3(s: Svg, d, width=3.4, dark=OUTLINE, gold=GOLD, hi=GOLD_HI):
    """A gilt line with an outline under it and a highlight on it."""
    s.path(d, "none", dark, width + 2.6)
    s.path(d, "none", gold, width)
    s.path(d, "none", hi, max(0.7, width * 0.28), op=0.85, extra='transform="translate(-0.5,-0.6)"')


def leaf(s: Svg, x, y, ang, length=9, width=4.2):
    s.ellipse(x, y, length / 2, width / 2, GOLD, stroke=OUTLINE, sw=1.1, rot=ang)
    dx, dy = math.cos(math.radians(ang)) * length * 0.18, math.sin(math.radians(ang)) * length * 0.18
    s.ellipse(x - dx - 0.4, y - dy - 0.5, length * 0.28, width * 0.16, GOLD_HI, op=0.8, rot=ang)


def rivet(s: Svg, x, y, r):
    g = s.rad([(0, GOLD_HI), (0.55, GOLD), (1, GOLD_LO)], 0.5, 0.5, 0.5, 0.35, 0.3)
    s.circle(x, y, r + 1.2, OUTLINE)
    s.circle(x, y, r, g)
    s.circle(x - r * 0.28, y - r * 0.32, r * 0.22, "#fff3c4", op=0.9)


def corner_scroll(s: Svg):
    """A bracket of two curled arms and a spiral, drawn in the top-left 64 x 64."""
    for flip in (False, True):
        sx = (lambda x, y: (y, x)) if flip else (lambda x, y: (x, y))

        def p(*pts):
            return " ".join(f"{sx(x, y)[0]:.1f},{sx(x, y)[1]:.1f}" for x, y in pts)

        d = f"M{p((18, 20))} C{p((30, 20), (38, 13), (50, 15))} C{p((57, 16), (61, 21), (58, 26))} C{p((56, 30), (50, 30), (50, 25))}"
        stroke3(s, d, 4.2)
        for x, y, a in ((33, 16.5, 20), (44, 14.5, -8)):
            fx, fy = sx(x, y)
            leaf(s, fx, fy, (90 - a if flip else a) % 360, 8, 3.6)
    # spiral
    stroke3(s, "M16,13 C9,13 9,23 16,23 C21,23 21,16 17,16 C15,16 14.5,18 16,18.5", 3.0)
    rivet(s, 11, 11, 4.4)
    rivet(s, 26, 26, 2.6)


def corner_boss(s: Svg):
    """A round boss with curling tendrils, drawn in the top-left 64 x 64."""
    for flip in (False, True):
        sx = (lambda x, y: (y, x)) if flip else (lambda x, y: (x, y))

        def p(*pts):
            return " ".join(f"{sx(x, y)[0]:.1f},{sx(x, y)[1]:.1f}" for x, y in pts)

        d = f"M{p((38, 22))} C{p((46, 22), (50, 14), (58, 16))} C{p((62, 17), (62, 23), (58, 23))}"
        stroke3(s, d, 3.0)
        fx, fy = sx(47, 18.5)
        leaf(s, fx, fy, (90 - 30 if flip else -30) % 360, 8, 3.4)
    c = 23
    s.circle(c, c, 20.5, OUTLINE)
    ring_g = s.rad([(0, GOLD_HI), (0.5, GOLD), (1, GOLD_LO)], 0.5, 0.5, 0.5, 0.32, 0.28)
    s.circle(c, c, 19, ring_g)
    s.circle(c, c, 13.4, BRONZE_LO)
    dome = s.rad([(0, "#fff0b8"), (0.35, GOLD_HI), (0.75, GOLD), (1, GOLD_LO)], 0.5, 0.5, 0.5, 0.33, 0.28)
    s.circle(c, c, 12.2, dome)
    # a little engraved compass rose on the dome
    for k in range(8):
        a = k * math.pi / 4
        r = 8.2 if k % 2 == 0 else 5.0
        s.line((c, c), (c + math.cos(a) * r, c + math.sin(a) * r), GOLD_LO, 1.1, 0.8)
    s.circle(c, c, 2.4, GOLD_LO)
    s.circle(c - 0.6, c - 0.7, 1.0, "#fff3c4", op=0.9)


def frame_svg(small=False):
    size = 160
    s = Svg(size, size)
    ring(s, 0, 3, OUTLINE)
    # bronze band with a bevel: lit on the upper left, in shade on the lower right
    ring(s, 3, 11, BRONZE)
    s.raw(f'<path d="M4,4 H156 M4,4 V156" stroke="{BRONZE_HI}" stroke-width="1.6" opacity="0.85" fill="none"/>')
    s.raw(f'<path d="M156,156 H4 M156,156 V4" stroke="{BRONZE_LO}" stroke-width="1.8" opacity="0.8" fill="none" transform="translate(-1.8,-1.8)"/>')
    ring(s, 11, 13, GOLD)
    s.raw(f'<path d="M11.8,11.8 H148.2 M11.8,11.8 V148.2" stroke="{GOLD_HI}" stroke-width="0.9" opacity="0.9" fill="none"/>')
    ring(s, 13, 15, DARK)
    # soft shade just inside the band
    for x, y, w, h, grad in (
        (15, 15, 130, 20, (0, 0, 0, 1)),
        (15, 125, 130, 20, (0, 1, 0, 0)),
        (15, 15, 20, 130, (0, 0, 1, 0)),
        (125, 15, 20, 130, (1, 0, 0, 0)),
    ):
        s.rect(x, y, w, h, s.lin([(0, SHADOW, 0.46), (1, SHADOW, 0.0)], *grad))
    corners = (
        ("", corner_scroll),
        ('transform="translate(160,0) scale(-1,1)"', corner_boss),
        ('transform="translate(0,160) scale(1,-1)"', corner_boss),
        ('transform="translate(160,160) scale(-1,-1)"', corner_scroll),
    )
    for tf, draw in corners:
        s.begin(tf)
        draw(s)
        s.end()
    return s


def render_frame(px):
    big = rasterize(frame_svg().render(), px * 3, px * 3)
    return downsample(big, px, px)


# --- studs and cartouches ----------------------------------------------------------------------------
def stud():
    s = Svg(32, 32)
    rivet(s, 16, 16, 11)
    return downsample(rasterize(s.render(), 96, 96), 32, 32)


def cartouche(vertical=False):
    """A lozenge flanked by two studs, set on the middle of a side of a frame."""
    s = Svg(72, 36)
    rivet(s, 11, 18, 5)
    rivet(s, 61, 18, 5)
    s.poly([(36, 3), (53, 18), (36, 33), (19, 18)], OUTLINE)
    g = s.lin([(0, GOLD_HI), (0.5, GOLD), (1, GOLD_LO)], 0, 0, 1, 1)
    s.poly([(36, 5.4), (50.6, 18), (36, 30.6), (21.4, 18)], g)
    s.poly([(36, 11), (44, 18), (36, 25), (28, 18)], BRONZE_LO)
    s.poly([(36, 13), (41.2, 18), (36, 23), (30.8, 18)], GOLD)
    s.circle(35, 17, 1.2, "#fff3c4", op=0.9)
    im = downsample(rasterize(s.render(), 216, 108), 72, 36)
    return im.rotate(90, expand=True) if vertical else im


def rule_end():
    """The left half of a rule with a diamond and a tiny arrowhead at its inner end; mirrored for the right."""
    s = Svg(160, 16)
    s.line((2, 8), (140, 8), "#6a5230", 1.6, 0.75)
    s.line((2, 10.2), (140, 10.2), "#fff3d0", 0.8, 0.35)
    s.poly([(140, 8), (150, 3.5), (154, 8), (150, 12.5)], "#6a5230", op=0.95)
    s.poly([(144, 8), (150, 5.6), (151.6, 8), (150, 10.4)], "#c79a45", op=0.95)
    return downsample(rasterize(s.render(), 480, 48), 160, 16)


# --- the compass that joins the date to the day button ------------------------------------------------
def compass():
    s = Svg(64, 64)
    c = 32
    s.circle(c, c + 1.5, 28, "#000000", op=0.35, blur=s.blur(1.6))
    s.circle(c, c, 29, OUTLINE)
    g = s.rad([(0, GOLD_HI), (0.45, GOLD), (1, GOLD_LO)], 0.5, 0.5, 0.5, 0.32, 0.26)
    s.circle(c, c, 27.6, g)
    s.circle(c, c, 21.5, BRONZE_LO)
    face = s.rad([(0, "#c8964a"), (0.7, "#8e5f25"), (1, "#5d3b14")], 0.5, 0.5, 0.5, 0.4, 0.35)
    s.circle(c, c, 20.2, face)
    for k in range(16):
        a = k * math.pi / 8
        r0, r1 = (15.5, 20.2) if k % 2 == 0 else (17.6, 20.2)
        s.line((c + math.cos(a) * r0, c + math.sin(a) * r0), (c + math.cos(a) * r1, c + math.sin(a) * r1), "#2a1808", 1.0 if k % 4 else 1.6, 0.9)
    s.poly([(c, c - 17), (c + 4, c), (c, c + 17), (c - 4, c)], "#f2cf7a", "#2a1808", 0.9)
    s.poly([(c - 17, c), (c, c - 3.2), (c + 17, c), (c, c + 3.2)], "#d9aa55", "#2a1808", 0.8)
    s.poly([(c, c - 17), (c + 4, c), (c - 4, c)], "#f6e2a0", op=0.9)
    s.circle(c, c, 2.6, GOLD_HI, stroke=OUTLINE, sw=0.9)
    return downsample(rasterize(s.render(), 192, 192), 64, 64)


# --- everything --------------------------------------------------------------------------------------
def render_all():
    out = {
        "frame": render_frame(160),
        "frame_small": render_frame(100),
        "parchment": parchment(),
        "slate": slate(),
        "felt": felt(),
        "stud": stud(),
        "cartouche_h": cartouche(False),
        "cartouche_v": cartouche(True),
        "rule_end": rule_end(),
        "compass": compass(),
    }
    return out
