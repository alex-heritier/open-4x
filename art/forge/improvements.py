"""Tile improvements drawn over the terrain: the farm (a whole-tile overlay) and the mine.

Both are authored on a 256 x 128 canvas, shown at 128 x 64: the same 2:1 diamond as one terrain
cell, so they sit exactly on a tile. Lit from the upper left like everything else.

The farm comes in four styles. `build_farm` is the mixed-crop field every culture without a
crop of its own gets; `build_rice`, `build_corn` and `build_wheat` are the paddies, maize and
wheat of the nations the pack names them for (`visuals.farms`). All of them fill the same
diamond, so any can stand on any tile, and all keep their tall plants inside it: the shore cut
clips a piece to its cell, and a stalk poking out of the diamond would be sliced off there.
"""

import math
import random

from .svgkit import Iso, Svg, lerp, mixc, shade

W, H = 256, 128
INK = "#17130f"
N = 6  # the field is N x N iso units
K = 112 / N  # it fills 90% of the diamond so a margin of terrain shows round it


def _iso() -> Iso:
    iso = Iso(128, 0, K)
    iso.oy = 64 - N * K / 2
    return iso


def _corners(iso: Iso):
    return [iso.p(0, 0), iso.p(N, 0), iso.p(N, N), iso.p(0, N)]


def _hedge(svg: Svg, corners, sides=(0, 1, 2, 3)):
    """A dark line round the field with leafy blobs along it. `sides` picks which of its four
    edges to draw, in order back right, front right, front left, back left, so a field can put
    its far hedge behind the crop and its near one in front."""
    ring = corners + [corners[0]]
    for p, q in (pair for i, pair in enumerate(zip(ring, ring[1:])) if i in sides):
        svg.line(p, q, "#243a1c", 5.0)
        for t in range(9):
            c = lerp(p, q, (t + 0.5) / 9)
            svg.circle(c[0], c[1] - 1.5, 4.2, "#35592a", stroke=None)
            svg.circle(c[0] - 1, c[1] - 2.6, 2.4, "#5a8a3c", op=0.9)


def _paint_in_depth(items):
    """Run the drawing calls of `items` [(x + y, call)] from the back of the field to the front."""
    for _, draw in sorted(items, key=lambda item: item[0]):
        draw()


def build_farm() -> Svg:
    """Ploughed strips in three crops inside a hedge, with a pair of haystacks."""
    svg = Svg(W, H)
    n = N
    iso = _iso()
    k = K
    corners = _corners(iso)
    soil = "#6e4f2f"
    svg.poly(corners, soil)
    crops = ["#7fae45", "#d2b552", "#5e9a3c", "#e0c867", "#8bb84a", "#a77f45"]
    for i in range(n):
        a, b = i, i + 1
        strip = [iso.p(a, 0), iso.p(b, 0), iso.p(b, n), iso.p(a, n)]
        svg.poly(strip, crops[i], op=0.96)
        # furrows run the length of each strip
        for f in range(1, 5):
            t = a + f / 5
            svg.line(iso.p(t, 0.1), iso.p(t, n - 0.1), shade(crops[i], 0.74), 1.4, 0.55)
        svg.line(iso.p(a, 0), iso.p(a, n), shade(soil, 0.7), 1.6, 0.8)
        # light catches the upper-left edge of each strip
        svg.line(iso.p(a + 0.04, 0.1), iso.p(a + 0.04, n - 0.1), shade(crops[i], 1.35), 1.0, 0.6)
    _hedge(svg, corners)
    # a pair of haystacks in the lower corner
    for gx, gy in ((n - 1.3, n - 1.4), (n - 0.7, n - 0.8)):
        x, y = iso.p(gx, gy)
        svg.ellipse(x + 2, y + 2, 11, 4.4, "#1a1308", op=0.4)
        svg.poly([(x - 10, y), (x + 10, y), (x + 5, y - 14), (x - 5, y - 14)], svg.lin([(0, "#e4c870"), (1, "#a9863a")], 0, 0, 1, 0.4), INK, 0.7)
        svg.ellipse(x, y - 14, 5, 2.4, "#efd98c", INK, 0.6)
    return svg


