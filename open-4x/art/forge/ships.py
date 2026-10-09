"""Steam-age ships: troop transport, battleship, protected cruiser and torpedo boat (256 x 192, shown 112 x 84).

They share one isometric hull frame, `Hull`: the long axis runs lower-left (stern) to upper-right (bow), the beam towards the
lower-right, and the starboard side faces the viewer.  Each ship is centred on the same point, so the tile centre
(128, 123) sits under the middle of the hull whatever its length.  Lit from the upper left, dark ink outlines.
"""

import random

from .buildings import flag, puffs
from .svgkit import Svg, shade

INK = "#15191b"
HULL_INK = "#0b0e10"
AX = (0.906, -0.423)  # along the ship, towards the bow
BX = (0.906, 0.423)  # across the ship, towards the starboard side
CENTRE = (136, 120)
FOAM = "#e9f4ef"
GUN = ("#56605f", "#252b2c")
GUN_CAP = ("#8a948f", "#4b5453")


class Hull:
    """Hull geometry plus the fittings every ship needs; (a, b, z) = (along, across, up) in ship units."""

    def __init__(self, svg: Svg, length, beam, freeboard, bow=38, rise=2.0, seed=2):
        self.svg, self.L, self.B, self.hh, self.bow, self.rise = svg, length, beam, freeboard, bow, rise
        self.ox = CENTRE[0] - AX[0] * length / 2 - BX[0] * beam / 2
        self.oy = CENTRE[1] - AX[1] * length / 2 - BX[1] * beam / 2
        self.rng = random.Random(seed)

    def P(self, a, b, z=0.0):
        return (self.ox + AX[0] * a + BX[0] * b, self.oy + AX[1] * a + BX[1] * b - z)

    def outline(self, z=0.0, grow=0.0):
        P, L, B = self.P, self.L, self.B
        return [P(-grow, -grow, z), P(L - self.bow, -grow, z), P(L + grow, B / 2, z), P(L - self.bow, B + grow, z), P(-grow, B + grow, z)]

    # ---- water and hull ---------------------------------------------------------------------
    def water(self, speed=1.0):
        """Shadow, churned wake astern and the flare of foam along the starboard flank; `speed` widens the wake."""
        s, P, L, B = self.svg, self.P, self.L, self.B
        s.poly(self.outline(0, 6), "#0a2a35", op=0.3, extra=f'filter="{s.blur(5)}"')
        edge = 3  # keep the wake inside the sprite so it never ends in a hard line

        def wedge(reach):
            return [P(-2, 1), P(-2, B - 1), P(-reach, B + 6 + 12 * speed), P(-reach, -6 - 12 * speed)]

        reach = 36 + 30 * speed
        while reach > 10 and min(x for x, _ in wedge(reach)) < edge:
            reach -= 3
        s.poly(wedge(reach), s.lin([(0, FOAM, 0.0), (1, FOAM, 0.42)], 0, 0.8, 1, 0.2), extra=f'filter="{s.blur(2.4)}"')
        for i in range(6):
            t = i / 5
            d, w = 8 + (reach - 8) * t, 3 + (8 + 12 * speed) * t
            p0, pc, p1 = P(-d, -w), P(-d + 12, B / 2), P(-d, B + w)  # a V: the crest trails further back at the sides
            if min(p0[0], pc[0], p1[0]) >= edge:
                s.path(f"M{p0[0]:.1f},{p0[1]:.1f} Q{pc[0]:.1f},{pc[1]:.1f} {p1[0]:.1f},{p1[1]:.1f}", stroke="#e8f4f0", sw=1.4, op=0.6 * (1 - 0.6 * t))
        flare = [P(L * 0.3, B + 3), P(L - self.bow + 6, B + 5 + 4 * speed), P(L + 4, B / 2 + 4 + 3 * speed), P(L - 2, B / 2 + 1), P(L - self.bow, B + 1), P(L * 0.3, B)]
        s.poly(flare, "#eaf6f1", op=0.5, extra=f'filter="{s.blur(1.6)}"')

    def body(self, side, deck, band="#5a2a22", stripe=None, belt=None, ports=(), seams=8, rail=4.5, spray=8):
        """Starboard side, bow facet, deck and surf.  `side`/`deck` are colour ramps (light -> dark); `stripe` = (colour, z0, z1)."""
        s, P, L, B, hh, bow = self.svg, self.P, self.L, self.B, self.hh, self.bow
        end = L - bow
        s.poly([P(0, B, 0), P(end, B, 0), P(end, B, hh), P(0, B, hh)], s.lin([(0, side[0]), (0.5, side[1]), (1, side[2])], 0, 0, 1, 0.3), HULL_INK, 1.2)
        s.poly([P(end, B, 0), P(L, B / 2, 0), P(L, B / 2, hh + self.rise), P(end, B, hh)], s.lin([(0, side[1]), (1, side[2])], 0, 0, 1, 0), HULL_INK, 1.2)
        if belt:
            s.poly([P(0, B, 3.4), P(end, B, 3.4), P(end, B, belt), P(0, B, belt)], "#000", op=0.2)
        if stripe:
            colour, z0, z1 = stripe
            s.poly([P(0, B, z0), P(end, B, z0), P(end, B, z1), P(0, B, z1)], colour, op=0.95)
        s.poly([P(0, B, 0), P(end, B, 0), P(end, B, 3.4), P(0, B, 3.4)], band, op=0.95)
        for k in range(1, seams + 1):
            a = k * end / (seams + 1)
            s.line(P(a, B, 1), P(a, B, hh - 0.5), HULL_INK, 0.7, 0.5)
        for a, z0, z1, w in ports:
            s.poly([P(a, B, z0), P(a + w, B, z0), P(a + w, B, z1), P(a, B, z1)], "#07090b")
        s.poly(self.outline(hh), s.lin([(0, deck[0]), (1, deck[1])], 0, 0, 1, 1), "#14181a", 1.0)
        s.poly([P(-4, B + 0.5, 0), P(end, B + 0.5, 0), P(L - 3, B / 2 + 1, 0), P(end + 8, B + 6, -1.5), P(-10, B + 6, -1.5)], "#f0f8f3", op=0.62, extra=f'filter="{s.blur(1.0)}"')
        for _ in range(spray):
            x, y = P(L - 2 + self.rng.uniform(-6, 5), B / 2 + self.rng.uniform(-3, 7), self.rng.uniform(0, 3))
            s.ellipse(x, y, self.rng.uniform(3, 6), self.rng.uniform(1.6, 3), "#f6fbf8", op=0.8, blur=s.blur(0.8))
        for k in range(1, int(end / 14)):
            a = k * end / int(end / 14)
            s.line(P(a, 3, hh + 0.4), P(a, B - 3, hh + 0.4), "#6d6a5e", 0.5, 0.45)
        if rail:
            s.line(P(0, B, hh + rail), P(end, B, hh + rail), "#2a2f31", 1.0, 0.9)
            for k in range(0, 14):
                a = k * end / 13
                s.line(P(a, B, hh), P(a, B, hh + rail + 0.5), "#2a2f31", 0.8)

    # ---- fittings ---------------------------------------------------------------------------
    def box(self, a0, a1, b0, b1, z0, z1, wall, windows=False):
        """A block: the stern face catches the light, the starboard face is in shade, the top is brightest."""
        s, P = self.svg, self.P
        s.poly([P(a0, b0, z0), P(a0, b1, z0), P(a0, b1, z1), P(a0, b0, z1)], s.lin([(0, shade(wall, 1.0)), (1, shade(wall, 0.78))], 0, 0, 0, 1), INK, 0.9)
        s.poly([P(a0, b1, z0), P(a1, b1, z0), P(a1, b1, z1), P(a0, b1, z1)], s.lin([(0, shade(wall, 0.78)), (1, shade(wall, 0.52))], 0, 0, 0, 1), INK, 0.9)
        s.poly([P(a0, b0, z1), P(a1, b0, z1), P(a1, b1, z1), P(a0, b1, z1)], s.lin([(0, shade(wall, 1.25)), (1, shade(wall, 0.95))], 0, 0, 1, 1), INK, 0.9)
        if windows:
            zm = (z0 + z1) / 2
            for a in range(int(a0 + 2), int(a1 - 2), 5):
                s.poly([P(a, b1, zm - 1.3), P(a + 2.6, b1, zm - 1.3), P(a + 2.6, b1, zm + 1.3), P(a, b1, zm + 1.3)], "#10181c", op=0.9)

    def funnel(self, a, z0, hgt, w, body, band=None, top=None, b=None):
        """Vertical stack: `body` is a three-colour ramp, `band` a colour stripe, `top` the colour of the black cap."""
        s, P = self.svg, self.P
        cx, cy = P(a, self.B / 2 if b is None else b, z0)
        ty = cy - hgt
        s.poly([(cx - w / 2, cy), (cx + w / 2, cy), (cx + w / 2, ty), (cx - w / 2, ty)], s.lin([(0, body[0]), (0.45, body[1]), (1, body[2])], 0, 0, 1, 0), "#030405", 0.9)
        s.ellipse(cx, cy, w / 2, w * 0.2, body[2])
        if top:
            s.poly([(cx - w / 2, ty), (cx + w / 2, ty), (cx + w / 2, ty + hgt * 0.24), (cx - w / 2, ty + hgt * 0.24)], s.lin([(0, shade(top, 1.2)), (1, shade(top, 0.6))], 0, 0, 1, 0), "#030405", 0.6)
        if band:
            s.poly([(cx - w / 2 - 0.4, cy - hgt * 0.5), (cx + w / 2 + 0.4, cy - hgt * 0.5), (cx + w / 2 + 0.4, cy - hgt * 0.5 - 3), (cx - w / 2 - 0.4, cy - hgt * 0.5 - 3)], band, op=0.95)
        s.ellipse(cx, ty, w / 2, w * 0.22, "#0a0c0d", stroke="#2b3236", sw=0.8)
        s.ellipse(cx, ty, w * 0.36, w * 0.14, "#000", op=0.9)
        return (cx, ty)

    def turret(self, a, b, z, r=11, barrels=2, reach=24, toward=1, h=6.0):
        """Drum turret with `barrels` guns pointing along the hull (towards the bow when `toward` > 0)."""
        s, P = self.svg, self.P
        cx, cy = P(a, b, z)
        s.ellipse(cx, cy, r, r * 0.58, INK, op=0.4, blur=s.blur(1.4))
        s.poly([(cx - r, cy), (cx + r, cy), (cx + r, cy - h), (cx - r, cy - h)], s.lin([(0, GUN[0]), (1, GUN[1])], 0, 0, 1, 0), "#0c1011", 0.9)
        s.ellipse(cx, cy - h, r, r * 0.56, s.lin([(0, GUN_CAP[0]), (1, GUN_CAP[1])], 0, 0, 1, 1), stroke="#0c1011", sw=0.9)
        for k in range(barrels):
            off = (k - (barrels - 1) / 2) * 3.4
            e = P(a + toward * reach, b + off, z + h * 0.8)
            start = (cx + 2 * toward, cy - h + 0.8 + off * 0.42)
            s.line(start, e, "#12171a", 3.0 if r > 8 else 2.2)
            s.line(start, e, "#6b7473", 0.9, 0.6)

    def mast(self, a, z0, height, yard=8, top=None, w=2.2, col="#1b1612"):
        """Pole mast with a yard; `top` (0..1) adds a fighting top at that fraction of the height."""
        s, P = self.svg, self.P
        base, tip = P(a, self.B / 2, z0), P(a, self.B / 2, z0 + height)
        s.line(base, tip, col, w)
        if top:
            x, y = P(a, self.B / 2, z0 + height * top)
            s.poly([(x - 6.5, y), (x + 6.5, y), (x + 5, y + 4), (x - 5, y + 4)], "#1d2224", INK, 0.6)
            s.ellipse(x, y, 6.5, 2.6, "#4a5356", stroke=INK, sw=0.7)
        s.line((tip[0] - yard, tip[1] + yard * 0.9), (tip[0] + yard, tip[1] + yard * 0.9), col, 1.4)
        return tip

    def ensign(self, z=0.0, w=30, h=20, pole_h=30, a=3):
        x, y = self.P(a, self.B / 2, self.hh + z)
        flag(self.svg, x, y - pole_h, w=w, h=h, wave=2.4, pole_h=pole_h)

    def smoke(self, tops, seed=20, size=8.5, drift=(-9, -9), n=7):
        for i, t in enumerate(tops):
            puffs(self.svg, (t[0], t[1] - 2), seed=seed + i, n=n, drift=drift, size=size, op=0.85, dark=True)

    def stay(self, p, q):
        self.svg.line(p, q, "#2a2520", 0.6, 0.7)


