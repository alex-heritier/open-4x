"""Layered dual-grid terrain atlases, painted at 1.5x (192 x 96 cells, drawn 128 x 64).

The ground is split into two sprite layers, the way a Civ3 tile is split into base terrain and
overlays, so any mixture of climates and depths is covered without a combinatorial sheet per
terrain triple:

* **ground** (`terrain/ground.png`): digits 0 grassland, 1 plains, 2 desert, 3 tundra.  A cell has
  the four surrounding tile centres as its N/E/S/W vertices; a vertex that is water carries the
  ground type of the land beside it (the water layer covers it).
* **water** (`terrain/water.png`): digits 0 land, 1 coast, 2 sea, 3 ocean.  It paints the shoreline
  (beach strip, foam, surf, shallows) with alpha over the ground, so a coast cell is two sprites.

Both sheets are 16 x 16 cells, `index = (4*S + E)*16 + 4*W + N`, followed by two rows of tonal
variants of the pure cells (`256 + 4*(k-1) + digit`, k in 1..6).  All texture fields are periodic
on the diamond lattice (noise.py), so two cells that share an edge evaluate identical pixels
there: a border pixel depends only on the two vertex types of that edge.
"""

import numpy as np
from PIL import Image

from . import palette as P
from .noise import fbm, flat_noise, lattice_coords, unit
from .util import to_image

W, H = 192, 96  # cell size in the file; the game draws every cell at 128 x 64 world units
BASE = 4  # digits per vertex
GRID = BASE * BASE  # sheet is GRID x GRID cells
VARIANT_ROWS = 2
VARIANTS = 6
EDGE = 1.045  # cells are cut slightly outside the diamond so neighbours overlap (no hairlines)
HI_COAST = 0.34  # how far noise may push the shoreline off the straight vertex blend
GROUND_NAMES = ("grassland", "plains", "desert", "tundra")
WATER_NAMES = ("land", "coast", "sea", "ocean")
# how deep each water digit reads, 0 = shoreline shallows, 1 = open ocean
WATER_DEPTH = np.array([0.0, 0.16, 0.58, 1.0], dtype=np.float32)

GRASSLAND_STOPS = [(0.00, "#3a5c1e"), (0.25, "#4c7223"), (0.50, "#63892b"), (0.75, "#7c9d35"), (1.00, "#9ab648")]
PLAINS_STOPS = [(0.00, "#686c2d"), (0.25, "#80833a"), (0.50, "#979441"), (0.75, "#b0a752"), (1.00, "#c9bb6c")]
DESERT_STOPS = [(0.00, "#ad8b4a"), (0.35, "#c9a766"), (0.65, "#ddc183"), (1.00, "#eddcab")]
TUNDRA_STOPS = [(0.00, "#566049"), (0.30, "#6e7860"), (0.60, "#8a9378"), (1.00, "#b0b6a2")]


def _vertex_weights(u, v):
    """Pyramid weights of the N/E/S/W vertices plus the centre share."""
    w = np.stack([np.maximum(-v, 0), np.maximum(u, 0), np.maximum(v, 0), np.maximum(-u, 0)])
    centre = np.maximum(1.0 - np.abs(u) - np.abs(v), 0.0)
    w = w + centre / 4.0
    return w / w.sum(axis=0, keepdims=True)  # extrapolates smoothly beyond the diamond


