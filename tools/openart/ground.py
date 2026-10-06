"""Ground textures and the 9x9 vertex-blend sheets (`xggc`, `wCSO`, ...).

Contract (src/blend.rs): a sheet is 9x9 cells of 128x64. Cell (col, row) has
the terrain digits N = col % 3, W = col // 3, E = row % 3, S = row // 3, each
indexing the sheet's terrain triple, and is a diamond centred on a tile
*corner* whose four vertices are the centres of the four tiles around it.

Every cell here is a real blend of the textures of its vertex terrains:

  * the texture of each terrain is lattice-periodic (see gfx.py), so cells
    that share an edge agree along it;
  * the blend weights are bilinear in the vertex digits, so along an edge
    only the two end vertices count;
  * a lattice-periodic noise field roughens the transition, so it reads as
    a ragged, organic border instead of a gradient.

Land next to water additionally gets a wet-sand band and surf, computed from
the same continuous score fields so it also lines up across cells.
"""
from PIL import Image, ImageChops, ImageFilter

from . import gfx
from .gfx import BOX, clamp8

# ---------------------------------------------------------------- textures


def _stripes(gray, freq, phase=0.0):
    import math
    return gray.point([clamp8(128 + 127 * math.sin(math.tau * (i / 255.0 * freq + phase))) for i in range(256)])


def _grass_blades(draw, x, y, rnd, light, dark):
    for _ in range(rnd.randint(2, 4)):
        dx = rnd.uniform(-2.5, 2.5)
        h = rnd.uniform(2.5, 5.5)
        lean = rnd.uniform(-1.5, 1.5)
        col = light if rnd.random() < 0.55 else dark
        draw.line([(x + dx, y), (x + dx + lean, y - h)], fill=col + (200,), width=1)


def grass(seed=1):
    n1 = gfx.lattice_noise(seed, cells=3, octaves=3)
    n2 = gfx.lattice_noise(seed + 1, cells=9, octaves=2, persistence=0.6)
    # Weight the fine field: a strong cell-wide gradient repeats on every
    # tile and shows up as diagonal banding across a whole map.
    base = gfx.colorize(gfx.levels(Image.blend(n1, n2, 0.82), 20, 235),
                        [(0, (58, 104, 36)), (0.45, (92, 146, 50)), (0.8, (128, 176, 66)), (1, (158, 198, 84))])
    lit = gfx.apply_light(base, gfx.relief(n2, 1.1), 0.8)

    def blades(d, x, y, rnd):
        _grass_blades(d, x, y, rnd, (176, 212, 96), (44, 84, 30))
    out = gfx.over(lit, gfx.scatter(seed + 7, 70, blades))

    def flower(d, x, y, rnd):
        c = rnd.choice([(250, 244, 214), (246, 216, 92), (232, 150, 170)])
        d.ellipse([x - 0.9, y - 0.9, x + 0.9, y + 0.9], fill=c + (230,))
    return gfx.over(out, gfx.scatter(seed + 9, 7, flower))


def plains(seed=2):
    n1 = gfx.lattice_noise(seed, cells=3, octaves=3)
    n2 = gfx.lattice_noise(seed + 1, cells=8, octaves=2, persistence=0.55)
    base = gfx.colorize(gfx.levels(Image.blend(n1, n2, 0.4), 20, 235),
                        [(0, (146, 138, 60)), (0.5, (184, 172, 84)), (0.85, (212, 198, 112)), (1, (228, 214, 134))])
    lit = gfx.apply_light(base, gfx.relief(n2, 1.4), 0.8)

    def straw(d, x, y, rnd):
        for _ in range(rnd.randint(2, 3)):
            dx = rnd.uniform(-2, 2)
            h = rnd.uniform(3, 7)
            c = (232, 214, 128) if rnd.random() < 0.6 else (118, 120, 50)
            d.line([(x + dx, y), (x + dx + rnd.uniform(-1.5, 1.5), y - h)], fill=c + (190,), width=1)
    out = gfx.over(lit, gfx.scatter(seed + 7, 55, straw))

    def pebble(d, x, y, rnd):
        d.ellipse([x - 1.2, y - 0.8, x + 1.2, y + 0.8], fill=(150, 130, 90, 180))
    return gfx.over(out, gfx.scatter(seed + 8, 8, pebble))