# ---- the ships --------------------------------------------------------------------------------
BLACK_STACK = ("#2c3236", "#14181b", "#050607")
BUFF_STACK = ("#d8b97f", "#b08a4b", "#7a5e30")
RED_STACK = ("#b44b34", "#8c3523", "#5a2116")


def transport(seed=2) -> Svg:
    """Troop transport: black merchant hull with lit portholes, cream deckhouse, red funnel, hatches and cargo derricks."""
    svg = Svg(256, 192)
    L, B, hh = 178, 38, 19
    h = Hull(svg, L, B, hh, bow=30, rise=3, seed=seed)
    P = h.P
    h.water(speed=0.7)
    h.body(("#3b4146", "#272c30", "#121618"), ("#c2a574", "#8f7650"), band="#7a2c22", stripe=("#e4dec8", hh * 0.8, hh - 1), seams=0, rail=5, spray=7)
    for a in range(14, L - 42, 11):  # lit portholes: troop decks
        x, y = P(a, B, hh * 0.42)
        svg.ellipse(x, y, 1.15, 1.35, "#f2de9c", stroke=HULL_INK, sw=0.4)
    h.box(112, 134, 9, B - 9, hh, hh + 2.8, "#9a8a5a")  # fore hatch under its tarpaulin
    for k, (a, b) in enumerate([(138, B - 9), (140, B - 14), (136, B - 14)]):  # crates stacked at the bow
        h.box(a, a + 5, b, b + 5, hh, hh + 4 + k % 2 * 2, "#b08850")
    h.mast(122, hh + 2.8, 42, yard=6, w=2.6)
    svg.line(P(122, B / 2, hh + 14), P(146, B / 2 + 8, hh + 28), "#2a2018", 1.8)  # cargo boom swung out over the bow
    h.stay(P(122, B / 2, hh + 44.8), P(146, B / 2 + 8, hh + 28))
    h.box(70, 106, 8, B - 8, hh, hh + 11, "#e8e2cf", windows=True)  # deckhouse
    h.box(84, 102, 11, B - 11, hh + 11, hh + 18, "#f1ecdc", windows=True)  # bridge
    top = h.funnel(74, hh + 11, 32, 12, RED_STACK, band="#e9e2c8", top="#16181a")
    h.box(36, 58, 9, B - 9, hh, hh + 2.8, "#9a8a5a")  # aft hatch
    h.mast(62, hh, 44, yard=6, w=2.6)
    svg.line(P(62, B / 2, hh + 14), P(36, B / 2 + 8, hh + 28), "#2a2018", 1.8)  # boom over the stern
    h.stay(P(62, B / 2, hh + 44), P(36, B / 2 + 8, hh + 28))
    for k in range(6):  # a company of infantry taking the air aft
        x, y = P(8 + k * 4.4, B - 8 + k % 2 * 2.4, hh)
        svg.ellipse(x, y - 3, 1.9, 3.2, "#33465f", stroke=INK, sw=0.4)
        svg.circle(x, y - 7, 1.3, "#e6bd94")
    h.ensign(z=0, w=30, h=20, pole_h=30)
    h.smoke([top], seed=30, size=7.5, drift=(-8, -7), n=6)
    return svg


