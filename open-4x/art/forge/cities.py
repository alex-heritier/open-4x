"""The city skins: one 256 x 224 isometric town per `Flavor`, drawn on the same 8 x 8 ground diamond.

Every skin shares the camera, the earth disc and the light of `buildings.build_city_svg`, so they
swap freely on a tile. What differs is the architecture: the silhouette a player reads at a glance
(domes and minarets, curved eaves, stupas, round huts, yurts, tipis, stilt houses) and its material.
Skins carry no flag or owner mark: the client tints the nation's plate over them.
"""

import math
import random

from .buildings import building, chimney, puffs, rail, roof
from .buildings import pagoda
from .kit import (
    K, arc_points, awning, canoe, cone, cylinder, dome, earth, finial, horse, minaret, onion, palm, path_strip, post,
    rack, radii, stilts, tree, tuft,
)
from .svgkit import Iso, Svg, lerp, shade

W, H = 256, 224
CX, CY = 128, 160

PLASTER = "#c9b58d"
CONCRETE = "#a7a396"
BRICK = "#a8693f"
OCHRE_WALL = "#bb8f56"
GREY_STUCCO = "#9b988a"
ROOF_TILE = "#a1401f"
ROOF_BROWN = "#6f4230"
ROOF_SLATE = "#454f56"
ROOF_ORANGE = "#b5582c"
TERRACOTTA = "#b8562e"


class Scene:
    """A town under construction: things are queued with a depth and painted far to near."""

    def __init__(self, seed):
        self.svg = Svg(W, H)
        self.iso = Iso(CX, CY - 64, K)  # an 8 x 8 grid exactly fills the 256 x 128 ground diamond
        self.rng = random.Random(seed)
        self.items = []

    def add(self, depth, draw):
        self.items.append((depth, len(self.items), draw))

    def house(self, x, y, dx, dy, h, wall, kind, col, rh=10, **kw):
        svg, iso, rng = self.svg, self.iso, self.rng
        self.add(x + dx / 2 + y + dy / 2, lambda: building(svg, iso, rng, x, y, dx, dy, h, wall, kind, col, rh=rh, **kw))

    def tree(self, x, y, **kw):
        svg, iso = self.svg, self.iso
        self.add(x + y, lambda: tree(svg, iso, x, y, **kw))

    def palm(self, x, y, **kw):
        svg, iso = self.svg, self.iso
        self.add(x + y, lambda: palm(svg, iso, x, y, **kw))

    def render(self):
        for _, _, draw in sorted(self.items, key=lambda t: (t[0], t[1])):
            draw()
        return self.svg


def _ring(cx, cy, r, angles):
    return [(cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))) for a in angles]


# ---- western: brick and slate, chimneys, a clock tower ------------------------------------------
def western(seed=7):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng)
    rail(svg, iso, [(0.7, 4.4), (2.4, 4.7), (4.2, 5.4), (7.3, 6.9)])
    lots = [
        (0.9, 1.5, 1.7, 1.6, 20, OCHRE_WALL, "gable_x", ROOF_TILE, 9),
        (5.3, 0.9, 2.1, 1.9, 28, CONCRETE, "flat", "#85827a", 0),
        (2.8, 3.1, 1.7, 1.6, 30, GREY_STUCCO, "gable_y", ROOF_SLATE, 11),
        (0.7, 3.4, 1.5, 1.5, 16, PLASTER, "hip", ROOF_BROWN, 8),
        (4.7, 3.2, 1.7, 1.6, 24, BRICK, "hip", ROOF_SLATE, 10),
        (6.3, 3.1, 1.3, 1.5, 18, PLASTER, "gable_y", ROOF_ORANGE, 8),
        (1.0, 5.2, 1.7, 1.5, 18, GREY_STUCCO, "gable_x", ROOF_ORANGE, 8),
        (3.1, 5.0, 1.8, 1.7, 22, PLASTER, "gable_x", ROOF_TILE, 10),
        (5.2, 5.1, 1.7, 1.5, 17, BRICK, "gable_y", ROOF_BROWN, 8),
        (2.3, 6.8, 1.4, 0.9, 11, OCHRE_WALL, "gable_x", ROOF_BROWN, 6),
        (4.6, 6.8, 1.5, 0.9, 12, CONCRETE, "gable_x", ROOF_TILE, 6),
    ]
    for x, y, dx, dy, h, wall, kind, col, rh in lots:
        sc.house(x, y, dx, dy, h, wall, kind, col, rh=rh)
    for x, y, h in ((4.9, 0.55, 78), (7.35, 0.7, 64), (2.55, 2.55, 52)):
        sc.add(x + y - 1.5, lambda x=x, y=y, h=h: chimney(svg, iso, x, y, h, seed=int(x * 10)))

    def tower():
        building(svg, iso, rng, 3.3, 1.2, 1.4, 1.4, 40, "#b9b19c", "hip", "#4f8a72", rh=27, cols=1, rows=2, o=0.2, lit=0.3)
        face = lambda s, t: p(3.3 + s * 1.4, 2.6, t * 40)
        c = face(0.5, 0.64)
        svg.circle(c[0], c[1], 4.3, "#f1e8cf", stroke="#3a3126", sw=0.9)
        svg.line(c, (c[0], c[1] - 2.9), "#2a241c", 0.9)
        svg.line(c, (c[0] + 2.1, c[1] + 0.8), "#2a241c", 0.9)
        finial(svg, p(4.0, 1.9, 40 + 27), h=9, col="#c9a85c")

    sc.add(3.3 + 1.2 + 1.4, tower)
    return sc.render()


