"""Road and railroad sheets: one overlay cell per combination of joined neighbours.

This is the table Civ3 uses for roads and rails (`roads.pcx`, `railroads.pcx`: 256 cells, one per
neighbour mask), rebuilt as original procedural art.  A tile that has a road looks at its eight
neighbours; bit `i` of the mask is set when neighbour `i` is joined, in the order

    bit 0 NE   1 E   2 SE   3 S   4 SW   5 W   6 NW   7 N          (screen directions)

which is `Coord::NEIGHBORS` order, so the client builds the mask with no translation.  The four
bits that face a diamond side (NE, SE, SW, NW: the square-grid neighbours) leave the cell through
the middle of that side; the four that face a corner (E, S, W, N) leave through the vertex.  Every
road, whatever the mask or variant, crosses that point with the same direction (straight along the
line from the cell centre), the same width and the same colours, so any two cells that meet
agree and a network is seamless.  Inside the cell nothing is fixed, which is where the life comes
from:

* **A real layout per mask.**  Exits are paired into through-routes (the straightest pairs first),
  each a smooth cubic that cuts the corner instead of bending at a hub, and exits that cannot be
  paired become branches that curve into a route and merge with its direction.  Roads therefore
  swing through bends, fork in Ys and cross, where a spoke-and-hub drawing would be a star.
* **Wander.**  Each route wobbles sideways and swells and narrows, fading to nothing toward the
  exits, so long straight roads are not rulers.
* **Variants.**  Masks with one or two exits (nearly every road tile) have several variants with
  different wander; the client picks one per tile from its position, so a long road never repeats
  its wobble.  Busier masks are used rarely and are drawn once.
* **Surface.**  Dirt roads have worn ground along their edges, wheel ruts, grain, and a bevel lit
  from the upper left like every other sprite.  Railroads are ballast, sleepers on the true arc
  length of the route, and two steel rails, with the track forking into real turnouts.

Both sheets are 16 columns of cells.  Cell `m` (0..255) is mask `m`, variant 0.  Cells from 256
on hold variants 1.. of the 37 masks with at most two exits, in `SIMPLE` order, variant by
variant.  Cell 0 of a sheet is the isolated tile: a short stretch of track at its centre.
"""

from functools import lru_cache

import numpy as np

from . import palette as P
from .noise import fbm, lattice_coords
from .terrain import H, W
from .util import to_image

COLUMNS = 16
ROAD_VARIANTS = 4
RAIL_VARIANTS = 3
SIMPLE = [m for m in range(256) if bin(m).count("1") <= 2]  # masks that get variants
NAMES = ("NE", "E", "SE", "S", "SW", "W", "NW", "N")

CENTRE = np.array([W / 2, H / 2], dtype=np.float64)
EXIT = np.array(
    [[W * 0.75, H * 0.25], [W, H * 0.5], [W * 0.75, H * 0.75], [W * 0.5, H],
     [W * 0.25, H * 0.75], [0, H * 0.5], [W * 0.25, H * 0.25], [W * 0.5, 0]],
    dtype=np.float64,
)


def _unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-9)


INWARD = _unit(CENTRE - EXIT)  # direction of travel when entering the cell through each exit
# Outward normals of the four diamond sides (the even exits); roads are cut along those lines.
SIDE_NORMAL = {
    0: _unit(np.array([H, -W], dtype=np.float64)),
    2: _unit(np.array([H, W], dtype=np.float64)),
    4: _unit(np.array([-H, W], dtype=np.float64)),
    6: _unit(np.array([-H, -W], dtype=np.float64)),
}
EXTEND = 14.0  # a route runs this far past its exit so the cut is made on a straight, full-width road
STEP = 3.0  # distance between centreline samples
LIGHT = np.array([-0.62, -0.78])  # towards the light, in pixels (y down): upper left


def sheet_index(mask: int, variant: int = 0) -> int:
    """Cell of `mask` in a sheet.  Variants only exist for masks with at most two exits."""
    if variant == 0:
        return mask
    return 256 + (variant - 1) * len(SIMPLE) + SIMPLE.index(mask)


def cell_content(index: int):
    """(mask, variant) drawn in cell `index`, or None for the unused cells at the end."""
    if index < 256:
        return index, 0
    k, rank = divmod(index - 256, len(SIMPLE))
    return SIMPLE[rank], k + 1


# --- the layout of one mask ---------------------------------------------------------------------


def _sep(i, j):
    d = (i - j) % 8
    return min(d, 8 - d)


