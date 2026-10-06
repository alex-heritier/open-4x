"""Procedural imaging toolkit for the open test assets (pure Pillow).

Nothing here samples, traces or palette-copies an installed Civ3. Everything
is built from random numbers, arithmetic and drawing primitives.

The one idea worth knowing: **lattice-periodic textures**. Civ3's ground
cells are 128x64 diamonds centred on tile corners, and neighbouring cells
sit a lattice step (64, 32) / (64, -32) apart. A texture T with

    T(p + (64, 32)) == T(p)   and   T(p + (64, -32)) == T(p)

looks identical wherever it is cut, so any two cells that share an edge show
the same pixels along it and the map has no seams. `lattice_noise` makes such
fields; `scatter` places detail marks with the same symmetry.
"""
import random

from PIL import Image, ImageChops, ImageDraw, ImageFilter

CELL_W, CELL_H = 128, 64
BOX = (CELL_W, CELL_H)
KEY = (255, 0, 255)          # transparent in every sheet (palette index 255)

_BICUBIC = Image.Resampling.BICUBIC
_AFFINE = Image.Transform.AFFINE


# ---------------------------------------------------------------- colour

def clamp8(v):
    return 0 if v < 0 else 255 if v > 255 else int(v)


def mix(a, b, t):
    """Linear blend of two RGB triples."""
    return tuple(clamp8(round(x + (y - x) * t)) for x, y in zip(a, b))


def shade_rgb(c, k):
    """Scale brightness (k < 1 darkens, k > 1 lightens toward white)."""
    if k <= 1:
        return tuple(clamp8(round(v * k)) for v in c)
    return mix(c, (255, 255, 255), k - 1)


def ramp_lut(stops):
    """Three 256-entry tables from `[(t, (r, g, b)), ...]`, t in 0..1."""
    stops = sorted(stops, key=lambda s: s[0])
    out = ([], [], [])
    for i in range(256):
        t = i / 255.0
        lo = stops[0]
        hi = stops[-1]
        for a, b in zip(stops, stops[1:]):
            if a[0] <= t <= b[0]:
                lo, hi = a, b
                break
        else:
            lo = hi = stops[0] if t < stops[0][0] else stops[-1]
        span = hi[0] - lo[0]
        f = 0.0 if span <= 0 else (t - lo[0]) / span
        c = mix(lo[1], hi[1], f)
        for ch in range(3):
            out[ch].append(c[ch])
    return out


def colorize(gray, stops):
    """Map an 'L' image through a colour ramp."""
    r, g, b = ramp_lut(stops)
    return Image.merge("RGB", (gray.point(r), gray.point(g), gray.point(b)))


# ---------------------------------------------------------------- noise

def _tile(g, k=4):
    n = g.width
    t = Image.new("L", (n * k, n * k))
    for i in range(k):
        for j in range(k):
            t.paste(g, (i * n, j * n))
    return t


def lattice_noise(seed, cells=4, octaves=3, persistence=0.5, size=BOX, contrast=True):
    """Smooth noise ('L') with the cell lattice's symmetry.

    Sampled in lattice coordinates a = x/w + y/h, b = x/w - y/h: a shift by
    (w/2, h/2) adds 1 to a, a shift by (w/2, -h/2) adds 1 to b, and the
    underlying value grids repeat every unit in both, so the field repeats
    under both lattice steps (and so under the box itself).
    """
    w, h = size
    acc = None
    total = 0.0
    amp = 1.0
    for o in range(octaves):
        n = cells * (2 ** o)
        rnd = random.Random(seed * 7919 + o * 104729 + 13)
        grid = Image.new("L", (n, n))
        grid.putdata([rnd.randrange(256) for _ in range(n * n)])
        tiled = _tile(grid, 4)
        sx, sy = n / w, n / h
        layer = tiled.transform((w, h), _AFFINE, (sx, sy, n, sx, -sy, n * 1.5), resample=_BICUBIC)
        if acc is None:
            acc, total = layer, amp
        else:
            acc = Image.blend(acc, layer, amp / (total + amp))
            total += amp
        amp *= persistence
    if contrast:
        acc = stretch(acc)
    return acc