# ---- latin: whitewash and terracotta around a cathedral and a plaza --------------------------------
def latin(seed=11):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#6b5a40", mid="#a8946c", core="#c3b08a", stone="#bfae88")
    svg.poly(iso.ground(2.7, 3.0, 3.0, 2.9), "#dccda8", "#8a7a58", 0.8, op=0.92)
    for i in range(1, 6):  # paving
        t = i / 6
        svg.line(p(2.7 + 3.0 * t, 3.0, 0), p(2.7 + 3.0 * t, 5.9, 0), "#a8986f", 0.6, 0.35)
        svg.line(p(2.7, 3.0 + 2.9 * t, 0), p(5.7, 3.0 + 2.9 * t, 0), "#a8986f", 0.6, 0.35)

    def cathedral():
        building(svg, iso, rng, 3.2, 0.8, 2.6, 1.5, 24, "#ece1c6", "gable_x", TERRACOTTA, rh=8, cols=3, rows=1, arch=True, lit=0.1, tiles=8)
        cylinder(svg, iso, 4.5, 1.55, 0.8, 15, "#ece1c6", z0=26, shadow=False, grain=14, seed=3, bands=[(0.3, 0.62, "#6d655a")])
        c = p(4.5, 1.55, 41)
        top = dome(svg, c[0], c[1], radii(0.8)[0] * 1.12, 24, "#3a7d78", ribs=5)
        finial(svg, top, h=9, col="#d6b25e", cross=True)

    sc.add(4.5 + 1.55, cathedral)
    for tx in (3.0, 4.95):
        def bell(tx=tx):
            building(svg, iso, rng, tx, 1.45, 0.95, 0.95, 38, "#efe5cc", "hip", "#3a7d78", rh=13, cols=1, rows=3, arch=True, lit=0.25, o=0.16)
            finial(svg, p(tx + 0.475, 1.925, 51), h=7, col="#d6b25e", cross=True)

        sc.add(tx + 0.5 + 1.9 + 1.0, bell)
    for x, y, dx, dy, h, wall, kind, col, rh in (
        (0.8, 4.4, 1.8, 1.5, 17, "#e2a57e", "gable_x", TERRACOTTA, 8),
        (1.0, 2.6, 1.6, 1.4, 20, "#ead9a8", "hip", "#a5482a", 9),
        (6.0, 2.3, 1.5, 1.6, 16, "#dcaa60", "gable_y", TERRACOTTA, 8),
        (5.9, 4.4, 1.7, 1.5, 19, "#bdd0c2", "hip", TERRACOTTA, 9),
        (2.8, 6.0, 1.7, 1.3, 14, "#e6b9a2", "gable_x", "#a5482a", 7),
        (4.8, 6.0, 1.6, 1.3, 15, "#f0e5c3", "gable_x", TERRACOTTA, 7),
    ):
        sc.house(x, y, dx, dy, h, wall, kind, col, rh=rh, arch=True, lit=0.2, tiles=8)

    def fountain():
        cylinder(svg, iso, 4.2, 4.3, 0.62, 5, "#b8b19e", grain=8, seed=2, top=False)
        c = p(4.2, 4.3, 5)
        rx, ry = radii(0.62)
        svg.ellipse(c[0], c[1], rx, ry, "#b8b19e", stroke="#5f5a4c", sw=0.8)
        svg.ellipse(c[0], c[1], rx * 0.8, ry * 0.8, "#4aa6b0")
        svg.line((c[0], c[1] + 1), (c[0], c[1] - 10), "#9ad6dc", 1.8)
        svg.circle(c[0], c[1] - 11, 2.4, "#cfeff2", op=0.9)

    sc.add(8.5, fountain)
    sc.palm(0.7, 0.9, h=44, seed=1)
    sc.palm(7.3, 0.8, h=40, seed=2)
    return sc.render()


# ---- orthodox: white churches with gold onion domes, log houses, a bell tower --------------------------
def orthodox(seed=13):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#4f5a36", mid="#86905a", core="#a2a870", stone="#9a9a72")
    path_strip(svg, iso, [(0.5, 4.2), (2.6, 4.9), (4.6, 5.3), (7.5, 5.8)], "#a89a78")

    def church():
        building(svg, iso, rng, 2.9, 2.4, 2.8, 2.2, 22, "#f1ebde", "parapet", "#f1ebde", cols=3, rows=2, arch=True, lit=0.12)
        for dx, dy in ((0.6, 0.6), (2.2, 0.6), (0.6, 1.6), (2.2, 1.6)):
            c = p(2.9 + dx, 2.4 + dy, 25.4)
            cylinder(svg, iso, 2.9 + dx, 2.4 + dy, 0.28, 7, "#f1ebde", z0=25.4, shadow=False)
            onion(svg, c[0], c[1] - 7, 8, 13, "#2f7f8a" if (dx, dy) != (0.6, 0.6) else "#d9b13c")
        cylinder(svg, iso, 4.3, 3.5, 0.72, 13, "#f1ebde", z0=25.4, shadow=False, bands=[(0.35, 0.75, "#394a52")])
        c = p(4.3, 3.5, 38.4)
        onion(svg, c[0], c[1], 17, 29, "#d9b13c")

    sc.add(4.3 + 3.5 + 0.6, church)

    def bell_tower():
        building(svg, iso, rng, 6.0, 1.1, 1.0, 1.0, 36, "#f1ebde", "hip", "#3e7a62", rh=21, cols=1, rows=3, arch=True, lit=0.3, o=0.14)
        finial(svg, p(6.5, 1.6, 57), h=10, col="#d9b13c", cross=True)

    sc.add(6.5 + 1.6, bell_tower)
    for x, y, dx, dy, h, kind, col, rh in (
        (0.8, 3.0, 1.8, 1.5, 12, "gable_x", "#6c5b49", 7),
        (0.7, 5.3, 1.6, 1.4, 11, "gable_y", "#5a5a4f", 7),
        (5.9, 3.8, 1.6, 1.5, 12, "gable_y", "#6c5b49", 7),
        (5.5, 5.7, 1.8, 1.4, 11, "gable_x", "#5a5a4f", 7),
        (2.6, 6.3, 1.6, 1.3, 11, "gable_x", "#6c5b49", 6),
    ):
        sc.house(x, y, dx, dy, h, "#8a5d3a", kind, col, rh=rh, course=3.4, course_op=0.4, tiles=11, lit=0.3)
    sc.tree(0.6, 0.9, kind="birch", h=40, seed=1)
    sc.tree(7.5, 7.0, kind="birch", h=38, seed=2)
    sc.tree(1.0, 7.3, kind="birch", h=34, seed=3)
    sc.tree(7.4, 0.5, kind="pine", h=36, seed=4, leaf="#3d6b48")
    return sc.render()