def plan(mask: int):
    """Exits of `mask` as (pairs, leftovers): pairs run straight through the cell, and a pair is
    at least a right angle apart (on the grid).  The widest pair goes first."""
    left = [i for i in range(8) if mask >> i & 1]
    pairs = []
    while True:
        best = None
        for a in left:
            for b in left:
                if a < b and _sep(a, b) >= 2 and (best is None or _sep(a, b) > best[0]):
                    best = (_sep(a, b), a, b)
        if best is None:
            return pairs, left
        pairs.append((best[1], best[2]))
        left.remove(best[1])
        left.remove(best[2])


def _bezier(p0, p1, p2, p3, n=240):
    t = np.linspace(0.0, 1.0, n)[:, None]
    return (1 - t) ** 3 * p0 + 3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t**2 * p2 + t**3 * p3


def _uniform(pts, step=STEP):
    """Resample a polyline at equal arc-length steps; returns points and the length."""
    seg = np.linalg.norm(np.diff(pts, axis=0), axis=1)
    cum = np.concatenate([[0.0], np.cumsum(seg)])
    n = max(int(round(cum[-1] / step)), 2)
    at = np.linspace(0.0, cum[-1], n + 1)
    return np.stack([np.interp(at, cum, pts[:, 0]), np.interp(at, cum, pts[:, 1])], axis=1), cum[-1]


def _normals(pts):
    t = _unit(np.gradient(pts, axis=0))
    return np.stack([-t[:, 1], t[:, 0]], axis=1)


def _smooth(x):
    x = np.clip(x, 0.0, 1.0)
    return x * x * (3 - 2 * x)


class Route:
    """A centreline with a per-point width factor.  `pts` include the straight run past an exit;
    `arc` counts from the route's own start (the exit, or the dead end), so the run past the exit
    has negative arc.  `parent` is the route a branch merges into."""

    def __init__(self, curve, style, rng, start_exit, end_exit, free_start=False, free_end=False, parent=None, taper=0.42):
        pts, length = _uniform(curve)
        u = np.linspace(0.0, 1.0, len(pts))
        # a wander that fades out toward every end that must meet a neighbour or a route
        lead = 0.20 if length > 110 else 0.28
        env = _smooth(np.minimum(u, 1 - u) / lead)
        arc = u * length
        lat = np.zeros(len(pts))
        wid = np.zeros(len(pts))
        for lam_lo, lam_hi, amp in style["wander"]:
            lam = rng.uniform(lam_lo, lam_hi)
            lat += amp * np.sin(2 * np.pi * arc / lam + rng.uniform(0, 2 * np.pi))
        for lam_lo, lam_hi, amp in style["swell"]:
            lam = rng.uniform(lam_lo, lam_hi)
            wid += amp * np.sin(2 * np.pi * arc / lam + rng.uniform(0, 2 * np.pi))
        pts = pts + _normals(pts) * (lat * env)[:, None]
        factor = 1.0 + wid * env
        # a dead end narrows to a rounded tip
        if free_start:
            factor = factor * (taper + (1 - taper) * _smooth(u / 0.22))
        if free_end:
            factor = factor * (taper + (1 - taper) * _smooth((1 - u) / 0.22))
        # straight run beyond the exits
        pre, post = np.empty((0, 2)), np.empty((0, 2))
        if start_exit is not None:
            d = -INWARD[start_exit]
            pre = np.array([pts[0] + d * EXTEND, pts[0] + d * EXTEND * 0.5])
        if end_exit is not None:
            d = -INWARD[end_exit]
            post = np.array([pts[-1] + d * EXTEND * 0.5, pts[-1] + d * EXTEND])
        self.pts = np.concatenate([pre, pts, post])
        self.factor = np.concatenate([np.full(len(pre), factor[0]), factor, np.full(len(post), factor[-1])])
        seg = np.linalg.norm(np.diff(self.pts, axis=0), axis=1)
        arc_all = np.concatenate([[0.0], np.cumsum(seg)])
        self.arc = arc_all - (arc_all[len(pre)] if len(pre) else 0.0)
        self.length = length
        self.parent = parent
        self.free_start, self.free_end = free_start, free_end

    def field(self, px):
        """Distance to the centreline, signed lateral offset, arc position, and width factor at
        each pixel centre `px` (M x 2)."""
        a, b = self.pts[:-1], self.pts[1:]
        ab = b - a
        l2 = np.maximum((ab**2).sum(1), 1e-9)
        ap = px[:, None, :] - a[None]
        t = np.clip((ap * ab[None]).sum(-1) / l2[None], 0.0, 1.0)
        delta = ap - t[..., None] * ab[None]
        d2 = (delta**2).sum(-1)
        j = d2.argmin(1)
        rows = np.arange(len(px))
        tj = t[rows, j]
        d = np.sqrt(d2[rows, j])
        side = ab[j, 0] * ap[rows, j, 1] - ab[j, 1] * ap[rows, j, 0]
        seglen = np.sqrt(l2[j])
        arc = self.arc[j] + tj * seglen
        fac = self.factor[j] * (1 - tj) + self.factor[j + 1] * tj
        return d, np.where(side >= 0, d, -d), arc, fac


