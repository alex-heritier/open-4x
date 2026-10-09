"""Unit sprites: infantry, pioneers, cavalry, field artillery (160 x 160) and the ironclad (256 x 192).

Figures are drawn in a local frame with the feet at (0, 0) and y pointing down; `place()` scales
and translates them.  In the game the ground tile centre sits at (80, 108) of a 160 sprite and
(128, 123) of the ship sprite.  Lit from the upper left, shaded to the right, dark ink outlines.
"""

import math
import random

from .buildings import flag, puffs
from .svgkit import Svg, lerp, mixc, shade

U = 160
INK = "#17130f"
SKIN = ("#e6bd94", "#c18f68")
NAVY = "#33465f"
SLATE = "#59667a"
LEATHER = "#2a1d14"
BRASS = "#cda95c"
CREAM = "#e4dcc3"
RED = "#a8322a"


def place(svg: Svg, x, y, s=1.0, flip=False):
    sx = -s if flip else s
    svg.begin(f'transform="translate({x} {y}) scale({sx} {s})"')


def limb(svg, a, b, wa, wb, col, ink=INK, sw=0.9, hi=1.22, lo=0.66):
    """Tapered limb from a to b (widths wa -> wb) with a left-lit gradient and round ends."""
    dx, dy = b[0] - a[0], b[1] - a[1]
    n = math.hypot(dx, dy) or 1
    nx, ny = -dy / n, dx / n
    pts = [(a[0] + nx * wa / 2, a[1] + ny * wa / 2), (b[0] + nx * wb / 2, b[1] + ny * wb / 2), (b[0] - nx * wb / 2, b[1] - ny * wb / 2), (a[0] - nx * wa / 2, a[1] - ny * wa / 2)]
    g = svg.lin([(0, shade(col, hi)), (0.55, col), (1, shade(col, lo))], 0, 0, 1, 0.15)
    svg.circle(a[0], a[1], wa / 2, g, ink, sw)
    svg.circle(b[0], b[1], wb / 2, g, ink, sw)
    svg.poly(pts, g, None)
    svg.line(pts[0], pts[1], ink, sw)
    svg.line(pts[3], pts[2], ink, sw)


def shadow(svg, x, y, rx, ry, soft, op=0.5):
    svg.ellipse(x + rx * 0.18, y, rx, ry, "#0b1007", op=op, blur=soft)


def head(svg, x, y, r=6.6, cap=NAVY, band=RED, plume=None):
    g = svg.rad([(0, SKIN[0]), (1, SKIN[1])], 0.35, 0.3, 0.8)
    svg.ellipse(x, y, r * 0.95, r * 1.1, g, INK, 0.8)
    svg.ellipse(x - r * 0.28, y + r * 0.1, 0.9, 1.1, "#3a2418")
    svg.ellipse(x + r * 0.34, y + r * 0.1, 0.9, 1.1, "#3a2418")
    svg.rect(x - r * 0.52, y + r * 0.50, r * 1.04, r * 0.26, "#3b2a1d", rx=0.6, op=0.8)  # moustache
    # kepi: drum, band, visor, brass badge
    top = y - r * 0.95
    svg.poly([(x - r, top + r * 0.45), (x + r, top + r * 0.45), (x + r * 0.9, top - r * 0.55), (x - r * 0.9, top - r * 0.55)], svg.lin([(0, shade(cap, 1.3)), (1, shade(cap, 0.7))], 0, 0, 1, 0), INK, 0.9)
    svg.ellipse(x, top - r * 0.55, r * 0.9, r * 0.22, shade(cap, 1.35), INK, 0.7)
    svg.rect(x - r, top + r * 0.12, r * 2, r * 0.32, band, stroke=INK, sw=0.6)
    svg.poly([(x - r * 0.7, top + r * 0.5), (x + r * 1.5, top + r * 0.55), (x + r * 1.2, top + r * 0.8), (x - r * 0.6, top + r * 0.72)], "#1a1511", INK, 0.6)
    svg.circle(x + 0.2, top + r * 0.28, 1.3, BRASS)
    if plume:
        svg.path(f"M{x + r*0.3:.1f},{top - r*0.6:.1f} q{r*0.5:.1f},{-r*1.4:.1f} {r*0.9:.1f},{-r*0.8:.1f}", stroke=plume, sw=2.2)


