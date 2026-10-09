"""Animated steam-age ships: ironclad, troop transport, battleship, protected cruiser and torpedo boat.

Every ship is a 3D model (see `rig.py`) built along its own axes: x from the stern to the bow, y to port, z up, the
waterline at z = 0 and the middle of the hull at the origin.  The model turns to any of the eight facings and is drawn
centred on the tile point (128, 123) of a 256 x 192 sprite, as before.  Clips:

    idle     loops: the hull rides the swell, funnel smoke drifts aft
    run      loops: under way, a bow wave, a wake and thicker smoke
    attack   a broadside: the guns fire 30% of the way in, the hull heels from the recoil, smoke hangs at the guns
    victory  the whistle blows white steam and the ensign streams
    death    on fire, listing and going down by the bow; the last frame is nearly under

Lit from the upper left, dark ink outlines; colours from the original 2D ships.
"""

import math
import random

from .rig import Scene, Xf, add, ease, facing_frame, keys, mix, mul, smoke, span, unit
from .svgkit import shade

W_, H_ = 256, 192
ORIGIN = (128, 123)
INK = "#15191b"
HULL_INK = "#0b0e10"
FOAM = "#eef7f2"
GUN = "#3e4646"
BLACK_STACK = "#20262a"
BUFF_STACK = "#b8955a"
RED_STACK = "#9c412c"

CLIPS = {
    "idle": (8, 170, True),
    "run": (8, 90, True),
    "attack": (8, 120, False),
    "victory": (8, 110, False),
    "death": (8, 220, False),
}
FIRE_AT = 0.30


class Motion:
    """How the hull moves and what the ship is doing at one instant of a clip."""

    def __init__(self, clip, t):
        self.clip, self.t = clip, t
        a = math.tau * t
        loop = CLIPS[clip][2]
        self.speed = 1.0 if clip == "run" else 0.0
        self.bob = 0.8 * math.sin(a) if loop else 0.0
        self.roll = 1.4 * math.sin(a) if loop else 0.0
        self.pitch = 0.8 * math.cos(a) if loop else 0.0
        self.fire = None  # age of the broadside's smoke, 0..1
        self.flames = 0.0
        self.steam = None
        self.smoke = 1.0 + self.speed * 0.5
        if clip == "run":
            self.pitch = 1.6 * math.cos(a)
            self.bob = 1.2 * math.sin(a)
        elif clip == "attack":
            heel = 0.0
            if t >= FIRE_AT:
                x = span(t, FIRE_AT, 1.0)
                heel = 5.0 * math.exp(-4.0 * x) * math.cos(9.0 * x)
                self.fire = x
            self.roll = -heel
        elif clip == "victory":
            self.steam = t
            self.bob = 0.8 * math.sin(math.tau * t)
            self.roll = 1.2 * math.sin(math.tau * t)
        elif clip == "death":
            self.flames = keys(t, [(0.0, 0.3), (0.3, 1.0), (1.0, 0.6)])
            self.roll = keys(t, [(0.0, 0.0), (0.15, -6.0), (0.9, 24.0)])
            self.pitch = keys(t, [(0.0, 0.0), (0.4, 2.0), (1.0, 9.0)])
            self.sink_depth = keys(t, [(0.15, 0.0), (0.55, 0.22), (1.0, 1.0)])