class Fields:
    """Cell-independent painted material fields (computed once, reused by every cell)."""

    def __init__(self):
        u, v, a, b = lattice_coords(W, H)
        self.u, self.v, self.a, self.b = u, v, a, b
        self.coast = np.clip(fbm(a, b, 4, 3, seed=1, warp=0.2, gain=0.42) * 1.8, -1, 1)
        self.beach_edge = fbm(a, b, 10, 2, seed=2) * 1.6
        self.sand_noise = np.clip(fbm(a, b, 5, 4, seed=3, warp=0.15) * 1.8, -1, 1)
        self.foam_noise = fbm(a, b, 44, 2, seed=4)
        self.bump = self._bump()
        self.ground = [self._grassland(), self._plains(), self._desert(), self._tundra()]
        # one low-frequency perturbation per ground type: ragged, organic ecotones
        self.edge_noise = [fbm(a, b, 6, 3, seed=60 + 7 * k, warp=0.2) * 1.5 for k in range(4)]
        self.beach = self._beach()
        self.ripple = fbm(a, b, 8, 3, seed=31, aniso=2.5, warp=0.12)
        self.ripple2 = fbm(a, b, 16, 2, seed=32, aniso=3.5, warp=0.10)
        self.sparkle = fbm(a, b, 70, 1, seed=33)
        self.swell = fbm(a, b, 3, 2, seed=34, warp=0.15)

    def _height(self, du=0.0, dv=0.0):
        """Small-scale relief used for the painterly bump shading (periodic)."""
        da, db = (du + dv) / 2, (du - dv) / 2
        a, b = self.a, self.b
        return 0.6 * fbm(a + da, b + db, 28, 2, seed=21) + 0.4 * fbm(a + da, b + db, 60, 1, seed=22)

    def _bump(self):
        e = 1.2 / (W / 2)
        # light from the upper left: lit where the height falls toward the lower right
        return (self._height(-e, -e * 2) - self._height(e, e * 2)) * 2.4

    def _grassland(self):
        a, b = self.a, self.b
        low = fbm(a, b, 3, 3, seed=11, warp=0.2)
        mid = fbm(a, b, 11, 3, seed=12, warp=0.12)
        fine = fbm(a, b, 46, 2, seed=13)
        t = unit(1.15 * low + 1.0 * mid + 0.7 * fine)
        col = P.ramp(t, GRASSLAND_STOPS)
        meadow = P.smoothstep(0.15, 0.55, fbm(a, b, 5, 3, seed=14, warp=0.25) * 1.5)
        col = P.mix(col, P.hx("#95a548"), meadow * 0.30)
        shade = P.smoothstep(0.10, 0.45, fbm(a, b, 18, 2, seed=15) * 1.5)
        col = col * (1.0 - 0.26 * shade)[..., None]
        col = col * (1.0 + 0.30 * self.bump)[..., None]
        tuft = P.smoothstep(0.28, 0.5, fbm(a, b, 64, 1, seed=16) * 1.7)
        col = col * (1.0 - 0.22 * tuft)[..., None]
        return np.clip(col, 0, 1)

    def _plains(self):
        a, b = self.a, self.b
        low = fbm(a, b, 3, 3, seed=111, warp=0.2)
        mid = fbm(a, b, 9, 3, seed=112, warp=0.12)
        fine = fbm(a, b, 42, 2, seed=113)
        t = unit(1.1 * low + 1.0 * mid + 0.8 * fine)
        col = P.ramp(t, PLAINS_STOPS)
        # sun-bleached ochre drifts and the odd strip of darker scrub, like mottled steppe
        ochre = P.smoothstep(0.0, 0.40, fbm(a, b, 5, 3, seed=114, warp=0.25) * 1.4)
        col = P.mix(col, P.hx("#a98f4a"), ochre * 0.45)
        scrub = P.smoothstep(0.12, 0.5, fbm(a, b, 14, 2, seed=115) * 1.5)
        col = P.mix(col, P.hx("#6a6a30"), scrub * 0.32)
        streak = fbm(a, b, 10, 2, seed=116, aniso=3.0, warp=0.1)
        col = col * (1.0 + 0.07 * streak)[..., None]
        col = col * (1.0 + 0.28 * self.bump)[..., None]
        return np.clip(col, 0, 1)

    def _desert(self):
        a, b = self.a, self.b
        t = unit(1.3 * fbm(a, b, 6, 3, seed=41, warp=0.2) + 0.8 * fbm(a, b, 40, 2, seed=42))
        col = P.ramp(t, DESERT_STOPS)
        dunes = fbm(a, b, 7, 2, seed=43, aniso=4.0, warp=0.25)
        crest = 1.0 - np.abs(dunes * 3.0)
        col = col * (1.0 - 0.10 * P.smoothstep(0.55, 0.95, crest))[..., None]
        col = col * (1.0 + 0.07 * dunes)[..., None]
        rock = P.smoothstep(0.30, 0.55, fbm(a, b, 9, 3, seed=44, warp=0.2) * 1.6)
        col = P.mix(col, P.hx("#9b7c4a"), rock * 0.28)
        col = col * (1.0 + 0.20 * self.bump)[..., None]
        return np.clip(col, 0, 1)

    def _tundra(self):
        a, b = self.a, self.b
        t = unit(1.2 * fbm(a, b, 4, 3, seed=141, warp=0.2) + 0.9 * fbm(a, b, 30, 2, seed=142))
        col = P.ramp(t, TUNDRA_STOPS)
        moss = P.smoothstep(0.05, 0.45, fbm(a, b, 8, 3, seed=143, warp=0.2) * 1.5)
        col = P.mix(col, P.hx("#6b7a43"), moss * 0.30)
        drift = fbm(a, b, 2, 3, seed=144, warp=0.3, gain=0.4) * 2.0
        snow = P.smoothstep(-0.12, 0.34, drift) * (0.80 + 0.20 * P.smoothstep(-0.3, 0.3, fbm(a, b, 9, 2, seed=146) * 1.6))
        lit = 0.95 + 0.22 * np.clip(self.bump, -0.25, 0.25) + 0.025 * fbm(a, b, 30, 2, seed=147)
        col = P.mix(col, P.hx("#e4ecef") * lit[..., None], snow * 0.9)
        speck = P.smoothstep(0.30, 0.5, fbm(a, b, 56, 1, seed=145) * 1.7)
        col = col * (1.0 - 0.20 * speck)[..., None]
        col = col * (1.0 + 0.22 * self.bump)[..., None]
        return np.clip(col, 0, 1)

    def _beach(self):
        """Pale shingle/sand that rims every shoreline, whatever the land behind it."""
        a, b = self.a, self.b
        t = unit(1.3 * fbm(a, b, 6, 3, seed=241, warp=0.2) + 0.8 * fbm(a, b, 40, 2, seed=242))
        col = P.ramp(t, DESERT_STOPS)
        col = P.mix(col, P.hx("#e9dfc0"), 0.35)
        return np.clip(col * (1.0 + 0.20 * self.bump)[..., None], 0, 1)


