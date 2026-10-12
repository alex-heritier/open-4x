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

# Olive-drab, khaki and brown: the land is a worn, sun-baked painting, not a lawn.  Shadow runs to
# a brown-green near black, the lit crowns to a dry yellow-olive.
GRASSLAND_STOPS = [(0.00, "#2e3b18"), (0.22, "#434f1f"), (0.48, "#5d6b27"), (0.74, "#7d8a33"), (1.00, "#a2a748")]
PLAINS_STOPS = [(0.00, "#48442a"), (0.25, "#645d34"), (0.52, "#7f7642"), (0.78, "#9b9059"), (1.00, "#b9ad82")]
DESERT_STOPS = [(0.00, "#8f8264"), (0.35, "#b0a482"), (0.65, "#cbc1a0"), (1.00, "#e3dbc0")]
TUNDRA_STOPS = [(0.00, "#767a68"), (0.30, "#969886"), (0.60, "#b9baac"), (1.00, "#dcddd3")]
BEACH_STOPS = [(0.00, "#9a8a58"), (0.40, "#bfb078"), (0.75, "#d9cc96"), (1.00, "#ebe2b8")]


# The land is painted like a shaded relief map: broad rolling light and shadow across every tile
# (`Fields.rolling`), dry mottled colour on top, and just enough pull toward the mean colour that
# the sprites standing on it still win.
GROUND_CALM = 0.95  # share of the painted variation kept
GROUND_SAT = 0.96
GROUND_WARM = np.array([1.015, 1.0, 0.975], dtype=np.float32)  # sun-warmed, never cold
_LUM = np.array([0.3, 0.59, 0.11], dtype=np.float32)