class Ship:
    """One frame of one ship being drawn: hull geometry plus the fittings every design needs."""

    def __init__(self, sc: Scene, xf: Xf, m: Motion, length, beam, freeboard, bow=38, rise=2.0, seed=2):
        self.sc, self.m = sc, m
        self.L, self.B, self.hh, self.bow, self.rise = length, beam, freeboard, bow, rise
        self.rng = random.Random(seed)
        self.ground = xf  # the water plane at the ship's position, unaffected by the swell
        x0 = -length / 2
        self.x0 = x0
        drop = 0.0
        if m.clip == "death":
            drop = m.sink_depth * (freeboard + 34.0)
        self.xf = xf.at((0.0, 0.0, m.bob - drop)).pitch(m.pitch).roll(m.roll)
        self.tops = []
        self.guns = []  # muzzles, for the broadside smoke

    # geometry helpers ------------------------------------------------------------------------
    def P(self, a, b, z=0.0):
        """A point in the old 2D ship units: `a` from the stern, `b` across from the port side, `z` up."""
        return self.xf((self.x0 + a, self.B / 2 - b, z))

    def half(self, a):
        """Half the beam at `a` from the stern: full amidships, a rounded stern, a fine bow."""
        L, B, bow = self.L, self.B, self.bow
        if a > L - bow:
            u = (a - (L - bow)) / bow
            return B / 2 * math.sqrt(max(0.0, 1.0 - u * u)) * (1 - 0.15 * u)
        if a < 12:
            u = 1 - a / 12
            return B / 2 * (1 - 0.28 * u * u)
        return B / 2

    def top(self, a):
        return self.hh + self.rise * max(0.0, (a - (self.L - self.bow)) / self.bow) ** 1.5

    def stations(self):
        L = self.L
        xs = [0.0, 4.0, 8.0, 12.0]
        n = max(4, int((L - self.bow - 12) / 22))
        xs += [12 + (L - self.bow - 12) * i / n for i in range(1, n + 1)]
        xs += [L - self.bow + self.bow * i / 6 for i in range(1, 7)]
        return xs

    # water -----------------------------------------------------------------------------------
    def water(self, speed):
        sc, L, B = self.sc, self.L, self.B
        g = self.ground
        x0 = self.x0
        soft = sc.svg.blur(4.5)
        outline = [g((x0 + a, s * (self.half(a) + 6), 0.0)) for s in (-1, 1) for a in (self.stations() if s < 0 else self.stations()[::-1])]
        pts = [sc.P(p) for p in outline]
        sc.add(1e6, lambda svg: svg.poly(pts, "#0a2a35", op=0.3, extra=f'filter="{soft}"'), -2)
        sink = self.m.clip == "death"
        if speed > 0 and not sink:
            # a wake fanning out astern, and the crests of its V
            reach = 34 + 22 * speed
            wake = [g((x0 + 2, B / 2 - 1, 0)), g((x0 + 2, -B / 2 + 1, 0)), g((x0 - reach, -B / 2 - 8 - 10 * speed, 0)), g((x0 - reach, B / 2 + 8 + 10 * speed, 0))]
            wp = [sc.P(p) for p in wake]
            s2 = sc.svg.blur(2.4)
            a0, a1 = sc.P(g((x0, 0, 0))), sc.P(g((x0 - reach, 0, 0)))
            sc.add(1e6 - 1, lambda svg: svg.poly(wp, sc.userlin([(0, FOAM), (1, "#7fb3b8")], a0, a1), op=0.42, extra=f'filter="{s2}"'), -2)
            for i in range(5):
                k = (i + self.m.t * 1.0) / 5
                d, w = 8 + (reach - 8) * k, 3 + (8 + 10 * speed) * k
                p0, pc, p1 = sc.P(g((x0 - d, -B / 2 - w, 0))), sc.P(g((x0 - d + 12, 0, 0))), sc.P(g((x0 - d, B / 2 + w, 0)))
                op = 0.6 * (1 - 0.7 * k)
                sc.add(1e6 - 2, lambda svg, p0=p0, pc=pc, p1=p1, op=op: svg.path(f"M{p0[0]:.1f},{p0[1]:.1f} Q{pc[0]:.1f},{pc[1]:.1f} {p1[0]:.1f},{p1[1]:.1f}", stroke="#e8f4f0", sw=1.3, op=op), -2)
        # surf along the waterline, and a bow wave when under way
        ring = [g((x0 + a, s * (self.half(a) + 1.5 + 2.5 * speed), 0.0)) for s in (-1, 1) for a in (self.stations() if s < 0 else self.stations()[::-1])]
        rp = [sc.P(p) for p in ring]
        s1 = sc.svg.blur(1.2)
        surf = (0.45 + 0.2 * speed) * (1.0 - 0.5 * getattr(self.m, "sink_depth", 0.0))
        sc.add(1e6 - 3, lambda svg: svg.poly(rp, FOAM, op=surf, extra=f'filter="{s1}"'), -1.5)
        if speed > 0 and not sink:
            bow = [g((x0 + L + 4, 0, 0)), g((x0 + L - self.bow, -B / 2 - 9, 0)), g((x0 + L - self.bow - 26, -B / 2 - 5, 0)), g((x0 + L - self.bow - 26, B / 2 + 5, 0)), g((x0 + L - self.bow, B / 2 + 9, 0))]
            bp = [sc.P(p) for p in bow]
            sc.add(1e6 - 4, lambda svg: svg.poly(bp, "#f4fbf7", op=0.55, extra=f'filter="{s1}"'), -1.5)
            for i in range(6):
                q = g((x0 + L + self.rng.uniform(-8, 3), self.rng.uniform(-B / 2 - 4, B / 2 + 4), self.rng.uniform(0, 3)))
                x, y = sc.P(q)
                rx, ry = self.rng.uniform(2.5, 5), self.rng.uniform(1.4, 2.6)
                sc.add(sc.depth(q) - 30, lambda svg, x=x, y=y, rx=rx, ry=ry: svg.ellipse(x, y, rx, ry, "#f6fbf8", op=0.8, blur=svg.blur(0.8)), 2)

    # hull ------------------------------------------------------------------------------------
    def body(self, side, deck, band="#5a2a22", stripe=None, ports=(), portholes=(), seams=8, rail=4.5):
        """Hull sides, bow and stern, deck, waterline band, stripe and gunports.  `side`, `deck` are base colours."""
        sc, xs = self.sc, self.stations()
        x0 = self.x0
        X = self.xf

        def pt(a, s, z):
            return X((x0 + a, s * self.half(a), z))

        for s in (-1, 1):
            for a0, a1 in zip(xs, xs[1:]):
                quad = [pt(a0, s, 0), pt(a1, s, 0), pt(a1, s, self.top(a1)), pt(a0, s, self.top(a0))]
                if s > 0:
                    quad = quad[::-1]
                sc.poly(quad, side, HULL_INK, 1.0, layer=0)
                # boot-topping and stripe follow each strake, so they turn with the hull
                for col, z0, z1 in ([(band, 0.0, 3.4)] + ([stripe] if stripe else [])):
                    lo0, lo1 = min(z0, self.top(a0)), min(z0, self.top(a1))
                    hi0, hi1 = min(z1, self.top(a0)), min(z1, self.top(a1))
                    strip = [pt(a0, s * 1.004, lo0), pt(a1, s * 1.004, lo1), pt(a1, s * 1.004, hi1), pt(a0, s * 1.004, hi0)]
                    if s > 0:
                        strip = strip[::-1]
                    sc.poly(strip, col, None, layer=0.5)
        # transom
        stern = [pt(0, -1, 0), pt(0, 1, 0), pt(0, 1, self.top(0)), pt(0, -1, self.top(0))]
        sc.poly(stern[::-1], shade(side, 0.92), HULL_INK, 1.0, layer=0)
        # deck
        outline = [pt(a, -1, self.top(a)) for a in xs] + [pt(a, 1, self.top(a)) for a in xs[::-1]]
        sc.poly(outline, deck, "#14181a", 1.0, two=True, layer=0)
        planks = max(4, int((self.L - self.bow) / 14))
        for k in range(1, planks):
            a = k * (self.L - self.bow) / planks
            sc.line(pt(a, -0.85, self.top(a) + 0.2), pt(a, 0.85, self.top(a) + 0.2), shade(deck, 0.7), 0.5, op=0.5, layer=0.6)
        for s in (-1, 1):
            for a, z0, z1, w in ports:
                q = [pt(a, s * 1.006, z0), pt(a + w, s * 1.006, z0), pt(a + w, s * 1.006, z1), pt(a, s * 1.006, z1)]
                if s > 0:
                    q = q[::-1]
                sc.poly(q, "#07090b", None, layer=0.7, lit=False, flat="#07090b")
            for a in portholes:
                c = pt(a, s * 1.01, self.hh * 0.42)
                sc.disk(c, X.vec((0.0, float(s), 0.0)), 1.3, "#f2de9c", two=False, sides=8, layer=0.7, sw=0.4)
            for k in range(1, seams + 1 if self._sees(s) else 1):
                a = k * (self.L - self.bow) / (seams + 1)
                sc.line(pt(a, s * 1.006, 1.0), pt(a, s * 1.006, self.hh - 0.5), HULL_INK, 0.6, op=0.45, layer=0.6)
        if rail:
            for s in (-1, 1):
                for a0, a1 in zip(xs, xs[1:]):
                    sc.line(pt(a0, s, self.top(a0) + rail), pt(a1, s, self.top(a1) + rail), "#2a2f31", 0.9, op=0.9, layer=1)
                for a in xs[::2]:
                    sc.line(pt(a, s, self.top(a)), pt(a, s, self.top(a) + rail), "#2a2f31", 0.7, layer=1)

    def _sees(self, s):
        """Whether the port (s > 0) or starboard (s < 0) side faces the camera."""
        n = self.xf.vec((0.0, float(s), 0.0))
        return n[1] < 0.0

    # fittings --------------------------------------------------------------------------------
    def box(self, a0, a1, b0, b1, z0, z1, wall, windows=False):
        sc, X = self.sc, self.xf
        base = X.at((self.x0, self.B / 2, 0.0))
        sc.box(base, a0, a1, -b1, -b0, z0, z1, wall, INK, 0.9, layer=1, top=shade(wall, 1.08))
        if windows:
            zm = (z0 + z1) / 2
            for s, b in ((-1, b1), (1, b0)):
                for a in range(int(a0 + 2), int(a1 - 2), 5):
                    q = [self.P(a, b + s * -0.3, zm - 1.3), self.P(a + 2.6, b + s * -0.3, zm - 1.3), self.P(a + 2.6, b + s * -0.3, zm + 1.3), self.P(a, b + s * -0.3, zm + 1.3)]
                    if s > 0:
                        q = q[::-1]
                    sc.poly(q, "#10181c", None, layer=1, bias=-0.5, lit=False, flat="#10181c")

    def funnel(self, a, z0, hgt, w, body, band=None, cap="#121416", b=None):
        sc = self.sc
        b = self.B / 2 if b is None else b
        base, top = self.P(a, b, z0), self.P(a, b, z0 + hgt)
        sc.tube(base, top, w / 2, w / 2 * 0.95, body, side=self.xf.vec((0, 1, 0)), squash=1.0, sides=14, caps=False, layer=1)
        sc.tube(self.P(a, b, z0 + hgt * 0.76), top, w / 2 + 0.3, w / 2 * 0.95 + 0.3, cap, sides=14, caps=False, layer=1, bias=-0.2)
        if band:
            sc.tube(self.P(a, b, z0 + hgt * 0.5), self.P(a, b, z0 + hgt * 0.5 + 3), w / 2 + 0.35, w / 2 + 0.35, band, sides=14, caps=False, layer=1, bias=-0.2)
        sc.disk(top, (0, 0, 1), w / 2 * 0.95, "#050607", sides=14, layer=1, bias=-0.3)
        self.tops.append(top)

    def turret(self, a, b, z, r=11, barrels=2, reach=24, toward=1, h=6.0, train=0.0):
        sc = self.sc
        c0, c1 = self.P(a, b, z), self.P(a, b, z + h)
        sc.tube(c0, c1, r, r * 0.96, GUN, sides=16, caps=True, layer=1)
        ang = math.radians(train) + (0.0 if toward > 0 else math.pi)
        fwd = (math.cos(ang), math.sin(ang), 0.0)
        side = (-fwd[1], fwd[0], 0.0)
        centre = (self.x0 + a, self.B / 2 - b, z + h * 0.55)
        for k in range(barrels):
            off = (k - (barrels - 1) / 2) * 3.4
            start = add(add(centre, mul(fwd, r * 0.7)), mul(side, off))
            end = add(add(centre, mul(fwd, reach)), mul(side, off))
            sc.limb(self.xf(start), self.xf(end), 1.6 if r > 8 else 1.2, 1.3 if r > 8 else 1.0, "#2a3032", sw=0.6, layer=1, bias=-0.1)
            self.guns.append(self.xf(end))

    def mast(self, a, z0, height, yard=8, top=None, w=2.2, col="#1b1612"):
        sc = self.sc
        base, tip = self.P(a, self.B / 2, z0), self.P(a, self.B / 2, z0 + height)
        sc.limb(base, tip, w * 0.6, w * 0.45, col, ink=None, layer=1)
        if top:
            c = self.P(a, self.B / 2, z0 + height * top)
            sc.tube(add(c, (0, 0, -3.0)), c, 4.5, 6.5, "#2b3234", sides=12, layer=1, bias=-0.3)
        ya, yb = self.P(a, self.B / 2 - yard, z0 + height - yard * 0.9), self.P(a, self.B / 2 + yard, z0 + height - yard * 0.9)
        sc.line(ya, yb, col, 1.3, layer=1, bias=-0.2)
        return tip

    def stay(self, p, q):
        self.sc.line(p, q, "#2a2520", 0.6, op=0.7, layer=1, bias=2.0)

    def ensign(self, a=3, w=30, h=20, pole_h=30, wave=1.0):
        """A staff at the stern with the rising-sun ensign streaming aft."""
        sc = self.sc
        foot = self.P(a, self.B / 2, self.hh)
        top = self.P(a, self.B / 2, self.hh + pole_h)
        sc.line(foot, top, "#3a2c1e", 1.5, layer=1, bias=-1.0)
        aft = self.xf.vec((-1.0, 0.0, 0.0))
        t = self.m.t
        pts_top, pts_bot = [], []
        for i in range(9):
            u = i / 8
            ripple = math.sin(u * 5.0 - t * math.tau * 2) * 2.0 * wave * u
            sidev = self.xf.vec((0.0, 1.0, 0.0))
            p = add(add(top, mul(aft, w * u)), mul(sidev, ripple))
            pts_top.append(p)
            pts_bot.append(add(p, (0.0, 0.0, -h)))
        cloth = pts_top + pts_bot[::-1]
        sc.poly(cloth, "#f3e8cf", shade("#f3e8cf", 0.45), 0.7, two=True, layer=1, bias=-1.2)
        mid = mix(pts_top[4], pts_bot[4], 0.5)
        n = unit(self.xf.vec((0.0, 1.0, 0.0)))
        sc.disk(mid, n, h * 0.3, "#b3261e", ink=None, sides=14, layer=1, bias=-1.3)

    def figures(self, at, n=6):
        """A company of troops on deck."""
        for k in range(n):
            a, b = at[0] + k * 4.4, at[1] + (k % 2) * 2.4
            foot = self.P(a, b, self.hh)
            self.sc.limb(foot, add(foot, (0, 0, 6.0)), 1.9, 1.6, "#33465f", sw=0.4, layer=1)
            self.sc.ball(add(foot, (0, 0, 7.8)), 1.3, "#e6bd94", sw=0.3, layer=1, bias=-0.1)

    # what the clip adds ----------------------------------------------------------------------
    def finish(self):
        sc, m = self.sc, self.m
        aft = self.ground.vec((-1.0, 0.0, 0.0))
        dying = m.clip == "death"
        for i, top in enumerate(self.tops):
            n = 6
            for j in range(n):
                age = (j + (m.t if CLIPS[m.clip][2] else m.t * 2.0)) / n % 1.0 if not dying else (j + m.t * 2.0) / n % 1.0
                drift = 9.0 + 5.0 * m.speed
                p = add(add(top, mul(aft, drift * age * 4.0)), (0.0, 0.0, 6.0 + 24.0 * age))
                r = (4.0 + 7.0 * age) * (1.2 if dying else 1.0) * (0.9 + 0.2 * m.smoke)
                op = 0.85 * (1 - age) ** 1.3
                sc.draw(p, lambda svg, x, y, r=r, op=op, j=j: smoke(svg, x, y, r, op, dark=True, seed=j + i), layer=3, bias=-age)
        if m.steam is not None and self.tops:
            u = m.steam
            top = self.tops[0]
            for j in range(4):
                age = span(u, 0.1 + j * 0.12, 0.6 + j * 0.12)
                if 0 < age < 1:
                    p = add(top, (0.0, 0.0, 6.0 + 20.0 * age))
                    sc.draw(p, lambda svg, x, y, age=age: smoke(svg, x - 2, y, 4.0 + 6.0 * age, 0.9 * (1 - age)), layer=3)
        if m.fire is not None:
            x = m.fire
            for i, g in enumerate(self.guns):
                p = add(g, (0.0, 0.0, 4.0 * x))
                sc.draw(p, lambda svg, px, py, i=i: smoke(svg, px, py, 4.0 + 9.0 * x, 0.85 * (1 - x) ** 1.2, seed=i), layer=3)
            # broadside smoke along both flanks
            for s in (-1, 1):
                for k in range(3):
                    a = self.L * (0.3 + 0.2 * k)
                    p = self.P(a, self.B / 2 - s * (self.B / 2 + 6 + 10 * x), self.hh * 0.6 + 4 * x)
                    sc.draw(p, lambda svg, px, py, k=k: smoke(svg, px, py, 5.0 + 9.0 * x, 0.75 * (1 - x) ** 1.3, seed=k), layer=3)
        if dying and m.flames > 0:
            rng = random.Random(7)
            for k in range(5):
                a = self.L * (0.25 + 0.12 * k)
                p = self.P(a, self.B / 2 + rng.uniform(-6, 6), self.hh + 2)
                if p[2] < -1.0:
                    continue
                flick = 0.75 + 0.25 * math.sin(m.t * 37.0 + k * 1.7)
                size = (5.0 + 3.0 * rng.random()) * m.flames * flick

                def fire(svg, x, y, size=size):
                    blur = svg.blur(1.2)
                    svg.ellipse(x, y - size * 0.6, size * 0.7, size * 1.1, "#e8742a", op=0.85, blur=blur)
                    svg.ellipse(x, y - size * 0.5, size * 0.4, size * 0.7, "#ffd25e", op=0.9, blur=blur)
                sc.draw(p, fire, layer=2.5)
                sp = add(p, (0.0, 0.0, 14.0))
                sc.draw(sp, lambda svg, x, y, k=k: smoke(svg, x, y - 10 * m.t, 6.0 + 8.0 * m.t, 0.7, dark=True, seed=k), layer=3)
            # foam where she goes under
            if m.sink_depth > 0.2:
                g = self.ground
                rng2 = random.Random(3)
                for k in range(8):
                    q = g((self.x0 + rng2.uniform(0, self.L), rng2.uniform(-self.B / 2 - 6, self.B / 2 + 6), 0.0))
                    x, y = sc.P(q)
                    rx = rng2.uniform(4, 8)
                    sc.add(sc.depth(q) - 40, lambda svg, x=x, y=y, rx=rx: svg.ellipse(x, y, rx, rx * 0.45, "#f4fbf7", op=0.7 * m.sink_depth, blur=svg.blur(1.0)), 2)