def battleship(seed=2) -> Svg:
    """Battleship: the biggest hull, steel-grey, two heavy twin turrets, wing turrets, two stacks, two fighting-top masts."""
    svg = Svg(256, 192)
    L, B, hh, bow = 208, 44, 17, 42
    h = Hull(svg, L, B, hh, bow=bow, rise=3, seed=seed)
    P = h.P
    h.water(speed=0.8)
    ports = [(a, 6.5, 10.5, 5.5) for a in range(22, L - bow - 14, 17)]
    h.body(("#8a9599", "#5d686c", "#323b3f"), ("#a9a794", "#76746a"), belt=hh * 0.5, ports=ports, seams=9, spray=9)
    h.turret(152, B / 2, hh, r=13, barrels=2, reach=32)  # main battery, forward
    h.box(112, 134, 11, B - 11, hh, hh + 10, "#cfd1cb", windows=True)  # armoured conning tower
    h.box(118, 130, 15, B - 15, hh + 10, hh + 17, "#dcdcd5", windows=True)
    fore = h.mast(124, hh + 17, 44, yard=9, top=0.55)
    for b in (9, B - 9):  # secondary guns in wing turrets
        h.turret(104, b, hh, r=7, barrels=1, reach=15, h=4.5)
    tops = [h.funnel(a, hh, 42, 12, BLACK_STACK, band="#d9d4c0") for a in (92, 72)]
    h.box(42, 60, 12, B - 12, hh, hh + 8, "#c6c8c1", windows=True)
    aft = h.mast(52, hh + 8, 40, yard=8, top=0.55)
    h.turret(28, B / 2, hh, r=13, barrels=2, reach=28, toward=-1)  # main battery, aft
    h.stay(fore, aft)
    h.ensign(z=0, w=26, h=17, pole_h=40, a=-2)
    h.smoke(tops, seed=40, size=8.5, drift=(-9, -8), n=6)
    return svg