# ---- arab: sand-coloured cubes, a blue-domed mosque, minarets, palms ------------------------------------
def arab(seed=17):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#8a7444", mid="#c5ae78", core="#dcc895", stone="#d3bf8a", spots=0.4)

    def mosque():
        building(svg, iso, rng, 2.8, 1.7, 2.7, 2.2, 18, "#f0e7d0", "parapet", "#f0e7d0", cols=3, rows=1, arch=True, lit=0.1)
        cylinder(svg, iso, 4.15, 2.8, 1.0, 9, "#f0e7d0", z0=21.4, shadow=False, bands=[(0.3, 0.7, "#5e7f86")])
        c = p(4.15, 2.8, 30.4)
        top = dome(svg, c[0], c[1], radii(1.0)[0] * 1.06, 27, "#2f9390", ribs=7)
        finial(svg, top, h=10, col="#d6b25e", crescent=True)
        for x, y in ((3.25, 3.35), (5.0, 3.4)):
            c2 = p(x, y, 21.4)
            dome(svg, c2[0], c2[1], 9, 8, "#d8b24e")

    sc.add(4.15 + 2.8, mosque)
    sc.add(2.5 + 1.5, lambda: minaret(svg, iso, 2.45, 1.45, 66, "#f0e7d0", "#2f9390"))
    sc.add(5.7 + 4.0, lambda: minaret(svg, iso, 5.75, 4.0, 60, "#f0e7d0", "#2f9390"))
    for x, y, dx, dy, h, wall in (
        (0.8, 3.0, 1.7, 1.5, 14, "#e1d0a4"),
        (0.9, 5.0, 1.8, 1.6, 18, "#d7be8c"),
        (3.1, 5.6, 1.7, 1.5, 12, "#efe7d0"),
        (5.3, 5.3, 1.8, 1.6, 16, "#dcc597"),
        (6.2, 1.9, 1.4, 1.4, 12, "#c9a56a"),
    ):
        sc.house(x, y, dx, dy, h, wall, "parapet", wall, arch=True, lit=0.14, cols=2)

    def small_dome():
        c = p(5.3 + 0.9, 5.3 + 0.8, 16 + 3.4)
        dome(svg, c[0], c[1], 14, 12, "#efe7d0")

    sc.add(5.3 + 0.9 + 5.3 + 0.8 + 0.1, small_dome)
    sc.add(4.3 + 4.7, lambda: awning(svg, iso, 3.8, 4.3, 1.3, 0.9, 12, cols=("#a3361f", "#efe3c4")))
    sc.palm(7.2, 0.7, h=48, seed=5)
    sc.palm(0.6, 1.0, h=44, seed=6)
    sc.palm(7.3, 7.1, h=42, seed=7)
    sc.palm(0.9, 7.2, h=38, seed=8)
    return sc.render()


# ---- east asian: red halls with sweeping grey eaves around a pagoda ---------------------------------------
def east_asian(seed=19):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#4f5a3a", mid="#7f8756", core="#aaa592", stone="#b7b2a1")
    svg.poly(iso.ground(1.4, 1.4, 5.4, 5.4), "#b6b19f", "#6c6a5c", 0.8, op=0.55)
    slate = "#4c5964"
    sc.add(4.0 + 2.0, lambda: pagoda(svg, iso, rng, 3.3, 0.9, 1.6, tiers=4, wall="#a23c2c", roof_col=slate, h0=15, step=0.84, rh=7))
    for x, y, dx, dy, h, wall, rh in (
        (0.7, 2.7, 2.2, 1.7, 13, "#a8402e", 12),
        (1.3, 5.2, 2.1, 1.6, 12, "#b8503a", 10),
        (5.9, 3.2, 1.7, 2.0, 13, "#a8402e", 12),
        (4.3, 5.9, 2.3, 1.4, 11, "#b8503a", 10),
    ):
        sc.house(x, y, dx, dy, h, wall, "hip_curved", slate, rh=rh, cols=3, lit=0.2, tiles=7, o=0.4)

    def lantern(x, y):
        a = p(x, y, 0)
        svg.rect(a[0] - 1.5, a[1] - 9, 3, 9, "#9a968a")
        svg.rect(a[0] - 3.4, a[1] - 13, 6.8, 5, "#b9b4a4", stroke="#5c594e", sw=0.5)
        svg.poly([(a[0] - 4.4, a[1] - 13), (a[0] + 4.4, a[1] - 13), (a[0], a[1] - 17)], "#7a766a")

    for x, y in ((3.4, 4.8), (5.2, 4.8)):
        sc.add(x + y, lambda x=x, y=y: lantern(x, y))
    sc.tree(0.7, 1.0, kind="pine", h=42, seed=1, leaf="#355f3a")
    sc.tree(7.2, 1.2, kind="pine", h=38, seed=2, leaf="#3b6a3a")
    sc.tree(7.3, 6.0, kind="round", h=34, seed=3, leaf="#c79a2e")
    sc.tree(0.6, 7.0, kind="pine", h=36, seed=4, leaf="#355f3a")
    return sc.render()