def _scene(k, clip, t, s=0.92):
    sc = Scene(W_, H_, ORIGIN, s)
    sc.clip_z = None
    m = Motion(clip, t)
    xf = facing_frame(k)
    return sc, m, xf


def _ship(k, clip, t, length, beam, freeboard, bow, rise, s=0.92, seed=2):
    sc, m, xf = _scene(k, clip, t, s)
    h = Ship(sc, xf, m, length, beam, freeboard, bow, rise, seed)
    # the water surface stays put while the hull moves; cut what goes below it
    sc.clip_z = 0.0 if clip == "death" else -1.5
    return sc, h


def _done(sc, h, speed):
    clip_z = sc.clip_z
    sc.clip_z = None
    h.water(speed)
    sc.clip_z = clip_z
    h.finish()
    return sc.render()


def ironclad(k, clip, t):
    """Steam ironclad: a long dark armoured hull, two turrets, two funnels, three masts."""
    L, B, hh = 190, 34, 15
    sc, h = _ship(k, clip, t, L, B, hh, 38, 2.0, s=0.9)
    ports = [(24 + i * 24, 7, 11, 6) for i in range(6)]
    h.body("#30383c", "#a3a08f", band="#5a2a22", stripe=("#6b2a22", hh * 0.42, hh * 0.46), ports=ports, seams=8)
    h.box(10, 40, 8, B - 8, hh, hh + 7, "#a09d8d")
    h.box(54, 112, 7, B - 7, hh, hh + 9, "#aaa798")
    for a in (62, 88):
        h.funnel(a, hh + 9, 38, 9.0, BLACK_STACK, band="#a8322a")
    h.box(112, 134, 9, B - 9, hh, hh + 10, "#9f9c8d", windows=True)
    h.box(116, 128, 12, B - 12, hh + 10, hh + 17, "#b3b0a1")
    fire = clip == "attack"
    h.turret(44, B / 2, hh, r=10, barrels=1, reach=26, toward=-1, train=40 if fire else 0)
    h.turret(150, B / 2, hh, r=10, barrels=1, reach=26, train=-15 if fire else 0)
    tips = [h.mast(a, hh, 62 - (a == 140) * 10, yard=8) for a in (14, 100, 140)]
    h.stay(tips[0], tips[1])
    h.stay(tips[1], tips[2])
    h.ensign(a=0, w=30, h=20, pole_h=30, wave=2.0 if clip in ("run", "victory") else 1.0)
    return _done(sc, h, h.m.speed)