# The rice country: (x0, y0, x1, y1, ground colour, rows run along "x" or "y"). Big flat fields
# cut by thin dikes, in the yellow-greens of a crop ripening at different rates.
RICE_FIELDS = [
    (0.0, 0.0, 3.3, 2.4, "#c3cc4e", "y"),
    (0.0, 2.4, 1.6, 6.0, "#d9c94f", "x"),
    (1.6, 2.4, 3.3, 4.5, "#a9c644", "y"),
    (1.6, 4.5, 3.3, 6.0, "#cdd058", "x"),
    (3.3, 0.0, 4.7, 3.6, "#8fba3e", "x"),
    (4.7, 0.0, 6.0, 3.6, "#e0cf62", "y"),
    (3.3, 3.6, 5.0, 6.0, "#c8cc50", "y"),
]
RICE_YARD = (5.0, 3.6, 6.0, 6.0)  # a farmyard in the last corner: house, pond and trees
RICE_DIKE = "#a99a62"


def build_rice() -> Svg:
    """Flat rice fields from the air: wide rectangles in greens and golds with fine row streaks,
    thin dikes between them, a line of trees along two of the dikes, and a red-roofed farmhouse
    beside a pond."""
    svg = Svg(W, H)
    iso = _iso()
    rng = random.Random(11)
    svg.poly(_corners(iso), RICE_DIKE)
    m = 0.1  # half the width of a dike
    for x0, y0, x1, y1, ground, along in RICE_FIELDS:
        x0, y0, x1, y1 = x0 + m, y0 + m, x1 - m, y1 - m
        quad = [iso.p(x0, y0), iso.p(x1, y0), iso.p(x1, y1), iso.p(x0, y1)]
        svg.poly(quad, svg.lin([(0, shade(ground, 1.12)), (1, shade(ground, 0.9))], 0, 0, 1, 1), shade(ground, 0.62), 0.9)
        rows = int(((y1 - y0) if along == "x" else (x1 - x0)) / 0.2)
        for r in range(rows):  # fine streaks, one per row of plants
            t = (r + 0.5) / rows
            tone = shade(ground, 0.84 if r % 2 else 1.14)
            if along == "x":
                y = y0 + (y1 - y0) * t
                svg.line(iso.p(x0 + 0.08, y), iso.p(x1 - 0.08, y), tone, 1.0, 0.55)
            else:
                x = x0 + (x1 - x0) * t
                svg.line(iso.p(x, y0 + 0.08), iso.p(x, y1 - 0.08), tone, 1.0, 0.55)
        for _ in range(int((x1 - x0) * (y1 - y0) * 3)):  # a few darker patches
            px, py = iso.p(rng.uniform(x0 + 0.1, x1 - 0.1), rng.uniform(y0 + 0.1, y1 - 0.1))
            svg.ellipse(px, py, rng.uniform(3, 6), rng.uniform(1.2, 2.2), shade(ground, 0.8), op=0.35)
        # the near and far dikes catch the light differently
        svg.line(quad[0], quad[1], shade(RICE_DIKE, 1.2), 1.1, 0.8)
        svg.line(quad[0], quad[3], shade(RICE_DIKE, 1.2), 1.1, 0.8)
    # the farmyard: grass, a pond, and the house
    x0, y0, x1, y1 = RICE_YARD
    svg.poly([iso.p(x0 + m, y0 + m), iso.p(x1 - m, y0 + m), iso.p(x1 - m, y1 - m), iso.p(x0 + m, y1 - m)], "#86b048", shade(RICE_DIKE, 0.7), 0.9)
    pond = iso.p(5.5, 5.35)
    svg.ellipse(pond[0], pond[1], 11, 5.5, "#4f8a8a", "#30605f", 1.0)
    svg.ellipse(pond[0] - 2, pond[1] - 1, 7, 3, "#a5d0cc", op=0.75)
    standing = []
    for i in range(9):  # trees along the dike at x = 3.3 and the one at y = 2.4
        y = 1.0 + 0.45 * i + rng.uniform(-0.08, 0.08)
        standing.append((3.3 + y, lambda y=y: _tree(svg, iso, rng, 3.3, y)))
    for i in range(5):
        x = 1.0 + 0.45 * i + rng.uniform(-0.08, 0.08)
        standing.append((x + 2.4, lambda x=x: _tree(svg, iso, rng, x, 2.4)))
    for x, y in ((5.2, 3.9), (5.8, 3.95)):  # behind the house
        standing.append((x + y, lambda x=x, y=y: _tree(svg, iso, rng, x, y)))
    standing.append((5.45 + 4.35, lambda: _farmhouse(svg, iso, 5.45, 4.35)))
    _paint_in_depth(standing)
    return svg