def desert(seed=3):
    n1 = gfx.lattice_noise(seed, cells=2, octaves=2)
    n2 = gfx.lattice_noise(seed + 1, cells=10, octaves=2, persistence=0.5)
    base = gfx.colorize(gfx.levels(Image.blend(n1, n2, 0.2), 25, 230),
                        [(0, (190, 156, 92)), (0.5, (222, 192, 124)), (1, (242, 222, 162))])
    # dune ripples: stripes of the broad field, only half-strength
    rip = _stripes(n1, 3.0)
    sh = Image.blend(gfx.relief(n2, 1.2), rip, 0.35)
    lit = gfx.apply_light(base, sh, 0.7)

    def stone(d, x, y, rnd):
        r = rnd.uniform(0.8, 1.6)
        d.ellipse([x - r, y - r * 0.7, x + r, y + r * 0.7], fill=(156, 126, 84, 210))
        d.ellipse([x - r + 0.4, y - r * 0.7 - 0.4, x + r - 0.4, y], fill=(220, 196, 150, 160))
    return gfx.over(lit, gfx.scatter(seed + 5, 14, stone))


def tundra(seed=4):
    n1 = gfx.lattice_noise(seed, cells=3, octaves=3)
    n2 = gfx.lattice_noise(seed + 1, cells=9, octaves=2, persistence=0.6)
    base = gfx.colorize(gfx.levels(Image.blend(n1, n2, 0.3), 20, 235),
                        [(0, (86, 98, 76)), (0.5, (118, 130, 100)), (1, (150, 160, 128))])
    snow_mask = gfx.levels(gfx.lattice_noise(seed + 3, cells=6, octaves=3), 165, 215).filter(ImageFilter.GaussianBlur(0.6))
    snow = Image.new("RGB", BOX, (226, 234, 238))
    out = Image.composite(snow, base, snow_mask)
    out = gfx.apply_light(out, gfx.relief(n2, 1.3), 0.8)

    def rock(d, x, y, rnd):
        r = rnd.uniform(0.8, 1.8)
        d.ellipse([x - r, y - r * 0.7, x + r, y + r * 0.7], fill=(90, 92, 88, 220))
    return gfx.over(out, gfx.scatter(seed + 6, 12, rock))


def ice(seed=5):
    n1 = gfx.lattice_noise(seed, cells=3, octaves=3)
    base = gfx.colorize(gfx.levels(n1, 20, 235), [(0, (206, 220, 232)), (0.6, (234, 242, 248)), (1, (252, 253, 255))])

    def crack(d, x, y, rnd):
        a = rnd.uniform(0, 6.283)
        import math
        px, py = x, y
        for _ in range(rnd.randint(3, 5)):
            a += rnd.uniform(-0.7, 0.7)
            nx, ny = px + math.cos(a) * rnd.uniform(4, 9), py + math.sin(a) * rnd.uniform(2, 5)
            d.line([(px, py), (nx, ny)], fill=(176, 196, 210, 190), width=1)
            px, py = nx, ny
    return gfx.over(base, gfx.scatter(seed + 2, 9, crack))