def _edge_alpha(f):
    r = np.abs(f.u) + np.abs(f.v)
    return np.clip((EDGE - r) / 0.0175 + 0.5, 0.0, 1.0)


def render_ground(f: Fields, n: int, e: int, s: int, w_: int) -> np.ndarray:
    """RGBA float image of one ground cell for vertex ground digits 0..3."""
    vw = _vertex_weights(f.u, f.v)  # N, E, S, W
    digits = (n, e, s, w_)
    present = sorted(set(digits))
    if len(present) == 1:
        rgb = f.ground[present[0]]
    else:
        score = []
        for k in present:
            mass = sum(vw[i] for i in range(4) if digits[i] == k)
            score.append(mass + 0.17 * f.edge_noise[k])
        score = np.stack(score) * 11.0
        score -= score.max(axis=0, keepdims=True)
        wgt = np.exp(score)
        wgt /= wgt.sum(axis=0, keepdims=True)
        rgb = sum(wgt[j][..., None] * f.ground[k] for j, k in enumerate(present))
    return np.dstack([np.clip(rgb, 0, 1), _edge_alpha(f)])


def _water(f: Fields, depth):
    """Painted sea.  `depth` in 0..1 picks the colour ramp; ripples and glints are fine-grained on
    purpose: open-sea cells repeat, so no blob may stand out per tile."""
    base = P.ramp(depth, P.WATER_STOPS)
    shade = 1.0 + 0.30 * f.ripple + 0.16 * f.ripple2
    base = base * shade[..., None]
    ridge = 1.0 - np.abs(f.ripple2 * 3.2)
    streak = P.smoothstep(0.72, 0.97, ridge)
    base = P.mix(base, P.hx("#78c4cc"), streak * 0.20 * (1.0 - 0.5 * depth))
    ridge1 = 1.0 - np.abs(f.ripple * 3.0)
    base = P.mix(base, P.hx("#0f3f52"), P.smoothstep(0.78, 0.97, ridge1) * 0.20)
    glint = P.smoothstep(0.55, 0.85, f.sparkle * 1.7)
    return P.mix(base, P.hx("#d8f1ec"), glint * (0.24 - 0.10 * depth))


