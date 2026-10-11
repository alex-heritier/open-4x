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


def build_rice() -> Svg:
    """Flooded paddies in a grid of earthen bunds, planted at every stage from seedlings in
    bright water to golden heads ready for the sickle."""
    svg = Svg(W, H)
    iso = _iso()
    rng = random.Random(11)
    svg.poly(_corners(iso), "#8d7b4a")
    plots, m = 3, 0.16  # a 3 x 3 grid; m is half the width of a bund
    span = N / plots
    # how far along each paddy's crop is: 0 just transplanted, 1 ready to harvest
    ripeness = [[0.10, 0.50, 0.90], [0.60, 0.25, 0.75], [0.95, 0.40, 0.05]]
    water = svg.lin([(0, "#b4dad0"), (1, "#5a9795")], 0, 0, 1, 1)
    plants = []
    for i in range(plots):
        for j in range(plots):
            x0, y0, x1, y1 = i * span + m, j * span + m, (i + 1) * span - m, (j + 1) * span - m
            paddy = [iso.p(x0, y0), iso.p(x1, y0), iso.p(x1, y1), iso.p(x0, y1)]
            # a bund: a lit top, then the paddy sunk below it with a shadow under the near bank
            svg.poly([iso.p(x0 - m, y0 - m), iso.p(x1 + m, y0 - m), iso.p(x1 + m, y1 + m), iso.p(x0 - m, y1 + m)], "#a8975c", "#6a5a34", 0.8)
            svg.poly(paddy, water, "#3f7670", 1.0)
            svg.line(iso.p(x0, y0), iso.p(x1, y0), "#2f625e", 2.2, 0.55)
            svg.line(iso.p(x0, y0), iso.p(x0, y1), "#2f625e", 2.2, 0.55)
            for s in range(2):  # sky glinting off the water
                gy = y0 + (y1 - y0) * (0.3 + 0.4 * s)
                svg.line(iso.p(x0 + 0.3 + 0.3 * s, gy), iso.p(x0 + 0.9 + 0.3 * s, gy), "#f2fbf4", 1.3, 0.55)
            ripe = ripeness[i][j]
            rows, per_row = 5, 5
            for r in range(rows):
                for c in range(per_row):
                    px = x0 + 0.3 + (x1 - x0 - 0.6) * (r + 0.5) / rows + rng.uniform(-0.04, 0.04)
                    py = y0 + 0.3 + (y1 - y0 - 0.6) * (c + 0.5) / per_row + rng.uniform(-0.05, 0.05)
                    plants.append((px + py, lambda px=px, py=py, ripe=ripe: _rice_clump(svg, iso, rng, px, py, ripe)))
    _paint_in_depth(plants)
    return svg


