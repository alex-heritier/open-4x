"""Tile improvements drawn over the terrain: the farm (a whole-tile overlay) and the mine.

Both are authored on a 256 x 128 canvas, shown at 128 x 64: the same 2:1 diamond as one terrain
cell, so they sit exactly on a tile. Lit from the upper left like everything else.
"""

from .svgkit import Iso, Svg, lerp, mixc, shade

W, H = 256, 128
INK = "#17130f"


def build_farm() -> Svg:
    """Ploughed strips in three crops inside a hedge, with a pair of haystacks."""
    svg = Svg(W, H)
    n = 6
    k = 112 / n  # the field fills 90% of the diamond so a margin of terrain shows round it
    iso = Iso(128, 64 - 56 * 0.5 * 1.0, k)
    iso.oy = 64 - n * k / 2
    corners = [iso.p(0, 0), iso.p(n, 0), iso.p(n, n), iso.p(0, n)]
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
    # hedge round the field: a dark line with leafy blobs
    ring = corners + [corners[0]]
    for p, q in zip(ring, ring[1:]):
        svg.line(p, q, "#243a1c", 5.0)
        for t in range(9):
            c = lerp(p, q, (t + 0.5) / 9)
            svg.circle(c[0], c[1] - 1.5, 4.2, "#35592a", stroke=None)
            svg.circle(c[0] - 1, c[1] - 2.6, 2.4, "#5a8a3c", op=0.9)
    # a pair of haystacks in the lower corner
    for gx, gy in ((n - 1.3, n - 1.4), (n - 0.7, n - 0.8)):
        x, y = iso.p(gx, gy)
        svg.ellipse(x + 2, y + 2, 11, 4.4, "#1a1308", op=0.4)
        svg.poly([(x - 10, y), (x + 10, y), (x + 5, y - 14), (x - 5, y - 14)], svg.lin([(0, "#e4c870"), (1, "#a9863a")], 0, 0, 1, 0.4), INK, 0.7)
        svg.ellipse(x, y - 14, 5, 2.4, "#efd98c", INK, 0.6)
    return svg


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


def farm_shore_sheet(farm, water):
    """Sheet of farm pieces; `farm` is the 256 x 128 sprite and `water` the water terrain sheet.

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
            out[..., :3] = out[..., :3] * (1 - edge[..., None] * 0.85) + np.array(HEDGE, dtype=np.float32) * edge[..., None] * 0.85
            out[..., 3] = out[..., 3] * land * cell
            sheet.paste(to_image(out), (mask * CELL_W, role * CELL_H))
    return sheet