def render_water(f: Fields, n: int, e: int, s: int, w_: int) -> np.ndarray:
    """RGBA float image of one water cell for vertex digits 0 land, 1 coast, 2 sea, 3 ocean."""
    vw = _vertex_weights(f.u, f.v)
    digits = (n, e, s, w_)
    is_water = np.array([d != 0 for d in digits], dtype=np.float32)
    t_water = sum(vw[i] * is_water[i] for i in range(4))
    alpha_edge = _edge_alpha(f)
    # depth the vertices ask for, averaged over the water vertices only
    ask = sum(vw[i] * is_water[i] * WATER_DEPTH[digits[i]] for i in range(4)) / np.maximum(t_water, 1e-4)

    if t_water.min() >= 1.0 - 1e-6:  # open water: no shoreline anywhere in this cell
        rgb = _water(f, np.clip(ask + 0.07 * f.swell, 0, 1))
        return np.dstack([np.clip(rgb, 0, 1), alpha_edge])

    # signed shoreline margin: > 0 water, < 0 land
    base_m = 2.0 * t_water - 1.0
    m = base_m + HI_COAST * f.coast * (1.0 - np.abs(base_m))
    water_amt = P.smoothstep(-0.018, 0.018, m)

    # shallows hug the shore and deepen toward whatever the water vertices ask for
    shore = np.clip(m / 0.9, 0.0, 1.0)
    depth = np.clip((ask + 0.07 * f.swell) * shore, 0.0, 1.0)  # == the open-water formula once m >= 0.9
    water = _water(f, depth)
    near = np.abs(m - 0.02)
    foam = 1.0 - P.smoothstep(0.010, 0.050, near + 0.030 * f.foam_noise)
    swell = 1.0 - P.smoothstep(0.012, 0.040, np.abs(m - 0.17 + 0.03 * f.foam_noise))
    swell = swell * P.smoothstep(0.0, 0.3, np.abs(f.foam_noise) + 0.2)
    water = P.mix(water, P.hx(P.FOAM), np.clip(swell * 0.34, 0, 1))
    shore_glow = (1.0 - P.smoothstep(0.0, 0.16, m)) * P.smoothstep(-0.01, 0.01, m)
    water = P.mix(water, P.hx("#8fd6cc"), shore_glow * 0.30)

    # beach strip on the land side, fading out inland (alpha), darker where the sea wets it
    strip = P.smoothstep(-0.40, -0.07, m + f.beach_edge * 0.10) * (1.0 - P.smoothstep(0.0, 0.03, m))
    sand_w = P.smoothstep(0.25, 0.75, strip + 0.25 * f.sand_noise)
    wet = P.smoothstep(-0.13, -0.01, m)
    beach = P.mix(f.beach, f.beach * P.hx("#b49a62") / 0.75, wet * 0.55)
    beach = beach * (1.0 - 0.18 * wet)[..., None]
    beach_a = P.smoothstep(0.10, 0.62, sand_w * strip + 0.5 * strip)

    rgb = P.mix(beach, water, water_amt)
    alpha = water_amt + (1.0 - water_amt) * beach_a
    rgb = P.mix(rgb, P.hx(P.FOAM), np.clip(foam * 0.92, 0, 1))
    alpha = np.maximum(alpha, np.clip(foam * 0.92, 0, 1))
    # a hair-thin dark waterline under the foam reads as the painted shore contour
    contour = 1.0 - P.smoothstep(0.0, 0.020, np.abs(m + 0.045))
    rgb = rgb * (1.0 - 0.20 * contour * (1.0 - foam))[..., None]
    alpha = np.maximum(alpha, 0.55 * contour)
    return np.dstack([np.clip(rgb, 0, 1), np.clip(alpha, 0, 1) * alpha_edge])


def interior_weight(f: Fields):
    """1 in the middle of the diamond, 0 on its border: variants only change the inside."""
    r = np.abs(f.u) + np.abs(f.v)
    return P.smoothstep(0.90, 0.30, r)


# soft tonal drift per pure terrain: (amplitude, per-channel response)
_GROUND_VARIANT = {
    0: (0.20, np.array([0.95, 1.00, 0.72], dtype=np.float32)),  # grassland drifts yellow <-> deep green
    1: (0.20, np.array([1.00, 0.97, 0.78], dtype=np.float32)),
    2: (0.18, np.array([1.00, 0.95, 0.82], dtype=np.float32)),
    3: (0.18, np.array([0.97, 1.00, 0.97], dtype=np.float32)),
}
_WATER_VARIANT = {
    1: (0.20, np.array([0.80, 1.00, 1.04], dtype=np.float32)),
    2: (0.26, np.array([0.80, 1.00, 1.08], dtype=np.float32)),
    3: (0.30, np.array([0.78, 0.98, 1.10], dtype=np.float32)),
}


def _drift(base, style, seed, f):
    amp, resp = style
    drift = 0.6 * flat_noise(W, H, 2, 3, seed=seed) + 0.4 * flat_noise(W, H, 5, 2, seed=seed + 1)
    delta = np.clip(drift * 1.7, -1, 1) * amp * interior_weight(f)
    rgb = base[..., :3] * (1.0 + delta[..., None] * resp)
    return np.dstack([np.clip(rgb, 0, 1), base[..., 3]])


def cell_position(index: int):
    """(column, row) of an atlas index; variants simply continue the row-major order."""
    row, col = divmod(index, GRID)
    return col, row


def _blank_sheet():
    rows = GRID + VARIANT_ROWS
    return np.zeros((H * rows, W * GRID, 4), dtype=np.float32)


def _put(sheet, index, cell):
    col, row = cell_position(index)
    sheet[row * H : (row + 1) * H, col * W : (col + 1) * W] = cell