def soldier(svg, x, y, s, coat=NAVY, trousers=SLATE, facing=1, rifle=True, seed=0, cap=None, band=RED, plume=None):
    """Standing rifleman at ease, rifle grounded on the right. (x, y) is the point between the feet."""
    soft = svg.blur(2.2)
    shadow(svg, x, y, 17 * s, 5.2 * s, soft)
    place(svg, x, y, s, flip=(facing < 0))
    # legs (slightly apart), boots
    limb(svg, (-4.5, -40), (-7.5, -10), 9.5, 7.2, trousers)
    limb(svg, (4.5, -40), (8.5, -10), 9.5, 7.2, trousers)
    for bx, bw in ((-8.2, 1), (9.6, 1)):
        g = svg.lin([(0, "#3b3028"), (1, "#14100c")], 0, 0, 1, 1)
        svg.poly([(bx - 5, -12), (bx + 4, -12), (bx + 6 * bw, -1), (bx + 8, 0), (bx - 6, 0), (bx - 6, -4)], g, INK, 0.9)
    # coat tails and torso
    g = svg.lin([(0, shade(coat, 1.35)), (0.5, coat), (1, shade(coat, 0.62))], 0, 0, 1, 0.2)
    svg.poly([(-12.5, -77), (12.5, -77), (11.5, -50), (13, -36), (-13, -36), (-11.5, -50)], g, INK, 1.0)
    svg.poly([(-3, -77), (3, -77), (2, -36), (-2, -36)], shade(coat, 0.7), op=0.55)
    for k in range(6):  # brass buttons
        svg.circle(0.2, -73 + k * 6.0, 1.05, BRASS)
    # white cross belts and waist belt
    svg.poly([(-12, -76), (-8, -77), (12, -50), (9, -46)], CREAM, shade(CREAM, 0.5), 0.6, op=0.95)
    svg.poly([(12, -76), (8, -77), (-12, -50), (-9, -46)], shade(CREAM, 0.9), shade(CREAM, 0.5), 0.6, op=0.95)
    svg.rect(-12.5, -46, 25, 3.4, "#201a14", stroke=INK, sw=0.7)
    svg.rect(-2, -46.6, 4.4, 4.6, BRASS, stroke="#5a4520", sw=0.5)
    # red collar and epaulettes
    svg.poly([(-5, -79), (5, -79), (4, -75), (-4, -75)], band, INK, 0.6)
    svg.ellipse(-12.5, -76, 4.2, 2.2, shade(BRASS, 0.9), INK, 0.6)
    svg.ellipse(12.5, -76, 4.2, 2.2, shade(BRASS, 0.9), INK, 0.6)
    # bedroll across the shoulder
    svg.path("M-12,-77 Q-16,-58 -2,-47", stroke="#a58f67", sw=5.0)
    svg.path("M-12,-77 Q-16,-58 -2,-47", stroke=INK, sw=0.7, op=0.6)
    # arms
    limb(svg, (-13, -73), (-17.5, -57), 8, 6.8, coat)
    limb(svg, (-17.5, -57), (-14.5, -44), 6.8, 6.0, coat)
    svg.circle(-14.3, -42.5, 3.0, SKIN[0], INK, 0.7)
    if rifle:
        limb(svg, (13, -73), (21, -62), 8, 6.8, coat)
        limb(svg, (21, -62), (19.5, -52), 6.8, 6.0, coat)
        # musket grounded at the right, bayonet up
        svg.poly([(18.0, 1), (21.0, 1), (21.6, -100), (18.6, -100)], svg.lin([(0, "#8a6238"), (1, "#4a3220")], 0, 0, 1, 0), INK, 0.8)
        svg.poly([(18.4, -34), (21.2, -34), (21.4, -100), (18.7, -100)], "#9aa2a8", INK, 0.6, op=0.0)
        svg.line((20.2, -100), (20.2, -120), "#cfd5d8", 1.4)
        svg.line((20.2, -100), (20.2, -118), INK, 0.3, 0.5)
        svg.circle(20.0, -102, 1.6, "#6a7076")
        svg.circle(19.6, -54, 3.0, SKIN[0], INK, 0.7)
    else:
        limb(svg, (13, -73), (18.5, -58), 8, 6.8, coat)
        limb(svg, (18.5, -58), (15.5, -45), 6.8, 6.0, coat)
        svg.circle(15.3, -43.5, 3.0, SKIN[0], INK, 0.7)
    svg.rect(-2.5, -81, 5, 5, shade(SKIN[1], 0.9))
    head(svg, 0, -87, 6.4, cap or coat, band, plume)
    svg.end()