def _calm(col, keep=GROUND_CALM, sat=GROUND_SAT):
    """Compress `col` toward its mean, per channel.  The mean is a constant of the periodic field,
    so cells that share an edge still agree there."""
    mean = col.reshape(-1, 3).mean(axis=0)
    col = mean + (col - mean) * keep
    lum = (col @ _LUM)[..., None]
    return np.clip((lum + (col - lum) * sat) * GROUND_WARM, 0, 1)


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
        self.rolling = self._rolling()
        self.ground = [self._grassland(), self._plains(), self._desert(), self._tundra()]
        # one low-frequency perturbation per ground type: ragged, organic ecotones
        self.edge_noise = [fbm(a, b, 6, 3, seed=60 + 7 * k, warp=0.2) * 1.5 for k in range(4)]
        self.beach = self._beach()
        self.ripple = fbm(a, b, 8, 3, seed=31, aniso=2.5, warp=0.12)
        self.ripple2 = fbm(a, b, 16, 2, seed=32, aniso=3.5, warp=0.10)
        self.sparkle = fbm(a, b, 70, 1, seed=33)
        self.swell = fbm(a, b, 3, 2, seed=34, warp=0.15)
        # where the depth bands meet: a ragged, blotchy isoline rather than a smooth gradient
        self.zone = np.clip(fbm(a, b, 5, 3, seed=35, warp=0.25, gain=0.55) * 1.7, -1, 1)
        self.zone_fine = fbm(a, b, 18, 2, seed=36, warp=0.1)
        self.chop = fbm(a, b, 12, 3, seed=37, aniso=2.0, warp=0.2)

    def _height(self, du=0.0, dv=0.0):
        """Small-scale relief used for the painterly bump shading (periodic)."""
        da, db = (du + dv) / 2, (du - dv) / 2
        a, b = self.a, self.b
        return 0.6 * fbm(a + da, b + db, 28, 2, seed=21) + 0.4 * fbm(a + da, b + db, 60, 1, seed=22)

    def _bump(self):
        e = 1.2 / (W / 2)
        # light from the upper left: lit where the height falls toward the lower right
        return (self._height(-e, -e * 2) - self._height(e, e * 2)) * 2.4

    def _rolling(self):
        """Broad hill-shading: soft knolls lit from the upper left, a few tiles' worth of relief
        compressed into one cell.  Periodic like every field, so cells still tile."""
        def h(du, dv):
            da, db = (du + dv) / 2, (du - dv) / 2
            return fbm(self.a + da, self.b + db, 3, 3, seed=51, warp=0.22, gain=0.45)

        e = 5.0 / (W / 2)
        return np.clip((h(-e, -e * 2) - h(e, e * 2)) * 3.2, -1.0, 1.0)

    def _relief_light(self, col, amount):
        """Multiply the rolling light into `col`: sunlit knolls warm and bright, lee sides deep."""
        shade = 1.0 + amount * self.rolling
        warm = np.stack([1.0 + 0.05 * self.rolling, 1.0 + 0.015 * self.rolling, 1.0 - 0.07 * self.rolling], axis=-1)
        return col * shade[..., None] * warm

    def _grassland(self):
        a, b = self.a, self.b
        low = fbm(a, b, 3, 3, seed=11, warp=0.2)
        mid = fbm(a, b, 11, 3, seed=12, warp=0.12)
        fine = fbm(a, b, 46, 2, seed=13)
        t = unit(1.15 * low + 1.0 * mid + 0.35 * fine)
        col = P.ramp(t, GRASSLAND_STOPS)
        meadow = P.smoothstep(0.15, 0.55, fbm(a, b, 5, 3, seed=14, warp=0.25) * 1.5)
        col = P.mix(col, P.hx("#95a548"), meadow * 0.30)
        shade = P.smoothstep(0.10, 0.45, fbm(a, b, 18, 2, seed=15) * 1.5)
        col = col * (1.0 - 0.18 * shade)[..., None]
        col = col * (1.0 + 0.16 * self.bump)[..., None]
        tuft = P.smoothstep(0.28, 0.5, fbm(a, b, 64, 1, seed=16) * 1.7)
        col = col * (1.0 - 0.10 * tuft)[..., None]
        col = self._relief_light(col, 0.16)
        return _calm(col)

    def _plains(self):
        a, b = self.a, self.b
        low = fbm(a, b, 3, 3, seed=111, warp=0.2)
        mid = fbm(a, b, 9, 3, seed=112, warp=0.12)
        fine = fbm(a, b, 42, 2, seed=113)
        t = unit(1.1 * low + 1.0 * mid + 0.4 * fine)
        col = P.ramp(t, PLAINS_STOPS)
        # sun-bleached ochre drifts and the odd strip of darker scrub, like mottled steppe
        ochre = P.smoothstep(0.0, 0.40, fbm(a, b, 5, 3, seed=114, warp=0.25) * 1.4)
        col = P.mix(col, P.hx("#a98f4a"), ochre * 0.45)
        scrub = P.smoothstep(0.12, 0.5, fbm(a, b, 14, 2, seed=115) * 1.5)
        col = P.mix(col, P.hx("#6a6a30"), scrub * 0.22)
        streak = fbm(a, b, 10, 2, seed=116, aniso=3.0, warp=0.1)
        col = col * (1.0 + 0.07 * streak)[..., None]
        col = col * (1.0 + 0.18 * self.bump)[..., None]
        col = self._relief_light(col, 0.14)
        return _calm(col)

    def _desert(self):
        a, b = self.a, self.b
        t = unit(1.3 * fbm(a, b, 6, 3, seed=41, warp=0.2) + 0.4 * fbm(a, b, 40, 2, seed=42))
        col = P.ramp(t, DESERT_STOPS)
        dunes = fbm(a, b, 7, 2, seed=43, aniso=4.0, warp=0.25)
        crest = 1.0 - np.abs(dunes * 3.0)
        col = col * (1.0 - 0.10 * P.smoothstep(0.55, 0.95, crest))[..., None]
        col = col * (1.0 + 0.07 * dunes)[..., None]
        rock = P.smoothstep(0.30, 0.55, fbm(a, b, 9, 3, seed=44, warp=0.2) * 1.6)
        col = P.mix(col, P.hx("#9b7c4a"), rock * 0.28)
        col = col * (1.0 + 0.13 * self.bump)[..., None]
        col = self._relief_light(col, 0.10)
        return _calm(col)

    def _tundra(self):
        a, b = self.a, self.b
        t = unit(1.2 * fbm(a, b, 4, 3, seed=141, warp=0.2) + 0.45 * fbm(a, b, 30, 2, seed=142))
        col = P.ramp(t, TUNDRA_STOPS)
        moss = P.smoothstep(0.05, 0.45, fbm(a, b, 8, 3, seed=143, warp=0.2) * 1.5)
        col = P.mix(col, P.hx("#6b7a43"), moss * 0.30)
        drift = fbm(a, b, 2, 3, seed=144, warp=0.3, gain=0.4) * 2.0
        snow = P.smoothstep(-0.12, 0.34, drift) * (0.80 + 0.20 * P.smoothstep(-0.3, 0.3, fbm(a, b, 9, 2, seed=146) * 1.6))
        lit = 0.95 + 0.22 * np.clip(self.bump, -0.25, 0.25) + 0.025 * fbm(a, b, 30, 2, seed=147)
        col = P.mix(col, P.hx("#e4ecef") * lit[..., None], snow * 0.55)
        speck = P.smoothstep(0.30, 0.5, fbm(a, b, 56, 1, seed=145) * 1.7)
        col = col * (1.0 - 0.08 * speck)[..., None]
        col = col * (1.0 + 0.12 * self.bump)[..., None]
        col = self._relief_light(col, 0.09)
        return _calm(col)

    def _beach(self):
        """Pale shingle/sand that rims every shoreline, whatever the land behind it."""
        a, b = self.a, self.b
        t = unit(1.3 * fbm(a, b, 6, 3, seed=241, warp=0.2) + 0.8 * fbm(a, b, 40, 2, seed=242))
        col = P.ramp(t, BEACH_STOPS)
        return _calm(col * (1.0 + 0.12 * self.bump)[..., None], keep=0.80)


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


