"""Icons, 64 x 64 (shown at 16 to 32 logical px).

Flat glyphs, one shape each, drawn in white on transparent so the client can tint them any colour
(`ImageNode::color`).  A glyph's detail lines are cut out of it rather than painted over it: they
are drawn in `CUT` and become holes (alpha) when the sheet is rendered, so a tint never leaves a
second colour behind.  `icon_*` are the command glyphs of the action bar and the city view;
`hud_*` are the readouts of the top bar and the city's economy.
"""

import math

from .util import downsample, rasterize
from .svgkit import Svg

INK = "#ffffff"
# Drawn over a glyph, this is a hole in it once rendered (see `render_flat`).
CUT = "#000000"


def cog_pts(cx, cy, r_out, r_in, teeth=8, tw=0.16):
    pts = []
    for k in range(teeth):
        a = k / teeth * math.tau
        for da, rr in ((-tw * 1.45, r_in), (-tw, r_out), (tw, r_out), (tw * 1.45, r_in)):
            pts.append((cx + math.cos(a + da) * rr, cy + math.sin(a + da) * rr))
    return pts


def star_pts(cx, cy, r_out, r_in, n=5):
    pts = []
    for k in range(n * 2):
        rr = r_out if k % 2 == 0 else r_in
        a = k / (n * 2) * math.tau - math.pi / 2
        pts.append((cx + math.cos(a) * rr, cy + math.sin(a) * rr))
    return pts


# ---- glyphs: draw(svg, c) paints the glyph in colour c ---------------------------------------------
def g_book(s: Svg, c):
    s.path("M32,19 C24,14 14,15 8,18 L8,48 C14,45 24,45 32,50 Z", c)
    s.path("M32,19 C40,14 50,15 56,18 L56,48 C50,45 40,45 32,50 Z", c)
    for y in (24, 29, 34, 39):
        s.line((13, y), (27, y + 1.5), CUT, 1.3, 0.8)
        s.line((51, y), (37, y + 1.5), CUT, 1.3, 0.8)
    s.line((32, 19), (32, 50), CUT, 1.2, 0.9)


def g_city(s: Svg, c):
    for x, y, w, h in [(6, 28, 10, 24), (17, 20, 9, 32), (27, 30, 10, 22), (38, 14, 8, 38), (47, 26, 11, 26)]:
        s.rect(x, y, w, h, c)
    s.rect(40, 7, 3.5, 8, c)
    for x, y in [(8.5, 33), (8.5, 40), (19.5, 26), (19.5, 33), (19.5, 40), (29.5, 36), (29.5, 43), (40, 22), (40, 30), (40, 38), (50, 32), (50, 40), (54, 32)]:
        s.rect(x, y, 3, 3.6, CUT, op=0.85)
    s.rect(4, 52, 56, 3, c)


def g_tools(s: Svg, c):
    s.line((14, 52), (44, 14), c, 4.2)
    s.path("M34,10 L52,8 L54,16 L40,20 Z", c)
    s.line((50, 52), (20, 14), c, 4.2)
    s.path("M8,16 Q20,6 30,12 L28,17 Q20,13 10,21 Z", c)


def g_laurel(s: Svg, c):
    for sign in (-1, 1):
        s.path(f"M{32 + sign*2},52 C{32 + sign*20},46 {32 + sign*24},28 {32 + sign*10},12", "none", c, 2.2)
        for k in range(8):
            t = k / 7
            x = 32 + sign * (4 + 21 * math.sin(t * 1.2) * (1 - t * 0.35))
            y = 50 - t * 38
            s.ellipse(x + sign * 3, y, 8.2, 3.4, c, rot=-sign * (30 + t * 60))
    s.poly(star_pts(32, 29, 8, 3.4), c)


def g_gear(s: Svg, c):
    s.poly(cog_pts(32, 32, 24, 18, 8, 0.17), c)
    s.circle(32, 32, 8, CUT, stroke=c, sw=2.2)


def g_globe(s: Svg, c):
    s.circle(32, 32, 24, "none", stroke=c, sw=3.4)
    s.ellipse(32, 32, 10.5, 24, "none", stroke=c, sw=2.6)
    s.line((32, 8), (32, 56), c, 2.6)
    for y, rx in ((22, 22.6), (32, 24), (42, 22.6)):
        s.line((32 - rx, y), (32 + rx, y), c, 2.4)