def _tree(svg, iso, rng, x, y):
    """A round tree seen from above and the side: a dark crown with a lit upper left."""
    px, py = iso.p(x, y)
    r = rng.uniform(3.4, 4.6)
    svg.ellipse(px + 2, py + 1, r + 1, r * 0.45, "#1a2410", op=0.4)
    svg.line((px, py), (px, py - r), "#4a3622", 1.4)
    svg.circle(px, py - r - 2, r, "#2d5726", stroke="#1c3a18", sw=0.6)
    svg.circle(px - r * 0.3, py - r - 2 - r * 0.3, r * 0.62, "#4f8a38", op=0.95)
    svg.circle(px - r * 0.45, py - r - 2 - r * 0.45, r * 0.25, "#8fc058", op=0.8)


def _farmhouse(svg, iso, x, y):
    """A small whitewashed house under a red tiled roof."""
    w, d, wall, ridge = 0.5, 0.36, 4.5, 6.0
    svg.ellipse(*iso.p(x + w / 2, y + d / 2), 14, 6, "#1a2410", op=0.35)
    a, b, c, e = iso.p(x, y + d, 0), iso.p(x + w, y + d, 0), iso.p(x + w, y, 0), iso.p(x, y, 0)
    up = lambda p, h: (p[0], p[1] - h)
    svg.poly([a, b, up(b, wall), up(a, wall)], "#f1e8d2", INK, 0.6)  # the lit front, facing lower left
    svg.poly([b, c, up(c, wall), up(b, wall)], "#b9ad94", INK, 0.6)  # the shaded end
    r0, r1 = lerp(up(a, wall), up(e, wall), 0.5), lerp(up(b, wall), up(c, wall), 0.5)
    svg.poly([up(a, wall), up(b, wall), up(r1, ridge), up(r0, ridge)], "#d2523a", INK, 0.6)
    svg.poly([up(b, wall), up(c, wall), up(r1, ridge)], "#a63a28", INK, 0.6)
    svg.poly([up(a, wall), up(e, wall), up(r0, ridge)], "#c04a34", INK, 0.6)
    svg.poly([up(e, wall), up(c, wall), up(r1, ridge), up(r0, ridge)], "#e0644a", INK, 0.6)


def build_corn() -> Svg:
    """Rows of maize standing tall in dark soil behind a split-rail fence."""
    svg = Svg(W, H)
    iso = _iso()
    rng = random.Random(21)
    corners = _corners(iso)
    svg.poly(corners, "#68482a")
    for r in range(7):  # furrows between the rows, running to the right
        y = 0.7 + 0.8 * r
        svg.line(iso.p(0.2, y), iso.p(N - 0.2, y), "#3f2a15", 2.0, 0.55)
        svg.line(iso.p(0.2, y + 0.12), iso.p(N - 0.2, y + 0.12), "#8c6a41", 1.0, 0.45)
    top, left, bottom, right = corners[0], corners[3], corners[2], corners[1]
    _fence(svg, top, right)  # the two edges behind the crop
    _fence(svg, top, left)
    stalks = []
    for r in range(6):
        y = 1.1 + 0.8 * r
        for c in range(10):
            x = 1.0 + 0.5 * c + 0.2 * (r % 2) * 0.5
            ripe = rng.random() < 0.3
            stalks.append((x + y, lambda x=x, y=y, ripe=ripe, h=rng.uniform(14.0, 17.0): _stalk(svg, iso, rng, x, y, h, ripe)))
    _paint_in_depth(stalks)
    _fence(svg, left, bottom)  # the near edges stand in front of it
    _fence(svg, right, bottom)
    return svg


