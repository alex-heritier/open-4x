"""Heightfield mountains: procedural relief, upper-left light, isometric projection.

The ground diamond of a tile (256 x 128 at 2x, sitting in the lower half of a 256 x 224 sprite)
is parameterised by (u, v) in [0, 1]^2:   sx = 128 + (u - v) * 128,   sy = 96 + (u + v) * 64 - h.
Rendering is a front-to-back "voxel space" painter with a per-column occlusion buffer, run on a
4x supersampled canvas.
"""

import numpy as np
from PIL import Image

from . import palette as P
from .noise import fbm
from .paint import _window_mean
from .palette import smoothstep

W, H = 256, 224
SS = 4
GRID = 512

# Palettes for the same relief in different climates.  `snow` is the height (0..1.2 of the
# summit) where snow starts, `foot` how far the scrub climbs the slopes.
STYLES = {
    "temperate": dict(
        rock=[(0, "#4a3426"), (0.35, "#6b4c33"), (0.65, "#8c6844"), (1.0, "#ad8a5e")],
        scrub=[(0, "#3f4c22"), (0.5, "#566330"), (1.0, "#727c3e")],
        snow=0.62,
        foot=0.40,
    ),
    "arid": dict(
        rock=[(0, "#6a5238"), (0.35, "#8a6c48"), (0.65, "#aa8c64"), (1.0, "#c9ad84")],
        scrub=[(0, "#6a5f36"), (0.5, "#807446"), (1.0, "#9c9060")],
        snow=0.95,
        foot=0.22,
    ),
    "arctic": dict(
        rock=[(0, "#4e545a"), (0.35, "#6c7279"), (0.65, "#8e949a"), (1.0, "#b4babf")],
        scrub=[(0, "#4c5744"), (0.5, "#626e58"), (1.0, "#808c76")],
        snow=0.15,
        foot=0.14,
    ),
}


def _cone(u, v, cu, cv, radius, amp, power):
    r = np.hypot(u - cu, v - cv) / radius
    return amp * np.clip(1.0 - r, 0.0, 1.0) ** power


def mountain_height(seed=3):
    """Height in 2x-sprite pixels over the (u, v) grid; Fuji-like main cone + two rocky shoulders."""
    u, v = np.meshgrid(np.linspace(0, 1, GRID, dtype=np.float32), np.linspace(0, 1, GRID, dtype=np.float32), indexing="ij")
    h = (
        _cone(u, v, 0.50, 0.50, 0.50, 132.0, 1.5)
        + _cone(u, v, 0.30, 0.66, 0.27, 52.0, 1.25)
        + _cone(u, v, 0.72, 0.30, 0.30, 60.0, 1.25)
    )
    a, b = u * 0.97, v * 0.97
    ridged = 1.0 - np.abs(fbm(a, b, 5, 4, seed=seed, warp=0.12) * 2.4)
    radial = 1.0 - np.abs(
        fbm(np.arctan2(v - 0.5, u - 0.5) / (2 * np.pi) % 1.0, np.hypot(u - 0.5, v - 0.5) * 0.9, 9, 3, seed=seed + 9, aniso=1.0) * 2.8
    )
    env = np.clip(h / 70.0, 0.0, 1.0) ** 0.9
    summit = 1.0 - 0.55 * np.clip((h - 70.0) / 70.0, 0.0, 1.0) ** 2
    h = h + env * summit * (ridged * 11.0 + radial * 8.0 - 9.0) + fbm(a, b, 24, 3, seed=seed + 2) * 2.2 * env
    return np.maximum(h, 0.0).astype(np.float32), u, v