def worker(svg, x, y, s, facing=1, tool="pick", smock="#8a6a43", pants="#55483a", hat="#5d4a32"):
    """Labourer in a brimmed hat and canvas smock, tool on the shoulder, pack on the back.

    `tool` is "pick" (pioneers) or "shovel" (workers)."""
    soft = svg.blur(2.2)
    shadow(svg, x, y, 16 * s, 5 * s, soft)
    place(svg, x, y, s, flip=(facing < 0))
    limb(svg, (-4.5, -40), (-7, -10), 9.5, 7.2, pants)
    limb(svg, (4.5, -40), (8, -10), 9.5, 7.2, pants)
    for bx in (-8, 9.2):
        svg.poly([(bx - 5, -12), (bx + 4, -12), (bx + 6, -1), (bx + 8, 0), (bx - 6, 0), (bx - 6, -4)], "#2b2018", INK, 0.9)
    # rucksack behind
    svg.poly([(-17, -72), (-9, -77), (-9, -46), (-17, -50)], svg.lin([(0, "#8d7650"), (1, "#4a3a26")], 0, 0, 1, 0), INK, 0.9)
    svg.path("M-17,-70 Q-21,-60 -17,-52", stroke="#c4b38a", sw=2.0)  # bedroll
    g = svg.lin([(0, shade(smock, 1.3)), (0.5, smock), (1, shade(smock, 0.62))], 0, 0, 1, 0.2)
    svg.poly([(-12.5, -77), (12.5, -77), (12, -48), (14, -34), (-14, -34), (-12, -48)], g, INK, 1.0)
    svg.poly([(-3, -77), (3, -77), (2, -36), (-2, -36)], shade(smock, 0.7), op=0.5)
    svg.poly([(11, -76), (7.5, -77), (-12, -48), (-9, -45)], "#4a3a28", INK, 0.5, op=0.95)  # shoulder strap
    svg.rect(-13, -46, 26, 3.4, "#2c2016", stroke=INK, sw=0.7)
    svg.rect(-9.5, -44, 6.5, 9, "#6a5236", INK, 0.6)  # tool pouch
    limb(svg, (-13, -73), (-17, -58), 8, 6.8, smock)
    limb(svg, (-17, -58), (-13.5, -45), 6.8, 6.0, smock)
    svg.circle(-13.3, -43.5, 3.0, SKIN[0], INK, 0.7)
    # pickaxe resting on the right shoulder
    limb(svg, (13, -73), (20, -66), 8, 6.8, smock)
    limb(svg, (20, -66), (14, -58), 6.8, 6.0, smock)
    svg.line((6, -38), (26, -112), "#6a4a2c", 3.0)
    svg.line((6, -38), (26, -112), "#a27c4c", 1.0, 0.7)
    if tool == "shovel":
        # a spade: flat blade at the top of the handle, with a cross grip
        svg.poly([(20, -110), (34, -116), (39, -100), (29, -94), (22, -98)], svg.lin([(0, "#b3bbc0"), (1, "#6f777c")], 0, 0, 1, 1), INK, 0.8)
        svg.line((20, -114), (30, -118), "#6a4a2c", 2.6)
    else:
        svg.path("M14,-114 Q26,-120 38,-110 L36,-107 Q26,-113 16,-109 Z", "#8c949a", INK, 0.8)
    svg.circle(14.4, -58.5, 3.0, SKIN[0], INK, 0.7)
    svg.rect(-2.5, -81, 5, 5, shade(SKIN[1], 0.9))
    g = svg.rad([(0, SKIN[0]), (1, SKIN[1])], 0.35, 0.3, 0.8)
    svg.ellipse(0, -87, 6.1, 7.0, g, INK, 0.8)
    svg.ellipse(-1.9, -86.4, 0.9, 1.1, "#3a2418")
    svg.ellipse(2.2, -86.4, 0.9, 1.1, "#3a2418")
    svg.rect(-3.1, -84.2, 6.2, 1.5, "#3b2a1d", rx=0.5, op=0.8)
    svg.ellipse(0, -92.5, 12.4, 3.6, shade(hat, 0.9), INK, 0.8)
    svg.poly([(-6.2, -92), (6.2, -92), (5.2, -101), (-5.2, -101)], svg.lin([(0, shade(hat, 1.3)), (1, shade(hat, 0.7))], 0, 0, 1, 0), INK, 0.8)
    svg.ellipse(0, -101, 5.3, 1.6, shade(hat, 1.2), INK, 0.6)
    svg.rect(-6.2, -95.6, 12.4, 2.2, "#2a2018", stroke=INK, sw=0.4)
    svg.end()