def transport(k, clip, t):
    """Troop transport: black merchant hull with lit portholes, cream deckhouse, red funnel, hatches and derricks."""
    L, B, hh = 178, 38, 19
    sc, h = _ship(k, clip, t, L, B, hh, 30, 3.0, s=0.92)
    h.body("#30363a", "#b39a6c", band="#7a2c22", stripe=("#e4dec8", hh * 0.8, hh - 1), portholes=list(range(14, L - 42, 11)), seams=0, rail=5)
    h.box(112, 134, 9, B - 9, hh, hh + 2.8, "#9a8a5a")
    for i, (a, b) in enumerate([(138, B - 9), (140, B - 14), (136, B - 14)]):
        h.box(a, a + 5, b, b + 5, hh, hh + 4 + i % 2 * 2, "#b08850")
    fore = h.mast(122, hh + 2.8, 42, yard=6, w=2.6)
    h.sc.line(h.P(122, B / 2, hh + 14), h.P(146, B / 2 + 8, hh + 28), "#2a2018", 1.8, layer=1)
    h.stay(fore, h.P(146, B / 2 + 8, hh + 28))
    h.box(70, 106, 8, B - 8, hh, hh + 11, "#e8e2cf", windows=True)
    h.box(84, 102, 11, B - 11, hh + 11, hh + 18, "#f1ecdc", windows=True)
    h.funnel(74, hh + 11, 32, 12, RED_STACK, band="#e9e2c8")
    h.box(36, 58, 9, B - 9, hh, hh + 2.8, "#9a8a5a")
    aft = h.mast(62, hh, 44, yard=6, w=2.6)
    h.sc.line(h.P(62, B / 2, hh + 14), h.P(36, B / 2 + 8, hh + 28), "#2a2018", 1.8, layer=1)
    h.stay(aft, h.P(36, B / 2 + 8, hh + 28))
    h.figures((8, B - 10))
    h.guns = [h.P(a, B / 2, hh + 4) for a in (40, 120)]
    h.ensign(a=3, w=30, h=20, pole_h=30, wave=2.0 if clip in ("run", "victory") else 1.0)
    return _done(sc, h, h.m.speed * 0.7)