def shade_height(h, u, v, seed=3, style="temperate"):
    """Per-sample RGB + alpha: rock strata, grassy foothills, snow that sticks to gentle slopes."""
    G = 400.0  # ground units across the footprint: sets how steep the relief looks
    gy, gx = np.gradient(h, G / GRID)  # axis 0 = u (screen right-down), axis 1 = v (screen left-down)
    nz = 1.0 / np.sqrt(1.0 + gx**2 + gy**2)
    nx, ny = -gx * nz, -gy * nz
    light = np.array([-0.62, 0.30, 0.72], dtype=np.float32)
    light /= np.linalg.norm(light)
    lam = np.clip(nx * light[0] + ny * light[1] + nz * light[2], 0.0, 1.0)
    steep = 1.0 - nz
    hn = np.clip(h / 130.0, 0.0, 1.2)
    a, b = u * 0.97, v * 0.97
    n1 = fbm(a, b, 14, 3, seed=seed + 4)
    n2 = fbm(a, b, 40, 2, seed=seed + 5)
    ao = np.clip((_window_mean(h, -6, 6, -6, 6) - h) / 9.0, 0.0, 1.0)

    t = np.clip(0.5 + 0.9 * n1 + 0.5 * n2, 0, 1)
    st = STYLES[style]
    rock = P.ramp(t, st["rock"])
    strata = 0.5 + 0.5 * np.sin(h * 0.28 + n1 * 5.0)
    rock = rock * (0.96 + 0.08 * strata)[..., None]
    rock = P.mix(rock, np.array([0.34, 0.26, 0.20], dtype=np.float32), np.clip(steep * 2.0, 0, 0.30))

    # foothills: olive scrub that fades up into rock
    green_t = np.clip(0.45 + 0.8 * n1 + 0.4 * n2, 0, 1)
    scrub = P.ramp(green_t, st["scrub"])
    low = 1.0 - smoothstep(0.10, st["foot"], hn + 0.06 * n1)
    col = P.mix(rock, scrub, low * (1.0 - smoothstep(0.1, 0.45, steep)))

    snow_line = st["snow"] + 0.09 * n1 + 0.05 * n2
    snow = smoothstep(snow_line - 0.04, snow_line + 0.05, hn) * (1.0 - smoothstep(0.24, 0.46, steep + 0.03 * n2))
    lit = 0.62 + 0.52 * lam
    snow_col = P.mix(P.hx("#a9b6c6"), P.hx("#e6e8e2"), np.clip(lam * 1.15, 0, 1))
    col = P.mix(col, snow_col / np.maximum(lit[..., None], 0.35) * 0.95, snow)

    col = col * (lit * (1.0 - 0.22 * ao))[..., None] * 1.05
    alpha = smoothstep(1.5, 9.0, h + 3.0 * n2)
    return np.clip(col, 0, 1), alpha


def project(h, col, alpha):
    """Front-to-back painter. Returns an RGBA float image (SS*W x SS*H)."""
    cw, ch = W * SS, H * SS
    img = np.zeros((ch, cw, 4), dtype=np.float32)
    n = GRID
    idx = np.arange(n)
    # each column's occlusion limit starts at the front edge of the footprint diamond
    xs_all = np.arange(cw)
    e = (xs_all - cw / 2) / (cw / 2)
    ymin = (96 * SS + (2.0 - np.abs(e)) * 64 * SS + 1).astype(np.int64)
    ymin = np.minimum(ymin, ch)
    for d in range(2 * n - 2, -1, -1):  # front (large u+v) to back
        i = idx[max(0, d - n + 1) : min(n, d + 1)]
        j = d - i
        uu, vv = i / (n - 1), j / (n - 1)
        sx = np.rint(cw / 2 + (uu - vv) * cw / 2).astype(np.int64)
        sg = 96 * SS + (uu + vv) * 64 * SS
        top = np.rint(sg - h[i, j] * SS).astype(np.int64)
        c, a = col[i, j], alpha[i, j]
        for ox in (0, 1):  # samples are 2 px apart on a diagonal; paint both columns
            x = np.clip(sx + ox, 0, cw - 1)
            lim = ymin[x]
            lens = lim - top
            active = lens > 0
            if not active.any():
                continue
            order = np.argsort(top)  # deterministic, irrelevant to the result
            for k in range(int(lens[active].max())):
                m = active & (lens > k)
                yy = top[m] + k
                ok = (yy >= 0) & (yy < ch)
                xx, yy = x[m][ok], yy[ok]
                img[yy, xx, :3] = c[m][ok]
                img[yy, xx, 3] = a[m][ok]
            new = np.minimum(ymin[x], top)
            ymin[x] = new
    return img


