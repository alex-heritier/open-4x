"""River sheet: a river runs along tile edges, so its sprite is a dual-grid cell centred on a tile
corner.  Each of the four edges that meet at the corner contributes a half-edge branch from the
cell centre to the middle of one diamond side:

    bit 0  NE   (edge shared by the N and E tiles)      centre -> (3/4 W, 1/4 H)
    bit 1  SE   (E and S)                                centre -> (3/4 W, 3/4 H)
    bit 2  SW   (S and W)                                centre -> (1/4 W, 3/4 H)
    bit 3  NW   (W and N)                                centre -> (1/4 W, 1/4 H)

The sheet has 16 columns (the mask) and `VARIANTS` rows (different meanders).  Every branch leaves
the cell side exactly at the side's midpoint, heading straight along the tile edge, with the same
width and colours whatever the variant, so any two cells join seamlessly.  The sheet is drawn
over the ground and the water layers, below forests and hills.
"""

import numpy as np
from PIL import Image

from . import palette as P
from .noise import fbm, lattice_coords
from .terrain import EDGE, H, W
from .util import to_image

VARIANTS = 3
SS = 3  # supersampling for clean edges
HALF = 6.2  # half width of the water in cell pixels (about 8 world units across)
BANK = 2.2  # dark bank band beyond the water
MUD = 5.0  # soft wet-earth halo beyond the bank

CENTRE = np.array([W / 2, H / 2], dtype=np.float32)
MIDS = [  # NE, SE, SW, NW side midpoints
    np.array([W * 0.75, H * 0.25], dtype=np.float32),
    np.array([W * 0.75, H * 0.75], dtype=np.float32),
    np.array([W * 0.25, H * 0.75], dtype=np.float32),
    np.array([W * 0.25, H * 0.25], dtype=np.float32),
]


def _unit_normals(pts):
    tangent = np.gradient(pts, axis=0)
    tangent /= np.maximum(np.linalg.norm(tangent, axis=1, keepdims=True), 1e-6)
    return np.stack([-tangent[:, 1], tangent[:, 0]], axis=1)


def _path(p0, p1, bend, amp, freq, phase, samples=72):
    """A meandering path from p0 to p1, optionally bent through the control point `bend`."""
    s = np.linspace(0.0, 1.0, samples, dtype=np.float32)[:, None]
    if bend is None:
        pts = p0 + s * (p1 - p0)
    else:
        pts = (1 - s) ** 2 * p0 + 2 * (1 - s) * s * bend + s**2 * p1
    envelope = np.sin(np.pi * s[:, 0]) ** 2  # zero offset and zero slope offset at both ends
    wobble = amp * envelope * np.sin(2 * np.pi * freq * s[:, 0] + phase)
    return pts + _unit_normals(pts) * wobble[:, None], s[:, 0]


def _branches(mask, variant):
    """Paths (points, s along path, width scale along path) that make up the river of `mask`."""
    rng = np.random.default_rng(1000 + 97 * variant + mask)
    ends = [MIDS[i] for i in range(4) if mask >> i & 1]
    out = []
    if len(ends) == 2:  # a bend or a straight run: one smooth curve through the centre
        pts, s = _path(ends[0], ends[1], CENTRE, rng.uniform(2.5, 5.0), rng.choice([1.0, 1.5, 2.0]), rng.uniform(0, 6.28))
        out.append((pts, np.ones_like(s)))
    else:
        for end in ends:
            pts, s = _path(CENTRE, end, None, rng.uniform(2.0, 4.0), rng.choice([1.0, 1.5]), rng.uniform(0, 6.28))
            scale = np.ones_like(s)
            if len(ends) == 1:  # a spring: the stream thins toward its head
                scale = 0.45 + 0.55 * np.minimum(s * 1.6, 1.0)
            out.append((pts, scale))
    return out


