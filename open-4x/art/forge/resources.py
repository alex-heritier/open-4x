"""Pictures of natural resources, drawn on the map over the square that holds them.

Each resource is a small painting of the thing itself standing on the ground: a heap of ore with
nuggets in it, a sheaf of wheat, a cow, a derrick over a pool of oil. Nothing frames it: no disc, no
ring and no outline, only a soft contact shadow, so on the map it reads as an object lying on the
terrain rather than as an interface badge. Shapes are shaded from the upper left like everything else
(a light facet, a dark facet, a highlight), and the painterly pass in `build.py` gives them the
same brush texture as the relief and the cities.

Canvas 128 x 128, shown at 60 x 60. Every picture stands on the point `BASE` = (64, 104).
"""

import math
import random

from .svgkit import Svg, lerp, shade

SIZE = 128
BASE = (64, 104)
SCALE = 1.1
LIGHT = (-0.62, -0.78)  # towards the sun, in picture coordinates (y down)


# --- helpers --------------------------------------------------------------------------------------------
def grad(svg: Svg, col: str, lo=0.6, hi=1.28, x2=0.55, y2=1.0):
    """Light at the top left, falling to shade at the bottom right."""
    return svg.lin([(0, shade(col, hi)), (1, shade(col, lo))], 0, 0, x2, y2)


def glow(svg: Svg, col: str, lo=0.5, hi=1.4, r=0.75):
    """A radial gradient with its highlight at the upper left: round, lit objects."""
    return svg.rad([(0, shade(col, hi)), (0.55, col), (1, shade(col, lo))], 0.36, 0.32, r)


def shadow(svg: Svg, cx=BASE[0], cy=BASE[1] + 1, rx=34, ry=8, op=0.42):
    svg.ellipse(cx + 3, cy, rx, ry, "#0d0a06", op=op, blur=svg.blur(2.6))


def sphere(svg: Svg, cx, cy, r, col, lo=0.5, hi=1.45):
    svg.circle(cx, cy, r, glow(svg, col, lo, hi))
    svg.circle(cx - r * 0.38, cy - r * 0.42, max(0.8, r * 0.2), "#ffffff", op=0.55)


def blob(svg: Svg, cx, cy, rx, ry, col, rot=0, lo=0.55, hi=1.35, op=1.0):
    svg.ellipse(cx, cy, rx, ry, glow(svg, col, lo, hi, 0.8), op=op, rot=rot)


def stroke(svg: Svg, d: str, col: str, w: float, op=1.0):
    svg.path(d, "none", col, w, op)


def tube(svg: Svg, d: str, col: str, w: float):
    """A lit stem or limb: a dark underside, the colour, then a highlight offset to the sun."""
    stroke(svg, d, shade(col, 0.55), w)
    stroke(svg, d, col, w * 0.72)
    svg.path(d, "none", shade(col, 1.45), max(0.8, w * 0.2), 0.6, extra='transform="translate(-0.6,-0.6)"')


def leaf(svg: Svg, x, y, ang, length, width, col, vein=True, curve=0.0):
    """A pointed leaf from (x, y) in direction ang (degrees, 0 = right, -90 = up)."""
    a = math.radians(ang)
    ux, uy = math.cos(a), math.sin(a)
    px, py = -uy, ux
    tip = (x + ux * length, y + uy * length)
    mid = (x + ux * length * 0.5 + px * curve, y + uy * length * 0.5 + py * curve)
    left = (mid[0] + px * width, mid[1] + py * width)
    right = (mid[0] - px * width, mid[1] - py * width)
    d = f"M{x:.1f},{y:.1f} Q{left[0]:.1f},{left[1]:.1f} {tip[0]:.1f},{tip[1]:.1f} Q{right[0]:.1f},{right[1]:.1f} {x:.1f},{y:.1f} Z"
    # the half facing the sun is lighter
    svg.path(d, svg.lin([(0, shade(col, 1.3)), (1, shade(col, 0.62))], 0, 0, 0.7, 1))
    if vein:
        stroke(svg, f"M{x:.1f},{y:.1f} Q{mid[0]:.1f},{mid[1]:.1f} {tip[0]:.1f},{tip[1]:.1f}", shade(col, 1.5), 0.9, 0.55)


def facets(svg: Svg, pts, col: str, centre=None, lo=0.5, hi=1.34, jitter=None):
    """A faceted lump: every edge makes a triangle with a point inside, lit by its direction."""
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    c = centre or (cx - 2.5, cy - 2.5)
    for i, p in enumerate(pts):
        q = pts[(i + 1) % len(pts)]
        mx, my = (p[0] + q[0]) / 2 - cx, (p[1] + q[1]) / 2 - cy
        norm = math.hypot(mx, my) or 1.0
        lit = (mx / norm) * LIGHT[0] + (my / norm) * LIGHT[1]  # -1 .. 1
        f = lo + (hi - lo) * (0.5 + 0.5 * lit)
        svg.poly([c, p, q], shade(col, f) if f <= 1 else shade(col, f))
    # a thin bright rim along the sunlit upper edges
    for i, p in enumerate(pts):
        q = pts[(i + 1) % len(pts)]
        mx, my = (p[0] + q[0]) / 2 - cx, (p[1] + q[1]) / 2 - cy
        norm = math.hypot(mx, my) or 1.0
        if (mx / norm) * LIGHT[0] + (my / norm) * LIGHT[1] > 0.55:
            svg.line(p, q, shade(col, 1.55), 1.0, 0.55)


def lump(rng: random.Random, cx, cy, rx, ry, n=7, rough=0.22, squash=1.0):
    """An irregular outline for a rock or nugget."""
    pts = []
    for i in range(n):
        a = i / n * math.tau + rng.uniform(-0.2, 0.2)
        k = 1.0 + rng.uniform(-rough, rough)
        pts.append((cx + math.cos(a) * rx * k, cy + math.sin(a) * ry * k * squash))
    return pts


def smooth_path(pts) -> str:
    """A closed outline through the midpoints of `pts`, rounding every corner."""
    mids = [((p[0] + q[0]) / 2, (p[1] + q[1]) / 2) for p, q in zip(pts, pts[1:] + pts[:1])]
    d = f"M{mids[-1][0]:.1f},{mids[-1][1]:.1f}"
    for p, m in zip(pts, mids):
        d += f" Q{p[0]:.1f},{p[1]:.1f} {m[0]:.1f},{m[1]:.1f}"
    return d + " Z"


def boulder(svg: Svg, rng: random.Random, pts, col: str, lo=0.42, hi=1.42, gloss=0.35):
    """A rounded stone: soft light on its upper left, shade pooling under it, a highlight and a
    crack or two.  Painted volume rather than cut facets."""
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    rx, ry = (max(xs) - min(xs)) / 2, (max(ys) - min(ys)) / 2
    d = smooth_path(pts)
    svg.path(d, svg.rad([(0, shade(col, hi)), (0.45, col), (1, shade(col, lo))], 0.34, 0.28, 0.82))
    svg.ellipse(cx + rx * 0.15, cy + ry * 0.55, rx * 0.85, ry * 0.4, shade(col, lo * 0.8), op=0.35, blur=svg.blur(1.4))
    svg.ellipse(cx - rx * 0.35, cy - ry * 0.4, rx * 0.32, ry * 0.18, "#ffffff", op=gloss * 0.6, rot=-25, blur=svg.blur(0.8))
    for _ in range(rng.randint(1, 2)):
        x0, y0 = cx + rng.uniform(-rx * 0.4, rx * 0.3), cy + rng.uniform(-ry * 0.3, ry * 0.3)
        svg.path(f"M{x0:.1f},{y0:.1f} q{rng.uniform(2, 5):.1f},{rng.uniform(-1, 2):.1f} {rng.uniform(4, 8):.1f},{rng.uniform(1, 4):.1f}", "none", shade(col, lo), 0.8, 0.55)