def hills_height(seed=21):
    """Subtle rolling swells: one broad low dome plus two faint shoulders (mountains reach 130+)."""
    u, v = np.meshgrid(np.linspace(0, 1, GRID, dtype=np.float32), np.linspace(0, 1, GRID, dtype=np.float32), indexing="ij")

    def dome(cu, cv, radius, amp):
        r = np.clip(np.hypot(u - cu, v - cv) / radius, 0.0, 1.0)
        return amp * (0.5 + 0.5 * np.cos(np.pi * r))  # round top, wide soft skirts

    rng = np.random.default_rng(seed)
    j = rng.uniform(-0.04, 0.04, 6)
    h = dome(0.50 + j[0], 0.50 + j[1], 0.52, 30.0) + dome(0.28 + j[2], 0.66 + j[3], 0.30, 12.0) + dome(0.73 + j[4], 0.33 + j[5], 0.32, 14.0)
    a, b = u * 0.97, v * 0.97
    env = np.clip(h / 12.0, 0.0, 1.0)
    h = h + env * (fbm(a, b, 4, 3, seed=seed, warp=0.15) * 3.2 + fbm(a, b, 18, 2, seed=seed + 3) * 1.0)
    return np.maximum(h, 0.0).astype(np.float32), u, v


HILL_STYLES = {
    "temperate": dict(
        cover=[(0, "#3d4c20"), (0.5, "#5a6a2a"), (1.0, "#848f40")],
        dry=[(0, "#6a6434"), (1.0, "#9c9050")],
        rock=[(0, "#5a4430"), (1.0, "#9a7c56")],
        snow=2.0,
    ),
    "arid": dict(
        cover=[(0, "#9a8e6c"), (0.5, "#b4a986"), (1.0, "#cdc4a2")],
        dry=[(0, "#a6976e"), (1.0, "#cec29e")],
        rock=[(0, "#7a5e44"), (1.0, "#b8996e")],
        snow=2.0,
    ),
    "arctic": dict(
        cover=[(0, "#7e8274"), (0.5, "#9a9c8c"), (1.0, "#b6b8a9")],
        dry=[(0, "#8a8a7a"), (1.0, "#b2b2a2")],
        rock=[(0, "#585e66"), (1.0, "#a0a8b0")],
        snow=0.45,
    ),
}