def horse(svg, x, y, s, coat="#7a4524", facing=1):
    soft = svg.blur(3.0)
    shadow(svg, x, y, 44 * s, 7 * s, soft, 0.5)
    place(svg, x, y, s, flip=(facing < 0))
    dark = shade(coat, 0.55)
    g = svg.lin([(0, shade(coat, 1.35)), (0.5, coat), (1, shade(coat, 0.6))], 0, 0, 0.7, 1)
    # far legs first (darker), near legs after the body
    for a, k, h in (((-20, -42), (-22, -22), (-20, -2)), ((34, -42), (44, -22), (46, -4))):
        limb(svg, a, k, 10, 7, dark, hi=1.1, lo=0.7)
        limb(svg, k, h, 7, 5.4, dark, hi=1.1, lo=0.7)
        svg.poly([(h[0] - 3.6, h[1] - 2), (h[0] + 3.6, h[1] - 2), (h[0] + 4.4, h[1] + 3), (h[0] - 4.4, h[1] + 3)], "#17120e", INK, 0.6)
    tail = "M-38,-58 C-52,-52 -56,-34 -50,-16 C-46,-26 -46,-40 -36,-48 Z"
    svg.path(tail, svg.lin([(0, "#3a2418"), (1, "#16100b")], 0, 0, 1, 1), INK, 0.8)
    body = (
        "M-38,-58 C-28,-70 -2,-68 20,-70 C28,-72 34,-84 40,-96 C44,-102 52,-100 56,-94 "
        "C62,-86 66,-78 64,-72 C60,-67 54,-69 50,-73 C46,-69 42,-63 40,-57 C41,-49 38,-44 34,-40 "
        "C12,-33 -14,-35 -28,-38 C-38,-41 -44,-48 -43,-55 C-42,-58 -40,-58 -38,-58 Z"
    )
    svg.path(body, g, INK, 1.1)
    # belly shading, shoulder and haunch highlights
    svg.path("M-30,-40 C-10,-34 14,-34 34,-42 L34,-40 C14,-31 -12,-31 -30,-37 Z", dark, op=0.45)
    svg.ellipse(-24, -56, 11, 8, shade(coat, 1.25), op=0.35, rot=-15)
    svg.ellipse(24, -58, 9, 9, shade(coat, 1.2), op=0.3)
    # mane, ear, eye, blaze
    svg.path("M20,-70 C28,-72 34,-84 40,-96 C42,-90 36,-76 28,-68 Z", "#2a1a10", INK, 0.7)
    svg.path("M46,-100 L48,-108 L52,-100 Z", shade(coat, 0.8), INK, 0.7)
    svg.circle(54, -88, 1.3, "#0f0b08")
    svg.path("M56,-94 C60,-88 62,-82 61,-77", stroke="#e8dcc0", sw=1.5, op=0.7)
    svg.ellipse(62, -73, 3.2, 2.6, shade(coat, 0.7), op=0.9)
    # near legs
    for a, k, h in (((-28, -44), (-39, -24), (-35, -2)), ((30, -44), (35, -22), (31, -2))):
        limb(svg, a, k, 12, 7.6, coat)
        limb(svg, k, h, 7.6, 5.8, coat)
        svg.poly([(h[0] - 4.0, h[1] - 2), (h[0] + 4.0, h[1] - 2), (h[0] + 5.0, h[1] + 3), (h[0] - 5.0, h[1] + 3)], "#1a140f", INK, 0.7)
        svg.rect(h[0] - 3.2, h[1] - 6, 6.4, 3.2, "#e6ddc6", stroke=INK, sw=0.4, op=0.85)
    # saddle blanket and saddle
    svg.poly([(-14, -68), (14, -70), (16, -50), (-14, -48)], "#7a2f28", INK, 0.8)
    svg.poly([(-14, -50), (16, -52), (16, -48), (-14, -46)], BRASS, op=0.9)
    svg.path("M-16,-70 C-8,-79 10,-79 17,-71 L15,-65 C8,-70 -8,-70 -15,-64 Z", "#2a1d14", INK, 0.8)
    svg.end()