def _stalk(svg, iso, rng, x, y, h, ripe):
    """One maize plant: a stem, drooping blade-like leaves, a tassel on top and a cob in a husk."""
    px, py = iso.p(x, y)
    svg.ellipse(px + 1.5, py + 1.0, 4.5, 1.7, "#1e1208", op=0.4)
    svg.line((px, py), (px, py - h), "#4a7a2a", 2.0)
    leaf = "#5e9a34" if not ripe else "#8aa83a"
    for t, side in ((0.30, -1), (0.45, 1), (0.62, -1), (0.78, 1)):
        ly = py - h * t
        reach = 8.0 + 3.0 * (1 - t)
        d = f"M{px:.1f},{ly:.1f} Q{px + side * reach * 0.7:.1f},{ly - 5.0:.1f} {px + side * reach:.1f},{ly + 3.0:.1f}"
        svg.path(d, stroke=shade(leaf, 0.7), sw=2.6)
        svg.path(d, stroke=shade(leaf, 1.12), sw=1.3)
    side = 1 if rng.random() < 0.5 else -1
    cx, cy = px + side * 2.8, py - h * 0.52
    husk, silk = ("#a9c453", "#c9a24a") if not ripe else ("#e2b93e", "#a8742a")
    svg.ellipse(cx, cy, 2.3, 4.6, husk, INK, 0.5, rot=side * 14)
    svg.ellipse(cx + side * 0.6, cy - 3.9, 1.0, 2.0, silk, rot=side * 14)
    for d in (-2, 0, 2):  # the tassel
        svg.line((px, py - h), (px + d * 1.3, py - h - 4.5), "#e3d27a" if not ripe else "#c9a24a", 0.9)


def _fence(svg, a, b, posts=6):
    """A split-rail fence between two points: posts with two rails."""
    wood, dark = "#a47a48", "#3c2a16"
    pts = [lerp(a, b, i / (posts - 1)) for i in range(posts)]
    for rail, height in ((1, 3.2), (2, 6.4)):
        for p, q in zip(pts, pts[1:]):
            svg.line((p[0], p[1] - height), (q[0], q[1] - height), dark, 2.6)
            svg.line((p[0], p[1] - height), (q[0], q[1] - height), wood, 1.5)
    for p in pts:
        svg.line((p[0], p[1] + 0.5), (p[0], p[1] - 8.5), dark, 3.2)
        svg.line((p[0] - 0.4, p[1]), (p[0] - 0.4, p[1] - 8.3), shade(wood, 1.1), 1.6)


