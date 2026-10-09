"""Icons, 64 x 64 (shown at 32 logical px).  Two families, as in the reference:

* engraved ink glyphs for the beige brass command buttons (book, city, tools, laurel, gear, ...);
* small full-colour HUD badges (gold, research, industry, people, morale).
"""

import math

from .paint import render_sprite
from .svgkit import Svg

INK = "#2a2017"
LIGHT = "#fff1c4"


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


# ---- engraved glyphs: draw(svg, c) paints the glyph in colour c ----------------------------------
def g_book(s: Svg, c):
    s.path("M32,19 C24,14 14,15 8,18 L8,48 C14,45 24,45 32,50 Z", c)
    s.path("M32,19 C40,14 50,15 56,18 L56,48 C50,45 40,45 32,50 Z", c)
    for y in (24, 29, 34, 39):
        s.line((13, y), (27, y + 1.5), LIGHT, 1.3, 0.8)
        s.line((51, y), (37, y + 1.5), LIGHT, 1.3, 0.8)
    s.line((32, 19), (32, 50), LIGHT, 1.2, 0.9)


def g_city(s: Svg, c):
    for x, y, w, h in [(6, 28, 10, 24), (17, 20, 9, 32), (27, 30, 10, 22), (38, 14, 8, 38), (47, 26, 11, 26)]:
        s.rect(x, y, w, h, c)
    s.rect(40, 7, 3.5, 8, c)
    for x, y in [(8.5, 33), (8.5, 40), (19.5, 26), (19.5, 33), (19.5, 40), (29.5, 36), (29.5, 43), (40, 22), (40, 30), (40, 38), (50, 32), (50, 40), (54, 32)]:
        s.rect(x, y, 3, 3.6, LIGHT, op=0.85)
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
    s.circle(32, 32, 8, LIGHT, stroke=c, sw=2.2)


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
    s.ellipse(16, 28, 2.4, 12, LIGHT, op=0.6)
    for y in (20, 27, 34):
        s.line((22, y), (44, y), LIGHT, 1.8, 0.85)
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
    s.circle(32, 24, 4, LIGHT, op=0.85)


def g_hourglass(s: Svg, c):
    s.rect(14, 8, 36, 5, c)
    s.rect(14, 51, 36, 5, c)
    s.path("M18,13 L46,13 C46,26 36,28 36,32 C36,36 46,38 46,51 L18,51 C18,38 28,36 28,32 C28,28 18,26 18,13 Z", c)
    s.path("M24,18 L40,18 C38,24 34,27 32,28 C30,27 26,24 24,18 Z", LIGHT, op=0.8)
    s.path("M22,50 C24,42 28,39 32,38 C36,39 40,42 42,50 Z", LIGHT, op=0.8)


def g_shield(s: Svg, c):  # fortify: a heater shield with a boss
    s.path("M32,6 L54,13 C54,35 46,48 32,58 C18,48 10,35 10,13 Z", c)
    s.path("M32,13 L46,18 C46,33 40,42 32,49 C24,42 18,33 18,18 Z", LIGHT, op=0.8)
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
        s.line((32, y0), (32, y1), LIGHT, 2.2 + (y0 - 10) * 0.05, 0.9)


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


ENGRAVED = {
    "book": g_book, "city": g_city, "tools": g_tools, "laurel": g_laurel, "gear": g_gear,
    "globe": g_globe, "scroll": g_scroll, "target": g_target, "fire": g_fire, "shock": g_shock,
    "star": g_star, "banner": g_banner, "hourglass": g_hourglass,
    "shield": g_shield, "cancel": g_cancel, "cross": g_cross, "road": g_road, "rails": g_rails,
    "pickaxe": g_pickaxe, "wheat": g_wheat,
}


def engraved(name) -> Svg:
    s = Svg(64, 64)
    s.begin('transform="translate(1.2 1.4)"')
    ENGRAVED[name](s, "#fff3cc")  # sunlit lower lip
    s.end()
    ENGRAVED[name](s, INK)
    return s


