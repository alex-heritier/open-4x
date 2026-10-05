"""Terrain overlay sheets: hills, mountains, and (below) trees, rivers, roads...

Every sheet is returned as `(rgb, alpha, shadow)`: magenta where alpha is
clear, `shadow` a soft-shadow mask for the exact-red palette entry
(`gfx.to_indexed(..., shadow=)`). All of it is drawn here, from nothing.
"""
import math
import random

from PIL import Image

from . import gfx, relief3d
from .gfx import KEY, mix


def _sampler(seed, cells=4, octaves=3, persistence=0.5, size=gfx.BOX):
    """Smooth noise as a function (x, y) -> 0..1."""
    im = gfx.lattice_noise(seed, cells, octaves, persistence, size)
    px = im.load()
    w, h = size

    def f(x, y):
        return px[int(x) % w, int(y) % h] / 255.0
    return f


def _ramp(stops, t):
    t = max(0.0, min(1.0, t))
    for (a, ca), (b, cb) in zip(stops, stops[1:]):
        if t <= b:
            return mix(ca, cb, 0 if b == a else (t - a) / (b - a))
    return stops[-1][1]


def _lit(c, lam, lo=0.62, hi=0.60):
    """Apply Lambert light to a colour, cooling the shade a touch."""
    k = lo + hi * lam
    r, g, b = (v * k for v in c)
    cool = max(0.0, 0.95 - k) * 22
    return (gfx.clamp8(r - cool * 0.3), gfx.clamp8(g - cool * 0.1), gfx.clamp8(b + cool * 0.5))


def sheet(cols, rows, cw, ch, cell):
    """Assemble `cell(index) -> (rgb, alpha, shadow)` into a sheet."""
    rgb = Image.new("RGB", (cols * cw, rows * ch), KEY)
    alpha = Image.new("L", rgb.size, 0)
    shadow = Image.new("L", rgb.size, 0)
    for i in range(cols * rows):
        c, r = i % cols, i // cols
        a, b, s = cell(i)
        rgb.paste(a, (c * cw, r * ch), b)
        alpha.paste(b, (c * cw, r * ch))
        shadow.paste(s, (c * cw, r * ch))
    return rgb, alpha, shadow


# ---------------------------------------------------------------- hills

HILL_GRASS = [(0.0, (92, 128, 52)), (0.45, (128, 164, 66)), (1.0, (186, 194, 104))]


def hill_cell(index, ground=HILL_GRASS):
    """One 128x72 hill cell: 1-3 domes, grass shading to dry tops."""
    rnd = random.Random(7000 + index)
    n = _sampler(3100 + index, 4, 3)
    fine = _sampler(3200 + index, 12, 2, 0.55)
    dry = _sampler(3300 + index, 3, 2)
    count = 1 + index % 3
    bumps = []
    for k in range(count):
        if count == 1:
            cx, cg, rx, rg, h = 64 + rnd.uniform(-6, 6), 36 + rnd.uniform(-3, 3), rnd.uniform(36, 44), rnd.uniform(17, 22), rnd.uniform(19, 24)
        else:
            off = (k - (count - 1) / 2) * rnd.uniform(24, 30)
            cx, cg = 64 + off, 36 + rnd.uniform(-5, 5) - (4 if k == 1 else 0)
            rx, rg, h = rnd.uniform(24, 30), rnd.uniform(14, 18), rnd.uniform(15, 21)
        bumps.append((cx, cg, rx, rg, h))

    def elev(x, g):
        if not relief3d.inside_diamond(x, g, 0.97):
            return None
        best = None
        for cx, cg, rx, rg, h in bumps:
            q = 1 - ((x - cx) / rx) ** 2 - ((g - cg) / rg) ** 2 + 0.34 * (n(x, g) - 0.5)
            if q > 0:
                v = h * q ** 0.8 + 1.4 * (fine(x, g) - 0.5)
                best = v if best is None else max(best, v)
        return None if best is None else max(0.0, best)

    def colour(x, g, e, lam, slope):
        c = _ramp(ground, e / 21.0)
        patch = dry(x, g)
        if patch > 0.58:
            c = mix(c, (186, 168, 100), min(0.65, (patch - 0.58) * 4))
        if slope > 0.9 and fine(x * 3, g * 3) > 0.55:
            c = mix(c, (132, 118, 92), 0.55)
        if e < 1.4:
            c = mix(c, (50, 70, 34), 0.45)
        return _lit(c, lam)

    img, alpha, sh = relief3d.render((128, 72), 8, elev, colour, sx=0.8, sg=1.1)
    return img, alpha, sh


def hills():
    return sheet(4, 4, 128, 72, hill_cell)


# ---------------------------------------------------------------- mountains

ROCK = [(0.0, (104, 118, 70)), (0.16, (150, 134, 108)), (0.55, (176, 162, 142)), (1.0, (226, 220, 210))]
SNOW_LINE = 0.40


def mountain_cell(index, snow=False):
    """One 128x88 mountain cell: a footprint filling the diamond, 2-4 peaks."""
    rnd = random.Random(8000 + index)
    rough = _sampler(4100 + index, 5, 4, 0.55)
    fine = _sampler(4200 + index, 14, 2, 0.6)
    edge = _sampler(4300 + index, 6, 2)
    npeaks = 2 + index % 3
    peaks = []
    for k in range(npeaks):
        spread = 26 if npeaks > 2 else 20
        cx = 64 + (k - (npeaks - 1) / 2) * spread + rnd.uniform(-5, 5)
        cg = 30 + rnd.uniform(-8, 8) + (3 if k % 2 else -2)
        R = rnd.uniform(30, 44)
        h = rnd.uniform(34, 52) - abs(k - (npeaks - 1) / 2) * 5
        peaks.append((cx, cg, R, h))
    top = max(p[3] for p in peaks)

    def elev(x, g):
        d = abs(x + 0.5 - 64) / 64.0 + abs(g + 0.5 - 32) / 32.0
        if d > 0.99 - 0.10 * edge(x, g):
            return None
        best = 2.5 * rough(x, g)
        for cx, cg, R, h in peaks:
            # elliptical cone, ridged so the faces break into crags
            r = math.hypot((x - cx) / R, (g - cg) / (R * 0.52))
            if r < 1:
                ridge = 1 - abs(2 * rough(x * 1.3, g * 1.3) - 1)
                v = h * (1 - r) ** 1.05 * (0.72 + 0.5 * ridge) + 2.2 * (fine(x, g) - 0.5)
                best = max(best, v)
        # fade the rim into foothills so a tile blends with its neighbors
        best *= min(1.0, (1 - d) * 7 + 0.15)
        return best

    def colour(x, g, e, lam, slope):
        t = e / top
        c = _ramp(ROCK, t)
        if slope > 1.1:
            c = mix(c, (110, 98, 84), min(0.5, (slope - 1.1) * 0.5))
        if t < 0.1 and fine(x * 2, g * 2) > 0.52:
            c = mix(c, (94, 124, 58), 0.6)          # sparse grass on the foothills
        if snow and t > SNOW_LINE - 0.12 * fine(x, g) and lam > 0.22:
            c = mix(c, (244, 247, 252), min(1.0, (t - SNOW_LINE + 0.12) * 6))
        return _lit(c, lam, 0.58, 0.68)

    return relief3d.render((128, 88), 24, elev, colour, sx=0.6, sg=1.0, shadow_offset=(7, 3))


def mountains(snow=False):
    return sheet(4, 4, 128, 88, lambda i: mountain_cell(i, snow))