def render_river(mask: int, variant: int) -> np.ndarray:
    """RGBA float image of one river cell (all zero for mask 0)."""
    w, h = W * SS, H * SS
    if mask == 0:
        return np.zeros((H, W, 4), dtype=np.float32)
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32)
    px = np.stack([(xs + 0.5) / SS, (ys + 0.5) / SS], axis=-1).reshape(-1, 2)

    # distance to the nearest branch in units where the nominal width applies everywhere: a branch
    # that is thinner at some point (a spring) simply divides its distances by its width scale
    rows = np.arange(len(px))
    d_flat = np.full(len(px), 1e9, dtype=np.float32)
    lat_flat = np.zeros(len(px), dtype=np.float32)
    for pts, scale in _branches(mask, variant):
        normals = _unit_normals(pts)
        for lo in range(0, len(pts), 12):  # chunk the sample loop to bound memory
            seg = pts[lo : lo + 12]
            diff = px[:, None, :] - seg[None, :, :]
            dist = np.linalg.norm(diff, axis=2) / scale[lo : lo + 12][None, :]
            j = dist.argmin(axis=1)
            nd = dist[rows, j]
            better = nd < d_flat
            if better.any():
                d_flat = np.where(better, nd, d_flat)
                signed = (diff[rows, j] * normals[lo + j]).sum(axis=1) / scale[lo + j]
                lat_flat = np.where(better, signed, lat_flat)
    d = d_flat.reshape(h, w)
    lat = lat_flat.reshape(h, w)
    half = HALF

    u, v, a, b = lattice_coords(W, H)
    flow = fbm(a, b, 14, 2, seed=71, aniso=3.0, warp=0.15)
    flow = np.kron(flow, np.ones((SS, SS), dtype=np.float32))  # same texture on every cell
    shimmer = np.kron(fbm(a, b, 50, 1, seed=72), np.ones((SS, SS), dtype=np.float32))

    # cross-section: water -> dark bank -> wet earth
    water_a = P.smoothstep(half + 0.5, half - 0.7, d)
    bank_a = P.smoothstep(half + BANK + 0.6, half + BANK - 0.8, d) * 0.88
    mud_a = P.smoothstep(half + BANK + MUD, half + BANK, d) * 0.30
    across = np.clip(lat / half, -1, 1)  # -1 .. 1 over the water
    deep = P.smoothstep(1.0, 0.0, np.abs(across))  # 1 mid-stream
    water = P.mix(P.hx("#2c7d8d"), P.hx("#56b0c0"), deep)
    lit = np.clip(0.5 - 0.5 * across, 0, 1)  # the upper-left bank catches the light
    water = P.mix(water, P.hx("#9adbd8"), P.smoothstep(0.55, 1.0, lit) * 0.30 * (1 - deep * 0.4))
    water = water * (1.0 + 0.16 * flow)[..., None]
    water = P.mix(water, P.hx("#d6f2ee"), P.smoothstep(0.62, 0.9, shimmer * 1.7) * 0.22 * deep)

    rgb = P.hx("#4a4128") * np.ones((h, w, 1), dtype=np.float32)  # wet earth
    alpha = mud_a
    over = bank_a
    rgb = P.mix(rgb, P.hx("#2f4636"), over)
    alpha = alpha + (1 - alpha) * over
    rgb = P.mix(rgb, water, water_a)
    alpha = alpha + (1 - alpha) * water_a

    # clip like the terrain cells so neighbouring river cells overlap without hairlines
    r = np.abs(np.kron(u, np.ones((SS, SS), dtype=np.float32))) + np.abs(np.kron(v, np.ones((SS, SS), dtype=np.float32)))
    alpha = alpha * np.clip((EDGE - r) / 0.0175 + 0.5, 0.0, 1.0)

    out = np.dstack([rgb * alpha[..., None], alpha])  # premultiplied while averaging
    out = out.reshape(H, SS, W, SS, 4).mean(axis=(1, 3))
    a_out = out[..., 3:4]
    colour = np.where(a_out > 1e-4, out[..., :3] / np.maximum(a_out, 1e-4), 0.0)
    return np.dstack([np.clip(colour, 0, 1), a_out[..., 0]])


def build_river_sheet() -> Image.Image:
    sheet = np.zeros((H * VARIANTS, W * 16, 4), dtype=np.float32)
    for variant in range(VARIANTS):
        for mask in range(1, 16):
            sheet[variant * H : (variant + 1) * H, mask * W : (mask + 1) * W] = render_river(mask, variant)
    return to_image(sheet)