def stretch(gray, lo=2, hi=98):
    """Percentile contrast stretch of an 'L' image."""
    hist = gray.histogram()
    n = sum(hist)
    a = b = 0
    run = 0
    for i, v in enumerate(hist):
        run += v
        if run >= n * lo / 100.0:
            a = i
            break
    run = 0
    for i, v in enumerate(hist):
        run += v
        if run >= n * hi / 100.0:
            b = i
            break
    if b <= a:
        return gray
    scale = 255.0 / (b - a)
    return gray.point([clamp8((i - a) * scale) for i in range(256)])


def levels(gray, lo, hi, gamma=1.0):
    """Remap 'L' so lo..hi spans 0..255, with optional gamma."""
    span = max(1, hi - lo)
    return gray.point([clamp8(255 * (max(0.0, min(1.0, (i - lo) / span)) ** gamma)) for i in range(256)])


def relief(height, strength=2.0, light=(-1, -1)):
    """Shading ('L', 128 = flat) of a box-periodic height field.

    `light` is the direction the light comes from, in pixels. Lit slopes are
    brighter than 128, shadowed ones darker."""
    shifted = ImageChops.offset(height, -light[0], -light[1])
    d = ImageChops.subtract(height, shifted, 1, 128)
    return d.point([clamp8(128 + (i - 128) * strength) for i in range(256)])


def apply_light(rgb, shade, amount=1.0):
    """Modulate an RGB image by an 'L' shading map (128 neutral)."""
    if amount != 1.0:
        shade = shade.point([clamp8(128 + (i - 128) * amount) for i in range(256)])
    return ImageChops.soft_light(rgb, shade.convert("RGB"))


# ---------------------------------------------------------------- details