# ---- south asian: sandstone havelis with chhatris, a shikhara temple -----------------------------------------
def chhatri(svg, iso, x, y, z, col="#efe3c8", dome_col="#efe3c8"):
    a, b = iso.p(x - 0.18, y, z), iso.p(x + 0.18, y, z)
    for q in (a, b):
        svg.line(q, (q[0], q[1] - 9), shade(col, 0.55), 2.2)
        svg.line((q[0] - 0.4, q[1]), (q[0] - 0.4, q[1] - 9), col, 1.2)
    c = iso.p(x, y, z + 9)
    top = dome(svg, c[0], c[1], 8.5, 8, dome_col)
    finial(svg, top, h=4, col="#d6b25e")


def shikhara(svg, cx, cy, w, h, col, seed=0):
    d = (
        f"M{cx - w:.2f},{cy:.2f} C{cx - w * 1.02:.2f},{cy - h * 0.4:.2f} {cx - w * 0.6:.2f},{cy - h * 0.76:.2f} {cx - w * 0.14:.2f},{cy - h:.2f} "
        f"L{cx + w * 0.14:.2f},{cy - h:.2f} C{cx + w * 0.6:.2f},{cy - h * 0.76:.2f} {cx + w * 1.02:.2f},{cy - h * 0.4:.2f} {cx + w:.2f},{cy:.2f} A{w:.2f},{w / 2:.2f} 0 0 1 {cx - w:.2f},{cy:.2f} Z"
    )
    svg.path(d, svg.lin([(0, shade(col, 1.2)), (0.35, shade(col, 1.02)), (0.75, shade(col, 0.66)), (1, shade(col, 0.5))], 0, 0, 1, 0), stroke=shade(col, 0.4), sw=1.0)
    for i in range(1, 9):  # horizontal courses that follow the curve
        t = i / 9
        half = w * (1.0 - 0.86 * t ** 0.85) * (1.0 - 0.1 * math.sin(t * math.pi))
        y = cy - h * t
        svg.path(f"M{cx - half:.2f},{y:.2f} Q{cx:.2f},{y + half * 0.5:.2f} {cx + half:.2f},{y:.2f}", stroke=shade(col, 0.55), sw=0.9, op=0.6)
    for k in (-0.5, 0.0, 0.5):  # vertical ribs
        svg.path(f"M{cx + k * w * 0.9:.2f},{cy + w / 2 * (1 - k * k) ** 0.5 * 0.5:.2f} Q{cx + k * w * 0.8:.2f},{cy - h * 0.5:.2f} {cx + k * w * 0.1:.2f},{cy - h:.2f}", stroke=shade(col, 0.55), sw=0.8, op=0.45)
    svg.ellipse(cx, cy - h, w * 0.3, w * 0.15, shade(col, 0.9), stroke=shade(col, 0.4), sw=0.7)  # amalaka disc
    top = (cx, cy - h - 2)
    finial(svg, top, h=9, col="#d6b25e")
    svg.poly([(cx, top[1] - 9), (cx + 10, top[1] - 6.5), (cx, top[1] - 4)], "#e0762a", stroke="#8a3a12", sw=0.5)  # saffron pennant


def south_asian(seed=23):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#6b5836", mid="#b09a66", core="#cdb988", stone="#c4ae7a", spots=0.4)

    def haveli():
        building(svg, iso, rng, 2.0, 1.0, 3.0, 2.2, 24, "#c97a5a", "parapet", "#c97a5a", cols=4, rows=2, arch=True, lit=0.2)
        for cx_, cy_ in ((2.25, 1.25), (4.6, 1.25), (2.25, 2.85), (4.6, 2.85)):
            chhatri(svg, iso, cx_, cy_, 27.4)
        c = p(3.5, 2.1, 27.4)
        dome(svg, c[0], c[1], 17, 18, "#f1e8d2", ribs=5)

    sc.add(3.5 + 2.1 + 0.4, haveli)

    def temple():
        building(svg, iso, rng, 5.6, 3.3, 1.8, 1.8, 11, "#d9b88a", "parapet", "#d9b88a", cols=2, rows=1, arch=True, lit=0.05)
        c = p(6.5, 4.2, 14.4)
        shikhara(svg, c[0], c[1], 17, 44, "#dfba84")

    sc.add(6.5 + 4.2 + 0.2, temple)
    for x, y, dx, dy, h, wall in (
        (0.8, 4.0, 1.7, 1.6, 13, "#e0b044"),
        (1.0, 5.9, 1.8, 1.4, 12, "#d98a8a"),
        (3.5, 5.7, 1.8, 1.6, 15, "#ece2c8"),
        (6.0, 6.0, 1.5, 1.4, 11, "#c98a5a"),
    ):
        sc.house(x, y, dx, dy, h, wall, "parapet", wall, arch=True, lit=0.16, cols=2)
    sc.tree(7.1, 1.0, kind="round", h=46, seed=2, leaf="#4d7b30")
    sc.tree(0.7, 0.9, kind="round", h=38, seed=3, leaf="#5c8a34")
    return sc.render()