def tufts(svg: Svg, rng: random.Random, cx=BASE[0], cy=BASE[1], half=34, n=6, col="#56702a"):
    """Grass blades round the foot of a resource, so it stands in the ground instead of on it."""
    for _ in range(n):
        x = cx + rng.uniform(-half, half)
        y = cy + rng.uniform(-3, 3)
        for k in range(3):
            h = rng.uniform(4, 8)
            lean = (k - 1) * 2.2 + rng.uniform(-1, 1)
            svg.path(f"M{x + k * 1.4 - 1.4:.1f},{y:.1f} Q{x + lean * 0.4:.1f},{y - h * 0.6:.1f} {x + lean:.1f},{y - h:.1f}", "none", shade(col, rng.uniform(0.7, 1.2)), 1.0, 0.75)


def sparkle(svg: Svg, x, y, r=4.0, col="#ffffff", op=0.9):
    svg.path(
        f"M{x},{y - r} L{x + r * 0.22},{y - r * 0.22} L{x + r},{y} L{x + r * 0.22},{y + r * 0.22} "
        f"L{x},{y + r} L{x - r * 0.22},{y + r * 0.22} L{x - r},{y} L{x - r * 0.22},{y - r * 0.22} Z",
        col,
        op=op,
    )


def ground_patch(svg: Svg, cx, cy, rx, ry, col="#6b5a3a", op=0.55):
    """Bare earth under a crop, a pit or a pool."""
    svg.ellipse(cx, cy, rx, ry, col, op=op, blur=svg.blur(2.0))


# --- ore ------------------------------------------------------------------------------------------------
def ore(svg: Svg, rock: str, nugget: str | None = None, glint=False, seed=1, gems: str | None = None, streak: str | None = None, heap=False):
    """A heap of rock with the ore showing in it."""
    rng = random.Random(seed)
    shadow(svg, rx=36, ry=9)
    spec = [(40, 90, 17, 12), (78, 92, 19, 13), (58, 74, 20, 15), (30, 99, 11, 7), (92, 100, 10, 6)]
    ground_patch(svg, 62, 100, 40, 8, "#5b4a2e", 0.45)
    for cx, cy, rx, ry in sorted(spec, key=lambda s: s[1]):  # back to front
        if heap:  # a mound packed with small lumps (coal) instead of a few stones
            svg.path(smooth_path(lump(rng, cx, cy + 1, rx * 1.05, ry, 9, 0.1)), svg.rad([(0, shade(rock, 1.6)), (1, shade(rock, 0.4))], 0.35, 0.3, 0.8))
            for _ in range(int(rx * ry / 10)):
                a_, d_ = rng.uniform(0, math.tau), rng.random() ** 0.6
                x, y = cx + math.cos(a_) * d_ * rx * 0.85, cy + math.sin(a_) * d_ * ry * 0.8
                r = rng.uniform(3.6, 6.0)
                boulder(svg, rng, lump(rng, x, y, r, r * 0.75, 6, 0.3), rock, lo=0.35, hi=2.2, gloss=0.25)
        else:
            boulder(svg, rng, lump(rng, cx, cy, rx, ry, 8, 0.16), rock)
    if streak:
        for cx, cy, rx, ry in spec[:3]:
            for _ in range(2):
                x = cx + rng.uniform(-rx * 0.5, rx * 0.4)
                y = cy + rng.uniform(-ry * 0.5, ry * 0.3)
                svg.path(f"M{x - 5:.1f},{y + 2:.1f} L{x:.1f},{y - 2:.1f} L{x + 6:.1f},{y + 1:.1f} L{x + 1:.1f},{y + 3:.1f} Z", streak, op=0.85)
    if nugget:
        for cx, cy, r in ((44, 82, 9), (70, 66, 10.5), (86, 86, 8.5), (56, 95, 7), (34, 95, 5.5), (62, 82, 6.5)):
            r *= 0.8
            boulder(svg, rng, lump(rng, cx, cy, r, r * 0.7, 6, 0.32), nugget, lo=0.45, hi=1.5, gloss=0.6)
    if gems:
        for cx, cy, s, rot in ((60, 70, 13, -8), (42, 84, 9, 14), (80, 84, 10, 8)):
            gem(svg, cx, cy, s, gems, rot)
    if glint:
        for x, y, r in ((46, 78, 4.5), (72, 62, 5.5), (88, 82, 4)):
            sparkle(svg, x, y, r)
    tufts(svg, rng, cy=BASE[1] - 1, half=38, n=5)


def gem(svg: Svg, cx, cy, s, col: str, rot=0):
    """A cut stone, point down: table, crown facets and pavilion."""
    t = f'transform="rotate({rot} {cx} {cy})"'
    p = lambda x, y: (cx + x * s, cy + y * s)  # noqa: E731
    outline = [p(-1.0, -0.35), p(-0.55, -0.9), p(0.55, -0.9), p(1.0, -0.35), p(0.0, 1.05)]
    svg.begin(t)
    svg.poly(outline, svg.lin([(0, shade(col, 1.5)), (0.5, col), (1, shade(col, 0.55))], 0, 0, 0.8, 1), op=0.96)
    svg.poly([p(-1.0, -0.35), p(-0.55, -0.9), p(-0.25, -0.35)], shade(col, 1.55), op=0.85)
    svg.poly([p(-0.25, -0.35), p(-0.55, -0.9), p(0.55, -0.9), p(0.25, -0.35)], shade(col, 1.25), op=0.9)
    svg.poly([p(0.25, -0.35), p(0.55, -0.9), p(1.0, -0.35)], shade(col, 0.85), op=0.9)
    svg.poly([p(-1.0, -0.35), p(-0.25, -0.35), p(0.0, 1.05)], shade(col, 1.1), op=0.8)
    svg.poly([p(0.25, -0.35), p(1.0, -0.35), p(0.0, 1.05)], shade(col, 0.6), op=0.85)
    svg.poly([p(-0.25, -0.35), p(0.25, -0.35), p(0.0, 1.05)], shade(col, 0.95), op=0.7)
    svg.end()
    sparkle(svg, cx - s * 0.45, cy - s * 0.55, s * 0.4)


# --- crops and plants -----------------------------------------------------------------------------------
def wheat(svg: Svg, grain="#e2b53c", stem="#b9a34a", bend=0.0, awns=True, heads=9, seed=2, drooping=False, base_leaf="#7f9a3c"):
    rng = random.Random(seed)
    shadow(svg, rx=32, ry=7)
    ground_patch(svg, 64, 102, 30, 6, "#5b4a2a", 0.5)
    for i in range(heads):
        t = (i / (heads - 1)) * 2 - 1
        x0 = 64 + t * 22 + rng.uniform(-3, 3)
        top = 28 + abs(t) * 12 + rng.uniform(-4, 4)
        lean = t * 11 + bend
        x1 = x0 + lean
        d = f"M{x0:.1f},{BASE[1]} C{x0 + lean * 0.1:.1f},{BASE[1] - 34} {x1 - lean * 0.2:.1f},{top + 26:.1f} {x1:.1f},{top + 6:.1f}"
        tube(svg, d, stem, 2.6)
        # the ear: kernels in a pair of rows, each one a small lit ellipse
        n = 7 if not drooping else 6
        for k in range(n):
            y = top + k * 5.2
            sway = (math.sin(k * 0.5) * 0.6 + (k * 0.55 if drooping else 0.0))
            for sx in (-1, 1):
                blob(svg, x1 + sx * 3.1 + sway, y, 2.5, 4.4, grain, rot=sx * 26, lo=0.62, hi=1.4)
        blob(svg, x1 + (0.5 if drooping else 0), top - 3.5, 2.2, 4.0, grain, lo=0.62, hi=1.4)
        if awns:
            for sx in (-1, 1):
                svg.line((x1 + sx * 2.5, top + 4), (x1 + sx * 9.5, top - 14), shade(grain, 1.15), 0.8, 0.8)
    for i in range(5):  # blades at the foot
        x = 36 + i * 14 + rng.uniform(-3, 3)
        leaf(svg, x, BASE[1] + 1, -90 + (i - 2) * 16 + rng.uniform(-6, 6), 30 + rng.uniform(0, 8), 3.4, base_leaf, vein=False, curve=3)


def rice(svg: Svg):
    """Rice standing in a flooded paddy."""
    ground_patch(svg, 64, 100, 38, 12, "#7ec4d6", 0.55)
    svg.ellipse(64, 100, 38, 12, "#9bd4e2", op=0.35)
    svg.path("M30,98 C44,95 54,99 64,97 C74,95 88,99 98,97", "none", "#ffffff", 1.2, 0.5)
    wheat(svg, grain="#e6d58f", stem="#7da13c", bend=-4, awns=False, heads=8, seed=5, drooping=True, base_leaf="#5f9a35")