def scatter(seed, count, paint, size=BOX, lattice=True):
    """An RGBA layer of `count` marks that also respects the lattice symmetry.

    `paint(draw, x, y, rnd)` draws one mark at (x, y). Each mark is painted at
    its random position, at the half-lattice shift (w/2, h/2), and at every
    wrap of the box, with a fresh `Random` of the same seed each time so the
    copies are identical."""
    w, h = size
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer, "RGBA")
    rnd = random.Random(seed * 40503 + 17)
    shifts = ((0, 0), (w // 2, h // 2)) if lattice else ((0, 0),)
    for _ in range(count):
        x, y = rnd.random() * w, rnd.random() * h
        ms = rnd.randrange(1 << 30)
        for sx, sy in shifts:
            for tx in (-w, 0, w):
                for ty in (-h, 0, h):
                    paint(d, x + sx + tx, y + sy + ty, random.Random(ms))
    return layer


def over(base, layer):
    """Alpha-composite RGBA `layer` onto RGB `base` (returns RGB)."""
    out = base.copy()
    out.paste(layer.convert("RGB"), mask=layer.getchannel("A"))
    return out


# ---------------------------------------------------------------- masks

def diamond(size=BOX, grow=0.0, feather=0):
    """'L' mask (255 inside) of the tile diamond, optionally grown by `grow`
    pixels so adjacent cells overlap and leave no hairline between them."""
    w, h = size
    k = 4
    big = Image.new("L", (w * k, h * k), 0)
    d = ImageDraw.Draw(big)
    gx, gy = grow * k, grow * k / 2
    d.polygon([(w * k / 2, -gy), (w * k + gx, h * k / 2), (w * k / 2, h * k + gy), (-gx, h * k / 2)], fill=255)
    out = big.resize(size, Image.Resampling.LANCZOS)
    if feather:
        out = out.filter(ImageFilter.GaussianBlur(feather))
    return out


def vertex_weights(size=BOX):
    """Bilinear weights of the diamond's N, E, S, W vertices as 'L' images.

    With u = (p + q + 1) / 2 and v = (p - q + 1) / 2 for p, q in -1..1, the
    vertices sit at (u, v) = N(0,1) E(1,1) S(1,0) W(0,0). Along an edge only
    its two end vertices carry weight, which is what makes neighbouring cells
    agree on their shared edge."""
    w, h = size
    out = {k: Image.new("L", size) for k in "NESW"}
    data = {k: [] for k in "NESW"}
    for y in range(h):
        q = (y + 0.5 - h / 2) / (h / 2)
        for x in range(w):
            p = (x + 0.5 - w / 2) / (w / 2)
            u = min(1.0, max(0.0, (p + q + 1) / 2))
            v = min(1.0, max(0.0, (p - q + 1) / 2))
            data["N"].append(round(255 * (1 - u) * v))
            data["E"].append(round(255 * u * v))
            data["S"].append(round(255 * u * (1 - v)))
            data["W"].append(round(255 * (1 - u) * (1 - v)))
    for k in "NESW":
        out[k].putdata(data[k])
    return out


def sat_add(a, b):
    return ImageChops.add(a, b)


# ---------------------------------------------------------------- indexed output

SHADOW = (255, 0, 0)


def to_indexed(rgb, alpha=None, colors=254, dither=False, shadow=None):
    """Quantise `rgb` to a 'P' image whose index 255 is the transparency key.

    Pixels with `alpha` < 128 become index 255 (magenta). Everything else
    gets one of `colors` (<= 254) adaptive colours. Civ3's loaders turn exact
    red (255,0,0) into a translucent shadow, so `shadow` (an 'L' mask, set
    where a soft shadow should fall on otherwise transparent pixels) is
    written as index 254 with that colour."""
    rgb = rgb.convert("RGB")
    colors = min(colors, 254)
    if alpha is not None:
        opaque = alpha.point([255 if i >= 128 else 0 for i in range(256)])
        bb = opaque.getbbox()
        if bb is None:
            q = Image.new("P", rgb.size, 255)
            q.putpalette(bytes([0] * 762 + list(SHADOW) + list(KEY)))
            if shadow is not None:
                q.paste(254, mask=shadow.point([255 if i >= 128 else 0 for i in range(256)]))
            return q
        # Fill the keyed area with the mean opaque colour so it does not
        # steal palette entries.
        stat = rgb.copy()
        stat.paste((0, 0, 0), mask=ImageChops.invert(opaque))
        r, g, b = [sum(i * c for i, c in enumerate(ch.histogram())) for ch in stat.split()]
        n = max(1, sum(opaque.histogram()[255:]))
        fill = (round(r / n), round(g / n), round(b / n))
        src = rgb.copy()
        src.paste(fill, mask=ImageChops.invert(opaque))
    else:
        opaque = None
        src = rgb
    q = src.quantize(colors=colors, method=Image.Quantize.MEDIANCUT,
                     dither=Image.Dither.FLOYDSTEINBERG if dither else Image.Dither.NONE)
    pal = list(q.getpalette() or [])[: colors * 3]
    pal += [0] * (762 - len(pal))
    pal += list(SHADOW) + list(KEY)
    q.putpalette(pal)
    if opaque is not None:
        q.paste(255, mask=ImageChops.invert(opaque))
        if shadow is not None:
            hard = shadow.point([255 if i >= 128 else 0 for i in range(256)])
            q.paste(254, mask=ImageChops.multiply(hard, ImageChops.invert(opaque)))
    return q


def indexed_from_rgba(rgba, colors=254, dither=False, shadow=None):
    return to_indexed(rgba.convert("RGB"), rgba.getchannel("A"), colors, dither, shadow)


# ---------------------------------------------------------------- sprites


def outline(rgba, color=(20, 18, 24), width=1, alpha=255):
    """A hard outline around the opaque part of an RGBA sprite."""
    a = rgba.getchannel("A").point([255 if i >= 96 else 0 for i in range(256)])
    grown = a.filter(ImageFilter.MaxFilter(2 * width + 1))
    ring = ImageChops.subtract(grown, a)
    out = Image.new("RGBA", rgba.size, color + (0,))
    out.putalpha(ring.point([alpha if i else 0 for i in range(256)]))
    out.alpha_composite(rgba)
    return out