def _rice_clump(svg, iso, rng, x, y, ripe):
    """A tuft of rice: a fan of blades, greener when young and heavy with gold heads when ripe."""
    px, py = iso.p(x, y)
    height = 5.5 + 5.0 * ripe
    young, grown, golden = "#8fd24e", "#58a83a", "#d2bd48"
    base = mixc(young, grown, min(1.0, ripe * 2.0)) if ripe < 0.5 else mixc(grown, golden, (ripe - 0.5) * 2.0)
    svg.ellipse(px, py + 0.8, 3.2, 1.2, "#2f625e", op=0.45)  # reflection under the tuft
    for blade in range(5):
        lean = (blade - 2) * (1.5 + 1.5 * ripe) + rng.uniform(-0.6, 0.6)
        top = (px + lean, py - height * (0.85 + 0.15 * (blade % 2)))
        svg.path(
            f"M{px + (blade - 2) * 0.5:.1f},{py:.1f} Q{px + lean * 0.4:.1f},{py - height * 0.6:.1f} {top[0]:.1f},{top[1]:.1f}",
            stroke=shade(base, 0.85 + 0.12 * (blade % 3)),
            sw=1.5,
        )
    if ripe > 0.6:  # drooping heads
        svg.ellipse(px + 3.0, py - height * 0.8, 2.6, 1.2, "#e9d56a", INK, 0.4, rot=-25)
        svg.ellipse(px - 2.6, py - height * 0.7, 2.4, 1.1, "#dcc558", INK, 0.4, rot=20)


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
    """A timbered adit in the hillside with rails, an ore cart, and a spoil heap."""
    svg = Svg(W, H)
    # spoil heap to the right, and a smaller one to the left
    for cx, cy, rx, ry, col in ((172, 98, 34, 15, "#7a6a58"), (86, 100, 22, 9, "#6e6050")):
        svg.ellipse(cx + 3, cy + 5, rx, ry * 0.55, "#0e0b07", op=0.45, blur=svg.blur(2.4))
        svg.poly(
            [(cx - rx, cy + ry * 0.5), (cx - rx * 0.5, cy - ry), (cx + rx * 0.2, cy - ry * 1.15), (cx + rx, cy + ry * 0.5)],
            svg.lin([(0, shade(col, 1.35)), (1, shade(col, 0.6))], 0, 0, 1, 0.8),
            INK,
            0.8,
        )
        for d in range(7):  # loose stones
            svg.ellipse(cx - rx * 0.6 + d * rx * 0.2, cy + ry * 0.1 - (d % 3) * 3, 2.6, 1.6, shade(col, 0.6 + 0.1 * (d % 3)), op=0.9)
    # the opening: dark interior, depth gradient
    opening = [(106, 92), (106, 56), (116, 46), (140, 46), (150, 56), (150, 92)]
    svg.poly(opening, svg.lin([(0, "#0b0907"), (1, "#2a2118")], 0, 0, 0.3, 1), INK, 1.2)
    svg.poly([(112, 92), (112, 58), (120, 52), (136, 52), (144, 58), (144, 92)], "#050403", op=0.8)
    # timber frame: posts, lintel, and two angled braces
    wood = "#8a5c2e"
    for x in (100, 150):
        svg.poly([(x, 94), (x + 8, 94), (x + 8, 50), (x, 50)], svg.lin([(0, shade(wood, 1.3)), (1, shade(wood, 0.6))], 0, 0, 1, 0), INK, 1.0)
    svg.poly([(94, 54), (164, 54), (164, 44), (94, 44)], svg.lin([(0, shade(wood, 1.35)), (1, shade(wood, 0.65))], 0, 0, 0, 1), INK, 1.0)
    svg.poly([(104, 58), (112, 54), (112, 62)], shade(wood, 0.9), INK, 0.7)
    svg.poly([(150, 58), (142, 54), (142, 62)], shade(wood, 0.8), INK, 0.7)
    # rails curling out toward the lower left, with sleepers
    left, right = [(116, 92), (80, 118)], [(140, 92), (110, 122)]
    for i in range(1, 7):
        t = i / 7
        a, b = lerp(left[0], left[1], t), lerp(right[0], right[1], t)
        svg.line((a[0] - 4, a[1] - 0.5), (b[0] + 4, b[1] + 0.5), "#4a3220", 3.0)
    svg.line(*left, "#aeb4b8", 2.2)
    svg.line(*right, "#aeb4b8", 2.2)
    svg.line((left[0][0] - 1, left[0][1] - 1), (left[1][0] - 1, left[1][1] - 1), "#eef0f0", 0.8, 0.7)
    # ore cart on the rails
    x, y = 97, 112
    svg.ellipse(x + 2, y + 6, 17, 5, "#0e0b07", op=0.4, blur=svg.blur(1.6))
    svg.poly([(x - 16, y - 12), (x + 14, y - 12), (x + 10, y + 2), (x - 12, y + 2)], svg.lin([(0, "#7a828a"), (1, "#3a4046")], 0, 0, 1, 1), INK, 1.0)
    svg.poly([(x - 16, y - 12), (x + 14, y - 12), (x + 11, y - 8), (x - 13, y - 8)], "#a8b0b6", INK, 0.7)
    for d in range(6):  # heaped ore
        svg.ellipse(x - 12 + d * 5, y - 14 - (d % 2) * 2, 3.8, 2.6, ("#2c2a2a", "#5a4a3a", "#3a3434")[d % 3], INK, 0.5)
    for wx in (-9, 8):
        svg.circle(x + wx, y + 3, 4.2, "#22201f", stroke=INK, sw=0.8)
        svg.circle(x + wx, y + 3, 1.4, "#a8a095")
    # a hanging lantern at the lintel
    svg.line((122, 54), (122, 62), INK, 1.0)
    svg.circle(122, 65, 3.2, "#f2c25a", stroke=INK, sw=0.8)
    svg.circle(122, 65, 7.5, "#f6d27a", op=0.28, blur=svg.blur(2.4))
    # a pick leaning on the post
    svg.line((172, 100), (160, 62), "#6a4a2c", 2.6)
    svg.path("M153,60 Q160,54 168,60 L167,62 Q160,58 155,63 Z", "#8c949a", INK, 0.7)
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
