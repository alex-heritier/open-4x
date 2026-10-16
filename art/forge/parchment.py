"""The city view's parchment: paper, a gilt corner, and three small engravings.

Everything here is drawn in code (seeded, so a build is repeatable) and written under `ui/`:

* `paper.png`: a seamless sheet of aged paper, 256 px, tiled by the client at half size.
* `flourish.png`: one gilt corner bracket; the client flips it into the four corners of a window.
* `city_panorama.png`: a harbour with sailing ships and a steamer, in sepia ink, for the header.
* `city_fields.png`: farmers in a field under hills, for the granary.
* `city_backdrop.png`: a soft landscape the city's own picture is set in.

The engravings are ink on transparent, faded at their edges, so they sit on the paper like prints.
"""

import math
import random

import numpy as np
from PIL import Image

from .svgkit import Svg
from .util import downsample, rasterize

INK = "#3b2f23"
WASH = "#8f8571"
PAPER = (237, 224, 192)


# ---- paper ---------------------------------------------------------------------------------------
def _periodic(size, sigma, rng):
    """Smooth noise that wraps at the edges (low-passed in the Fourier domain), scaled to -1..1."""
    spectrum = np.fft.fft2(rng.standard_normal((size, size)))
    f = np.fft.fftfreq(size)
    r2 = f[:, None] ** 2 + f[None, :] ** 2
    field = np.fft.ifft2(spectrum * np.exp(-r2 * (2 * math.pi * sigma) ** 2 / 2)).real
    return field / (np.abs(field).max() + 1e-9)


def paper(size=256):
    rng = np.random.default_rng(11)
    blotch = _periodic(size, 38, rng) * 0.5 + _periodic(size, 14, rng) * 0.35
    fibre = _periodic(size, 2.2, rng) * 0.35 + _periodic(size, 0.9, rng) * 0.25
    tone = 1.0 + blotch * 0.035 + fibre * 0.03
    base = np.array(PAPER, dtype=np.float32)[None, None, :]
    # warmer where the paper is darker, like foxing
    warm = np.stack([tone, tone * (1 + (tone - 1) * 0.35), tone * (1 + (tone - 1) * 0.9)], axis=-1)
    rgb = np.clip(base * warm, 0, 255).astype(np.uint8)
    return Image.fromarray(rgb, "RGB").convert("RGBA")


# ---- gilt corner ---------------------------------------------------------------------------------
def flourish(size=96):
    """A bracket for the top-left corner: two arms along the edges, a curl, and a diamond."""
    s = Svg(size, size)
    gold, deep = "#b78b32", "#6c4a17"
    arm = 78
    for colour, width in ((deep, 4.2), (gold, 2.4)):
        s.path(f"M{arm},10 L22,10 Q10,10 10,22 L10,{arm}", "none", colour, width)
        s.path(f"M{arm - 18},19 L30,19 Q19,19 19,30 L19,{arm - 18}", "none", colour, width * 0.55)
    # the curl that turns inward at the bracket's corner
    for colour, width in ((deep, 3.4), (gold, 1.8)):
        s.path("M30,30 C46,30 50,44 40,48 C33,51 29,44 35,41", "none", colour, width)
    for colour, grow in ((deep, 1.0), (gold, 0.62)):
        r = 7.5 * grow
        s.poly([(10, 10 - r), (10 + r, 10), (10, 10 + r), (10 - r, 10)], colour)
    s.circle(arm + 3, 10, 2.4, gold, stroke=deep, sw=0.8)
    s.circle(10, arm + 3, 2.4, gold, stroke=deep, sw=0.8)
    return _render(s, size * 2, size * 2)


def _render(s: Svg, w, h):
    """Rasterise `s` at w x h."""
    body = "".join(s.defs and [f"<defs>{''.join(s.defs)}</defs>"]) + "".join(s.items)
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s.w} {s.h}" width="{s.w}" height="{s.h}">{body}</svg>'
    return rasterize(svg, w, h)