def shade_hills(h, u, v, seed=21, style="temperate"):
    st = HILL_STYLES[style]
    G = 560.0  # wider ground units than the mountains' 400: the same rise reads as a gentler slope
    gy, gx = np.gradient(h, G / GRID)
    nz = 1.0 / np.sqrt(1.0 + gx**2 + gy**2)
    nx, ny = -gx * nz, -gy * nz
    light = np.array([-0.62, 0.30, 0.72], dtype=np.float32)
    light /= np.linalg.norm(light)
    lam = np.clip(nx * light[0] + ny * light[1] + nz * light[2], 0.0, 1.0)
    steep = 1.0 - nz
    a, b = u * 0.97, v * 0.97
    n1 = fbm(a, b, 9, 3, seed=seed + 4)
    n2 = fbm(a, b, 38, 2, seed=seed + 5)
    ao = np.clip((_window_mean(h, -5, 5, -5, 5) - h) / 7.0, 0.0, 1.0)
    hn = np.clip(h / 34.0, 0.0, 1.2)

    t = np.clip(0.5 + 0.8 * n1 + 0.5 * n2, 0, 1)
    col = P.ramp(t, st["cover"])
    dry = P.smoothstep(0.05, 0.5, n1 * 1.6 + 0.2 * hn)
    col = P.mix(col, P.ramp(np.clip(0.5 + n2, 0, 1), st["dry"]), dry * 0.35)
    # outcrops: faint stone flecks on the steepest flanks and highest crowns only,
    # so the hill reads as grass first and rock a distant second
    ledge = 1.0 - np.abs(fbm(a, b, 13, 3, seed=seed + 8, warp=0.1) * 2.6)
    crown = P.smoothstep(0.62, 0.95, hn + 0.20 * n1)
    outcrop = np.clip(
        P.smoothstep(0.10, 0.20, steep + 0.02 * n2) * 0.55
        + crown * P.smoothstep(0.45, 0.75, ledge) * 0.35
        + P.smoothstep(0.80, 0.95, ledge) * P.smoothstep(0.25, 0.6, hn) * 0.30,
        0.0, 1.0,
    )
    rock_col = P.ramp(np.clip(0.5 + 0.8 * n2 + 0.25 * (ledge - 0.5), 0, 1), st["rock"])
    strata = 0.5 + 0.5 * np.sin(h * 0.55 + n1 * 6.0)
    rock_col = rock_col * (0.90 + 0.18 * strata)[..., None]
    col = P.mix(col, rock_col, outcrop * 0.85)
    snow = P.smoothstep(st["snow"] - 0.08, st["snow"] + 0.06, hn + 0.12 * n1) * (1.0 - outcrop * 0.8)
    lit = 0.72 + 0.36 * lam
    snow_col = P.mix(P.hx("#a9b6c6"), P.hx("#e6e8e2"), np.clip(lam * 1.15, 0, 1))
    col = P.mix(col, snow_col / np.maximum(lit[..., None], 0.35) * 0.95, snow)
    col = col * (lit * (1.0 - 0.18 * ao))[..., None] * 1.05
    # a long feathered skirt: the lower slopes thin out so the ground's own texture shows through
    alpha = smoothstep(0.5, 9.0, h + 2.0 * n2)
    return np.clip(col, 0, 1), alpha


def _finish(img) -> Image.Image:
    from PIL import ImageFilter

    pil = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8), "RGBA")
    pil.putalpha(pil.getchannel("A").filter(ImageFilter.GaussianBlur(1.2)))
    return pil


def build_hills_layer(seed=21, style="temperate") -> Image.Image:
    h, u, v = hills_height(seed)
    col, alpha = shade_hills(h, u, v, seed, style)
    return _finish(project(h, col, alpha))


def build_mountain_layer(seed=3, style="temperate") -> Image.Image:
    h, u, v = mountain_height(seed)
    col, alpha = shade_height(h, u, v, seed, style)
    return _finish(project(h, col, alpha))