def battleship(k, clip, t):
    """Battleship: the biggest hull, steel-grey, two heavy twin turrets, wing turrets, two stacks, two fighting-top masts."""
    L, B, hh, bow = 208, 44, 17, 42
    sc, h = _ship(k, clip, t, L, B, hh, bow, 3.0, s=0.86)
    ports = [(a, 6.5, 10.5, 5.5) for a in range(22, L - bow - 14, 17)]
    h.body("#6f7a7e", "#a9a794", band="#5a2a22", ports=ports, seams=9)
    fire = clip == "attack"
    h.turret(152, B / 2, hh, r=13, barrels=2, reach=32, train=-20 if fire else 0)
    h.box(112, 134, 11, B - 11, hh, hh + 10, "#cfd1cb", windows=True)
    h.box(118, 130, 15, B - 15, hh + 10, hh + 17, "#dcdcd5", windows=True)
    fore = h.mast(124, hh + 17, 44, yard=9, top=0.55)
    for b in (9, B - 9):
        h.turret(104, b, hh, r=7, barrels=1, reach=15, h=4.5, train=(-70 if b > B / 2 else 70) if fire else 0)
    for a in (92, 72):
        h.funnel(a, hh, 42, 12, BLACK_STACK, band="#d9d4c0")
    h.box(42, 60, 12, B - 12, hh, hh + 8, "#c6c8c1", windows=True)
    aft = h.mast(52, hh + 8, 40, yard=8, top=0.55)
    h.turret(28, B / 2, hh, r=13, barrels=2, reach=28, toward=-1, train=25 if fire else 0)
    h.stay(fore, aft)
    h.ensign(a=-2, w=26, h=17, pole_h=40, wave=2.0 if clip in ("run", "victory") else 1.0)
    return _done(sc, h, h.m.speed * 0.8)