def corn(svg: Svg):
    shadow(svg, rx=30, ry=7)
    ground_patch(svg, 64, 102, 28, 6, "#5b4a2a", 0.5)
    # the stalk, with long arching leaves
    tube(svg, f"M64,{BASE[1]} C63,76 66,50 64,22", "#79a23c", 5.5)
    for i, (y, side) in enumerate(((98, -1), (88, 1), (76, -1), (64, 1), (52, -1), (40, 1))):
        leaf(svg, 64, y, (-90 + side * 88) if i % 2 else (-90 + side * 80), 34 - i * 2, 4.4, "#6a9a34" if side < 0 else "#82ae40", curve=-side * 6)
    # two cobs in their husks, kernels showing at the tip
    for cx, cy, rot in ((50, 60, -14), (79, 52, 12)):
        svg.begin(f'transform="rotate({rot} {cx} {cy})"')
        blob(svg, cx, cy + 4, 8.5, 20, "#8fb64a", lo=0.55, hi=1.3)
        blob(svg, cx, cy - 10, 6.4, 11, "#f1c63a", lo=0.62, hi=1.35)
        for r in range(4):
            for c in range(3):
                svg.circle(cx - 3.4 + c * 3.4, cy - 17 + r * 4.4, 1.5, shade("#f1c63a", 1.35 if (r + c) % 2 else 0.8), op=0.9)
        svg.end()
    # tassel
    for sx in (-1, 1):
        svg.path(f"M64,22 Q{64 + sx * 7},12 {64 + sx * 12},16", "none", "#d8c27a", 1.3, 0.9)
    svg.path("M64,22 Q64,10 64,6", "none", "#d8c27a", 1.3, 0.9)


def sugar(svg: Svg):
    shadow(svg, rx=32, ry=7)
    for x, lean in ((38, -7), (51, -2), (64, 3), (77, 7), (89, 11), (58, 1)):
        top = 20 + abs(x - 64) * 0.4
        d = f"M{x},{BASE[1]} L{x + lean * 1.4},{top}"
        stroke(svg, d, "#6d5d2c", 8.2, 0.9)
        stroke(svg, d, "#d9c777", 6.6)
        stroke(svg, d, "#f4e9b0", 1.8, 0.8)
        for k in range(1, 6):  # joints
            t = k / 6
            jx, jy = x + lean * 1.4 * (1 - t), BASE[1] + (top - BASE[1]) * (1 - t)
            svg.line((jx - 4.3, jy), (jx + 4.3, jy), "#8a7a38", 1.4, 0.85)
        for sx in (-1, 1):
            leaf(svg, x + lean * 1.4, top + 6, -90 + sx * 62, 25, 3.2, "#7dae3e" if sx < 0 else "#97c24c", curve=sx * 4)


def cotton(svg: Svg):
    shadow(svg, rx=32, ry=7)
    ground_patch(svg, 64, 102, 30, 6, "#5b4a2a", 0.5)
    for x0, x1 in ((46, 40), (64, 64), (82, 88)):
        tube(svg, f"M{x0},{BASE[1]} C{x0},80 {x1 + 2},60 {x1},40", "#7a8f3a", 3.2)
    for cx, cy in ((38, 52), (52, 40), (64, 32), (76, 44), (90, 54), (56, 62), (74, 66), (46, 76)):
        # the burr, then the lock of cotton rising out of it
        svg.path(f"M{cx - 7},{cy + 5} Q{cx},{cy + 14} {cx + 7},{cy + 5} L{cx + 4},{cy + 2} L{cx - 4},{cy + 2} Z", "#75552e")
        for dx, dy, r in ((-4.2, 0, 6), (4.2, 0, 6), (0, -4.5, 6.5), (0, 1.5, 5.5)):
            sphere(svg, cx + dx, cy + dy, r, "#fbf9f0", lo=0.78, hi=1.1)
    for sx, y in ((-1, 84), (1, 74), (-1, 62), (1, 50)):
        leaf(svg, 64, y, -90 + sx * 70, 20, 4.4, "#6f9a3a", curve=sx * 3)


def tobacco(svg: Svg):
    shadow(svg, rx=32, ry=7)
    ground_patch(svg, 64, 102, 28, 6, "#5b4a2a", 0.5)
    tube(svg, f"M64,{BASE[1]} L64,52", "#7b8a36", 5.0)
    fan = ((-172, 44, "#7a8a32", -8), (-8, 44, "#7a8a32", 8), (-150, 50, "#89993a", -7), (-30, 50, "#89993a", 7), (-124, 52, "#97a644", -5), (-56, 52, "#97a644", 5), (-90, 46, "#a3b24c", 0))
    for ang, length, col, curve in fan:
        leaf(svg, 64, 96 if abs(ang + 90) > 50 else 80, ang, length, 14, col, curve=curve)
    for dx, dy in ((-5, 36), (0, 30), (5, 37), (-1, 40)):
        svg.circle(64 + dx, dy, 3.0, "#e8a0b4")
        svg.circle(64 + dx - 0.8, dy - 0.8, 1.0, "#ffe0ea", op=0.9)


def tea(svg: Svg):
    """A rounded bush of small leaves with pale new shoots at the tips."""
    rng = random.Random(7)
    shadow(svg, rx=36, ry=8)
    ground_patch(svg, 64, 102, 34, 7, "#5b4a2a", 0.5)
    svg.ellipse(64, 78, 36, 28, glow(svg, "#3f7a2e", 0.45, 1.25, 0.9))
    for _ in range(95):
        a = rng.uniform(0, math.tau)
        r = math.sqrt(rng.random())
        x, y = 64 + math.cos(a) * 33 * r, 76 + math.sin(a) * 25 * r
        if y > 100:
            continue
        lightness = 0.78 + 0.5 * (0.5 - (x - 40) / 70 * 0.5 - (y - 50) / 60 * 0.5) + rng.uniform(-0.1, 0.1)
        leaf(svg, x, y, rng.uniform(-170, 10), 11, 3.8, shade("#4a8f34", max(0.5, min(1.45, lightness))), vein=False)
    for _ in range(34):  # new growth
        a = rng.uniform(math.pi * 1.1, math.pi * 1.9)
        r = rng.uniform(0.3, 0.95)
        x, y = 64 + math.cos(a) * 32 * r, 76 + math.sin(a) * 25 * r
        for k in (-1, 1):
            leaf(svg, x, y, -90 + k * 28, 10, 3.0, "#cdea7c", vein=False)


def vine(svg: Svg):
    """Two bunches of grapes under broad vine leaves, on a post and wire."""
    shadow(svg, rx=34, ry=7)
    tube(svg, f"M28,{BASE[1]} L28,34", "#6e4f2c", 4.0)
    tube(svg, f"M100,{BASE[1]} L100,34", "#6e4f2c", 4.0)
    stroke(svg, "M28,38 Q64,46 100,38", "#bfb8a6", 1.6)
    stroke(svg, "M28,38 C40,30 52,48 64,40 C76,32 90,46 100,36", "#7a5a30", 2.4)
    for cx, top in ((48, 42), (82, 44)):
        rows = [(cx, top + 6, 1), (cx - 6, top + 14, 2), (cx + 6, top + 14, 2), (cx - 9, top + 23, 2), (cx + 1, top + 23, 1), (cx + 10, top + 23, 1), (cx - 4, top + 32, 2), (cx + 6, top + 32, 1), (cx + 1, top + 41, 1)]
        for x, y, _ in reversed(rows):
            sphere(svg, x, y, 6.4, "#5b2a82", lo=0.4, hi=1.7)
        stroke(svg, f"M{cx},{top - 2} L{cx},{top + 5}", "#5b8a2e", 2.0)
    for x, y, a, s in ((34, 56, -150, 28), (62, 52, -80, 26), (98, 60, -30, 28), (70, 36, -60, 22)):
        leaf(svg, x, y, a, s, 12, "#5d9a36", curve=3)