def _fade(im: Image.Image, left=0.0, right=0.0, top=0.0, bottom=0.0, oval=False):
    """Multiply the alpha by a soft ramp from each edge; fractions of the width or height."""
    a = np.asarray(im, dtype=np.float32) / 255.0
    h, w = a.shape[:2]
    x = (np.arange(w) + 0.5) / w
    y = (np.arange(h) + 0.5) / h

    def ramp(t, edge):
        if edge <= 0:
            return np.ones_like(t)
        k = np.clip(t / edge, 0, 1)
        return k * k * (3 - 2 * k)

    mx = ramp(x, left) * ramp(1 - x, right)
    my = ramp(y, top) * ramp(1 - y, bottom)
    mask = my[:, None] * mx[None, :]
    if oval:
        d = np.sqrt(((x[None, :] - 0.5) / 0.5) ** 2 + ((y[:, None] - 0.5) / 0.5) ** 2)
        k = np.clip((1.0 - d) / 0.35, 0, 1)
        mask = mask * k * k * (3 - 2 * k)
    a[..., 3] *= mask
    return Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA")


# ---- engraving pieces ----------------------------------------------------------------------------
def _ripples(s: Svg, rng, x0, x1, y0, y1, count, op=0.5, length=(8, 30)):
    for _ in range(count):
        y = y0 + (y1 - y0) * rng.random() ** 1.3
        x = x0 + (x1 - x0) * rng.random()
        n = rng.uniform(*length) * (0.5 + (y - y0) / (y1 - y0))
        s.line((x, y), (x + n, y), INK, 0.9, op * (0.5 + 0.5 * rng.random()))


def _sailing_ship(s: Svg, x, y, k, flip=1):
    """A three-masted ship under sail, bow to the right (flip=-1 turns it round)."""

    def X(v):
        return x + v * k * flip

    def Y(v):
        return y + v * k

    hull = [(-62, -10), (-66, -22), (-40, -22), (50, -22), (74, -34), (68, -10), (56, 0), (-50, 0)]
    s.poly([(X(a), Y(b)) for a, b in hull], "#3c3127", INK, 0.8)
    s.poly([(X(a), Y(b)) for a, b in [(-64, -18), (64, -18), (62, -14), (-64, -14)]], "#e9dfc6", op=0.85)
    for gx in range(-48, 50, 12):
        s.rect(X(gx) - 1.5 * k, Y(-12) - 1.5 * k, 3 * k, 3 * k, INK, op=0.7)
    # masts, with square sails stacked on each
    for mx, top, lift in ((-34, -112, 0), (2, -126, 0), (36, -102, 0)):
        s.line((X(mx), Y(-22)), (X(mx), Y(top)), INK, 1.6 * k)
        rows = [(top + 6, 24), (top + 36, 28), (top + 66, 32)]
        for ry, half in rows:
            if ry > -34:
                continue
            h = 27
            quad = [(mx - half * 0.8, ry), (mx + half * 0.8, ry), (mx + half, ry + h), (mx - half, ry + h)]
            s.poly([(X(a), Y(b)) for a, b in quad], "#f2ead4", INK, 0.7)
            s.path(
                f"M{X(mx - half * 0.7):.1f},{Y(ry + h * 0.5):.1f} Q{X(mx):.1f},{Y(ry + h * 0.35):.1f} {X(mx + half * 0.7):.1f},{Y(ry + h * 0.5):.1f}",
                "none",
                WASH,
                1.2,
                0.5,
            )
            s.line((X(mx - half), Y(ry)), (X(mx + half), Y(ry)), INK, 1.1 * k)
    # jibs and the bowsprit
    s.line((X(66), Y(-30)), (X(104), Y(-44)), INK, 1.4 * k)
    s.poly([(X(a), Y(b)) for a, b in [(100, -44), (40, -100), (64, -34)]], "#f2ead4", INK, 0.7)
    s.poly([(X(a), Y(b)) for a, b in [(62, -34), (36, -98), (-4, -34)]], "#efe6cd", INK, 0.7, 0.9)
    # rigging
    for a, b in ((-34, 36), (2, -62), (2, 66), (36, 66)):
        s.line((X(a), Y(-110 if a != 36 else -100)), (X(b), Y(-22)), INK, 0.5, 0.55)
    s.path(f"M{X(-34):.1f},{Y(-112):.1f} l{6 * k * flip:.1f},-3 l{-1 * k * flip:.1f},5 z", "#a33a2c", op=0.9)
    # its wake
    for dy, op in ((2, 0.55), (5, 0.35)):
        s.line((X(-78), Y(dy)), (X(86), Y(dy)), INK, 0.8, op)