def g_scroll(s: Svg, c):
    s.path("M16,12 L50,12 L50,44 L16,44 Z", c)
    s.ellipse(16, 28, 5, 16.5, c)
    s.ellipse(50, 28, 5, 16.5, c)
    s.ellipse(16, 28, 2.4, 12, CUT, op=0.6)
    for y in (20, 27, 34):
        s.line((22, y), (44, y), CUT, 1.8, 0.85)
    s.path("M26,44 L26,58 L32,53 L38,58 L38,44 Z", c)


def g_target(s: Svg, c):
    s.circle(32, 32, 17, "none", stroke=c, sw=3.2)
    s.circle(32, 32, 6, c)
    for a in (0, 90, 180, 270):
        r = math.radians(a)
        s.line((32 + math.cos(r) * 13, 32 + math.sin(r) * 13), (32 + math.cos(r) * 25, 32 + math.sin(r) * 25), c, 3.2)


def g_fire(s: Svg, c):  # crossed muskets with bayonets
    for sign in (-1, 1):
        x0, x1 = 32 - sign * 22, 32 + sign * 22
        s.line((x0, 56), (x1, 10), c, 3.4)
        s.line((x1, 10), (x1 + sign * 3, 3), c, 1.8)
        s.path(f"M{x0 - sign*3},56 L{x0 + sign*5},56 L{x0 + sign*6},47 Z", c)


def g_shock(s: Svg, c):  # crossed sabres
    for sign in (-1, 1):
        s.path(f"M{32 - sign*21},52 Q{32 + sign*6},38 {32 + sign*24},8 Q{32 + sign*20},32 {32 - sign*18},54 Z", c)
        s.line((32 - sign * 24, 49), (32 - sign * 13, 57), c, 3.4)
        s.circle(32 - sign * 22, 55, 3, c)


def g_star(s: Svg, c):
    s.circle(32, 32, 25, "none", stroke=c, sw=3.0)
    s.poly(star_pts(32, 33, 18, 7.4), c)


def g_banner(s: Svg, c):
    s.rect(14, 8, 3.6, 50, c)
    s.path("M17.6,12 C28,6 36,18 48,12 L48,34 C36,40 28,28 17.6,34 Z", c)
    s.circle(32, 24, 4, CUT, op=0.85)


def g_hourglass(s: Svg, c):
    s.rect(14, 8, 36, 5, c)
    s.rect(14, 51, 36, 5, c)
    s.path("M18,13 L46,13 C46,26 36,28 36,32 C36,36 46,38 46,51 L18,51 C18,38 28,36 28,32 C28,28 18,26 18,13 Z", c)
    s.path("M24,18 L40,18 C38,24 34,27 32,28 C30,27 26,24 24,18 Z", CUT, op=0.8)
    s.path("M22,50 C24,42 28,39 32,38 C36,39 40,42 42,50 Z", CUT, op=0.8)


def g_shield(s: Svg, c):  # fortify: a heater shield with a boss
    s.path("M32,6 L54,13 C54,35 46,48 32,58 C18,48 10,35 10,13 Z", c)
    s.path("M32,13 L46,18 C46,33 40,42 32,49 C24,42 18,33 18,18 Z", CUT, op=0.8)
    s.line((32, 18), (32, 46), c, 3.4)
    s.line((22, 28), (42, 28), c, 3.4)


def g_cancel(s: Svg, c):  # cancel orders: the "no" sign
    s.circle(32, 32, 22, "none", stroke=c, sw=5.4)
    s.line((17, 47), (47, 17), c, 5.4)


def g_cross(s: Svg, c):  # disband: strike the unit from the rolls
    s.line((15, 15), (49, 49), c, 7.0)
    s.line((49, 15), (15, 49), c, 7.0)


def g_road(s: Svg, c):  # a road running to the horizon
    s.path("M27,10 L37,10 L56,56 L8,56 Z", c)
    for y0, y1 in ((16, 22), (28, 36), (42, 53)):
        s.line((32, y0), (32, y1), CUT, 2.2 + (y0 - 10) * 0.05, 0.9)