def coffee(svg: Svg):
    shadow(svg, rx=32, ry=7)
    tube(svg, f"M64,{BASE[1]} C62,84 66,62 64,36", "#6a4a2a", 4.4)
    tube(svg, "M64,70 C54,66 48,60 40,54", "#6a4a2a", 3)
    tube(svg, "M64,60 C74,56 82,52 90,46", "#6a4a2a", 3)
    for x, y, a in ((64, 38, -100), (62, 52, -150), (66, 50, -30), (42, 56, -170), (44, 54, -120), (88, 48, -10), (86, 46, -60), (56, 72, 170), (74, 76, 10), (50, 64, -165), (80, 58, -20)):
        leaf(svg, x, y, a, 26, 8.5, "#2f6f2f", curve=3)
    for x, y in ((50, 64), (54, 70), (47, 71), (78, 60), (82, 66), (74, 66), (60, 46), (68, 44), (64, 52)):
        sphere(svg, x, y, 3.8, "#c92c2c", lo=0.4, hi=1.6)
    # beans in a small heap
    for x, y, a in ((50, 100, 20), (60, 102, -30), (70, 100, 40), (80, 102, -10), (56, 96, 60), (66, 95, -50)):
        svg.begin(f'transform="rotate({a} {x} {y})"')
        blob(svg, x, y, 5.2, 3.6, "#6a3d1c", lo=0.45, hi=1.4)
        svg.path(f"M{x - 4},{y} Q{x},{y - 1.5} {x + 4},{y}", "none", "#2a1608", 1.0, 0.8)
        svg.end()


def cocoa(svg: Svg):
    shadow(svg, rx=30, ry=7)
    tube(svg, f"M64,{BASE[1]} C60,80 68,60 64,26", "#6a4a2a", 7.0)
    for x, y, a, col in ((52, 76, -20, "#d98a2a"), (76, 66, 22, "#c9702a"), (50, 50, 24, "#e0a030"), (78, 42, -16, "#d98a2a"), (56, 32, 10, "#c9702a")):
        svg.begin(f'transform="rotate({a} {x} {y})"')
        svg.ellipse(x, y, 9, 16, glow(svg, col, 0.5, 1.4, 0.85))
        for k in (-5, 0, 5):
            svg.path(f"M{x + k * 0.9},{y - 14} Q{x + k * 1.6},{y} {x + k * 0.9},{y + 14}", "none", shade(col, 0.55), 1.0, 0.7)
        svg.end()
    for x, y, a in ((40, 38, -160), (90, 56, -20), (38, 66, 170), (88, 30, -40)):
        leaf(svg, 64, y + 10 if x < 64 else y + 6, a, 30, 9, "#3d7a2c", curve=3)


def timber(svg: Svg):
    shadow(svg, rx=40, ry=8)
    bark, wood = "#7a5230", "#d9ad6e"
    spec = [(38, 94), (66, 94), (94, 94), (52, 74), (80, 74), (66, 54)]
    for i, (cx, cy) in enumerate(spec):
        svg.rect(cx - 6, cy - 11, 40 if cx < 90 else 30, 22, svg.lin([(0, shade(bark, 1.3)), (0.5, bark), (1, shade(bark, 0.55))], 0, 0, 0, 1), rx=5)
    for i, (cx, cy) in enumerate(spec):
        # the sawn end faces the viewer on the left
        svg.circle(cx - 8, cy, 11.5, glow(svg, wood, 0.7, 1.25, 0.9))
        for r in (3, 6.5, 9.5):
            svg.circle(cx - 8, cy, r, "none", stroke=shade(wood, 0.72), sw=0.8)
        svg.circle(cx - 8, cy, 1.4, shade(wood, 0.6))
    for cx, cy in spec[:3]:
        svg.line((cx + 5, cy - 6), (cx + 30, cy - 6), shade(bark, 1.5), 1.1, 0.5)
    tufts(svg, random.Random(9), cy=BASE[1] + 1, half=40, n=6)


def coconuts(svg: Svg):
    palm(svg, nuts=True)


def palm(svg: Svg, water=False, nuts=False):
    if water:
        oasis_pool(svg)
    else:
        shadow(svg, rx=30, ry=7)
    base = 100
    trunk = f"M60,{base} C57,78 66,56 72,34"
    stroke(svg, trunk, "#4a3220", 8.6)
    stroke(svg, trunk, "#9a7448", 6.6)
    stroke(svg, trunk, "#d8b57a", 1.8, 0.7)
    for y in range(0, 7):
        t = y / 7
        x, yy = lerp((60, base), (70, 40), t)
        svg.line((x - 3.6, yy), (x + 3.4, yy - 1), "#6a4a2c", 1.1, 0.7)
    cx, cy = 72, 34
    fronds = [(-175, 36), (-150, 40), (-120, 38), (-90, 32), (-58, 38), (-30, 40), (-6, 34), (160, 26), (24, 26)]
    for ang, length in fronds:
        a = math.radians(ang)
        ex, ey = cx + math.cos(a) * length, cy + math.sin(a) * length * 0.8 + 12
        mx, my = cx + math.cos(a) * length * 0.55, cy + math.sin(a) * length * 0.6 - 8
        d = f"M{cx},{cy} Q{mx:.1f},{my:.1f} {ex:.1f},{ey:.1f}"
        stroke(svg, d, "#2c5a24", 3.2)
        stroke(svg, d, "#5aa43c", 1.6)
        for k in range(1, 7):  # pinnae
            t = k / 7
            px = (1 - t) ** 2 * cx + 2 * (1 - t) * t * mx + t**2 * ex
            py = (1 - t) ** 2 * cy + 2 * (1 - t) * t * my + t**2 * ey
            for s in (-1, 1):
                a2 = a + s * 1.2 + (0.5 if math.sin(a) < 0 else 0)
                svg.line((px, py), (px + math.cos(a2) * 8, py + math.sin(a2) * 8 + 2), "#4f9a36" if s < 0 else "#6bb447", 1.7, 0.95)
    if nuts:
        for x, y in ((68, 40), (76, 41), (72, 46)):
            sphere(svg, x, y, 4.6, "#8a5a34", lo=0.4, hi=1.5)


def oasis_pool(svg: Svg):
    svg.ellipse(66, 102, 40, 12, "#d7c28a", op=0.9)
    svg.ellipse(66, 102, 34, 9.5, svg.lin([(0, "#7fd0e0"), (1, "#2a8aa8")], 0, 0, 0.4, 1))
    svg.path("M40,100 C50,97 56,102 66,99 C76,96 84,101 92,99", "none", "#ffffff", 1.1, 0.55)
    svg.ellipse(54, 99, 7, 1.6, "#ffffff", op=0.35)


def fruit(svg: Svg):
    """A hand of bananas under a broad leaf."""
    shadow(svg, rx=30, ry=7)
    leaf(svg, 40, 96, -128, 56, 15, "#4d9a36", curve=-4)
    leaf(svg, 92, 96, -52, 52, 14, "#5aa840", curve=4)
    tube(svg, "M64,16 C64,24 66,30 64,38", "#6a8a2c", 4.0)
    for i, ox in enumerate((-22, -13, -4, 5, 14, 23)):
        x0 = 64 + ox * 0.18
        d = f"M{x0:.1f},38 C{64 + ox * 0.9 - 9:.1f},52 {64 + ox * 1.1 - 10:.1f},78 {64 + ox * 0.7 - 4:.1f},96"
        w = 8.6
        stroke(svg, d, "#8a6a14", w + 1.8)
        stroke(svg, d, "#f2cf3a", w - 0.6)
        svg.path(d, "none", "#fff4a6", 2.2, 0.65, extra='transform="translate(-1.8,0)"')
        tip = (64 + ox * 0.7 - 4, 96)
        svg.circle(tip[0], tip[1], 2.0, "#4a3418")
    svg.ellipse(64, 38, 9, 5, "#6a8a2c")