def protected_cruiser(seed=2) -> Svg:
    """Protected cruiser: a long white hull, two buff stacks, light turrets fore and aft, sponson guns, pole masts."""
    svg = Svg(256, 192)
    L, B, hh, bow = 178, 30, 13, 36
    h = Hull(svg, L, B, hh, bow=bow, rise=2, seed=seed)
    P = h.P
    h.water(speed=1.0)
    ports = [(a, 5, 8, 4) for a in range(26, L - bow - 10, 16)]
    h.body(("#f0eee2", "#c9c7b9", "#93928a"), ("#c4b185", "#927f58"), band="#6b2a22", ports=ports, seams=0, spray=8)
    h.turret(142, B / 2, hh, r=9, barrels=2, reach=20, h=5)
    h.box(100, 122, 7, B - 7, hh, hh + 8, "#f1eee2", windows=True)
    h.box(106, 118, 10, B - 10, hh + 8, hh + 14, "#f7f4e8", windows=True)
    fore = h.mast(112, hh + 14, 36, yard=7, top=0.55)
    tops = [h.funnel(a, hh, 34, 9.5, BUFF_STACK, top="#16181a") for a in (84, 64)]
    h.box(30, 48, 9, B - 9, hh, hh + 6, "#e6e3d4", windows=True)
    aft = h.mast(40, hh + 6, 32, yard=6)
    h.turret(18, B / 2, hh, r=9, barrels=2, reach=18, toward=-1, h=5)
    h.stay(fore, aft)
    h.ensign(z=0, w=28, h=19, pole_h=26)
    h.smoke(tops, seed=50, size=7.0, drift=(-8, -8), n=6)
    return svg