def trooper(svg, x, y, s, facing=1):
    """Cavalry: mounted dragoon with a raised sabre and a guidon."""
    horse(svg, x, y, s, facing=facing)
    place(svg, x, y, s, flip=(facing < 0))
    coat, pants = NAVY, "#d8d2c0"
    limb(svg, (-4, -70), (8, -60), 11, 8.5, pants)  # thigh
    limb(svg, (8, -60), (7, -40), 8.5, 6.5, pants)  # calf
    svg.poly([(2, -42), (13, -42), (14, -36), (3, -35)], "#1a140f", INK, 0.7)  # boot/stirrup
    g = svg.lin([(0, shade(coat, 1.35)), (0.5, coat), (1, shade(coat, 0.62))], 0, 0, 1, 0.2)
    svg.poly([(-11, -102), (9, -104), (9, -70), (-11, -69)], g, INK, 1.0)
    svg.poly([(9, -102), (3, -103), (-11, -78), (-6, -75)], CREAM, shade(CREAM, 0.5), 0.5, op=0.95)
    for k in range(4):
        svg.circle(-1, -98 + k * 6.5, 1.0, BRASS)
    svg.rect(-11, -73, 20, 3, "#201a14", stroke=INK, sw=0.6)
    svg.ellipse(-10, -102, 4.2, 2.2, shade(BRASS, 0.9), INK, 0.6)
    limb(svg, (-9, -100), (-14, -86), 7.5, 6.5, coat)
    limb(svg, (-14, -86), (-9, -78), 6.5, 5.8, coat)
    svg.circle(-8.4, -76.5, 2.8, SKIN[0], INK, 0.7)
    limb(svg, (8, -100), (18, -108), 7.5, 6.5, coat)
    limb(svg, (18, -108), (24, -120), 6.5, 5.8, coat)
    svg.circle(24.6, -121.5, 2.8, SKIN[0], INK, 0.7)
    # sabre
    svg.line((24.6, -122), (36, -146), "#d7dde0", 2.4)
    svg.line((24.6, -122), (36, -146), INK, 0.5, 0.6)
    svg.line((22, -121), (28, -124), BRASS, 2.2)
    svg.rect(-2.5, -108, 5, 5, shade(SKIN[1], 0.9))
    head(svg, 0, -114, 6.3, coat, RED, plume="#e9e2cf")
    svg.end()