# Water reads in three flat bands, like a painted chart: bright azure over the shelf (coast),
# royal blue in the sea, near-black navy in the ocean.  The bands meet along a ragged isoline with a
# dark lip on the deep side, so a sea tile cuts into the shallows instead of fading into them.
COAST_STOPS = [(0.00, "#86dcee"), (0.30, "#5ecbe6"), (0.65, "#42b5de"), (1.00, "#33a3d3")]
SEA_STOPS = [(0.00, "#1a56a6"), (0.50, "#2169b6"), (1.00, "#2c7cc6")]
OCEAN_STOPS = [(0.00, "#071f5e"), (0.50, "#0b2a72"), (1.00, "#12388a")]
SURF = "#d2f3f6"  # the thin pale line where water meets sand
SEA_AT, OCEAN_AT = 0.37, 0.79  # halfway between the depth digits 0.16 / 0.58 / 1.0
ZONE_JITTER = (0.12, 0.05)  # isoline wander; its sum stays under the 0.21 gap, so edges of equal digits never flip


def _water(f: Fields, depth, shore=1.0):
    """Painted sea.  `depth` is the depth the vertices ask for (0.16 coast, 0.58 sea, 1.0 ocean),
    `shore` runs 0 at the beach to 1 away from it and brightens the shelf.  Ripples are fine-grained
    on purpose: open-sea cells repeat, so no blob may stand out per tile."""
    d = depth + ZONE_JITTER[0] * f.zone + ZONE_JITTER[1] * f.zone_fine
    sea_w = P.smoothstep(SEA_AT - 0.018, SEA_AT + 0.018, d)
    ocean_w = P.smoothstep(OCEAN_AT - 0.018, OCEAN_AT + 0.018, d)

    chop = np.clip(f.ripple * 1.5 + 0.6 * f.chop, -1, 1)
    grain = 0.5 + 0.5 * chop
    coast = P.ramp(np.clip(0.80 * shore + 0.42 * grain - 0.12, 0, 1), COAST_STOPS)
    sea = P.ramp(grain, SEA_STOPS)
    ocean = P.ramp(grain, OCEAN_STOPS)
    col = P.mix(P.mix(coast, sea, sea_w), ocean, ocean_w)

    # wave strokes: pale on the shelf, a lighter blue in the sea, barely there in the ocean
    ridge2 = 1.0 - np.abs(f.ripple2 * 3.2)
    streak = P.smoothstep(0.70, 0.96, ridge2)
    col = P.mix(col, P.hx("#b4ecf6"), streak * 0.16 * (1.0 - sea_w))
    col = P.mix(col, P.hx("#5b9ad8"), streak * 0.20 * sea_w * (1.0 - ocean_w))
    col = P.mix(col, P.hx("#2a58a8"), streak * 0.20 * ocean_w)
    ridge1 = 1.0 - np.abs(f.ripple * 3.0)
    trough = P.smoothstep(0.78, 0.97, ridge1)
    col = col * (1.0 - 0.10 * trough)[..., None]

    # lips along the band edges: lighter just inside the shallow side, a dark rim on the deep side
    for at, deep_lip, shallow_lip in ((SEA_AT, "#0f3c7e", "#8fe3f4"), (OCEAN_AT, "#041440", "#2e72c4")):
        above = P.smoothstep(at - 0.004, at + 0.004, d)  # 1 on the deep side
        lip_dark = above * (1.0 - P.smoothstep(0.012, 0.075, d - at))
        lip_light = (1.0 - above) * (1.0 - P.smoothstep(0.012, 0.060, at - d))
        col = P.mix(col, P.hx(deep_lip), lip_dark * 0.80)
        col = P.mix(col, P.hx(shallow_lip), lip_light * 0.45)

    glint = P.smoothstep(0.62, 0.90, f.sparkle * 1.7)
    return P.mix(col, P.hx("#e4f8fa"), glint * 0.16 * (1.0 - sea_w))


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
        rgb = _water(f, ask)
        return np.dstack([np.clip(rgb, 0, 1), alpha_edge])

    # signed shoreline margin: > 0 water, < 0 land
    base_m = 2.0 * t_water - 1.0
    m = base_m + HI_COAST * f.coast * (1.0 - np.abs(base_m))
    water_amt = P.smoothstep(-0.018, 0.018, m)

    # shallows hug the shore and deepen toward whatever the water vertices ask for
    shore = np.clip(m / 0.9, 0.0, 1.0)
    water = _water(f, ask, shore)  # == the open-water formula once m >= 0.9
    near = np.abs(m - 0.02)
    foam = 1.0 - P.smoothstep(0.010, 0.050, near + 0.030 * f.foam_noise)
    swell = 1.0 - P.smoothstep(0.012, 0.040, np.abs(m - 0.17 + 0.03 * f.foam_noise))
    swell = swell * P.smoothstep(0.0, 0.3, np.abs(f.foam_noise) + 0.2)
    water = P.mix(water, P.hx(SURF), np.clip(swell * 0.22, 0, 1))
    shore_glow = (1.0 - P.smoothstep(0.0, 0.16, m)) * P.smoothstep(-0.01, 0.01, m)
    water = P.mix(water, P.hx("#a6eaf6"), shore_glow * 0.30)

    # beach strip on the land side, fading out inland (alpha), darker where the sea wets it
    strip = P.smoothstep(-0.40, -0.07, m + f.beach_edge * 0.10) * (1.0 - P.smoothstep(0.0, 0.03, m))
    sand_w = P.smoothstep(0.25, 0.75, strip + 0.25 * f.sand_noise)
    wet = P.smoothstep(-0.13, -0.01, m)
    beach = P.mix(f.beach, f.beach * P.hx("#b49a62") / 0.75, wet * 0.55)
    beach = beach * (1.0 - 0.18 * wet)[..., None]
    beach_a = P.smoothstep(0.10, 0.62, sand_w * strip + 0.5 * strip)

    rgb = P.mix(beach, water, water_amt)
    alpha = water_amt + (1.0 - water_amt) * beach_a
    rgb = P.mix(rgb, P.hx(SURF), np.clip(foam * 0.55, 0, 1))
    alpha = np.maximum(alpha, np.clip(foam * 0.55, 0, 1))
    # a hair-thin dark waterline under the foam reads as the painted shore contour
    contour = 1.0 - P.smoothstep(0.0, 0.020, np.abs(m + 0.045))
    rgb = rgb * (1.0 - 0.12 * contour * (1.0 - foam))[..., None]
    alpha = np.maximum(alpha, 0.40 * contour)
    return np.dstack([np.clip(rgb, 0, 1), np.clip(alpha, 0, 1) * alpha_edge])