def _glints(seed, n, color, alpha=190, length=(3, 7)):
    def paint(d, x, y, rnd):
        l = rnd.uniform(*length)
        d.line([(x - l / 2, y), (x + l / 2, y)], fill=color + (alpha,), width=1)
        d.line([(x - l / 4, y + 1), (x + l / 4, y + 1)], fill=color + (alpha // 2,), width=1)
    return gfx.scatter(seed, n, paint)


def water(kind, seed):
    n1 = gfx.lattice_noise(seed, cells=3, octaves=3)
    n2 = gfx.lattice_noise(seed + 1, cells=8, octaves=2, persistence=0.55)
    ramps = {
        "coast": [(0, (92, 178, 192)), (0.5, (124, 204, 208)), (1, (170, 230, 226))],
        "sea": [(0, (36, 104, 168)), (0.5, (54, 136, 188)), (1, (92, 170, 210))],
        "ocean": [(0, (14, 48, 112)), (0.5, (24, 74, 144)), (1, (46, 108, 176))],
    }
    base = gfx.colorize(gfx.levels(Image.blend(n1, n2, 0.4), 20, 235), ramps[kind])
    sh = gfx.relief(n2, 1.5, light=(-1, -1))
    base = gfx.apply_light(base, sh, 0.7)
    glint = {"coast": (240, 252, 250), "sea": (186, 224, 244), "ocean": (120, 170, 220)}[kind]
    count = {"coast": 26, "sea": 20, "ocean": 16}[kind]
    return gfx.over(base, _glints(seed + 3, count, glint, 170))


_CACHE = {}


def texture(name):
    if name not in _CACHE:
        _CACHE[name] = {
            "grass": lambda: grass(11), "plains": lambda: plains(12), "desert": lambda: desert(13),
            "tundra": lambda: tundra(14), "ice": lambda: ice(15),
            "coast": lambda: water("coast", 21), "sea": lambda: water("sea", 22), "ocean": lambda: water("ocean", 23),
        }[name]()
    return _CACHE[name]


# ---------------------------------------------------------------- blending

LAND_SHEETS = {
    "xggc": ("grass", "grass", "coast"),
    "xtgc": ("tundra", "grass", "coast"),
    "xpgc": ("plains", "grass", "coast"),
    "xdgc": ("desert", "grass", "coast"),
    "xdpc": ("desert", "plains", "coast"),
    "xdgp": ("desert", "grass", "plains"),
}
WATER_SHEET = ("wCSO", ("coast", "sea", "ocean"))
WATER_TYPES = {"coast", "sea", "ocean"}

_NOISE = {}


def _noise_for(slot, amp):
    key = (slot, amp)
    if key not in _NOISE:
        n = gfx.lattice_noise(100 + slot * 17, cells=4, octaves=2, persistence=0.5)
        _NOISE[key] = n.point([clamp8(128 + (i - 128) * amp) for i in range(256)])
    return _NOISE[key]


def blend_cell(types, digits, vw, rough=0.5, feather=0.8):
    """One cell: `types` is the sheet's terrain triple, `digits` = (n, e, s, w)."""
    # Weight of each sheet digit: the sum of the vertex weights carrying it.
    weight = [Image.new("L", BOX, 0) for _ in range(3)]
    for key, dg in zip("NESW", digits):
        weight[dg] = gfx.sat_add(weight[dg], vw[key])
    # Roughen with one noise field per digit, then take the strongest.
    score = [ImageChops.add(weight[t], _noise_for(t, rough), 1.0, -128) for t in range(3)]
    best = ImageChops.lighter(ImageChops.lighter(score[0], score[1]), score[2])
    wins = []
    taken = Image.new("L", BOX, 0)
    for t in range(3):
        w = ImageChops.subtract(best, score[t]).point([255 if i == 0 else 0 for i in range(256)])
        w = ImageChops.subtract(w, taken)
        taken = ImageChops.lighter(taken, w)
        wins.append(w)
    out = texture(types[0]).copy()
    for t in (1, 2):
        m = wins[t].filter(ImageFilter.GaussianBlur(feather)) if feather else wins[t]
        out.paste(texture(types[t]), mask=m)
    # Shoreline where land meets water.
    wdig = [t for t in range(3) if types[t] in WATER_TYPES]
    ldig = [t for t in range(3) if types[t] not in WATER_TYPES]
    if wdig and ldig and len(wdig) < 3:
        water_s = score[wdig[0]]
        for t in wdig[1:]:
            water_s = ImageChops.lighter(water_s, score[t])
        land_s = score[ldig[0]]
        for t in ldig[1:]:
            land_s = ImageChops.lighter(land_s, score[t])
        d = ImageChops.subtract(land_s, water_s, 1.0, 128)        # > 128: land
        if d.getextrema()[0] < 150 and d.getextrema()[1] > 106:
            wet = d.point([clamp8(170 * max(0.0, 1 - abs(i - 138) / 14.0)) if i >= 128 else 0 for i in range(256)])
            foam = d.point([clamp8(215 * max(0.0, 1 - (128 - i) / 18.0)) if 100 < i <= 128 else 0 for i in range(256)])
            darker = ImageChops.multiply(out, Image.new("RGB", BOX, (168, 150, 116)))
            out = Image.composite(darker, out, wet)
            out = Image.composite(Image.new("RGB", BOX, (236, 248, 246)), out, foam.filter(ImageFilter.GaussianBlur(0.6)))
    return out


def blend_sheet(types, rough=0.5, feather=0.8, cols=9, rows=9):
    """The full 9x9 sheet as (RGB, alpha) images."""
    vw = gfx.vertex_weights()
    cut = gfx.diamond(grow=1.0)
    rgb = Image.new("RGB", (cols * 128, rows * 64), gfx.KEY)
    alpha = Image.new("L", rgb.size, 0)
    for row in range(rows):
        for col in range(cols):
            digits = (col % 3, row % 3, row // 3, col // 3)       # n, e, s, w
            cell = blend_cell(types, digits, vw, rough, feather)
            rgb.paste(cell, (col * 128, row * 64), cut)
            alpha.paste(cut, (col * 128, row * 64))
    return rgb, alpha


def uniform_sheet(name, cols, rows, vary=True):
    """A sheet whose every cell is one terrain's pure texture."""
    cut = gfx.diamond(grow=1.0)
    rgb = Image.new("RGB", (cols * 128, rows * 64), gfx.KEY)
    alpha = Image.new("L", rgb.size, 0)
    tex = texture(name)
    for row in range(rows):
        for col in range(cols):
            rgb.paste(tex, (col * 128, row * 64), cut)
            alpha.paste(cut, (col * 128, row * 64))
    return rgb, alpha