def _routes(mask: int, variant: int, style) -> list:
    """The routes drawn in one cell: through-routes, then branches, or a lone track."""
    rng = np.random.default_rng(style["seed"] + 7919 * variant + 31 * mask)
    if mask == 0:  # an isolated tile: a short track whose direction depends on the variant
        ang = [0.0, np.pi / 6, -np.pi / 6, np.pi / 2][variant % 4]
        half = style["lone"]
        d = np.array([np.cos(ang), np.sin(ang)])
        p0, p3 = CENTRE - d * half, CENTRE + d * half
        curve = _bezier(p0, p0 + (p3 - p0) / 3, p0 + (p3 - p0) * 2 / 3, p3)
        return [Route(curve, style, rng, None, None, free_start=True, free_end=True)]
    pairs, left = plan(mask)
    routes = []
    for a, b in pairs:
        ra, rb = rng.uniform(0.5, 0.82, 2)
        p0, p3 = EXIT[a], EXIT[b]
        curve = _bezier(p0, p0 + (CENTRE - p0) * ra, p3 + (CENTRE - p3) * rb, p3)
        routes.append(Route(curve, style, rng, a, b))
    if not routes:  # no pair at a right angle or wider: the first exit runs to a dead end at the centre
        a = left.pop(0)
        stop = CENTRE + rng.uniform(-2, 2, 2)
        curve = _bezier(EXIT[a], EXIT[a] + (stop - EXIT[a]) / 3, EXIT[a] + (stop - EXIT[a]) * 2 / 3, stop)
        routes.append(Route(curve, style, rng, a, None, free_end=True))
    for s in left:
        start, inward = EXIT[s], INWARD[s]
        best = None
        for k, main in enumerate(routes):
            at = int(len(main.pts) * 0.5) if main.free_end is False else int(len(main.pts) * 0.62)
            q = main.pts[at]
            t = _unit(main.pts[min(at + 2, len(main.pts) - 1)] - main.pts[max(at - 2, 0)])
            for sign in (1.0, -1.0):
                if main.free_end and sign < 0:
                    continue  # a branch joining a stem always runs on toward the stem's dead end
                turn = float(np.degrees(np.arccos(np.clip(inward @ (t * sign), -1, 1))))
                if best is None or turn < best[0] - 1e-6:
                    best = (turn, k, q, t * sign)
        _, k, q, t = best
        reach = np.linalg.norm(q - start)
        kappa = rng.uniform(0.38, 0.55) * reach
        curve = _bezier(start, start + inward * kappa, q - t * kappa, q)
        routes.append(Route(curve, style, rng, s, None, parent=k))
    return routes


# --- raster -------------------------------------------------------------------------------------


@lru_cache(maxsize=1)
def _grid():
    ys, xs = np.mgrid[0:H, 0:W].astype(np.float64)
    px = np.stack([xs + 0.5, ys + 0.5], axis=-1).reshape(-1, 2)
    u, v, a, b = lattice_coords(W, H)
    return px, (a, b)


@lru_cache(maxsize=1)
def _noise():
    _, (a, b) = _grid()
    return {
        "edge": fbm(a, b, 14, 3, seed=301, warp=0.1),
        "halo": fbm(a, b, 9, 3, seed=302, warp=0.15),
        "grain": fbm(a, b, 44, 2, seed=303),
        "speck": fbm(a, b, 70, 1, seed=304),
        "gravel": fbm(a, b, 60, 2, seed=305),
        "tint": fbm(a, b, 5, 3, seed=306, warp=0.2),
    }


def _smin(a, b, k):
    h = np.clip(0.5 + 0.5 * (b - a) / k, 0.0, 1.0)
    return b * (1 - h) + a * h - k * h * (1 - h)