def artillery(svg, x, y, s, facing=1):
    soft = svg.blur(3.0)
    shadow(svg, x, y, 52 * s, 8 * s, soft, 0.5)
    place(svg, x, y, s, flip=(facing < 0))
    wood, iron, bronze = "#7c5530", "#2c2a28", "#8d6a30"
    # trail beam running back to the ground
    svg.poly([(-6, -26), (4, -30), (-54, -2), (-60, -6)], svg.lin([(0, shade(wood, 1.2)), (1, shade(wood, 0.6))], 0, 0, 1, 1), INK, 0.9)
    svg.ellipse(-56, -2, 5, 2.4, "#241c14", INK, 0.7)
    # far wheel
    svg.ellipse(18, -26, 15, 24, "#2a2a28", INK, 1.2, rot=0)
    svg.ellipse(18, -26, 11.5, 20.5, "#4b3a25", INK, 0.8)
    # barrel tube, breech and muzzle swell
    tube = svg.lin([(0, shade(bronze, 1.35)), (0.5, bronze), (1, shade(bronze, 0.5))], 0, 0, 0, 1)
    svg.poly([(-12, -46), (42, -66), (44, -57), (-10, -36)], tube, INK, 1.0)
    svg.poly([(36, -69), (50, -73), (52, -62), (38, -58)], tube, INK, 1.0)
    svg.ellipse(50.6, -67.5, 2.6, 5.8, "#1b1a18", INK, 0.7, rot=-14)
    svg.ellipse(-12, -41, 5.5, 6.6, shade(bronze, 0.95), INK, 0.9, rot=-14)
    svg.circle(-17, -43.6, 2.2, shade(bronze, 0.8), INK, 0.6)
    for t in (0.28, 0.55):
        px, py = -12 + 54 * t, -46 - 20 * t
        svg.line((px, py - 1.5), (px + 2.2, py + 8.0), shade(bronze, 0.55), 2.0, 0.9)
    # carriage cheek and axle
    svg.poly([(-8, -44), (24, -52), (26, -30), (-6, -30)], svg.lin([(0, shade(wood, 1.3)), (1, shade(wood, 0.6))], 0, 0, 1, 0.5), INK, 1.0)
    # near wheel with spokes
    cx, cy, rx, ry = -2, -24, 14.5, 24
    svg.ellipse(cx, cy, rx + 3.2, ry + 3.4, "#1c1b19", INK, 1.2)
    svg.ellipse(cx, cy, rx, ry, svg.rad([(0, "#7a5632"), (1, "#4a3220")], 0.4, 0.35, 0.8), INK, 0.8)
    for k in range(10):
        a = k / 10 * math.tau
        svg.line((cx, cy), (cx + math.cos(a) * rx * 0.9, cy + math.sin(a) * ry * 0.9), "#2e2013", 1.7)
        svg.line((cx - 0.7, cy - 0.7), (cx + math.cos(a) * rx * 0.9 - 0.7, cy + math.sin(a) * ry * 0.9 - 0.7), "#b78e55", 0.6, 0.6)
    svg.ellipse(cx, cy, 5, 7.2, "#2b2927", INK, 0.8)
    svg.circle(cx, cy, 2.0, "#a7a095")
    svg.end()
    soldier(svg, x - 40 * s, y + 9 * s, s * 0.86, rifle=False, facing=1)


def _quad(svg, pts, fill, stroke=None, sw=0.8):
    svg.poly(pts, fill, stroke, sw)