def build_wheat() -> Svg:
    """Strips of ripe wheat swaying in the wind, a green one still coming and a cut one with its
    sheaves stooked, inside a hedge."""
    svg = Svg(W, H)
    iso = _iso()
    rng = random.Random(31)
    corners = _corners(iso)
    soil = "#7a5a34"
    svg.poly(corners, soil)
    # per strip: ground colour, ear colour, and whether it still stands
    strips = [
        ("#c9a647", "#e8c95c", True),
        ("#d6b455", "#f0d36a", True),
        ("#a9b04a", "#b9c352", True),  # not ripe yet
        ("#cfae4e", "#ebcb5e", True),
        ("#d2b052", "#efd068", True),
        ("#b99a58", "#d8bd7c", False),  # cut: stubble and stooks, in front where they show
    ]
    for i, (ground, _, _) in enumerate(strips):
        svg.poly([iso.p(i, 0), iso.p(i + 1, 0), iso.p(i + 1, N), iso.p(i, N)], ground, op=0.97)
        svg.line(iso.p(i, 0), iso.p(i, N), shade(soil, 0.7), 1.4, 0.7)
    _hedge(svg, corners, (0, 3))
    items = []
    for i, (ground, ear, standing) in enumerate(strips):
        for f in range(5):  # five drills of grain in a strip
            x = i + 0.1 + 0.2 * f + 0.1
            for c in range(17):
                y = 0.55 + c * 0.29 + rng.uniform(-0.04, 0.04)
                if y > N - 0.35:
                    continue
                if standing and min(x, y) < 0.75:
                    continue  # leave the back edges bare: an ear that tall would poke out of the diamond
                if standing:
                    sway = math.sin((i * 5 + f) * 0.9 + c * 0.5) * 1.6
                    items.append((x + y, lambda x=x, y=y, ear=ear, sway=sway: _wheat_ear(svg, iso, x, y, ear, sway)))
                else:
                    px, py = iso.p(x, y)
                    items.append((x + y, lambda px=px, py=py: svg.line((px, py), (px + 0.8, py - 2.6), "#a58a47", 1.0, 0.8)))
    for sx, sy in ((5.45, 1.5), (5.45, 2.9), (5.45, 4.3), (5.15, 2.2)):  # the sheaves
        items.append((sx + sy + 0.2, lambda sx=sx, sy=sy: _stook(svg, iso, sx, sy)))
    _paint_in_depth(items)
    _hedge(svg, corners, (1, 2))
    return svg


def _wheat_ear(svg, iso, x, y, ear, sway):
    """A stalk of wheat: a thin stem, a plump ear and the awns on top, leaning with the wind."""
    px, py = iso.p(x, y)
    top = (px + sway, py - 8.5)
    stem = shade(ear, 0.78)
    svg.path(f"M{px:.1f},{py:.1f} Q{px + sway * 0.3:.1f},{py - 5.0:.1f} {top[0]:.1f},{top[1]:.1f}", stroke=stem, sw=1.0)
    svg.ellipse(top[0] + sway * 0.1, top[1] - 2.2, 1.5, 3.4, ear, shade(ear, 0.6), 0.4, rot=sway * 4)
    svg.ellipse(top[0] + sway * 0.1 - 0.4, top[1] - 2.6, 0.5, 2.4, shade(ear, 1.25), op=0.8, rot=sway * 4)
    for d in (-1.0, 1.0):  # awns
        svg.line((top[0], top[1] - 4.0), (top[0] + d * 1.0 + sway * 0.4, top[1] - 7.5), shade(ear, 1.1), 0.6, 0.9)


def _stook(svg, iso, x, y):
    """A stook: sheaves of cut wheat stood against each other and tied."""
    px, py = iso.p(x, y)
    svg.ellipse(px + 2, py + 2, 8, 3.2, "#1a1308", op=0.4)
    body = svg.lin([(0, "#f0d878"), (1, "#a9863a")], 0, 0, 1, 0.4)
    svg.poly([(px - 7.5, py), (px + 7.5, py), (px + 3.4, py - 13), (px, py - 16), (px - 3.4, py - 13)], body, INK, 0.7)
    for d in (-4.5, -1.5, 1.5, 4.5):  # straw, running up the sheaves
        svg.line((px + d, py - 1), (px + d * 0.45, py - 13), "#8c6a2c", 0.8, 0.7)
    svg.line((px - 5.2, py - 6), (px + 5.2, py - 6), "#7a5a2a", 1.8)  # the band
    for d in (-2.6, 0, 2.6):  # ears at the top
        svg.ellipse(px + d, py - 15.5 - (d == 0) * 1.5, 1.5, 3.0, "#f2db7c", INK, 0.4, rot=d * 6)