def _clips(mask: int, px, overlap: float):
    """Coverage that cuts every route at the diamond side it crosses, `overlap` px past the line.
    A neighbour cuts along the same line, so with an overlap their solid parts join with no
    seam; translucent parts use none.  Corner exits are cut by the cell itself."""
    out = np.ones(len(px))
    for i, n in SIDE_NORMAL.items():
        if mask >> i & 1:
            out *= np.clip(0.5 - ((px - EXIT[i]) @ n - overlap), 0.0, 1.0)
    return out


def _over(rgb, alpha, layer_rgb, layer_a):
    """Composite a straight-alpha layer over (rgb, alpha)."""
    a = layer_a[..., None]
    out_a = alpha + layer_a * (1 - alpha)
    out_rgb = np.where(out_a[..., None] > 1e-6, (rgb * alpha[..., None] * (1 - a) + layer_rgb * a) / np.maximum(out_a[..., None], 1e-6), 0.0)
    return out_rgb, out_a


def _finish(rgb, alpha):
    return np.dstack([np.clip(rgb, 0, 1).reshape(H, W, 3), np.clip(alpha, 0, 1).reshape(H, W, 1)]).astype(np.float32)


ROAD = {
    "seed": 5000,
    "wander": [(70, 120, 2.6), (30, 48, 0.9)],
    "swell": [(60, 100, 0.10), (26, 40, 0.05)],
    "lone": 24.0,
    "half": 4.7,
}
RAIL = {
    "seed": 9000,
    "wander": [(90, 150, 0.7)],
    "swell": [],
    "lone": 26.0,
    "half": 6.0,
}


def render_road(mask: int, variant: int) -> np.ndarray:
    """RGBA float cell of the dirt road for `mask`."""
    px, _ = _grid()
    n = _noise()
    flat = {k: v.reshape(-1) for k, v in n.items()}
    routes = _routes(mask, variant, ROAD)
    field = None
    lat = depth_w = None
    for r in routes:
        d, l, _, fac = r.field(px)
        half = ROAD["half"] * fac
        f = d - half
        if field is None:
            field, lat, depth_w = f, l / half, half
        else:
            nearer = f < field
            lat = np.where(nearer, l / half, lat)
            depth_w = np.where(nearer, half, depth_w)
            field = _smin(field, f, 3.0)
    ragged = field + 0.85 * flat["edge"]

    # worn ground along the roadside, ragged where the grass takes over
    halo_a = 0.42 * (1 - _smooth((field + 2.2 * flat["halo"] - 1.0) / 6.5))
    halo_a = halo_a * _clips(mask, px, 0.0)
    rgb = np.broadcast_to(P.hx("#5a4728"), (len(px), 3)) * (1 + 0.10 * flat["tint"])[:, None]
    alpha = halo_a

    # body
    inside = np.clip(-field / np.maximum(depth_w, 1e-6), 0.0, 1.0)
    body = P.ramp(inside, [(0.0, "#664b2a"), (0.25, "#86683a"), (0.6, "#a2844f"), (1.0, "#b49b68")])
    ruts = np.exp(-(((np.abs(lat) - 0.5) / 0.17) ** 2)) * _smooth((-field - 0.8) / 2.0)
    body = body * (1 - 0.15 * ruts)[:, None]
    body = body * (1 + 0.09 * flat["grain"])[:, None]
    body = body * (1 + 0.12 * (flat["speck"] > 0.4) - 0.10 * (flat["speck"] < -0.4))[:, None]
    # bevel: the bank that faces the light is bright, the one that faces away is dark
    gy, gx = np.gradient(ragged.reshape(H, W))
    grad = _unit(np.stack([gx.reshape(-1), gy.reshape(-1)], axis=1))
    band = np.exp(-(((ragged + 0.9) / 1.3) ** 2))
    body = body * (1 + 0.26 * (grad @ LIGHT) * band)[:, None]
    body_a = np.clip(0.5 - ragged, 0.0, 1.0) * _clips(mask, px, 1.5)
    rgb, alpha = _over(rgb, alpha, body, body_a)
    return _finish(rgb, alpha)


def _bar(along, across, half_along, half_across):
    """Anti-aliased coverage of a rectangle in route coordinates."""
    return np.clip(0.5 - (np.abs(along) - half_along), 0.0, 1.0) * np.clip(0.5 - (np.abs(across) - half_across), 0.0, 1.0)