# --- animals --------------------------------------------------------------------------------------------
def beast(svg: Svg, body: str, belly: str, *, kind: str, patch: str | None = None, seed=1, stripes=False):
    """A four-legged animal in side view, facing left. kind: cow, deer, camel, zebra."""
    rng = random.Random(seed)
    shadow(svg, cx=66, cy=104, rx=38, ry=7)
    far, near = shade(body, 0.62), body

    def leg(x, top, col, w=7.0, fold=0.0, hoof="#2a2018"):
        """A tapering leg: a full thigh under the body, a knee, a slim shank and a dark hoof."""
        foot = BASE[1] - 2
        knee_y = top + (foot - top) * 0.55
        kx = x + fold * 1.4
        fx = x + fold * 0.2
        t, k, f = w * 0.62, w * 0.34, w * 0.26  # half widths at the top, knee and fetlock
        d = (
            f"M{x - t:.1f},{top - 4:.1f} C{x - t:.1f},{knee_y - 6:.1f} {kx - k - 1:.1f},{knee_y - 3:.1f} {kx - k:.1f},{knee_y:.1f}"
            f" L{fx - f:.1f},{foot - 2:.1f} L{fx + f:.1f},{foot - 2:.1f} L{kx + k:.1f},{knee_y:.1f}"
            f" C{kx + k + 1:.1f},{knee_y - 3:.1f} {x + t:.1f},{knee_y - 8:.1f} {x + t:.1f},{top - 4:.1f} Z"
        )
        svg.path(d, svg.lin([(0, shade(col, 1.25)), (0.6, col), (1, shade(col, 0.5))], 0, 0, 1, 0))
        svg.ellipse(kx, knee_y, k + 0.6, 1.6, shade(col, 0.7), op=0.6)
        svg.path(f"M{fx - f - 1.2:.1f},{foot - 2:.1f} L{fx + f + 1.4:.1f},{foot - 2:.1f} L{fx + f + 2:.1f},{foot + 1:.1f} L{fx - f - 1.8:.1f},{foot + 1:.1f} Z", hoof)

    if kind == "cow":
        bx, by, bw, bh, legw, legtop = 68, 64, 31, 18, 8.0, 72
        neck = [(34, 56), (46, 52), (46, 70), (36, 72)]
        hx, hy = 28, 66
    elif kind == "deer":
        bx, by, bw, bh, legw, legtop = 70, 64, 26, 13, 4.6, 66
        neck = [(38, 38), (48, 46), (48, 64), (40, 58)]
        hx, hy = 34, 34
    elif kind == "zebra":
        bx, by, bw, bh, legw, legtop = 70, 64, 28, 14, 5.4, 66
        neck = [(38, 44), (49, 48), (49, 66), (42, 60)]
        hx, hy = 32, 42
    else:  # camel
        bx, by, bw, bh, legw, legtop = 70, 64, 28, 13, 5.4, 66
        neck = [(34, 36), (46, 30), (50, 64), (40, 62)]
        hx, hy = 28, 34
    # legs: far pair first
    for x, fold in ((bx - bw * 0.62, 2.0), (bx + bw * 0.55, -2.0)):
        leg(x + 4, legtop, far, legw, fold)
    # tail
    stroke(svg, f"M{bx + bw - 1},{by - 4} C{bx + bw + 8},{by} {bx + bw + 8},{by + 12} {bx + bw + 4},{by + 22}", shade(body, 0.5), 2.4)
    svg.ellipse(bx + bw + 4, by + 23, 2.6, 4.2, shade(body, 0.45))
    # neck and head behind the body
    svg.poly(neck, glow(svg, near, 0.6, 1.2))
    # body
    svg.ellipse(bx, by, bw, bh, svg.lin([(0, shade(body, 1.3)), (0.5, body), (1, shade(belly, 0.6))], 0, 0, 0.25, 1))
    if kind in ("camel",):
        for hxx in (bx - 8, bx + 12):
            svg.ellipse(hxx, by - bh + 1, 9, 10, glow(svg, body, 0.62, 1.3, 0.85))
    if patch:
        for _ in range(6):
            px = bx + rng.uniform(-bw * 0.75, bw * 0.7)
            py = by + rng.uniform(-bh * 0.6, bh * 0.4)
            svg.ellipse(px, py, rng.uniform(4, 8), rng.uniform(3, 6), patch, op=0.95, rot=rng.uniform(-40, 40))
    if stripes:
        for k in range(-5, 6):
            x = bx + k * 4.8
            svg.path(f"M{x},{by - bh + 1} Q{x + 3},{by} {x},{by + bh - 1}", "none", "#1d1a18", 2.2, 0.9)
        for k in range(0, 3):
            svg.line((46 + k * 3, 40 + k * 7), (51 + k * 3, 42 + k * 7), "#1d1a18", 2.0, 0.9)
    for x, fold in ((bx - bw * 0.62, -2.0), (bx + bw * 0.55, 2.0)):
        leg(x - 3, legtop, near, legw, fold)
    # head
    if kind == "cow":
        svg.ellipse(hx + 2, hy + 4, 11, 9, glow(svg, "#f0e6d4" if patch else body, 0.62, 1.25), rot=-20)
        svg.ellipse(hx - 4, hy + 8, 6.4, 5, "#e8b8a4", rot=-20)
        svg.circle(hx - 6, hy + 8, 1.2, "#3a2418")
        svg.circle(hx + 2, hy + 1, 1.7, "#1a1410")
        svg.path(f"M{hx + 8},{hy - 3} Q{hx + 14},{hy - 8} {hx + 10},{hy - 10}", "none", "#f5eed8", 2.4)
        svg.ellipse(hx + 10, hy + 1, 4.2, 2.2, shade(body, 0.8), rot=-30)
    elif kind in ("deer", "zebra"):
        svg.ellipse(hx + 2, hy + 3, 9.5, 5.4, glow(svg, body, 0.6, 1.25), rot=-24)
        svg.ellipse(hx - 4, hy + 6, 4.2, 3.2, shade(belly, 0.9), rot=-24)
        svg.circle(hx + 2, hy + 1, 1.5, "#1a1410")
        svg.ellipse(hx + 8, hy - 4, 2.4, 5, shade(body, 0.75), rot=30)
        if kind == "deer":
            for sx in (0, 6):
                d = f"M{hx + 6 + sx},{hy - 5} C{hx + 5 + sx},{hy - 16} {hx + 12 + sx},{hy - 20} {hx + 10 + sx},{hy - 30}"
                stroke(svg, d, "#4a3a2a", 2.4)
                for fy, fx in ((hy - 14, 5), (hy - 21, 4), (hy - 27, 3)):
                    stroke(svg, f"M{hx + 8 + sx},{fy} L{hx + 8 + sx - fx},{fy - 6}", "#4a3a2a", 1.8)
        else:
            stroke(svg, f"M{hx + 8},{hy - 4} L{hx + 22},{hy + 6}", "#1d1a18", 4)
    else:
        svg.ellipse(hx + 2, hy + 3, 9, 5.2, glow(svg, body, 0.6, 1.3), rot=-10)
        svg.circle(hx + 2, hy + 1, 1.5, "#1a1410")
    # camel heads sit high on a long neck; draw the mane tuft
    if kind == "camel":
        stroke(svg, f"M{hx + 8},{hy + 2} C{hx + 16},{hy + 6} {hx + 20},{hy + 20} {hx + 20},{hy + 30}", shade(body, 0.7), 2.0)
    tufts(svg, rng, cx=68, cy=BASE[1], half=36, n=5)


# --- water creatures ------------------------------------------------------------------------------------
def water_ring(svg: Svg, cx, cy, rx, ry, op=0.55):
    svg.ellipse(cx, cy, rx, ry, "#bfe6f2", op=op * 0.5)
    svg.ellipse(cx, cy, rx * 0.82, ry * 0.7, "none", stroke="#ffffff", sw=1.1, op=op)