# ---- southeast asian: a gilded stupa, stilt houses with steep thatch, coconut palms -----------------------------
def stupa(svg, iso, x, y):
    p = iso.p
    z = 0.0
    for s, h in ((2.6, 4), (2.0, 4), (1.5, 4)):
        wall = "#ece3cb"
        px, py = x - s / 2, y - s / 2
        a = [p(px, py + s, z), p(px + s, py + s, z), p(px + s, py + s, z + h), p(px, py + s, z + h)]
        b = [p(px + s, py + s, z), p(px + s, py, z), p(px + s, py, z + h), p(px + s, py + s, z + h)]
        t = [p(px, py, z + h), p(px + s, py, z + h), p(px + s, py + s, z + h), p(px, py + s, z + h)]
        svg.poly(a, shade(wall, 1.05), shade(wall, 0.5), 0.8)
        svg.poly(b, shade(wall, 0.62), shade(wall, 0.5), 0.8)
        svg.poly(t, shade(wall, 1.15), shade(wall, 0.5), 0.8)
        z += h
    cx, cy = p(x, y, z)
    gold = "#d8a93a"
    bell = (
        f"M{cx - 15:.2f},{cy:.2f} C{cx - 15:.2f},{cy - 14:.2f} {cx - 10:.2f},{cy - 30:.2f} {cx - 4:.2f},{cy - 40:.2f} L{cx + 4:.2f},{cy - 40:.2f} "
        f"C{cx + 10:.2f},{cy - 30:.2f} {cx + 15:.2f},{cy - 14:.2f} {cx + 15:.2f},{cy:.2f} A15,7.5 0 0 1 {cx - 15:.2f},{cy:.2f} Z"
    )
    svg.path(bell, svg.lin([(0, shade(gold, 1.45)), (0.3, shade(gold, 1.08)), (0.75, shade(gold, 0.66)), (1, shade(gold, 0.5))], 0, 0, 1, 0), stroke=shade(gold, 0.4), sw=1.0)
    svg.ellipse(cx - 6, cy - 22, 2.4, 9, shade(gold, 1.8), op=0.5, rot=12, blur=svg.blur(0.8))
    y0 = cy - 40
    for i, (rw, rh) in enumerate(((7.5, 3), (6.2, 3), (5.0, 3), (4.0, 3))):  # stacked rings
        yy = y0 - i * 3.1
        svg.path(f"M{cx - rw:.2f},{yy:.2f} L{cx - rw:.2f},{yy - rh:.2f} A{rw:.2f},{rw / 2:.2f} 0 0 0 {cx + rw:.2f},{yy - rh:.2f} L{cx + rw:.2f},{yy:.2f} A{rw:.2f},{rw / 2:.2f} 0 0 1 {cx - rw:.2f},{yy:.2f} Z", svg.lin([(0, shade(gold, 1.3)), (1, shade(gold, 0.55))], 0, 0, 1, 0), stroke=shade(gold, 0.4), sw=0.6)
    yt = y0 - 4 * 3.1
    svg.poly([(cx - 3, yt), (cx + 3, yt), (cx + 0.6, yt - 17), (cx - 0.6, yt - 17)], gold, stroke=shade(gold, 0.45), sw=0.6)
    for k in range(3):
        svg.ellipse(cx, yt - 5 - k * 4.5, 4.2 - k * 0.9, 1.6, "#e6c45a", stroke=shade(gold, 0.45), sw=0.5)
    svg.line((cx, yt - 17), (cx, yt - 25), shade(gold, 0.55), 1.4)
    svg.circle(cx, yt - 26, 1.5, "#fff0b0")


def stilt_house(sc, x, y, dx, dy, lift, h, wall, roof_col, rh, roof_kind="gable_x", tiles=12):
    svg, iso, rng = sc.svg, sc.iso, sc.rng

    def draw():
        stilts(svg, iso, x, y, dx, dy, lift)
        a, b = iso.p(x + dx * 0.5, y + dy, 0), iso.p(x + dx * 0.5, y + dy + 0.5, lift)
        svg.line((a[0] - 2, a[1]), (b[0] - 2, b[1]), "#5b4026", 1.3)
        svg.line((a[0] + 2, a[1]), (b[0] + 2, b[1]), "#5b4026", 1.3)
        building(svg, iso, rng, x, y, dx, dy, h, wall, roof_kind, roof_col, rh=rh, z0=lift, door_on=False, course=3.2, course_op=0.3, tiles=tiles, lit=0.2, o=0.32)

    sc.add(x + dx / 2 + y + dy / 2, draw)


