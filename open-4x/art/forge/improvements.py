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