def fish(svg: Svg):
    water_ring(svg, 64, 98, 38, 10)
    for cx, cy, s, rot in ((46, 72, 1.0, -22), (80, 58, 0.82, 18)):
        svg.begin(f'transform="rotate({rot} {cx} {cy}) translate({cx} {cy}) scale({s}) translate({-cx} {-cy})"')
        svg.path(f"M{cx + 18},{cy} L{cx + 36},{cy - 12} Q{cx + 31},{cy} {cx + 36},{cy + 12} Z", glow(svg, "#6f95b8", 0.55, 1.2, 0.9))
        svg.path(f"M{cx - 28},{cy} C{cx - 14},{cy - 20} {cx + 10},{cy - 18} {cx + 22},{cy} C{cx + 10},{cy + 17} {cx - 14},{cy + 18} {cx - 28},{cy} Z", svg.lin([(0, "#7fa4c6"), (0.45, "#c9dcea"), (0.55, "#e6eef4"), (1, "#f5f1e6")], 0, 0, 0, 1))
        svg.path(f"M{cx - 20},{cy - 2} C{cx - 8},{cy - 15} {cx + 8},{cy - 14} {cx + 20},{cy - 2} C{cx + 8},{cy - 8} {cx - 8},{cy - 8} {cx - 20},{cy - 2} Z", "#4a6f94", op=0.75)
        svg.path(f"M{cx - 4},{cy - 15} L{cx + 8},{cy - 24} L{cx + 12},{cy - 14} Z", "#4a6f94")
        for k in range(5):
            svg.path(f"M{cx - 2 + k * 5},{cy - 8} Q{cx + k * 5},{cy - 3} {cx - 2 + k * 5},{cy + 2}", "none", "#355878", 0.9, 0.5)
        svg.circle(cx - 20, cy - 3, 3.0, "#ffffff")
        svg.circle(cx - 20.5, cy - 3, 1.6, "#101820")
        svg.path(f"M{cx - 12},{cy - 9} C{cx - 2},{cy - 14} {cx + 8},{cy - 11} {cx + 14},{cy - 6}", "none", "#ffffff", 1.8, 0.55)
        svg.end()
    for x, y, r in ((30, 82, 2.4), (96, 72, 2.0), (58, 90, 1.8), (104, 86, 1.6), (22, 70, 1.6)):
        svg.circle(x, y, r, "#eaf8ff", op=0.8)


def whale(svg: Svg):
    water_ring(svg, 64, 98, 44, 11)
    # body rising out of the water: a dark back with a pale throat
    svg.path("M16,92 C18,70 40,56 66,58 C90,60 108,76 110,92 C100,86 90,84 80,88 C60,94 40,94 16,92 Z", svg.lin([(0, "#6c8aa6"), (0.5, "#3e5873"), (1, "#26384c")], 0, 0, 0.3, 1))
    svg.path("M20,90 C42,92 62,92 84,86 C92,84 100,84 108,88 C100,94 80,98 60,98 C42,98 28,96 20,90 Z", "#dfe6ea", op=0.9)
    for k in range(6):
        svg.path(f"M{30 + k * 7},91 Q{34 + k * 7},95 {38 + k * 7},93", "none", "#9aa8b0", 0.9, 0.7)
    svg.path("M38,66 C52,58 74,58 92,68", "none", "#a9c1d6", 2.2, 0.55)
    svg.circle(30, 84, 1.8, "#0e1620")
    # fluke
    svg.path("M104,84 C110,70 118,64 124,60 C122,70 116,76 112,82 C118,78 124,78 126,80 C118,86 110,88 104,88 Z", "#34495f")
    # a pectoral fin
    svg.path("M48,88 C54,96 62,100 70,100 C66,94 60,90 54,86 Z", "#4a6580")
    # spout
    for dx, dy, r in ((0, 0, 3), (-5, -9, 2.6), (5, -10, 2.6), (-10, -19, 2.2), (10, -21, 2.2), (0, -22, 2.4), (-14, -30, 1.9), (14, -32, 1.9), (0, -34, 2.0)):
        svg.circle(40 + dx, 56 + dy, r, "#ffffff", op=0.85)
    svg.path("M40,56 L40,34", "none", "#ffffff", 2.4, 0.6)


def pearls(svg: Svg):
    shadow(svg, rx=34, ry=7)
    rng = random.Random(3)
    # the lower shell, ribbed
    svg.path("M24,86 C24,98 40,106 64,106 C88,106 104,98 104,86 C92,92 36,92 24,86 Z", glow(svg, "#9c7f68", 0.5, 1.2, 1.0))
    # the upper shell, open behind it
    svg.path("M28,84 C22,60 40,40 64,40 C88,40 106,60 100,84 C84,90 44,90 28,84 Z", svg.lin([(0, "#c9b09a"), (1, "#7d6350")], 0, 0, 0.4, 1))
    for k in range(-6, 7):
        x = 64 + k * 5.6
        svg.path(f"M64,88 L{x:.1f},{46 + abs(k) * 2.2:.1f}", "none", "#5e473a", 1.0, 0.55)
        svg.path(f"M64,88 L{x - 1.4:.1f},{46 + abs(k) * 2.2:.1f}", "none", "#e6d2bd", 0.8, 0.45)
    # the nacre inside
    svg.path("M32,82 C28,66 44,52 64,52 C84,52 100,66 96,82 C82,88 46,88 32,82 Z", svg.rad([(0, "#fdf6ee"), (0.6, "#e9cfd2"), (1, "#b8a6c4")], 0.4, 0.35, 0.8))
    svg.path("M40,76 C48,66 76,62 92,72", "none", "#ffffff", 2.0, 0.5)
    sphere(svg, 60, 72, 11, "#f7f3ea", lo=0.7, hi=1.2)
    svg.ellipse(56, 66, 3.6, 2.2, "#ffffff", op=0.9, rot=-30)
    for x, y in ((84, 80), (40, 80)):
        sphere(svg, x, y, 3.2, "#f3eee4", lo=0.7, hi=1.2)
    del rng


# --- big things -----------------------------------------------------------------------------------------
def oil(svg: Svg):
    rng = random.Random(21)
    shadow(svg, rx=38, ry=8)
    ground_patch(svg, 64, 99, 40, 10, "#6b5536", 0.75)
    slick = smooth_path(lump(rng, 70, 99, 26, 6.5, 11, 0.28))
    svg.path(slick, svg.lin([(0, "#3a3640"), (0.5, "#141216"), (1, "#060506")], 0, 0, 0.4, 1))
    svg.path("M52,97 C60,94 70,96 80,94", "none", "#8a96b8", 1.4, 0.55)
    svg.path("M66,101 C74,99 82,101 90,99", "none", "#b07ac0", 0.9, 0.35)
    svg.ellipse(58, 96, 6, 1.4, "#c8d2ea", op=0.5)
    # the derrick's timber platform
    svg.poly([(40, 96), (88, 96), (84, 92), (44, 92)], svg.lin([(0, "#a07a4a"), (1, "#5e4226")], 0, 0, 0, 1))
    svg.line((44, 92.6), (84, 92.6), "#c79c62", 0.9, 0.7)
    # derrick: a lattice tower with a crown and a pipe down the middle
    top = (64, 18)
    for sx in (-1, 1):
        stroke(svg, f"M{64 + sx * 20},94 L{64 + sx * 3},{top[1] + 6}", "#3a3226", 4.0)
        stroke(svg, f"M{64 + sx * 20},94 L{64 + sx * 3},{top[1] + 6}", "#9a8a6a" if sx < 0 else "#6a5c44", 2.4)
    for y in (82, 68, 54, 40, 28):
        t = (94 - y) / (94 - top[1] - 6)
        w = 20 - 17 * t
        svg.line((64 - w, y), (64 + w, y), "#6a5c44", 1.8)
    for (y0, y1) in ((94, 82), (82, 68), (68, 54), (54, 40), (40, 28)):
        t0, t1 = (94 - y0) / 72, (94 - y1) / 72
        w0, w1 = 20 - 17 * t0, 20 - 17 * t1
        svg.line((64 - w0, y0), (64 + w1, y1), "#8a7a5a", 1.3, 0.9)
        svg.line((64 + w0, y0), (64 - w1, y1), "#8a7a5a", 1.3, 0.9)
    svg.poly([(60, 22), (68, 22), (66, top[1]), (62, top[1])], "#7a6a4c")
    svg.circle(64, top[1] - 2, 3.0, "#c9b88a")
    stroke(svg, "M64,26 L64,92", "#1a1a1e", 2.4, 0.8)
    tufts(svg, rng, cy=BASE[1] - 1, half=40, n=6)