def southeast_asian(seed=29):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#4d4a2a", mid="#8a8650", core="#a29c62", stone="#9a9460", spots=0.4)
    for i in range(10):  # lush green patches
        q = p(rng.uniform(0.8, 7.2), rng.uniform(0.8, 7.2), 0)
        svg.ellipse(q[0], q[1], rng.uniform(6, 11), rng.uniform(2.6, 5), "#5d7a34", op=0.4)
    sc.add(3.8 + 2.7, lambda: stupa(svg, iso, 3.7, 2.7))
    teak, thatch, dark = "#8a5a35", "#b39650", "#6a4630"
    stilt_house(sc, 0.9, 3.6, 1.5, 1.4, 11, 10, teak, thatch, 10)
    stilt_house(sc, 1.4, 5.5, 1.5, 1.4, 11, 10, "#a06a3c", dark, 10, tiles=9)
    stilt_house(sc, 5.9, 4.3, 1.5, 1.4, 11, 10, teak, thatch, 10)
    stilt_house(sc, 5.2, 6.0, 1.5, 1.3, 11, 10, "#a06a3c", thatch, 10)
    stilt_house(sc, 3.2, 6.2, 1.4, 1.3, 11, 9, teak, dark, 10, tiles=9)

    def hall():
        building(svg, iso, rng, 5.2, 1.0, 2.0, 1.6, 12, "#8a3a2a", "gable_x", "#b3562a", rh=10, cols=3, rows=1, lit=0.12, o=0.35, tiles=10)
        roof(svg, iso, 5.5, 1.25, 1.4, 1.1, 12 + 6.5, "gable_x", "#c8782e", rh=7, o=0.25, tiles=8)
        finial(svg, p(5.5 + 0.7, 1.8, 12 + 6.5 + 7), h=7, col="#d6b25e")

    sc.add(6.2 + 1.8, hall)
    sc.palm(0.7, 0.9, h=50, seed=1)
    sc.palm(7.3, 6.9, h=46, seed=2)
    sc.palm(4.9, 7.4, h=42, seed=3)
    return sc.render()


# ---- african: a palisaded ring of round mud huts under thatch ------------------------------------------------------
def round_hut(sc, x, y, r, wall_h, wall, thatch, roof_h, seed=0, band="#3a2a20", door=True):
    svg, iso = sc.svg, sc.iso

    def draw():
        rx, ry = radii(r)
        cylinder(svg, iso, x, y, r, wall_h, wall, bands=[(0.46, 0.64, band)] if band else (), grain=26, seed=seed, top=False)
        c = iso.p(x, y, wall_h)
        if door:
            bx, by = iso.p(x, y, 0)
            svg.path(f"M{bx - 3.6:.2f},{by + ry * 0.96:.2f} L{bx - 3.6:.2f},{by + ry * 0.96 - 8:.2f} Q{bx:.2f},{by + ry * 0.96 - 12:.2f} {bx + 3.6:.2f},{by + ry * 0.96 - 8:.2f} L{bx + 3.6:.2f},{by + ry * 0.96:.2f} Z", "#2a1c14")
        top = cone(svg, c[0], c[1], rx * 1.22, roof_h, thatch, ridges=16, seed=seed)
        svg.line(top, (top[0], top[1] - 4), shade(thatch, 0.6), 1.6)

    sc.add(x + y, draw)


def african(seed=31):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    earth(svg, iso, rng, rim="#6a4a2c", mid="#a87a50", core="#c39a68", stone="#b88c60", n=40, spots=0.4)
    for i in range(14):
        tuft(svg, iso, rng.uniform(0.6, 7.4), rng.uniform(0.6, 7.4), "#8a8a3a")
    ring = arc_points(4, 4, 3.55, 45 + 16, 45 + 360 - 16, 44)
    for x, y in ring:
        sc.add(x + y + 0.05, lambda x=x, y=y: post(svg, iso, x, y, h=11, col="#7a5a3a", w=1.6))
    walls = ["#b5764c", "#a86a44", "#c48a5a", "#b07048"]
    thatches = ["#bd9d52", "#a8843f", "#c7a85e"]
    angles = (85, 130, 175, 220, 265, 310, 355)
    for i, (x, y) in enumerate(_ring(4, 4, 2.55, angles)):
        round_hut(sc, x, y, 0.82, 10 + (i % 3), walls[i % 4], thatches[i % 3], 17 + (i % 2) * 2, seed=i)
    round_hut(sc, 4.0, 4.0, 1.05, 12, "#c8905e", "#cdb061", 24, seed=9, band="#4a2f20")
    sc.tree(6.5, 1.0, kind="baobab", h=52, seed=1, leaf="#5a7a2c")
    sc.tree(1.1, 6.5, kind="acacia", h=44, seed=2, leaf="#6a7f34")
    return sc.render()


# ---- steppe: felt yurts, the khan's banner, horses ----------------------------------------------------------------------
def yurt(sc, x, y, r, wall_h, felt, band, seed=0, finial_gold=False):
    svg, iso = sc.svg, sc.iso

    def draw():
        rx, ry = radii(r)
        cylinder(svg, iso, x, y, r, wall_h, felt, bands=[(0.8, 1.0, band)], grain=14, seed=seed, top=False)
        bx, by = iso.p(x, y, 0)
        door_y = by + ry * 0.97
        svg.path(f"M{bx - 4.4:.2f},{door_y:.2f} L{bx - 4.4:.2f},{door_y - 9:.2f} Q{bx:.2f},{door_y - 12.5:.2f} {bx + 4.4:.2f},{door_y - 9:.2f} L{bx + 4.4:.2f},{door_y:.2f} Z", "#8a3a22", stroke="#d9a441", sw=1.0)
        svg.line((bx, door_y), (bx, door_y - 10), "#3a1c10", 0.8)
        c = iso.p(x, y, wall_h)
        apex = cone(svg, c[0], c[1], rx * 1.06, rx * 0.62, shade(felt, 1.02), ridges=18, seed=seed, eave=True)
        svg.ellipse(apex[0], apex[1] + 0.6, rx * 0.2, rx * 0.1, "#5a4a3a", stroke="#2a2018", sw=0.6)
        if finial_gold:
            finial(svg, apex, h=9, col="#d6b25e")

    sc.add(x + y, draw)