def _steamer(s: Svg, x, y, k):
    hull = [(-88, -16), (-92, -26), (84, -26), (112, -30), (96, -4), (78, 0), (-76, 0)]
    s.poly([(x + a * k, y + b * k) for a, b in hull], "#2f2923", INK, 0.8)
    s.poly([(x + a * k, y + b * k) for a, b in [(-90, -22), (90, -22), (88, -17), (-90, -17)]], "#b2402f", op=0.85)
    s.rect(x - 52 * k, y - 46 * k, 96 * k, 20 * k, "#e9dfc6", stroke=INK, sw=0.8)
    for wx in range(-46, 40, 11):
        s.rect(x + wx * k, y - 42 * k, 6 * k, 7 * k, INK, op=0.75)
    for fx in (-30, 14):
        s.poly(
            [(x + (fx - 8) * k, y - 46 * k), (x + (fx + 8) * k, y - 46 * k), (x + (fx + 6) * k, y - 86 * k), (x + (fx - 6) * k, y - 86 * k)],
            "#2c2620",
            INK,
            0.7,
        )
        s.rect(x + (fx - 7) * k, y - 72 * k, 14 * k, 4 * k, "#b2402f", op=0.9)
    for mx, top in ((-66, -80), (64, -66)):
        s.line((x + mx * k, y - 26 * k), (x + mx * k, y + top * k), INK, 1.3 * k)
        s.line((x + mx * k, y + top * k * 0.75), (x + (mx + 30 * (1 if mx > 0 else -1)) * k, y - 26 * k), INK, 0.5, 0.6)
    # a long trail of smoke drifting astern
    blur = s.blur(5 * k)
    for i in range(7):
        px = x - 30 * k - i * 30 * k
        py = y - 98 * k - i * 3.5 * k - 6 * k * math.sin(i)
        s.ellipse(px, py, (14 + i * 6) * k, (8 + i * 2.4) * k, "#8c8579", op=0.55 - i * 0.05, blur=blur)
    for dy, op in ((2, 0.55), (5, 0.35)):
        s.line((x - 118 * k, y + dy), (x + 120 * k, y + dy), INK, 0.8, op)


def _boat(s: Svg, x, y, k):
    s.poly([(x - 16 * k, y - 4 * k), (x + 18 * k, y - 4 * k), (x + 11 * k, y + 2 * k), (x - 10 * k, y + 2 * k)], "#3c3127", INK, 0.6)
    s.line((x, y - 4 * k), (x, y - 36 * k), INK, 1.0)
    s.poly([(x + 1 * k, y - 35 * k), (x + 1 * k, y - 6 * k), (x + 20 * k, y - 6 * k)], "#f2ead4", INK, 0.6)
    s.poly([(x - 1 * k, y - 30 * k), (x - 1 * k, y - 6 * k), (x - 13 * k, y - 6 * k)], "#ece2c8", INK, 0.6)