def rubber(svg: Svg):
    shadow(svg, rx=30, ry=7)
    trunk = f"M60,{BASE[1]} C58,80 64,60 62,38"
    stroke(svg, trunk, "#3a2a1c", 13)
    stroke(svg, trunk, "#8a7a62", 11)
    stroke(svg, trunk, "#b8a98c", 2.6, 0.6)
    # tapping cuts, a spout and a cup
    for y in (64, 72):
        svg.path(f"M55,{y} L66,{y + 7}", "none", "#e8dcc0", 1.8, 0.9)
    svg.path("M64,76 L66,82", "none", "#e8e4dc", 2.4)
    svg.path("M60,82 L72,82 L70,94 L62,94 Z", svg.lin([(0, "#b8663a"), (1, "#6a361c")], 0, 0, 1, 0.4))
    svg.ellipse(66, 82, 6, 1.8, "#f1ede4")
    for cx, cy, rx, ry, col in ((44, 40, 20, 14, "#2f6f35"), (84, 38, 22, 14, "#3a7d3c"), (64, 22, 24, 14, "#438a42"), (52, 28, 14, 10, "#4b9549"), (76, 28, 14, 10, "#4b9549")):
        svg.ellipse(cx, cy, rx, ry, glow(svg, col, 0.5, 1.3, 0.9))
    for x, y, a in ((40, 38, 190), (88, 36, -10), (60, 20, -90), (44, 48, 160), (82, 48, 20), (70, 36, -60), (50, 30, -120)):
        leaf(svg, x, y, a, 18, 5, "#235a28", curve=2)


def tusk(svg: Svg, x0, y0, x1, y1, bulge, thick, base="#efe3c4"):
    """A tapering curved tusk from its root (x0, y0) to its point (x1, y1)."""
    mx, my = (x0 + x1) / 2 + bulge, (y0 + y1) / 2
    nx, ny = (y1 - y0), -(x1 - x0)
    n = math.hypot(nx, ny) or 1.0
    nx, ny = nx / n * thick, ny / n * thick
    d = (
        f"M{x0 + nx:.1f},{y0 + ny:.1f} Q{mx + nx * 0.8:.1f},{my + ny * 0.8:.1f} {x1:.1f},{y1:.1f} "
        f"Q{mx - nx * 0.8:.1f},{my - ny * 0.8:.1f} {x0 - nx:.1f},{y0 - ny:.1f} Z"
    )
    svg.path(d, svg.lin([(0, shade(base, 1.2)), (0.45, base), (1, shade(base, 0.62))], 0, 0, 0.4, 1))
    svg.path(f"M{x0 + nx * 0.2:.1f},{y0 + ny * 0.2:.1f} Q{mx + nx * 0.3:.1f},{my + ny * 0.3:.1f} {x1:.1f},{y1:.1f}", "none", "#fffaee", 1.4, 0.6)
    # the hollow root end, stained
    svg.ellipse(x0, y0, thick * 1.0, thick * 0.78, "#a58a5e")
    svg.ellipse(x0 - 0.5, y0 - 0.5, thick * 0.55, thick * 0.4, "#6a5236")


def ivory(svg: Svg):
    shadow(svg, rx=38, ry=7)
    # two tusks laid crossed on the ground, points to the sky
    tusk(svg, 34, 96, 98, 34, 18, 7.5)
    tusk(svg, 94, 98, 30, 36, -16, 8.5, "#e8dab6")
    for x, y in ((50, 70), (76, 62)):
        svg.line((x, y), (x + 4, y + 2), "#9a8a6a", 0.8, 0.5)


def flag(svg: Svg, col: str, charge: str | None = None):
    """A flag on a pole planted in a small mound, for the region markers of a colonial map."""
    shadow(svg, cx=60, rx=26, ry=6)
    svg.ellipse(60, 100, 18, 6, glow(svg, "#7a6a4c", 0.5, 1.2), op=1)
    stroke(svg, "M60,102 L60,20", "#3a2a1a", 4.6)
    stroke(svg, "M60,102 L60,20", "#a8794a", 3.0)
    svg.circle(60, 18, 3.4, "#e0c066")
    cloth = svg.lin([(0, shade(col, 1.35)), (0.5, col), (1, shade(col, 0.6))], 0, 0, 1, 0.2)
    svg.path("M62,24 C74,20 84,30 98,24 L98,56 C84,62 74,52 62,58 Z", cloth)
    svg.path("M62,24 C74,20 84,30 98,24", "none", shade(col, 1.6), 1.2, 0.6)
    svg.path("M62,41 C74,37 84,47 98,41", "none", "#ffffff", 4.0, 0.22)
    if charge == "cross":
        svg.path("M76,26 L80,26 L80,58 L76,58 Z", "#ffffff", op=0.75)


def cottage(svg: Svg, x, y, w, d, h, roof: str, wall: str, rise=None, chimney=False, lit_window=True):
    """A gabled cottage in the map's 2:1 view.  (x, y) is its back corner on the ground; `w` runs
    down-right along the ridge, `d` down-left across it.  Only the faces toward the viewer are drawn:
    the long front wall (lit, it faces the lower left), the gable end (in shade) and the front roof."""
    r = rise if rise is not None else d * 0.62

    def P(a, b, z=0.0):
        return (x + a - b, y + (a + b) * 0.5 - z)

    o = 1.6  # eaves overhang
    # contact shadow, cast toward the lower right
    foot = [P(0, 0), P(w, 0), P(w, d), P(0, d)]
    svg.poly([(px + 3, py + 1.5) for px, py in foot], "#0d0a06", op=0.35, extra=f'filter="{svg.blur(1.6)}"')
    # gable end, facing the lower right: in shade
    svg.poly([P(w, 0), P(w, d), P(w, d, h), P(w, d / 2, h + r), P(w, 0, h)], svg.lin([(0, shade(wall, 0.78)), (1, shade(wall, 0.55))], 0, 0, 0, 1))
    # the long wall, facing the lower left: in the light
    svg.poly([P(0, d), P(w, d), P(w, d, h), P(0, d, h)], svg.lin([(0, shade(wall, 1.12)), (1, shade(wall, 0.86))], 0, 0, 1, 1))
    # a timber sill and the corner post
    svg.line(P(0, d, 0.6), P(w, d, 0.6), shade(wall, 0.5), 1.0, 0.7)
    svg.line(P(w, d), P(w, d, h), shade(wall, 0.45), 0.9, 0.6)
    # door and windows on the long wall
    def quad(a0, a1, z0, z1, fill, op=1.0):
        svg.poly([P(a0, d, z0), P(a1, d, z0), P(a1, d, z1), P(a0, d, z1)], fill, op=op)
    quad(w * 0.42, w * 0.42 + 3.2, 0, h * 0.78, "#3e2a1a")
    for a0 in (w * 0.14, w * 0.70):
        if a0 + 3 < w:
            quad(a0, a0 + 3.0, h * 0.40, h * 0.74, "#f2c862" if lit_window else "#2a2622")
    # gable-end window
    svg.poly([P(w, d * 0.40, h * 0.45), P(w, d * 0.60, h * 0.45), P(w, d * 0.60, h * 0.78), P(w, d * 0.40, h * 0.78)], "#2a2420")
    # the front roof slope, from the eaves up to the ridge, overhanging the walls
    eave0, eave1 = P(-o, d + o, h - 0.8), P(w + o, d + o, h - 0.8)
    ridge0, ridge1 = P(-o, d / 2, h + r), P(w + o, d / 2, h + r)
    svg.poly([eave0, eave1, ridge1, ridge0], svg.lin([(0, shade(roof, 1.32)), (1, shade(roof, 0.72))], 0, 0, 0.3, 1))
    # thatch or tile courses across the slope, and a dark lip under the eaves
    for k in range(1, 4):
        t = k / 4
        a, b = lerp(eave0, ridge0, t), lerp(eave1, ridge1, t)
        svg.line(a, b, shade(roof, 0.7), 0.8, 0.55)
    svg.line(eave0, eave1, shade(roof, 0.45), 1.3, 0.8)
    svg.line(ridge0, ridge1, shade(roof, 1.45), 1.1, 0.8)
    # the gable's roof edge
    svg.line(P(w + o, d + o, h - 0.8), ridge1, shade(roof, 0.6), 1.2, 0.8)
    if chimney:
        c0 = P(w * 0.72, d * 0.5, h + r * 0.55)
        svg.rect(c0[0] - 2.2, c0[1] - 9, 4.4, 9, svg.lin([(0, "#a8705a"), (1, "#6a4234")], 0, 0, 1, 0))
        svg.rect(c0[0] - 2.8, c0[1] - 10, 5.6, 1.8, "#5a382c")
        for k, (dx, dy, rr) in enumerate(((1, -14, 3.0), (4, -20, 3.8), (8, -26, 4.4))):
            svg.circle(c0[0] + dx, c0[1] + dy, rr, "#d8d4cc", op=0.45 - 0.1 * k, blur=svg.blur(1.2))