def build_mine() -> Svg:
    """A timbered adit dug into a grassy knoll: a lamp glowing down the tunnel, rails running out to
    an ore cart, a spoil heap beside it.  Painted in soft light and shade with no ink outlines, so it
    sits in the terrain like the relief does."""
    svg = Svg(W, H)
    rng = random.Random(7)
    lit = lambda col, hi=1.3, lo=0.55, x2=0.9, y2=1.0: svg.lin([(0, shade(col, hi)), (1, shade(col, lo))], 0, 0, x2, y2)  # noqa: E731

    # the cut bank the adit is driven into: earth and stone with soft edges, so it melts into
    # whatever ground or relief the tile already shows
    svg.ellipse(136, 98, 58, 11, "#0e0b07", op=0.30, blur=svg.blur(4.0))
    bank = "M78,100 C80,84 90,66 104,56 C116,46 140,44 154,52 C170,60 180,80 184,100 Z"
    earth = svg.rad([(0, "#a08a62"), (0.5, "#6e5a3e"), (1, "#3a2e20")], 0.30, 0.22, 0.9)
    svg.path(bank, earth, extra=f'filter="{svg.blur(3.2)}"')
    turf = "M82,90 C86,74 96,62 108,54 C120,46 140,44 154,50 C166,56 174,66 178,76 C164,64 150,58 134,58 C116,58 98,70 82,90 Z"
    svg.path(turf, svg.lin([(0, "#94a844"), (1, "#4c6424")], 0, 0, 1, 1), op=0.9, extra=f'filter="{svg.blur(1.6)}"')
    for _ in range(40):  # grass blades hanging over the lip of the cut
        x = rng.uniform(86, 176)
        y = 48 + 0.006 * (x - 130) ** 2 + rng.uniform(0, 8)
        h = rng.uniform(3, 7)
        svg.line((x, y + h * 0.4), (x + rng.uniform(-2.5, 2.5), y - h * 0.6), shade("#7d9a3a", rng.uniform(0.6, 1.4)), 1.1, 0.8)
    # bare rock face round the portal
    svg.path("M92,99 C94,80 100,64 112,60 L146,60 C158,64 166,80 168,99 Z", lit("#857563", 1.25, 0.5, 1.0, 0.6), extra=f'filter="{svg.blur(1.0)}"')
    for _ in range(14):
        x, y = rng.uniform(96, 164), rng.uniform(62, 94)
        svg.ellipse(x, y, rng.uniform(2.5, 5.5), rng.uniform(1.6, 3.2), shade("#857563", rng.uniform(0.6, 1.3)), op=0.7)

    # the opening, deepening to black, with the lamp's warm light falling down the tunnel
    svg.path("M108,98 L108,66 Q128,58 148,66 L148,98 Z", svg.lin([(0, "#1a120a"), (1, "#060403")], 0, 0, 0, 1))
    svg.ellipse(128, 80, 15, 14, svg.rad([(0, "#ffd77a", 0.85), (0.35, "#e0902e", 0.55), (1, "#5a2a08", 0.0)]), blur=svg.blur(1.2))
    svg.circle(128, 78, 3.6, svg.rad([(0, "#fff6d0"), (0.6, "#ffd26a"), (1, "#e8952e")]))
    svg.line((128, 66), (128, 74), "#2a1d12", 1.1)
    svg.path("M112,98 L118,86 L138,86 L144,98 Z", "#ffb24a", op=0.12)  # lamplight on the floor

    # timber frame: two heavy posts and a lintel with its end grain showing
    wood = "#8c5f32"
    for x in (100, 148):
        svg.path(f"M{x},99 L{x},64 Q{x + 4},62 {x + 8},64 L{x + 8},99 Z", lit(wood, 1.35, 0.5, 1.0, 0.0))
        svg.line((x + 1.5, 66), (x + 1.5, 97), shade(wood, 1.55), 1.0, 0.5)
    svg.path("M94,66 L96,56 L162,56 L164,66 Z", lit(wood, 1.4, 0.55, 0.0, 1.0))
    svg.line((97, 57.5), (161, 57.5), shade(wood, 1.6), 1.0, 0.55)
    for x in (94.5, 163.5):
        svg.ellipse(x, 61, 3.2, 5.0, svg.rad([(0, "#d7a868"), (0.6, "#a8763e"), (1, "#5e3a1a")]))
        svg.ellipse(x, 61, 1.4, 2.2, "#7a5028", op=0.7)
    for x0, x1 in ((108, 116), (148, 140)):  # corner braces
        svg.path(f"M{x0},66 L{x1},66 L{x0},74 Z", shade(wood, 0.8))

    # spoil heap to the right, loose rubble at its foot
    svg.ellipse(186, 106, 26, 6, "#0e0b07", op=0.35, blur=svg.blur(2.4))
    svg.path("M160,106 C166,92 178,84 188,86 C198,88 206,98 210,106 Z", lit("#7a6e62", 1.3, 0.5, 1.0, 0.7))
    for _ in range(16):
        x, y = rng.uniform(166, 206), rng.uniform(92, 106)
        svg.ellipse(x, y, rng.uniform(1.8, 3.6), rng.uniform(1.2, 2.4), shade("#6c645c", rng.uniform(0.55, 1.45)), op=0.9)

    # rails curling out of the adit toward the lower left, on timber sleepers
    left, right = [(118, 98), (82, 122)], [(140, 98), (110, 126)]
    for i in range(0, 7):
        t = (i + 0.5) / 7
        a, b = lerp(left[0], left[1], t), lerp(right[0], right[1], t)
        svg.line((a[0] - 4, a[1] - 0.5), (b[0] + 4, b[1] + 0.5), "#3e2a18", 3.2)
        svg.line((a[0] - 4, a[1] - 1.2), (b[0] + 4, b[1] - 0.2), "#7a5634", 1.0, 0.6)
    for rail in (left, right):
        svg.line(*rail, "#4a4e52", 2.6)
        svg.line((rail[0][0] - 0.6, rail[0][1] - 0.8), (rail[1][0] - 0.6, rail[1][1] - 0.8), "#c9cdd0", 1.0, 0.8)

    # the ore cart, heaped with coal
    x, y = 96, 112
    svg.ellipse(x + 3, y + 7, 17, 4.5, "#0e0b07", op=0.4, blur=svg.blur(1.6))
    svg.path(f"M{x - 15},{y - 10} L{x + 14},{y - 10} L{x + 10},{y + 3} L{x - 11},{y + 3} Z", lit("#6a7076", 1.35, 0.5, 1.0, 1.0))
    svg.line((x - 14, y - 9.4), (x + 13.5, y - 9.4), "#b8bec2", 1.0, 0.7)
    for d in range(7):
        sphere_x = x - 12 + d * 4
        svg.ellipse(sphere_x, y - 12 - (d % 2) * 1.8, 3.4, 2.6, svg.rad([(0, "#6a6662"), (0.5, "#2c2a2a"), (1, "#121010")], 0.35, 0.3, 0.8))
    for wx in (-8, 7):
        svg.circle(x + wx, y + 3.5, 3.8, svg.rad([(0, "#5a5652"), (1, "#141210")], 0.35, 0.3, 0.8))
        svg.circle(x + wx, y + 3.5, 1.2, "#a8a095")

    # a pick leaning on the right post
    svg.line((166, 100), (158, 72), "#5e4026", 2.6)
    svg.line((165.2, 99.6), (157.4, 72.2), "#a07448", 0.9, 0.6)
    svg.path("M150,72 Q158,64 168,70 L166,72.5 Q158,68 152,74 Z", lit("#9aa2a8", 1.3, 0.55))
    return svg