def steppe(seed=37):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#6a5a30", mid="#a39a58", core="#bfb472", stone="#b0a468", n=30, spots=0.3)
    for i in range(18):
        tuft(svg, iso, rng.uniform(0.6, 7.4), rng.uniform(0.6, 7.4), "#9a9a40")
    felt = ["#ece4cf", "#e4dcc4", "#f0e9d6"]
    for i, (x, y) in enumerate(_ring(4, 4, 2.75, (95, 140, 185, 230, 275, 320, 5))):
        yurt(sc, x, y, 0.86, 12, felt[i % 3], "#a2432c", seed=i)
    yurt(sc, 4.0, 4.0, 1.25, 14, "#f3ecd8", "#b8832a", seed=9, finial_gold=True)

    def standard():
        a = p(7.0, 5.2, 0)
        svg.line(a, (a[0], a[1] - 56), "#4a3626", 1.8)
        svg.circle(a[0], a[1] - 57, 2.6, "#d6b25e", stroke="#7a5a1e", sw=0.6)
        for k, c in enumerate(("#1d1a16", "#efe8d6", "#1d1a16", "#efe8d6", "#1d1a16")):
            ang = math.radians(200 + k * 35)
            svg.line((a[0], a[1] - 54), (a[0] + math.cos(ang) * 11, a[1] - 54 + math.sin(ang) * 3 + 14), c, 2.2, 0.95)

    sc.add(12.4, standard)
    for hx, hy, f, col in ((176, 192, 1, "#7a4e2c"), (192, 186, -1, "#c9c0a4"), (150, 207, 1, "#3e2a1c")):
        sc.add(15.5, lambda hx=hx, hy=hy, f=f, col=col: horse(svg, hx, hy, 1.15, col, f))
    sc.add(13.0, lambda: puffs(svg, p(4.0, 4.0, 22), 3, n=5, drift=(4, -9), size=5.5, op=0.6))
    return sc.render()


# ---- native: a ring of tipis around a fire, a log lodge, a drying rack ----------------------------------------------------
def tipi(sc, x, y, R, H, hide, seed=0, paint=("#a8412c", "#27465e")):
    svg, iso = sc.svg, sc.iso

    def draw():
        c = iso.p(x, y, 0)
        cx, cy = c
        ry = R / 2
        apex = (cx, cy - H)
        for k, lean in enumerate((-9, -4, 1, 6, 11, 14)):  # pole tips crossing above the smoke flap
            svg.line((apex[0] - lean * 0.2, apex[1] + 6), (apex[0] + lean * 0.95, apex[1] - 13 - (k % 3) * 2), "#5a4026", 1.5)
        cone(svg, cx, cy, R, H, hide, ridges=12, seed=seed, texture=False)
        svg.poly([apex, (cx - R * 0.42, cy - H * 0.62), (cx + R * 0.42, cy - H * 0.62)], svg.lin([(0, "#2a2018", 0.8), (1, "#2a2018", 0.0)], 0, 0, 0, 1))
        for k in range(1, 8):  # canvas seams
            th = math.pi * k / 8
            base = (cx + R * math.cos(th), cy + ry * math.sin(th))
            svg.line(lerp(apex, base, 0.06), base, shade(hide, 0.5 + 0.55 * th / math.pi), 0.7, 0.45)
        svg.path(f"M{cx - R * 0.88:.2f},{cy - 6:.2f} A{R * 0.88:.2f},{ry * 0.88:.2f} 0 0 0 {cx + R * 0.88:.2f},{cy - 6:.2f}", stroke=paint[0], sw=4.0, op=0.85)
        svg.path(f"M{cx - R * 0.88:.2f},{cy - 10.5:.2f} A{R * 0.88:.2f},{ry * 0.88:.2f} 0 0 0 {cx + R * 0.88:.2f},{cy - 10.5:.2f}", stroke=paint[1], sw=2.0, op=0.85)
        door_y = cy + ry * 0.97
        svg.path(f"M{cx - 5.2:.2f},{door_y:.2f} Q{cx:.2f},{door_y - H * 0.55:.2f} {cx + 5.2:.2f},{door_y:.2f} Z", "#241a12", stroke="#5a4026", sw=0.8)
        for k in range(4):
            svg.circle(cx + (k % 2 * 2 - 1) * 1.6, door_y - 4 - k * 3.2, 0.7, "#c9a96a")

    sc.add(x + y, draw)