def ironclad(seed=2) -> Svg:
    """Steam ironclad warship, bow to the upper right, long dark hull facing the viewer."""
    svg = Svg(256, 192)
    rng = random.Random(seed)
    soft = svg.blur(3.2)
    # ---- water: wake behind, foam along the hull, bow wave
    svg.ellipse(120, 152, 104, 17, "#0a2a35", op=0.28, blur=svg.blur(6))
    svg.poly([(24, 150), (86, 126), (92, 138), (58, 158), (6, 168)], "#e9f4ef", op=0.38, extra=f'filter="{svg.blur(2.2)}"')
    for i in range(6):
        t = i / 5
        svg.path(f"M{74 - 52 * t:.1f},{136 + 22 * t:.1f} q{-10 - 6 * t:.1f},{2 + 6 * t:.1f} {-22:.1f},{3 + 2 * t:.1f}", stroke="#e8f4f0", sw=1.4, op=0.55 * (1 - 0.6 * t))
    svg.poly([(70, 142), (200, 98), (212, 106), (206, 112), (86, 154)], "#eaf6f1", op=0.5, extra=f'filter="{svg.blur(1.6)}"')

    # ---- hull: deck outline (x right-down, y left-down in iso); long axis runs lower-left -> upper-right
    def P(a, b, z=0):  # a along the ship (0 = stern), b across (0 = port side), z up
        # ship axis direction (up-right) = (cos, -sin) and beam direction = (cos, +sin) (down-right)
        ax = (0.906, -0.423)
        bx = (0.906, 0.423)
        return (40 + ax[0] * a + bx[0] * b, 150 + ax[1] * a + bx[1] * b - z)

    L, B, hh = 190, 34, 15
    deck = lambda z: [P(0, 0, z), P(L - 38, 0, z), P(L, B / 2, z), P(L - 38, B, z), P(0, B, z)]
    waterline = lambda: [P(6, 3, 0), P(L - 40, 3, 0), P(L - 4, B / 2, 0), P(L - 40, B - 3, 0), P(6, B - 3, 0)]
    # starboard hull side (faces the viewer, dark gunmetal), plus bow facet and stern face
    side = [P(0, B, 0), P(L - 38, B, 0), P(L - 38, B, hh), P(0, B, hh)]
    svg.poly(side, svg.lin([(0, "#3c4448"), (0.5, "#262c30"), (1, "#14191c")], 0, 0, 1, 0.3), "#0b0e10", 1.2)
    bow = [P(L - 38, B, 0), P(L, B / 2, 0), P(L, B / 2, hh + 2), P(L - 38, B, hh)]
    svg.poly(bow, svg.lin([(0, "#2a3034"), (1, "#0f1417")], 0, 0, 1, 0), "#0b0e10", 1.2)
    svg.poly([P(0, B, hh * 0.42), P(L - 38, B, hh * 0.42), P(L - 38, B, hh * 0.44), P(0, B, hh * 0.44)], "#6b2a22", op=0.9)  # red boot-topping
    svg.poly([P(0, B, 0), P(L - 38, B, 0), P(L - 38, B, 3.4), P(0, B, 3.4)], "#5a2a22", op=0.95)
    for k in range(1, 9):  # armour plate seams
        a = k * (L - 38) / 9
        svg.line(P(a, B, 1), P(a, B, hh - 0.5), "#05080a", 0.7, 0.5)
    for k in range(6):  # gunports
        a = 24 + k * 24
        q = [P(a, B, 7), P(a + 6, B, 7), P(a + 6, B, 11), P(a, B, 11)]
        svg.poly(q, "#07090b")
    # deck planking and bulwark rail
    svg.poly(deck(hh), svg.lin([(0, "#aaa795"), (1, "#7d7a6c")], 0, 0, 1, 1), "#14181a", 1.0)
    # surf: foam hugging the waterline and a bow splash
    svg.poly([P(-4, B + 0.5, 0), P(L - 38, B + 0.5, 0), P(L - 3, B / 2 + 1, 0), P(L - 30, B + 6, -1.5), P(-10, B + 6, -1.5)], "#f0f8f3", op=0.62, extra=f'filter="{svg.blur(1.0)}"')
    for k in range(8):
        bx, by = P(L - 2 + rng.uniform(-6, 5), B / 2 + rng.uniform(-3, 7), rng.uniform(0, 3))
        svg.ellipse(bx, by, rng.uniform(3, 6), rng.uniform(1.6, 3), "#f6fbf8", op=0.8, blur=svg.blur(0.8))
    svg.poly([P(6, 5, hh + 0.4), P(L - 44, 5, hh + 0.4), P(L - 12, B / 2, hh + 0.4), P(L - 44, B - 5, hh + 0.4), P(6, B - 5, hh + 0.4)], "#9c9686", op=0.55)
    for k in range(1, 14):
        a = k * (L - 40) / 14
        svg.line(P(a, 4, hh + 0.5), P(a, B - 4, hh + 0.5), "#6d6a5e", 0.5, 0.5)
    svg.line(P(0, B, hh + 4), P(L - 38, B, hh + 4), "#2a2f31", 1.0, 0.9)
    for k in range(0, 14):
        a = k * (L - 38) / 13
        svg.line(P(a, B, hh), P(a, B, hh + 4.5), "#2a2f31", 0.8)

    # ---- superstructure, turrets, funnels (drawn stern to bow so nearer parts overlap)
    def box(a0, a1, b0, b1, z0, z1, wall="#a8a698"):
        # right/starboard face (shaded) and top
        svg.poly([P(a0, b1, z0), P(a1, b1, z0), P(a1, b1, z1), P(a0, b1, z1)], svg.lin([(0, shade(wall, 0.75)), (1, shade(wall, 0.5))], 0, 0, 0, 1), "#15191b", 0.9)
        svg.poly([P(a1, b1, z0), P(a1, b0, z0), P(a1, b0, z1), P(a1, b1, z1)], shade(wall, 0.45), "#15191b", 0.9)
        svg.poly([P(a0, b0, z1), P(a1, b0, z1), P(a1, b1, z1), P(a0, b1, z1)], svg.lin([(0, shade(wall, 1.25)), (1, shade(wall, 0.95))], 0, 0, 1, 1), "#15191b", 0.9)

    def funnel(a, z0, hgt, w=9.0):
        b = B / 2
        cx, cy = P(a, b, z0)
        top = P(a, b, z0 + hgt)
        g = svg.lin([(0, "#2c3236"), (0.45, "#14181b"), (1, "#050607")], 0, 0, 1, 0)
        svg.poly([(cx - w / 2, cy), (cx + w / 2, cy), (top[0] + w / 2, top[1]), (top[0] - w / 2, top[1])], g, "#030405", 0.9)
        svg.ellipse(cx, cy, w / 2, w * 0.2, "#050607")
        svg.poly([(cx - w / 2 - 0.4, cy - hgt * 0.62), (cx + w / 2 + 0.4, cy - hgt * 0.62), (cx + w / 2 + 0.4, cy - hgt * 0.62 - 3), (cx - w / 2 - 0.4, cy - hgt * 0.62 - 3)], "#a8322a", op=0.95)
        svg.ellipse(top[0], top[1], w / 2, w * 0.22, "#0a0c0d", "#2b3236", 0.8)
        svg.ellipse(top[0], top[1], w * 0.36, w * 0.14, "#000", op=0.9)
        return top

    box(10, 40, 8, B - 8, hh, hh + 7, "#a09d8d")  # stern cabin
    box(54, 112, 7, B - 7, hh, hh + 9, "#aaa798")  # central citadel
    tops = []
    for a in (62, 88):
        tops.append(funnel(a, hh + 9, 38))
    box(112, 134, 9, B - 9, hh, hh + 10, "#9f9c8d")  # bridge house
    box(116, 128, 12, B - 12, hh + 10, hh + 17, "#b3b0a1")  # bridge
    for a, b in ((44, B / 2), (150, B / 2)):  # turrets with long barrels
        cx, cy = P(a, b, hh)
        svg.ellipse(cx, cy, 11, 6.4, "#15191b", op=0.4, blur=svg.blur(1.4))
        svg.poly([(cx - 10, cy), (cx + 10, cy), (cx + 10, cy - 6), (cx - 10, cy - 6)], svg.lin([(0, "#56605f"), (1, "#252b2c")], 0, 0, 1, 0), "#0c1011", 0.9)
        svg.ellipse(cx, cy - 6, 10, 5.6, svg.lin([(0, "#8a948f"), (1, "#4b5453")], 0, 0, 1, 1), "#0c1011", 0.9)
        e = P(a + 24, b, hh + 4.5) if a > 100 else P(a - 24, b, hh + 4.5)
        svg.line((cx + 2, cy - 5.2), e, "#12171a", 3.2)
        svg.line((cx + 2, cy - 5.2), e, "#6b7473", 1.0, 0.6)
    # masts, rigging, ensign
    for a in (14, 100, 140):
        b0 = P(a, B / 2, hh)
        tp = P(a, B / 2, hh + 62 - (a == 140) * 10)
        svg.line(b0, tp, "#1b1612", 2.2)
        svg.line((tp[0] - 8, tp[1] + 8), (tp[0] + 8, tp[1] + 8), "#1b1612", 1.4)
    svg.line(P(14, B / 2, hh + 62), P(100, B / 2, hh + 62), "#2a2520", 0.6, 0.7)
    svg.line(P(100, B / 2, hh + 62), P(140, B / 2, hh + 52), "#2a2520", 0.6, 0.7)
    stern_top = P(0, B / 2, hh + 24)
    flag(svg, stern_top[0] - 2, stern_top[1] - 6, w=34, h=22, wave=2.6, pole_h=34)
    # smoke drifting back (down-left) and up
    for i, t in enumerate(tops):
        puffs(svg, (t[0], t[1] - 2), seed=20 + i, n=7, drift=(-9, -9), size=8.5, op=0.85, dark=True)
    return svg


# ---- sprite compositions ----------------------------------------------------------------------
def build_infantry() -> Svg:
    svg = Svg(U, U)
    soldier(svg, 50, 103, 0.80, trousers="#4f5b6d", facing=1)
    soldier(svg, 92, 116, 0.98, facing=1)
    return svg


def build_pioneer() -> Svg:
    svg = Svg(U, U)
    worker(svg, 52, 102, 0.80, tool="pick")
    worker(svg, 94, 116, 0.98, tool="pick")
    return svg


def build_worker() -> Svg:
    """A single labourer with a spade, in blue-grey work clothes so he is not mistaken for a pioneer."""
    svg = Svg(U, U)
    worker(svg, 80, 116, 1.05, tool="shovel", smock="#4f6a82", pants="#4a4036", hat="#7a6a4a")
    return svg


def build_cavalry() -> Svg:
    svg = Svg(U, U)
    trooper(svg, 76, 116, 0.88)
    return svg


def build_artillery() -> Svg:
    svg = Svg(U, U)
    artillery(svg, 82, 116, 0.92)
    return svg