def _mountain(s: Svg, x, y, w, h, snow=True):
    """A single cone with hatching down its flanks and a ragged cap of snow, all clipped to its outline."""
    left, right = x - w / 2, x + w / 2
    body = f"M{left},{y} C{x - w * 0.28},{y - h * 0.18} {x - w * 0.10},{y - h * 0.80} {x},{y - h} C{x + w * 0.10},{y - h * 0.80} {x + w * 0.28},{y - h * 0.18} {right},{y} Z"
    clip = s.uid("mt")
    s.defs.append(f'<clipPath id="{clip}"><path d="{body}"/></clipPath>')
    s.path(body, "#a29a88", op=0.95)
    inner = [f'<path d="M{x},{y - h} C{x + w * 0.10},{y - h * 0.80} {x + w * 0.28},{y - h * 0.18} {right},{y} L{x + w * 0.05},{y} Z" fill="#6f6757" opacity="0.38"/>']
    rng = random.Random(int(x))
    for i in range(46):
        t_ = rng.random()
        side = rng.choice((-1, 1))
        sx = x + side * w * 0.015 * (1 + t_ * 4)
        sy = y - h * (0.95 - 0.8 * t_)
        ex = x + side * w * (0.05 + 0.34 * t_ * rng.uniform(0.9, 1.4))
        inner.append(f'<line x1="{sx:.1f}" y1="{sy:.1f}" x2="{ex:.1f}" y2="{y - 2}" stroke="{INK}" stroke-width="0.6" opacity="0.3"/>')
    if snow:
        pts = [(left, y - h * 0.55)]
        steps = 9
        for i in range(steps + 1):
            fx = left + (right - left) * i / steps
            dip = h * (0.52 + 0.07 * math.sin(i * 2.1) + 0.05 * (i % 2)) + abs(i - steps / 2) * h * 0.012
            pts.append((fx, y - dip))
        pts += [(right, y - h * 1.2), (left, y - h * 1.2)]
        inner.append(f'<polygon points="{" ".join(f"{a:.1f},{b:.1f}" for a, b in pts)}" fill="#f6f0e0" opacity="0.97"/>')
        # a few ridge strokes under the snow line
        for i in range(5):
            fx = x + (i - 2) * w * 0.045
            inner.append(f'<line x1="{fx:.1f}" y1="{y - h * 0.50:.1f}" x2="{fx + (i - 2) * 6:.1f}" y2="{y - h * 0.36:.1f}" stroke="{INK}" stroke-width="0.6" opacity="0.35"/>')
    s.raw(f'<g clip-path="url(#{clip})">{"".join(inner)}</g>')
    s.path(body, "none", INK, 0.9)


def _houses(s: Svg, rng, x0, x1, y, k=1.0):
    x = x0
    while x < x1:
        w, h = rng.uniform(9, 18) * k, rng.uniform(6, 11) * k
        s.rect(x, y - h, w, h, "#e6dcc2", stroke=INK, sw=0.6)
        s.poly([(x - 2 * k, y - h), (x + w / 2, y - h - 6 * k), (x + w + 2 * k, y - h)], "#6a5a46", INK, 0.6)
        x += w + rng.uniform(1, 7)


def panorama():
    """A harbour in sepia: a snow-capped cone behind a shore town, ships under sail, a steamer."""
    W, H = 960, 220
    s = Svg(W, H)
    rng = random.Random(7)
    horizon = 138
    # sky wash and distant ranges
    s.rect(0, 20, W, horizon - 20, s.lin([(0, "#cfc6b0", 0), (1, "#cfc6b0", 0.55)]))
    for i, (cx, hh, ww) in enumerate(((560, 46, 360), (770, 58, 380), (880, 40, 300), (80, 36, 300), (210, 50, 360))):
        s.path(
            f"M{cx - ww / 2},{horizon - 4} C{cx - ww * 0.2},{horizon - hh} {cx + ww * 0.2},{horizon - hh * 1.2} {cx + ww / 2},{horizon - 4} Z",
            "#aaa28f",
            op=0.5,
        )
    _mountain(s, 400, horizon - 3, 330, 112)
    # the shore town under the mountain
    s.path(f"M120,{horizon} Q330,{horizon - 14} 640,{horizon - 6} L700,{horizon} Z", "#948a76", INK, 0.6, 0.85)
    _houses(s, rng, 190, 360, horizon - 5, 0.85)
    _houses(s, rng, 450, 600, horizon - 3, 0.8)
    # a pagoda-ish tower and a few pines on the shore
    for tx in (368, 612):
        for tier in range(4):
            w = 15 - tier * 2.6
            s.poly([(tx - w, horizon - 14 - tier * 7), (tx + w, horizon - 14 - tier * 7), (tx + w * 0.55, horizon - 20 - tier * 7), (tx - w * 0.55, horizon - 20 - tier * 7)], "#6d5d49", INK, 0.6)
    for px in (150, 175, 640, 664, 690):
        s.poly([(px, horizon - 5), (px + 5, horizon - 20), (px + 10, horizon - 5)], "#5f6a50", INK, 0.5, 0.9)
    # water
    s.rect(0, horizon, W, H - horizon, s.lin([(0, "#7e8b87", 0.55), (1, "#7e8b87", 0.15)]))
    s.line((0, horizon), (W, horizon), INK, 0.9, 0.55)
    _ripples(s, rng, 0, W, horizon + 3, H - 6, 260, 0.5)
    # the ships, far to near
    _boat(s, 150, horizon + 14, 0.8)
    _boat(s, 760, horizon + 12, 0.65)
    _sailing_ship(s, 560, horizon + 28, 0.7)
    _steamer(s, 800, horizon + 54, 0.74)
    _sailing_ship(s, 270, horizon + 60, 0.92)
    _boat(s, 120, horizon + 62, 1.0)
    _boat(s, 600, horizon + 66, 0.9)
    _ripples(s, rng, 0, W, horizon + 40, H - 4, 90, 0.55, (14, 44))
    im = _render(s, W * 2, H * 2)
    return _fade(im, left=0.30, right=0.04, top=0.10, bottom=0.14)