# --- the farm, cut along the shore -------------------------------------------------------------------
# A farm fills its tile's diamond, but the shoreline of the terrain does not follow tile edges: it
# runs through the four dual cells around a coastal tile. So the farm is also shipped as a sheet of
# cell-sized pieces, one per (role, shore) pair, that stop where the water layer starts.
SHORE_COLUMNS = 8  # one column per combination of water among the three other vertices of a cell
CELL_W, CELL_H = 256, 128  # a piece, drawn 128 x 64 on the cell the way the terrain cells are
CELL_MARGIN = 1.03  # pieces overlap a little so no hairline shows between them
HEDGE = (0x35 / 255, 0x59 / 255, 0x2A / 255)


def _water_alpha(water, n, e, s, w):
    """Water coverage of the dual cell with vertex digits n, e, s, w (0 land, 1 coast), 0..1,
    resampled to the piece size."""
    import numpy as np
    from PIL import Image

    index = (4 * s + e) * 16 + 4 * w + n
    cw = water.width // 16
    ch = cw // 2
    x, y = (index % 16) * cw, (index // 16) * ch
    cell = water.crop((x, y, x + cw, y + ch)).getchannel("A").resize((CELL_W, CELL_H), Image.Resampling.BICUBIC)
    return np.asarray(cell, dtype=np.float32) / 255.0


def _cell_mask():
    import numpy as np

    yy, xx = np.mgrid[0:CELL_H, 0:CELL_W].astype(np.float32)
    d = np.abs(xx + 0.5 - CELL_W / 2) / (CELL_W / 2) + np.abs(yy + 0.5 - CELL_H / 2) / (CELL_H / 2)
    return np.clip((CELL_MARGIN - d) * 24.0, 0.0, 1.0)


def farm_shore_sheet(farm, water, bank=HEDGE):
    """Sheet of farm pieces; `farm` is the 256 x 128 sprite and `water` the water terrain sheet.
    `bank` is the colour (0..1 RGB) of the bank drawn along the cut: the style's own boundary.

    Piece `role * 8 + mask`: the part of the farm inside the dual cell that has the tile as its
    north (role 0), east (1), south (2) or west (3) vertex, cut where the water layer begins.
    Bit i of `mask` says that the i-th other vertex of the cell, in north, east, south, west
    order, is water.
    """
    import numpy as np
    from PIL import Image, ImageFilter

    from .util import to_array, to_image

    art = to_array(farm)
    cell = _cell_mask()
    # where the tile centre lies in a cell frame, for each role: the farm canvas is moved there
    centre = [(CELL_W // 2, 0), (CELL_W, CELL_H // 2), (CELL_W // 2, CELL_H), (0, CELL_H // 2)]
    sheet = Image.new("RGBA", (CELL_W * SHORE_COLUMNS, CELL_H * 4), (0, 0, 0, 0))
    for role in range(4):
        cx, cy = centre[role]
        framed = np.zeros((CELL_H, CELL_W, 4), dtype=np.float32)
        dx, dy = cx - CELL_W // 2, cy - CELL_H // 2  # farm pixel (fx, fy) lands at (fx + dx, fy + dy)
        for fy in range(farm.height):
            y = fy + dy
            if 0 <= y < CELL_H:
                x0, x1 = max(0, dx), min(CELL_W, farm.width + dx)
                if x1 > x0:
                    framed[y, x0:x1] = art[fy, x0 - dx : x1 - dx]
        others = [v for v in range(4) if v != role]
        for mask in range(SHORE_COLUMNS):
            digits = [0, 0, 0, 0]
            for bit, vertex in enumerate(others):
                digits[vertex] = (mask >> bit) & 1
            water_a = _water_alpha(water, *digits)
            # keep a little ground between the field and the beach, then soften the cut
            land = Image.fromarray(((1.0 - np.clip(water_a * 6.0, 0, 1)) * 255).astype(np.uint8))
            land = land.filter(ImageFilter.MinFilter(5)).filter(ImageFilter.GaussianBlur(1.6))
            land = np.asarray(land, dtype=np.float32) / 255.0
            land = np.clip((land - 0.35) * 3.0, 0.0, 1.0)
            # a hedge along the new edge, as along the old ones
            edge = np.clip(1.0 - np.abs(land - 0.5) * 2.0, 0.0, 1.0) * (land > 0.02)
            out = framed.copy()
            out[..., :3] = out[..., :3] * (1 - edge[..., None] * 0.85) + np.array(bank, dtype=np.float32) * edge[..., None] * 0.85
            out[..., 3] = out[..., 3] * land * cell
            sheet.paste(to_image(out), (mask * CELL_W, role * CELL_H))
    return sheet