def build_ground(f: Fields | None = None, progress=None) -> Image.Image:
    f = f or Fields()
    sheet = _blank_sheet()
    for index in range(GRID * GRID):
        row, col = divmod(index, GRID)
        digits = (col % BASE, row % BASE, row // BASE, col // BASE)  # N, E, S, W
        _put(sheet, index, render_ground(f, *digits))
        if progress:
            progress(index + 1, GRID * GRID)
    for k in range(1, VARIANTS + 1):
        for digit in range(BASE):
            base = render_ground(f, digit, digit, digit, digit)
            _put(sheet, GRID * GRID + BASE * (k - 1) + digit, _drift(base, _GROUND_VARIANT[digit], 500 + 31 * digit + 7 * k, f))
    return to_image(sheet)


def build_water(f: Fields | None = None, progress=None) -> Image.Image:
    f = f or Fields()
    sheet = _blank_sheet()
    for index in range(GRID * GRID):
        row, col = divmod(index, GRID)
        digits = (col % BASE, row % BASE, row // BASE, col // BASE)
        if all(d == 0 for d in digits):
            continue  # pure land has no water cell
        if 0 in digits and any(d > 1 for d in digits):
            continue  # water beside land is always coast, so sea/ocean never share a cell with land
        _put(sheet, index, render_water(f, *digits))
        if progress:
            progress(index + 1, GRID * GRID)
    for k in range(1, VARIANTS + 1):
        for digit in range(1, BASE):
            base = render_water(f, digit, digit, digit, digit)
            _put(sheet, GRID * GRID + BASE * (k - 1) + digit, _drift(base, _WATER_VARIANT[digit], 700 + 31 * digit + 7 * k, f))
    return to_image(sheet)


# --- preview helpers (not used by the game) ---------------------------------------------------
def preview_map(ground: Image.Image, water: Image.Image, kinds: np.ndarray, scale: float = 0.5, variants: bool = True) -> Image.Image:
    """Compose a map from a 2-D array of tile kinds using the game's own placement math.

    `kinds` holds 0..3 for grassland, plains, desert, tundra land and 4..6 for coast, sea, ocean.
    """
    ny, nx = kinds.shape
    cw, ch = int(W * scale), int(H * scale)

    def crop(sheet, i):
        col, row = cell_position(i)
        return sheet.crop((col * W, row * H, (col + 1) * W, (row + 1) * H)).resize((cw, ch), Image.Resampling.LANCZOS)

    cache = {}
    canvas = Image.new("RGBA", (int((nx + ny) * cw / 2 + cw), int((nx + ny) * ch / 2 + ch)), (20, 28, 34, 255))
    at = lambda x, y: int(kinds[y, x]) if 0 <= x < nx and 0 <= y < ny else 6
    land = lambda k: k < 4

    def under(x, y):
        """Ground digit under a vertex: its own, or the land beside a water tile."""
        k = at(x, y)
        if land(k):
            return k
        for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)):
            if land(at(x + dx, y + dy)):
                return at(x + dx, y + dy)
        return 0

    def wdigit(x, y):
        k = at(x, y)
        if land(k):
            return 0
        if any(land(at(x + dx, y + dy)) for dx in (-1, 0, 1) for dy in (-1, 0, 1)):
            return 1
        return k - 3

    for y in range(-1, ny):
        for x in range(-1, nx):
            tiles = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]  # N, E, S, W
            sx = (x - y) * cw / 2 + ny * cw / 2
            sy = (x + y) * ch / 2 + ch
            if not all(not land(at(*t)) for t in tiles) or True:
                if any(land(at(*t)) for t in tiles):
                    d = [under(*t) for t in tiles]
                    idx = (BASE * d[2] + d[1]) * GRID + BASE * d[3] + d[0]
                    if idx % 85 == 0 and variants:
                        k = ((x * 73856093) ^ (y * 19349663)) % (VARIANTS + 1)
                        if k:
                            idx = GRID * GRID + BASE * (k - 1) + idx // 85
                    key = ("g", idx)
                    if key not in cache:
                        cache[key] = crop(ground, idx)
                    canvas.alpha_composite(cache[key], (int(sx), int(sy)))
            if any(not land(at(*t)) for t in tiles):
                d = [wdigit(*t) for t in tiles]
                idx = (BASE * d[2] + d[1]) * GRID + BASE * d[3] + d[0]
                if idx % 85 == 0 and idx and variants:
                    k = ((x * 73856093) ^ (y * 19349663)) % (VARIANTS + 1)
                    if k:
                        idx = GRID * GRID + BASE * (k - 1) + idx // 85
                key = ("w", idx)
                if key not in cache:
                    cache[key] = crop(water, idx)
                canvas.alpha_composite(cache[key], (int(sx), int(sy)))
    return canvas