def _farmer(s: Svg, x, y, k, bend=1.0, flip=1):
    """A farmer stooped over the crop, in a conical straw hat."""
    hx, hy = x + 14 * k * bend * flip, y - 30 * k * (1.0 - 0.28 * bend)
    s.path(f"M{x - 5 * k * flip:.1f},{y:.1f} L{x - 4 * k * flip:.1f},{y - 16 * k:.1f} L{x + 9 * k * bend * flip:.1f},{y - 27 * k * (1 - .2 * bend):.1f} L{x + 12 * k * bend * flip:.1f},{y - 22 * k * (1 - .15 * bend):.1f} L{x + 3 * k * flip:.1f},{y - 12 * k:.1f} L{x + 5 * k * flip:.1f},{y:.1f} Z", "#5a4a38", INK, 0.6)
    s.line((x - 2 * k * flip, y), (x - 4 * k * flip, y + 8 * k), INK, 1.4 * k)
    s.line((x + 4 * k * flip, y), (x + 8 * k * flip, y + 8 * k), INK, 1.4 * k)
    s.line((hx - 3 * k * flip, hy + 12 * k * bend), (x + 16 * k * flip * bend, y - 6 * k), INK, 1.3 * k)
    s.poly([(hx - 12 * k, hy + 3 * k), (hx + 12 * k, hy + 3 * k), (hx, hy - 9 * k)], "#d9c48a", INK, 0.7)


def fields():
    """Hills, a farmhouse and a paddy: two farmers at work."""
    W, H = 840, 230
    s = Svg(W, H)
    rng = random.Random(3)
    # far hills and a faint peak
    s.path("M0,110 C120,60 220,90 330,84 C450,76 520,40 640,70 C720,90 790,70 840,86 L840,140 L0,140 Z", "#a9a28e", op=0.5)
    _mountain(s, 600, 100, 300, 62, snow=False)
    s.path("M0,126 C100,98 210,120 320,106 C440,92 560,128 700,108 C770,100 810,110 840,106 L840,150 L0,150 Z", "#9d9a79", INK, 0.5, 0.75)
    # farmhouse with a thatched roof, and trees
    hx, hy = 150, 122
    s.rect(hx, hy - 20, 54, 20, "#ece1c5", stroke=INK, sw=0.8)
    s.poly([(hx - 8, hy - 20), (hx + 27, hy - 44), (hx + 62, hy - 20)], "#9a7f4c", INK, 0.8)
    for i in range(10):
        x = hx - 6 + i * 6.8
        s.line((x, hy - 20), (x + 14 - i * 1.4, hy - 40 + i * 2.1), INK, 0.5, 0.4)
    s.rect(hx + 22, hy - 12, 10, 12, "#4d3e2e", stroke=INK, sw=0.5)
    s.rect(hx + 8, hy - 15, 8, 6, "#4d3e2e", op=0.8)
    for tx, th in ((112, 38), (222, 32), (250, 40), (292, 28), (420, 30), (452, 36), (478, 26)):
        s.rect(tx - 1.5, 118 - th * 0.35, 3, th * 0.35, "#4a3b2b")
        s.ellipse(tx, 118 - th * 0.7, th * 0.42, th * 0.46, "#6f7d4a", op=0.9, stroke=INK, sw=0.6)
        s.ellipse(tx - th * 0.1, 118 - th * 0.78, th * 0.18, th * 0.2, "#8e9b61", op=0.8)
    # the paddy: terraced strips in perspective, standing water between the rows
    s.rect(0, 140, W, H - 140, s.lin([(0, "#b9ac6c", 0.62), (1, "#9a9a58", 0.62)]))
    for i in range(1, 12):
        t = i / 12
        y = 142 + (H - 142) * t ** 1.5
        bow = 14 * t
        s.path(f"M0,{y + 5 * t:.1f} Q{W / 2},{y - bow:.1f} {W},{y + 5 * t:.1f}", "none", INK, 0.7 + t * 0.5, 0.42)
        s.path(
            f"M0,{y + 2 * t:.1f} Q{W / 2},{y - bow + 3:.1f} {W},{y + 2 * t:.1f} L{W},{y + 4 + 9 * t:.1f} Q{W / 2},{y - bow + 6:.1f} 0,{y + 4 + 9 * t:.1f} Z",
            "#8aa5a0",
            op=0.30,
        )
        for _ in range(int(40 + 40 * t)):
            x = rng.uniform(0, W)
            yy = y + rng.uniform(0, 8 + 10 * t) - bow * (1 - abs(x / W * 2 - 1) ** 2) * 0.9
            h = 3 + 6 * t
            s.line((x, yy), (x - 1.5 * t, yy - h), "#4f5a2c", 0.8, 0.65)
            s.line((x, yy), (x + 2.2 * t, yy - h * 0.8), "#4f5a2c", 0.8, 0.55)
    # a raised path along the back of the paddy
    s.path("M0,146 Q300,138 560,144 T840,140", "none", "#6d5c43", 3.0, 0.5)
    _farmer(s, 330, 192, 1.55, 1.0)
    _farmer(s, 405, 186, 1.35, 0.7, -1)
    # a few birds
    for bx, by in ((520, 50), (548, 40), (580, 54)):
        s.path(f"M{bx - 8},{by} q4,-6 8,0 q4,-6 8,0", "none", INK, 1.1, 0.6)
    im = _render(s, W * 2, H * 2)
    return _fade(im, left=0.10, right=0.10, top=0.12, bottom=0.14, oval=True)