def torpedo_boat(seed=2) -> Svg:
    """Torpedo boat: small, low and fast, dark hull with a turtleback bow, two thin stacks, torpedo tubes aft, a big bow wave."""
    svg = Svg(256, 192)
    L, B, hh, bow = 124, 17, 8, 44
    h = Hull(svg, L, B, hh, bow=bow, rise=3, seed=seed)
    P = h.P
    h.water(speed=1.8)
    h.body(("#5b6772", "#37424c", "#1b2229"), ("#7d7b6f", "#58564d"), band="#6b2a22", seams=0, rail=3, spray=16)
    svg.poly([P(L - bow - 14, 3, hh + 0.6), P(L - bow, 3, hh + 0.6), P(L - 5, B / 2, hh + 0.6), P(L - bow, B - 3, hh + 0.6), P(L - bow - 14, B - 3, hh + 0.6)], "#9a988a", op=0.5)  # turtleback
    h.turret(94, B / 2, hh, r=4.4, barrels=1, reach=14, h=3.4)
    h.box(76, 90, 4, B - 4, hh, hh + 7, "#6c7068", windows=True)  # conning tower
    h.box(79, 88, 5.5, B - 5.5, hh + 7, hh + 10, "#80857b")
    mast = h.mast(70, hh, 34, yard=6, w=1.8)
    tops = [h.funnel(a, hh, 25, 6.2, BLACK_STACK) for a in (56, 40)]
    h.box(10, 28, B / 2 - 6, B / 2 + 6, hh, hh + 1.8, "#4a5257")  # swivel mount
    for off in (-2.8, 2.8):  # two torpedo tubes trained fore and aft
        s, e = P(7, B / 2 + off, hh + 3.6), P(31, B / 2 + off, hh + 3.6)
        svg.line(s, e, "#222a2d", 4.8)
        svg.line((s[0], s[1] - 1.2), (e[0], e[1] - 1.2), "#8a9396", 1.3, 0.8)
        svg.ellipse(e[0], e[1], 1.6, 2.4, "#101517")
    h.stay(mast, P(8, B / 2, hh + 14))
    h.ensign(z=0, w=14, h=9, pole_h=14, a=0)
    h.smoke(tops, seed=60, size=5.2, drift=(-10, -6), n=6)
    return svg