def haystack(svg: Svg, x, y, r=7.0):
    svg.ellipse(x + 2, y + 1, r * 1.1, r * 0.35, "#0d0a06", op=0.35, blur=svg.blur(1.2))
    svg.path(f"M{x - r},{y} C{x - r},{y - r * 1.2} {x - r * 0.3},{y - r * 1.7} {x},{y - r * 1.75} C{x + r * 0.3},{y - r * 1.7} {x + r},{y - r * 1.2} {x + r},{y} Z",
             svg.rad([(0, "#f0d27a"), (0.55, "#c99a44"), (1, "#7a5a26")], 0.32, 0.25, 0.85))
    for k in range(3):
        yy = y - r * (0.35 + 0.4 * k)
        svg.path(f"M{x - r * (0.95 - 0.25 * k):.1f},{yy:.1f} Q{x:.1f},{yy + 2:.1f} {x + r * (0.95 - 0.25 * k):.1f},{yy:.1f}", "none", "#8a6a2e", 0.7, 0.6)


def hamlet(svg: Svg):
    """Bonus terrain: a little hamlet of cottages among their gardens, a haystack and a tree."""
    rng = random.Random(31)
    shadow(svg, rx=46, ry=10, op=0.32)
    # trodden earth and garden plots round the houses, with a path running off to the lower left
    ground_patch(svg, 66, 88, 50, 22, "#6e5a38", 0.45)
    ground_patch(svg, 68, 82, 34, 13, "#7a6440", 0.35)
    svg.path("M60,100 C50,104 40,106 26,112", "none", "#9a8256", 4.0, 0.7)
    for i in range(4):  # a vegetable plot in green and brown rows, behind the fence
        a, b = (78 + i * 3.0, 92 + i * 1.5), (92 + i * 3.0, 85 + i * 1.5)
        svg.line(a, b, "#5d7a2c" if i % 2 else "#6a5232", 2.2, 0.9)
    # a tree behind the hamlet
    tube(svg, "M30,92 C31,84 29,78 31,72", "#5a3e24", 3.2)
    for _ in range(26):  # the crown in leafy dabs, darker underneath and to the lower right
        a_, d_ = rng.uniform(0, math.tau), rng.random() ** 0.7
        dx, dy = math.cos(a_) * d_ * 11, math.sin(a_) * d_ * 9
        tone = 1.0 - 0.035 * (dx + dy) + rng.uniform(-0.1, 0.1)
        svg.ellipse(31 + dx, 64 + dy, rng.uniform(3.5, 5.5), rng.uniform(3, 4.5), shade("#3a6020", tone), rot=rng.uniform(-30, 30))
    # the houses, back to front
    cottage(svg, 66, 50, 24, 14, 10, "#c9a456", "#e6dcc4", chimney=True)
    cottage(svg, 92, 66, 20, 12, 9, "#b0563c", "#d8ccb0", lit_window=False)
    cottage(svg, 46, 72, 20, 13, 9, "#bf9a50", "#e2d6bc")
    haystack(svg, 98, 96, 6.5)
    # a wattle fence along the front of the garden
    for i in range(6):
        px, py = 70 + i * 5, 104 + i * -2.5
        svg.line((px, py), (px, py - 6), "#6a4a2a", 1.3)
    svg.line((70, 101), (95, 88.5), "#8a6a40", 1.1)
    svg.line((70, 103.5), (95, 91), "#8a6a40", 1.1)
    tufts(svg, rng, cx=60, cy=BASE[1] + 2, half=42, n=6)


# --- the table ------------------------------------------------------------------------------------------
# id -> (class, picture). The class is what the game says in its tile panel; the picture does not show it.
RESOURCES = {
    "iron": ("strategic", lambda s: ore(s, "#7c7468", "#a85a34", seed=11, streak="#b0572e")),
    "coal": ("strategic", lambda s: ore(s, "#2a2a30", None, seed=12, heap=True)),
    "oil": ("strategic", lambda s: oil(s)),
    "rubber": ("luxury", lambda s: rubber(s)),
    "wines": ("luxury", lambda s: vine(s)),
    "diamonds": ("luxury", lambda s: ore(s, "#7d7870", None, seed=13, gems="#9fe6f6")),
    "coffee": ("luxury", lambda s: coffee(s)),
    "tea": ("luxury", lambda s: tea(s)),
    "tobacco": ("luxury", lambda s: tobacco(s)),
    "big-game": ("luxury", lambda s: beast(s, "#c8c0b0", "#f0ece0", kind="zebra", stripes=True)),
    "gold": ("luxury", lambda s: ore(s, "#8a7a5c", "#f0c23a", glint=True, seed=14)),
    "sugar": ("luxury", lambda s: sugar(s)),
    "new-zealand": ("strategic", lambda s: flag(s, "#1f4f9a")),
    "oasis": ("luxury", lambda s: palm(s, water=True)),
    "silver": ("luxury", lambda s: ore(s, "#7a7a80", "#d6dce2", glint=True, seed=15)),
    "timber": ("strategic", lambda s: timber(s)),
    "tin": ("luxury", lambda s: ore(s, "#6f7378", "#a9b4bc", seed=16)),
    "swede-norge": ("strategic", lambda s: flag(s, "#2a6fbf")),
    "cotton": ("luxury", lambda s: cotton(s)),
    "copper": ("luxury", lambda s: ore(s, "#7a5a44", "#d3763a", seed=17, streak="#4fa88a")),
    "ivory": ("luxury", lambda s: ivory(s)),
    "british-egypt": ("strategic", lambda s: flag(s, "#c9a23a")),
    "west-africa": ("strategic", lambda s: flag(s, "#3f9a4a")),
    "sw-africa": ("strategic", lambda s: flag(s, "#b8793a")),
    "south-africa": ("strategic", lambda s: flag(s, "#d9692f")),
    "east-africa": ("strategic", lambda s: flag(s, "#9b2335")),
    "british-india": ("strategic", lambda s: flag(s, "#d04a72")),
    "australia": ("strategic", lambda s: flag(s, "#243f8f")),
    "canada": ("strategic", lambda s: flag(s, "#c9302c")),
    "denmark": ("strategic", lambda s: flag(s, "#b83a3a", "cross")),
    "livestock": ("bonus", lambda s: beast(s, "#8a5a38", "#d8c8b0", kind="cow", patch="#f1ead8", seed=4)),
    "wheat": ("bonus", lambda s: wheat(s)),
    "fish": ("bonus", lambda s: fish(s)),
    "corn": ("bonus", lambda s: corn(s)),
    "rice": ("bonus", lambda s: rice(s)),
    "whales": ("bonus", lambda s: whale(s)),
    "tropical-fruit": ("bonus", lambda s: fruit(s)),
    "game": ("bonus", lambda s: beast(s, "#9a6a40", "#e8d8bc", kind="deer", seed=6)),
    "cocoa": ("bonus", lambda s: cocoa(s)),
    "pearls": ("bonus", lambda s: pearls(s)),
    "camels": ("bonus", lambda s: beast(s, "#c4985a", "#e8d4a8", kind="camel")),
    "bonus-terrain": ("bonus", lambda s: hamlet(s)),
    "coconuts": ("bonus", lambda s: coconuts(s)),
}


def build(name: str) -> Svg:
    _, picture = RESOURCES[name]
    svg = Svg(SIZE, SIZE)
    # drawn a little small on its canvas so nothing clips; enlarge about the contact point
    k = SCALE
    svg.begin(f'transform="translate({BASE[0]} {BASE[1]}) scale({k}) translate({-BASE[0]} {-BASE[1]})"')
    picture(svg)
    svg.end()
    return svg