def backdrop():
    """A warm summer landscape the city stands in: sky, far hills, water, a green bank."""
    W, H = 312, 216
    s = Svg(W, H)
    rng = random.Random(5)
    s.rect(0, 0, W, H, s.lin([(0, "#8fb7cb"), (0.45, "#cfe0d8"), (0.62, "#f1dfbe"), (1, "#f1dfbe")]))
    for cx, hh, ww, c in ((60, 60, 220, "#8fa5ae"), (210, 74, 260, "#7f98a3"), (300, 50, 180, "#8da2a5")):
        s.path(f"M{cx - ww / 2},112 C{cx - ww * 0.2},{112 - hh} {cx + ww * 0.2},{112 - hh * 1.1} {cx + ww / 2},112 Z", c, op=0.85)
    s.path("M0,112 C70,98 140,108 210,100 C260,96 290,104 312,100 L312,128 L0,128 Z", "#7a9a76")
    s.rect(0, 112, W, 52, s.lin([(0, "#6aa7a6"), (1, "#3f7f86")]))
    for _ in range(70):
        x, y = rng.uniform(0, W), rng.uniform(116, 160)
        s.line((x, y), (x + rng.uniform(5, 16), y), "#e7f1ea", 0.8, 0.35)
    s.path("M0,158 C60,146 130,152 200,146 C250,142 290,150 312,146 L312,216 L0,216 Z", "#6f9b53")
    s.path("M0,184 C70,170 150,178 230,170 C270,167 300,172 312,170 L312,216 L0,216 Z", "#5e8a46")
    # blossom trees at the edges
    for tx, ty, r in ((22, 150, 26), (60, 160, 18), (284, 154, 24), (252, 164, 16)):
        s.rect(tx - 2, ty, 4, 22, "#4b392b")
        for _ in range(7):
            s.circle(tx + rng.uniform(-r * 0.8, r * 0.8), ty - rng.uniform(0, r * 0.9), r * rng.uniform(0.35, 0.6), rng.choice(("#e8a9b6", "#f0c3cb", "#dd93a5")), op=0.92)
    return downsample(_render(s, W * 2, H * 2), W * 2, H * 2)


def render_all():
    return {
        "paper": paper(),
        "flourish": flourish(),
        "city_panorama": panorama(),
        "city_fields": fields(),
        "city_backdrop": backdrop(),
    }