def protected_cruiser(k, clip, t):
    """Protected cruiser: a long white hull, two buff stacks, light turrets fore and aft, pole masts."""
    L, B, hh, bow = 178, 30, 13, 36
    sc, h = _ship(k, clip, t, L, B, hh, bow, 2.0, s=0.92)
    ports = [(a, 5, 8, 4) for a in range(26, L - bow - 10, 16)]
    h.body("#dedbcd", "#bba97f", band="#6b2a22", ports=ports, seams=0)
    fire = clip == "attack"
    h.turret(142, B / 2, hh, r=9, barrels=2, reach=20, h=5, train=-20 if fire else 0)
    h.box(100, 122, 7, B - 7, hh, hh + 8, "#f1eee2", windows=True)
    h.box(106, 118, 10, B - 10, hh + 8, hh + 14, "#f7f4e8", windows=True)
    fore = h.mast(112, hh + 14, 36, yard=7, top=0.55)
    for a in (84, 64):
        h.funnel(a, hh, 34, 9.5, BUFF_STACK)
    h.box(30, 48, 9, B - 9, hh, hh + 6, "#e6e3d4", windows=True)
    aft = h.mast(40, hh + 6, 32, yard=6)
    h.turret(18, B / 2, hh, r=9, barrels=2, reach=18, toward=-1, h=5, train=25 if fire else 0)
    h.stay(fore, aft)
    h.ensign(a=3, w=28, h=19, pole_h=26, wave=2.0 if clip in ("run", "victory") else 1.0)
    return _done(sc, h, h.m.speed)