# ---- HUD badges ----------------------------------------------------------------------------------
def hud_gold() -> Svg:
    s = Svg(64, 64)
    s.ellipse(34, 57, 20, 4, "#000", op=0.3, blur=s.blur(2))
    g = s.rad([(0, "#ffe27a"), (0.55, "#e0a92a"), (1, "#9a6412")], 0.38, 0.32, 0.8)
    s.path("M24,16 C14,28 9,40 14,50 C20,57 44,57 50,50 C55,40 50,28 40,16 Z", g, "#4a2e0a", 2)
    s.path("M22,16 Q32,23 42,16 L40,10 Q32,14 24,10 Z", "#c8891e", "#4a2e0a", 2)
    s.line((24, 17), (40, 17), "#6b3f0b", 2.4)
    s.circle(32, 38, 9.5, "none", op=0.8, stroke="#fff0a0", sw=1.8)
    s.path("M28,38 Q32,33 36,38 Q32,43 28,38", "#fff0a0", op=0.8)
    s.ellipse(22, 30, 3, 8, "#fff7c8", op=0.5, rot=22)
    return s


def hud_research() -> Svg:
    s = Svg(64, 64)
    s.ellipse(32, 57, 18, 3.5, "#000", op=0.3, blur=s.blur(2))
    s.path("M26,8 L38,8 L38,24 L52,50 Q54,56 48,56 L16,56 Q10,56 12,50 L26,24 Z", "#cfeff0", "#0f3a40", 2.2, op=0.55)
    s.path("M21,40 L43,40 L52,50 Q54,56 48,56 L16,56 Q10,56 12,50 Z", s.lin([(0, "#58e0d0"), (1, "#127c80")], 0, 0, 0, 1), "#0f3a40", 2.0)
    s.rect(24, 5, 16, 5, "#8a6a3a", rx=2, stroke="#2a1a0a", sw=1.6)
    for x, y, r in ((26, 47, 2.6), (35, 45, 1.8), (31, 51, 2.0)):
        s.circle(x, y, r, "#e8fff9", op=0.85)
    s.path("M27,12 L27,24 L16,46", "none", "#fff", 2.2, op=0.45)
    return s


def hud_industry() -> Svg:
    s = Svg(64, 64)
    s.ellipse(32, 57, 20, 3.6, "#000", op=0.3, blur=s.blur(2))
    s.poly(cog_pts(32, 34, 25, 19, 8, 0.17), s.lin([(0, "#f0dc9c"), (0.5, "#b99a58"), (1, "#6e5628")], 0, 0, 1, 1), "#2a1d0a", 2)
    s.circle(32, 34, 11, "#1e242a", stroke="#0b0f12", sw=1.6)
    s.circle(32, 34, 6, s.lin([(0, "#7a8791"), (1, "#2b343b")], 0, 0, 1, 1), stroke="#0b0f12", sw=1.2)
    return s


def hud_people() -> Svg:
    s = Svg(64, 64)
    col = s.lin([(0, "#f7d778"), (1, "#c28d1c")], 0, 0, 1, 1)
    for x, y, k in ((22, 26, 0.85), (40, 22, 1.0)):
        s.circle(x, y, 8.5 * k, col, stroke="#4a300a", sw=1.8)
        s.path(f"M{x - 15*k},{y + 32*k} C{x - 15*k},{y + 12*k} {x + 15*k},{y + 12*k} {x + 15*k},{y + 32*k} Z", col, "#4a300a", 1.8)
    return s


def hud_morale() -> Svg:
    s = Svg(64, 64)
    s.ellipse(33, 57, 18, 3.4, "#000", op=0.3, blur=s.blur(2))
    s.circle(32, 31, 24, s.rad([(0, "#fff08a"), (0.6, "#f2c320"), (1, "#b27d0a")], 0.38, 0.3, 0.85), stroke="#4a2e0a", sw=2.2)
    s.ellipse(24, 26, 2.8, 4.2, "#3a2308")
    s.ellipse(40, 26, 2.8, 4.2, "#3a2308")
    s.path("M19,36 Q32,52 45,36", "none", "#3a2308", 3.2)
    return s


HUD = {"gold": hud_gold, "research": hud_research, "industry": hud_industry, "people": hud_people, "morale": hud_morale}


def render_all():
    out = {}
    kw = dict(ss=4, seed=5, radius=0, grain=0.03, strokes=0.0, grade_kw=None)
    for name in ENGRAVED:
        out[f"icon_{name}"] = render_sprite(engraved(name).render(), 64, 64, **kw)
    for name, fn in HUD.items():
        out[f"hud_{name}"] = render_sprite(fn().render(), 64, 64, **kw)
    return out