def g_rails(s: Svg, c):  # a railroad: two rails and the ties between them
    s.line((26, 8), (14, 56), c, 4.0)
    s.line((38, 8), (50, 56), c, 4.0)
    for y in (16, 26, 36, 46, 55):
        half = 6 + (y - 8) * 0.25
        s.line((32 - half - 4, y), (32 + half + 4, y), c, 3.0)


def g_pickaxe(s: Svg, c):  # mine
    s.line((12, 56), (40, 22), c, 4.6)
    s.path("M8,26 C16,10 38,6 56,16 C44,14 34,18 28,26 C22,22 14,22 8,26 Z", c)


def g_wheat(s: Svg, c):  # farm: an ear of wheat
    s.line((32, 58), (32, 18), c, 3.2)
    for k in range(5):
        y = 14 + k * 8
        s.ellipse(25, y + 3, 3.4, 6.4, c, rot=-32)
        s.ellipse(39, y + 3, 3.4, 6.4, c, rot=32)
    s.ellipse(32, 9, 3.2, 6.4, c)




def flat(draw) -> Svg:
    s = Svg(64, 64)
    draw(s, INK)
    return s


# ---- readouts --------------------------------------------------------------------------------------
def g_coin(s: Svg, c):
    s.circle(32, 32, 26, c)
    s.circle(32, 32, 18, "none", stroke=CUT, sw=3.2)
    s.line((32, 22), (32, 42), CUT, 4.0)


def g_flask(s: Svg, c):  # research
    s.rect(24, 6, 16, 6, c, rx=3)
    s.path("M26,12 L38,12 L38,26 L55,52 Q58,58 51,58 L13,58 Q6,58 9,52 L26,26 Z", c)
    s.path("M17,42 L47,42", "none", CUT, 3.2)
    for x, y, r in ((27, 50, 2.6), (37, 48, 1.9)):
        s.circle(x, y, r, CUT)


def g_people(s: Svg, c):
    for x, y, k in ((22, 26, 0.9), (42, 22, 1.0)):
        s.circle(x, y, 8.6 * k, c)
        s.path(f"M{x - 15*k},{y + 32*k} C{x - 15*k},{y + 11*k} {x + 15*k},{y + 11*k} {x + 15*k},{y + 32*k} Z", c)
    s.path("M28,24 C34,22 36,28 34,32", "none", CUT, 2.4)


def g_smile(s: Svg, c):  # morale
    s.circle(32, 32, 26, c)
    s.circle(23, 26, 3.4, CUT)
    s.circle(41, 26, 3.4, CUT)
    s.path("M19,37 Q32,51 45,37", "none", CUT, 3.8)


GLYPHS = {
    "book": g_book, "city": g_city, "tools": g_tools, "laurel": g_laurel, "gear": g_gear,
    "globe": g_globe, "scroll": g_scroll, "target": g_target, "fire": g_fire, "shock": g_shock,
    "star": g_star, "banner": g_banner, "hourglass": g_hourglass,
    "shield": g_shield, "cancel": g_cancel, "cross": g_cross, "road": g_road, "rails": g_rails,
    "pickaxe": g_pickaxe, "wheat": g_wheat,
}
HUD = {"gold": g_coin, "research": g_flask, "industry": g_gear, "people": g_people, "morale": g_smile}


def render_flat(svg: Svg, size=64, ss=4):
    """White on transparent, with `CUT` shapes punched out."""
    from PIL import Image
    import numpy as np

    big = rasterize(svg.render(), size * ss, size * ss)
    a = np.asarray(big.convert("RGBA"), dtype=np.float32) / 255.0
    # Cut shapes are black over white: the red channel is how much of the glyph survives.
    alpha = a[..., 3] * a[..., 0]
    out = np.dstack([np.ones_like(alpha), np.ones_like(alpha), np.ones_like(alpha), alpha])
    return downsample(Image.fromarray((out * 255 + 0.5).astype(np.uint8), "RGBA"), size, size)


def render_all():
    out = {}
    for name, draw in GLYPHS.items():
        out[f"icon_{name}"] = render_flat(flat(draw))
    for name, draw in HUD.items():
        out[f"hud_{name}"] = render_flat(flat(draw))
    return out