def torpedo_boat(k, clip, t):
    """Torpedo boat: small, low and fast, dark hull with a turtleback bow, two thin stacks, torpedo tubes aft."""
    L, B, hh, bow = 124, 17, 8, 44
    sc, h = _ship(k, clip, t, L, B, hh, bow, 3.0, s=0.98)
    h.body("#3f4a54", "#7d7b6f", band="#6b2a22", seams=0, rail=3)
    sc2 = h.sc
    tb = [h.P(L - bow - 14, 3, hh + 0.6), h.P(L - bow, 3, hh + 0.6), h.P(L - 5, B / 2, hh + 0.6), h.P(L - bow, B - 3, hh + 0.6), h.P(L - bow - 14, B - 3, hh + 0.6)]
    sc2.poly(tb, "#9a988a", None, two=True, layer=0.8)
    fire = clip == "attack"
    h.turret(94, B / 2, hh, r=4.4, barrels=1, reach=14, h=3.4, train=-20 if fire else 0)
    h.box(76, 90, 4, B - 4, hh, hh + 7, "#6c7068", windows=True)
    h.box(79, 88, 5.5, B - 5.5, hh + 7, hh + 10, "#80857b")
    mast = h.mast(70, hh, 34, yard=6, w=1.8)
    for a in (56, 40):
        h.funnel(a, hh, 25, 6.2, BLACK_STACK)
    h.box(10, 28, B / 2 - 6, B / 2 + 6, hh, hh + 1.8, "#4a5257")
    for off in (-2.8, 2.8):
        s0, e0 = h.P(7, B / 2 + off, hh + 3.6), h.P(31, B / 2 + off, hh + 3.6)
        sc2.tube(s0, e0, 2.2, 2.2, "#222a2d", sides=10, layer=1)
        h.guns.append(e0)
    h.stay(mast, h.P(8, B / 2, hh + 14))
    h.ensign(a=0, w=14, h=9, pole_h=14, wave=2.0 if clip in ("run", "victory") else 1.0)
    return _done(sc, h, h.m.speed * 1.6)


BUILDERS = {
    "ironclad": ironclad,
    "transport": transport,
    "battleship": battleship,
    "protected-cruiser": protected_cruiser,
    "torpedo-boat": torpedo_boat,
}