def native(seed=41):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#6a5a34", mid="#a8995f", core="#c3b67e", stone="#b5a76e", n=30, spots=0.3)
    for i in range(16):
        tuft(svg, iso, rng.uniform(0.6, 7.4), rng.uniform(0.6, 7.4), "#8f8f3e")
    hides = ["#e0cfa6", "#d6c196", "#e6d7b2", "#d9c79f"]
    paints = [("#a8412c", "#27465e"), ("#27465e", "#d6a53a"), ("#2f6a48", "#a8412c")]
    for i, (x, y) in enumerate(_ring(4, 4.2, 2.65, (100, 148, 196, 244, 292, 340))):
        tipi(sc, x, y, 21 + (i % 2) * 2, 50 + (i % 3) * 3, hides[i % 4], seed=i, paint=paints[i % 3])

    def fire():
        c = p(4.0, 4.2, 0)
        svg.ellipse(c[0], c[1], 13, 6.5, "#f0a030", op=0.35, blur=svg.blur(3))
        for k in range(8):
            a = math.tau * k / 8
            svg.ellipse(c[0] + math.cos(a) * 6.2, c[1] + math.sin(a) * 3.1, 2.2, 1.4, "#6a6558", stroke="#2f2c25", sw=0.5)
        svg.poly([(c[0] - 4, c[1]), (c[0], c[1] - 11), (c[0] + 4, c[1])], "#e8682a")
        svg.poly([(c[0] - 2, c[1]), (c[0], c[1] - 7), (c[0] + 2, c[1])], "#f6c648")
        puffs(svg, (c[0], c[1] - 12), 5, n=5, drift=(3, -9), size=4.5, op=0.55)

    sc.add(8.6, fire)
    sc.house(5.0, 0.8, 1.9, 1.4, 11, "#80613f", "gable_x", "#6d5a3a", rh=8, course=3.2, course_op=0.4, tiles=13, lit=0.14)
    sc.add(6.9 + 6.9, lambda: rack(svg, iso, 6.1, 6.3, 1.6, 15))
    sc.add(1.4 + 7.4, lambda: rack(svg, iso, 0.9, 6.6, 1.2, 12))
    sc.tree(0.6, 0.8, kind="pine", h=40, seed=1, leaf="#3d6340")
    sc.tree(7.4, 7.5, kind="pine", h=34, seed=2, leaf="#3d6340")
    return sc.render()


# ---- oceanic: thatched fale on coral platforms, a carved meeting house, palms and a canoe --------------------------------------------
def fale(sc, x, y, r, seed=0, thatch="#b99a56"):
    svg, iso = sc.svg, sc.iso

    def draw():
        cylinder(svg, iso, x, y, r, 3.2, "#c4bfae", grain=10, seed=seed)
        rx, ry = radii(r)
        for a in (200, 240, 285, 330, 20, 100, 150):
            th = math.radians(a)
            qx, qy = x + r * 0.85 * math.cos(th), y + r * 0.85 * math.sin(th)
            a0, a1 = iso.p(qx, qy, 3.2), iso.p(qx, qy, 13)
            svg.line(a0, a1, "#4a3220", 2.4)
            svg.line((a0[0] - 0.4, a0[1]), (a1[0] - 0.4, a1[1]), "#8a6240", 1.2)
        c = iso.p(x, y, 13)
        top = dome(svg, c[0], c[1], rx * 1.16, 21, thatch, ribs=9)
        svg.line(top, (top[0], top[1] - 5), shade(thatch, 0.55), 1.6)

    sc.add(x + y, draw)


def oceanic(seed=43):
    sc = Scene(seed)
    svg, iso, rng = sc.svg, sc.iso, sc.rng
    p = iso.p
    earth(svg, iso, rng, rim="#8a7a52", mid="#cdbd8c", core="#e2d6ab", stone="#ebe0bd", n=36, spots=0.35)
    for i in range(16):
        q = p(rng.uniform(0.6, 7.4), rng.uniform(0.6, 7.4), 0)
        svg.ellipse(q[0], q[1], rng.uniform(5, 10), rng.uniform(2.2, 4.4), "#5d8a3a", op=0.34)

    def meeting_house():
        building(svg, iso, rng, 4.4, 0.9, 2.6, 1.9, 3, "#a9a290", "flat", "#a9a290", cols=1, rows=1, door_on=False, lit=0.0)
        building(svg, iso, rng, 4.7, 1.15, 2.0, 1.4, 10, "#7a2f22", "gable_x", "#9f8244", rh=11, z0=3, cols=2, rows=1, lit=0.1, tiles=15, o=0.4, door_on=True)
        # carved barge boards and the finial at the gable end
        a, b, c = p(4.7 + 2.0 + 0.4, 1.15 + 1.4 + 0.4, 13), p(4.7 + 2.0 + 0.4, 1.15 - 0.4, 13), p(4.7 + 2.0 + 0.4, 1.15 + 0.7, 24)
        svg.line(a, c, "#d9d0b4", 2.6)
        svg.line(b, c, "#d9d0b4", 2.6)
        for q0 in (a, b):
            for t in (0.25, 0.5, 0.75):
                m = lerp(q0, c, t)
                svg.circle(m[0], m[1], 1.3, "#b3261e")
        finial(svg, c, h=8, col="#d9d0b4")

    sc.add(5.7 + 1.85, meeting_house)
    for x, y, r, th in ((1.9, 2.5, 1.0, "#b99a56"), (1.4, 5.2, 1.0, "#a8884a"), (3.9, 4.4, 1.15, "#c0a25c"), (6.3, 4.7, 1.0, "#b99a56"), (4.1, 6.5, 0.95, "#a8884a")):
        fale(sc, x, y, r, seed=int(x * 10), thatch=th)
    sc.palm(0.6, 0.8, h=56, seed=1)
    sc.palm(7.5, 3.7, h=50, seed=2)
    sc.palm(0.7, 7.3, h=48, seed=3)
    sc.palm(7.0, 7.3, h=52, seed=4)
    sc.add(14.2, lambda: canoe(svg, 150, 205, 44))
    return sc.render()


# `Flavor` key -> builder, in the order of `fourx_sim::Flavor::ALL`.
BUILDERS = {
    "western": western,
    "latin": latin,
    "orthodox": orthodox,
    "arab": arab,
    "east_asian": east_asian,
    "south_asian": south_asian,
    "southeast_asian": southeast_asian,
    "african": african,
    "steppe": steppe,
    "native": native,
    "oceanic": oceanic,
}