def render_rail(mask: int, variant: int) -> np.ndarray:
    """RGBA float cell of the railroad for `mask`."""
    px, _ = _grid()
    n = _noise()
    flat = {k: v.reshape(-1) for k, v in n.items()}
    routes = _routes(mask, variant, RAIL)
    info = [r.field(px) for r in routes]
    half = RAIL["half"]
    field = None
    for d, _, _, _ in info:
        f = d - half
        field = f if field is None else _smin(field, f, 1.5)
    ragged = field + 0.5 * flat["edge"]
    clip_soft, clip_solid = _clips(mask, px, 0.0), _clips(mask, px, 1.5)

    # the ground the track lies on
    soil_a = 0.38 * (1 - _smooth((field + 1.5 * flat["halo"] - 0.5) / 4.5)) * clip_soft
    rgb = np.broadcast_to(P.hx("#4d4533"), (len(px), 3)) * (1 + 0.10 * flat["tint"])[:, None]
    alpha = soil_a

    # ballast
    ballast = P.ramp(np.clip(0.5 + 0.5 * flat["gravel"], 0, 1), [(0.0, "#6b6458"), (0.5, "#8b8477"), (1.0, "#a39c8d")])
    gy, gx = np.gradient(ragged.reshape(H, W))
    grad = _unit(np.stack([gx.reshape(-1), gy.reshape(-1)], axis=1))
    ballast = ballast * (1 + 0.30 * (grad @ LIGHT) * np.exp(-(((ragged + 0.9) / 1.3) ** 2)))[:, None]
    rgb, alpha = _over(rgb, alpha, ballast, np.clip(0.5 - ragged, 0, 1) * clip_solid)

    # sleepers, spaced so a half gap sits at each exit: two cells then continue the pattern
    tie = np.zeros(len(px))
    tone = np.zeros(len(px))
    for r, (d, l, arc, _) in zip(routes, info):
        spacing = 5.6 if (r.free_start or r.free_end) else r.length / max(round(r.length / 5.6), 1)
        index = np.floor(arc / spacing)
        cover = _bar(arc - (index + 0.5) * spacing, l, 1.05, 5.0)
        if r.parent is not None:  # a branch's sleepers give way to the route it merges into
            dm = routes[r.parent].field(px)[0]
            cover = cover * _smooth((dm - half * 0.55) / (half * 0.9))
        tie = np.maximum(tie, cover * (arc > -EXTEND) * (arc < r.length + EXTEND))
        tone = np.maximum(tone, (np.sin(index * 12.9898 + 4.1 * r.length) * 43758.5453) % 1.0)
    wood = P.mix(P.hx("#3e2c1d"), P.hx("#5a4129"), tone) * (1 + 0.12 * flat["grain"])[:, None]
    rgb, alpha = _over(rgb, alpha, wood, tie * clip_solid)

    # rails: two steel lines at the gauge, lit from the upper left
    rail = np.zeros(len(px))
    shade = np.zeros(len(px))
    gauge, width = 2.9, 0.85
    for d, l, arc, _ in info:
        for sgn in (-1.0, 1.0):
            off = np.abs(l - sgn * gauge)
            cover = np.clip(0.5 - (off - width), 0.0, 1.0) * (arc > -EXTEND)
            rail = np.maximum(rail, cover)
            shade = np.where(cover >= rail, (l - sgn * gauge) / width, shade)
    steel = P.mix(P.hx("#8f959c"), P.hx("#e4e7ea"), np.clip(1.0 - np.abs(shade) * 0.9, 0, 1) ** 1.5)
    shadow = np.zeros(len(px))
    sh = np.clip(rail.reshape(H, W), 0, 1)
    shifted = np.roll(np.roll(sh, 1, axis=0), 1, axis=1).reshape(-1)  # one pixel down and right
    shadow = np.clip(shifted - rail, 0, 1) * 0.45
    rgb, alpha = _over(rgb, alpha, np.broadcast_to(P.hx("#1c1a16"), (len(px), 3)), shadow * clip_solid)
    rgb, alpha = _over(rgb, alpha, steel, rail * clip_solid)
    return _finish(rgb, alpha)


def build_sheet(render, variants: int) -> "Image.Image":
    cells = 256 + (variants - 1) * len(SIMPLE)
    rows = -(-cells // COLUMNS)
    sheet = np.zeros((rows * H, COLUMNS * W, 4), dtype=np.float32)
    for index in range(cells):
        mask, variant = cell_content(index)
        row, col = divmod(index, COLUMNS)
        sheet[row * H : (row + 1) * H, col * W : (col + 1) * W] = render(mask, variant)
    return to_image(sheet)


def build_road_sheet():
    return build_sheet(render_road, ROAD_VARIANTS)


def build_rail_sheet():
    return build_sheet(render_rail, RAIL_VARIANTS)