def vegetation_surface(h, u, v, col, alpha, seed, cover, style):
    """Trees rooted in the heightfield, painted and occluded with the rock beneath them.

    Each crown replaces a patch of the surface, rather than overlaying a flat tree sprite.
    Forest is pointed evergreen growth; jungle is round, dense broadleaf growth. Summit
    and steep cliff exclusions leave the relief legible even in a dense woodland.
    """
    rng = np.random.default_rng(seed + 701)
    base = h.copy()
    gy, gx = np.gradient(base, 400.0 / GRID)
    steep = np.hypot(gx, gy)
    tree_top = base.copy()
    leaves = np.zeros_like(h)
    light = np.zeros_like(h)
    jungle = cover == "jungle"
    for _ in range(105 if jungle else 85):
        cu, cv = rng.uniform(0.09, 0.91, 2)
        i, j = int(cu * (GRID - 1)), int(cv * (GRID - 1))
        if base[i, j] < 3 or base[i, j] > (105 if jungle else 90) or steep[i, j] > 1.1:
            continue
        radius = rng.uniform(0.028, 0.050) if jungle else rng.uniform(0.021, 0.036)
        r = np.hypot(u - cu, v - cv) / radius
        crown = np.clip(1 - r, 0, 1) ** (0.55 if jungle else 1.15)
        height = rng.uniform(7, 14) if jungle else rng.uniform(10, 19)
        top = base[i, j] + crown * height
        visible = (r < 1) & (top > tree_top)
        tree_top = np.where(visible, top, tree_top)
        leaves = np.where(visible, smoothstep(0.0, 0.20, crown), leaves)
        light = np.where(visible, np.clip(0.55 + (cv - v) / radius * 0.22 + (cu - u) / radius * 0.12 + crown * 0.22, 0, 1), light)
    colors = ("#24471f", "#5a8a3a") if jungle else ("#263e1f", "#5a7a36")
    if style == "arctic":
        colors = ("#2e4535", "#7c9684")
    canopy = P.mix(P.hx(colors[0]), P.hx(colors[1]), light)
    col = P.mix(col, canopy, leaves)
    alpha = np.maximum(alpha, leaves)
    return tree_top, col, alpha


def scene(kind, seed, style, cover="bare"):
    """The single source of geometry for both the visible relief and its border ribbons."""
    height, shader = (mountain_height, shade_height) if kind == "mountain" else (hills_height, shade_hills)
    h, u, v = height(seed)
    col, alpha = shader(h, u, v, seed, style)
    if cover != "bare":
        h, col, alpha = vegetation_surface(h, u, v, col, alpha, seed, cover, style)
    return h, u, v, col, alpha


def border_layers(h, u, v):
    """Four dashed white ribbons draped over the actual heightfield, including occlusion.

    The footprint edge bows inward across a slope, returns to the same inset at its
    ends, and is displaced vertically by h. Dash phase is measured in footprint space
    so changing height does not stretch or renumber the beads. Transparent foreground
    samples still occlude hidden parts of a rear ribbon in `project`.
    """
    for col, alpha in border_masks(u, v, bow=0.12):
        yield _finish(project(h, col, alpha))


def border_masks(u, v, bow=0.0):
    """Shared bead/thread paint for flat and lifted borders; eight periods per edge.

    Bold Civ3-style dashes: a thick white core (tinted to the nation's colour by
    the client) ringed by a black outline that survives the tint, so the border
    reads on any ground. The outline works because the client multiplies the
    texture by the nation colour: white becomes the nation colour, black stays
    black.
    """
    # Orient parameters clockwise N->E->S->W->N, matching the client's edge neighbours.
    for t, distance in ((u, v), (v, 1-u), (1-u, 1-v), (1-v, u)):
        inset = 0.055 + bow * np.sin(np.pi * t) ** 2
        delta = np.abs(distance - inset) * 143.1  # source pixels normal to an edge
        phase = (t * 143.1) % 18.0
        bead = (phase >= 4) & (phase < 17)
        rim = 1 - smoothstep(3.4, 4.8, delta)
        core = 1 - smoothstep(1.6, 3.0, delta)
        alpha = np.where(bead, rim, (1 - smoothstep(0.5, 1.1, delta)) * 0.75)
        alpha *= smoothstep(0.025, 0.05, t) * (1-smoothstep(0.95, 0.975, t))
        col = np.ones((*u.shape, 3), dtype=np.float32) * core[..., None]
        yield col, alpha


def build_surface(kind, seed, style, cover="bare"):
    h, u, v, col, alpha = scene(kind, seed, style, cover)
    image = _finish(project(h, col, alpha))
    sheet = Image.new("RGBA", (4 * W, H))
    for edge, layer in enumerate(border_layers(h, u, v)):
        sheet.paste(layer.resize((W, H), Image.Resampling.LANCZOS), (edge * W, 0))
    return image, sheet