def interior_weight(f: Fields):
    """1 in the middle of the diamond, 0 on its border: variants only change the inside."""
    r = np.abs(f.u) + np.abs(f.v)
    return P.smoothstep(0.90, 0.30, r)


# soft tonal drift per pure terrain: (amplitude, per-channel response)
_GROUND_VARIANT = {
    0: (0.12, np.array([0.95, 1.00, 0.72], dtype=np.float32)),  # grassland drifts yellow <-> deep green
    1: (0.12, np.array([1.00, 0.97, 0.78], dtype=np.float32)),
    2: (0.11, np.array([1.00, 0.95, 0.82], dtype=np.float32)),
    3: (0.11, np.array([0.97, 1.00, 0.97], dtype=np.float32)),
}
_WATER_VARIANT = {
    1: (0.10, np.array([0.90, 1.00, 1.04], dtype=np.float32)),
    2: (0.15, np.array([0.80, 1.00, 1.08], dtype=np.float32)),
    3: (0.17, np.array([0.78, 0.98, 1.10], dtype=np.float32)),
}


def _knolls(seed, w=W, h=H):
    """Non-periodic hill-shading for a variant's interior: soft knolls lit from the upper left, so
    each variant lights a different landscape while its border pixels stay those of the base cell."""
    n = flat_noise(w, h, 3, 3, seed=seed, gain=0.5)
    return np.clip((np.roll(n, (-3, -2), (0, 1)) - np.roll(n, (3, 2), (0, 1))) * 3.4, -1.0, 1.0)


def _drift(base, style, seed, f, relief=0.0):
    amp, resp = style
    drift = 0.6 * flat_noise(W, H, 2, 3, seed=seed) + 0.4 * flat_noise(W, H, 5, 2, seed=seed + 1)
    delta = np.clip(drift * 1.7, -1, 1) * amp * interior_weight(f)
    rgb = base[..., :3] * (1.0 + delta[..., None] * resp)
    if relief:
        k = _knolls(seed + 2) * relief * interior_weight(f)
        warm = np.stack([1.0 + 0.05 * k / relief, 1.0 + 0.015 * k / relief, 1.0 - 0.07 * k / relief], axis=-1)
        rgb = rgb * (1.0 + k)[..., None] * warm
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
            _put(sheet, GRID * GRID + BASE * (k - 1) + digit, _drift(base, _GROUND_VARIANT[digit], 500 + 31 * digit + 7 * k, f, relief=0.27))
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
