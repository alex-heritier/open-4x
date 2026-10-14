"""Animated steam-age ships: ironclad, troop transport, battleship, protected cruiser and torpedo boat.

Every ship is a 3D model (see `rig.py`) built along its own axes: x from the stern to the bow, y to port, z up, the
waterline at z = 0 and the middle of the hull at the origin.  The model turns to any of the eight facings and is drawn
centred on the tile point (128, 117) of a 256 x 192 sprite (shown 144 x 108).  The look follows the steam navies of the
1890s-1910s: long, low hulls with a pale planked deck over a steel-grey side, a red boot-topping, tall oval funnels, a
tripod or pole mast, and as many turrets as the design can carry.  Clips:

    idle     loops: the hull rides the swell, funnel smoke rises and leans aft
    run      loops: under way, a bow wave, a churned wake and a long plume of smoke streaming aft
    attack   a broadside to starboard: the turrets swing out to the beam, every gun fires at once (frame FIRE_FRAME),
             the hull heels from the recoil and gun smoke clings to the muzzles.  The fireballs, shock rings and
             rolling smoke of the blast itself are drawn by the client, at the muzzles `muzzle_table` reports
    victory  the whistle blows white steam and the ensign streams
    death    a magazine goes up: the hull is wrapped in flame, two columns of fire and black smoke climb, and she
             goes down into a spreading disc of foam

Lit from the upper left, dark ink outlines.
"""

import math
import random

from .rig import Scene, Xf, add, ease, facing_frame, keys, mix, mul, span, unit
from .svgkit import shade

W_, H_ = 256, 192
# The tile point of the sprite: shown at 144 x 108, so the point sits 12 screen pixels under the cell's middle.
ORIGIN = (128, 117)
INK = "#15191b"
HULL_INK = "#0b0e10"
FOAM = "#eef7f2"
GUN = "#3e4646"
BARBETTE = "#4b5457"
TURRET = "#6a7478"
BLACK_STACK = "#262d33"
BUFF_STACK = "#b8955a"
RED_STACK = "#9c412c"
STEEL = "#8d9599"
STEEL_DK = "#5d666b"
DECK = "#c9a877"
BOOT = "#6a2c25"

CLIPS = {
    "idle": (8, 170, True),
    "run": (8, 90, True),
    "attack": (8, 120, False),
    "victory": (8, 110, False),
    "death": (8, 220, False),
}
FIRE_AT = 0.30
# The broadside, frame by frame (the clip has 8 frames and plays over one second).  The turrets swing out to the
# starboard beam, every gun fires in FIRE_FRAME (the client's blast begins at 30% of the clip, which falls inside it),
# the barrels run back and out, the hull heels away from the recoil, and the turrets come round again.
FIRE_FRAME = 2
STARBOARD = -90.0  # a turret trained on the starboard beam, in degrees from the bow, counter-clockwise
ATTACK_TRAIN = [0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 0.6, 0.15]
ATTACK_RECOIL = [0.0, 0.0, 1.0, 0.8, 0.4, 0.12, 0.0, 0.0]
ATTACK_HEEL = [0.0, 0.0, 0.55, 1.0, 0.6, 0.2, -0.1, 0.0]
HEEL_DEGREES = 5.5


def _wrap(deg):
    """An angle folded into [-180, 180)."""
    return (deg + 180.0) % 360.0 - 180.0


class Motion:
    """How the hull moves and what the ship is doing at one instant of a clip."""

    def __init__(self, clip, t):
        self.clip, self.t = clip, t
        frames, _, loop = CLIPS[clip]
        self.i = int(t * frames + 1e-6) % frames if loop else round(t * (frames - 1))
        a = math.tau * t
        self.speed = 1.0 if clip == "run" else 0.0
        self.bob = 0.8 * math.sin(a) if loop else 0.0
        self.roll = 0.85 * math.sin(a) if loop else 0.0
        self.pitch = 0.5 * math.cos(a) if loop else 0.0
        self.fire = None  # age of the broadside's smoke, 0..1
        self.train = self.recoil = self.glare = 0.0
        self.flames = 0.0
        self.steam = None
        self.smoke = 1.0 + self.speed * 0.5
        self.blast = None
        if clip == "run":
            self.pitch = 1.6 * math.cos(a)
            self.bob = 1.2 * math.sin(a)
        elif clip == "attack":
            i = self.i
            self.train = ATTACK_TRAIN[i]
            self.recoil = ATTACK_RECOIL[i]
            self.glare = {FIRE_FRAME: 1.0, FIRE_FRAME + 1: 0.3}.get(i, 0.0)
            if i >= FIRE_FRAME:
                self.fire = (i - FIRE_FRAME) / (frames - 1 - FIRE_FRAME)
            self.roll = -HEEL_DEGREES * ATTACK_HEEL[i]
            self.pitch = 0.5 * ATTACK_HEEL[i]
            self.smoke = 1.3
        elif clip == "victory":
            self.steam = t
            self.bob = 0.8 * math.sin(math.tau * t)
            self.roll = 1.2 * math.sin(math.tau * t)
        elif clip == "death":
            self.flames = keys(t, [(0.0, 0.5), (0.25, 1.0), (0.55, 0.8), (1.0, 0.2)])
            self.roll = keys(t, [(0.0, 0.0), (0.2, -3.0), (0.3, -7.0), (0.9, 22.0)])
            self.pitch = keys(t, [(0.0, 0.0), (0.3, 1.5), (1.0, 11.0)])
            self.sink_depth = keys(t, [(0.0, 0.0), (0.3, 0.0), (0.62, 0.5), (0.92, 1.0)])
            self.blast = span(t, 0.16, 1.0)  # age of the explosion: 0 at the flash, 1 at the end
            self.smoke = 1.8


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
        self.tops = []  # funnel mouths and their widths, for the smoke
        self.muzzles = []  # (point, weight) of every gun a broadside fires on the starboard beam

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
            reach = 38 + 24 * speed
            wake = [g((x0 + 2, B / 2 - 1, 0)), g((x0 + 2, -B / 2 + 1, 0)), g((x0 - reach, -B / 2 - 8 - 10 * speed, 0)), g((x0 - reach, B / 2 + 8 + 10 * speed, 0))]
            wp = [sc.P(p) for p in wake]
            s2 = sc.svg.blur(2.4)
            a0, a1 = sc.P(g((x0, 0, 0))), sc.P(g((x0 - reach, 0, 0)))
            sc.add(1e6 - 1, lambda svg: svg.poly(wp, sc.userlin([(0, FOAM), (1, "#7fb3b8")], a0, a1), op=0.5, extra=f'filter="{s2}"'), -2)
            for i in range(5):
                k = (i + self.m.t * 1.0) / 5
                d, w = 8 + (reach - 8) * k, 3 + (8 + 10 * speed) * k
                p0, pc, p1 = sc.P(g((x0 - d, -B / 2 - w, 0))), sc.P(g((x0 - d + 12, 0, 0))), sc.P(g((x0 - d, B / 2 + w, 0)))
                op = 0.65 * (1 - 0.7 * k)
                sc.add(1e6 - 2, lambda svg, p0=p0, pc=pc, p1=p1, op=op: svg.path(f"M{p0[0]:.1f},{p0[1]:.1f} Q{pc[0]:.1f},{pc[1]:.1f} {p1[0]:.1f},{p1[1]:.1f}", stroke="#e8f4f0", sw=1.4, op=op), -2)
            # churned white water under the counter
            rng = random.Random(11)
            for _ in range(7):
                q = g((x0 - rng.uniform(-4, 22), rng.uniform(-B / 2, B / 2) * 0.9, 0.0))
                x, y = sc.P(q)
                rx = rng.uniform(3.0, 6.5)
                sc.add(sc.depth(q) - 30, lambda svg, x=x, y=y, rx=rx: svg.ellipse(x, y, rx, rx * 0.45, "#f4fbf7", op=0.85, blur=svg.blur(0.8)), 2)
        # surf along the waterline, and a bow wave when under way
        ring = [g((x0 + a, s * (self.half(a) + 1.5 + 2.5 * speed), 0.0)) for s in (-1, 1) for a in (self.stations() if s < 0 else self.stations()[::-1])]
        rp = [sc.P(p) for p in ring]
        s1 = sc.svg.blur(1.2)
        surf = (0.45 + 0.2 * speed) * (1.0 - 0.5 * getattr(self.m, "sink_depth", 0.0))
        sc.add(1e6 - 3, lambda svg: svg.poly(rp, FOAM, op=surf, extra=f'filter="{s1}"'), -1.5)
        if speed > 0 and not sink:
            bow = [g((x0 + L + 5, 0, 0)), g((x0 + L - self.bow, -B / 2 - 10, 0)), g((x0 + L - self.bow - 30, -B / 2 - 6, 0)), g((x0 + L - self.bow - 30, B / 2 + 6, 0)), g((x0 + L - self.bow, B / 2 + 10, 0))]
            bp = [sc.P(p) for p in bow]
            sc.add(1e6 - 4, lambda svg: svg.poly(bp, "#f4fbf7", op=0.62, extra=f'filter="{s1}"'), -1.5)
            for i in range(8):
                q = g((x0 + L + self.rng.uniform(-10, 3), self.rng.uniform(-B / 2 - 5, B / 2 + 5), self.rng.uniform(0, 3)))
                x, y = sc.P(q)
                rx, ry = self.rng.uniform(2.5, 5.5), self.rng.uniform(1.4, 2.8)
                sc.add(sc.depth(q) - 30, lambda svg, x=x, y=y, rx=rx, ry=ry: svg.ellipse(x, y, rx, ry, "#f6fbf8", op=0.85, blur=svg.blur(0.8)), 2)

    # hull ------------------------------------------------------------------------------------
    def body(self, side, deck, band=BOOT, stripe=None, ports=(), portholes=(), seams=8, rail=4.5, planks=14):
        """Hull sides, bow and stern, deck, waterline band, stripe and gunports.  `side`, `deck` are base colours.
        `stripe` is one `(colour, z0, z1)` or a list of them."""
        sc, xs = self.sc, self.stations()
        x0 = self.x0
        X = self.xf
        stripes = [] if stripe is None else ([stripe] if isinstance(stripe[0], str) else list(stripe))

        def pt(a, s, z):
            return X((x0 + a, s * self.half(a), z))

        for s in (-1, 1):
            for a0, a1 in zip(xs, xs[1:]):
                quad = [pt(a0, s, 0), pt(a1, s, 0), pt(a1, s, self.top(a1)), pt(a0, s, self.top(a0))]
                if s > 0:
                    quad = quad[::-1]
                sc.poly(quad, side, HULL_INK, 1.0, layer=0)
                # boot-topping and stripes follow each strake, so they turn with the hull
                for col, z0, z1 in [(band, 0.0, 3.4)] + stripes:
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
        n = max(4, int((self.L - self.bow) / planks))
        for k in range(1, n):
            a = k * (self.L - self.bow) / n
            sc.line(pt(a, -0.85, self.top(a) + 0.2), pt(a, 0.85, self.top(a) + 0.2), shade(deck, 0.7), 0.5, op=0.5, layer=0.6)
        # a darker margin plank along each edge of the deck, so the planking reads as laid to the sheer
        for s in (-1, 1):
            for a0, a1 in zip(xs, xs[1:]):
                sc.line(pt(a0, s * 0.9, self.top(a0) + 0.2), pt(a1, s * 0.9, self.top(a1) + 0.2), shade(deck, 0.62), 0.9, op=0.7, layer=0.6)
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
    def box(self, a0, a1, b0, b1, z0, z1, wall, windows=False, roof=None):
        sc, X = self.sc, self.xf
        base = X.at((self.x0, self.B / 2, 0.0))
        sc.box(base, a0, a1, -b1, -b0, z0, z1, wall, INK, 0.9, layer=1, top=roof or shade(wall, 1.08))
        if windows:
            zm = (z0 + z1) / 2
            for s, b in ((-1, b1), (1, b0)):
                for a in range(int(a0 + 2), int(a1 - 2), 5):
                    q = [self.P(a, b + s * -0.3, zm - 1.3), self.P(a + 2.6, b + s * -0.3, zm - 1.3), self.P(a + 2.6, b + s * -0.3, zm + 1.3), self.P(a, b + s * -0.3, zm + 1.3)]
                    if s > 0:
                        q = q[::-1]
                    sc.poly(q, "#10181c", None, layer=1, bias=-0.5, lit=False, flat="#10181c")

    def funnel(self, a, z0, hgt, w, body, band=None, cap="#121416", b=None, rake=0.0):
        """An oval funnel, wider along the keel than across it, with a black cap, an optional band and a raked top."""
        sc = self.sc
        b = self.B / 2 if b is None else b
        across = w / 2 * 0.82
        base, top = self.P(a, b, z0), self.P(a - rake, b, z0 + hgt)
        side = self.xf.vec((0, 1, 0))
        sc.tube(base, top, across, across * 0.95, body, side=side, squash=1.32, sides=16, caps=False, layer=1)
        sc.tube(self.P(a - rake * 0.76, b, z0 + hgt * 0.8), top, across + 0.3, across * 0.95 + 0.3, cap, side=side, squash=1.32, sides=16, caps=False, layer=1, bias=-0.2)
        if band:
            sc.tube(self.P(a - rake * 0.5, b, z0 + hgt * 0.5), self.P(a - rake * 0.5, b, z0 + hgt * 0.5 + 3), across + 0.35, across + 0.35, band, side=side, squash=1.32, sides=16, caps=False, layer=1, bias=-0.2)
        sc.disk(top, (0, 0, 1), across * 0.95, "#050607", sides=16, layer=1, bias=-0.3)
        self.tops.append((top, w))

    def turret(self, a, b, z, r=11, barrels=2, reach=24, home=0.0, h=6.0, weight=1.0, spread=3.4):
        """A gun turret on the keel line or a wing, its barrels along `home` degrees from the bow (0 forward, 180 aft).
        In a broadside it swings out to the starboard beam, the barrels run back in the recoil, and the muzzles
        are noted for the client's blast."""
        sc, m = self.sc, self.m
        ang = home + _wrap(STARBOARD - home) * m.train
        rad = math.radians(ang)
        fwd = (math.cos(rad), math.sin(rad), 0.0)
        side = (-fwd[1], fwd[0], 0.0)
        sc.tube(self.P(a, b, z), self.P(a, b, z + h * 0.4), r * 1.04, r * 1.04, BARBETTE, sides=18, caps=False, layer=1)
        c1 = self.P(a, b, z + h)
        sc.tube(self.P(a, b, z + h * 0.35), c1, r, r * 0.96, TURRET, sides=18, caps=True, layer=1)
        # a sighting hood on the roof, forward of the middle, shows which way the turret is trained
        hood = self.xf.at((self.x0 + a, self.B / 2 - b, z + h)).yaw(ang)
        sc.box(hood, r * 0.05, r * 0.62, -r * 0.42, r * 0.42, 0.0, 1.8, shade(TURRET, 1.12), INK, 0.7, layer=1, top=shade(TURRET, 1.3))
        centre = (self.x0 + a, self.B / 2 - b, z + h * 0.58)
        radius = 1.15 + r * 0.05
        back = 5.0 * m.recoil * (r / 11.0)
        for k in range(barrels):
            off = (k - (barrels - 1) / 2) * spread * (r / 11.0 if r < 11 else 1.0)
            start = add(add(centre, mul(fwd, r * 0.55)), mul(side, off))
            end = add(add(centre, mul(fwd, reach - back)), mul(side, off))
            sc.limb(self.xf(start), self.xf(end), radius, radius * 0.82, "#2c3436", sw=0.6, layer=1, bias=-0.1)
            if m.clip == "attack" and m.train > 0.99:
                tip = add(add(centre, mul(fwd, reach + 3.0)), mul(side, off))
                self.muzzles.append((self.xf(tip), weight * (0.8 + 0.2 * barrels)))

    def sponson(self, a, z, weight=0.5, reach=6.5, r=1.5):
        """A small gun in a port on the flank: both flanks carry one, the starboard one fires in a broadside."""
        sc, m = self.sc, self.m
        back = 2.2 * m.recoil if m.clip == "attack" else 0.0
        half = self.half(a)
        for s in (-1, 1):
            start = (self.x0 + a, s * (half - 0.2), z)
            end = (self.x0 + a, s * (half + reach - back), z)
            if self._sees(s):
                sc.disk(self.xf(start), self.xf.vec((0.0, float(s), 0.0)), r * 1.5, "#07090b", ink=None, sides=10, layer=1, bias=-0.5)
                sc.limb(self.xf(start), self.xf(end), r, r * 0.8, "#2c3436", sw=0.5, layer=1, bias=-0.6)
            if s < 0 and m.clip == "attack" and m.train > 0.99:
                self.muzzles.append((self.xf((self.x0 + a, s * (half + reach + 2.0), z)), weight))

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

    def tripod(self, a, z0, height, spread=8.0, platform=0.72):
        """A tripod mast: two legs forward and one aft, a fire-control top, and a topmast above it."""
        sc, b = self.sc, self.B / 2
        col = "#2a3033"
        apex = self.P(a, b, z0 + height * platform)
        for da, db in ((spread * 1.1, 0.0), (-spread * 0.8, spread * 0.9), (-spread * 0.8, -spread * 0.9)):
            sc.limb(self.P(a + da, b + db, z0), apex, 1.1, 0.9, col, sw=0.5, layer=1)
        top = self.P(a, b, z0 + height * platform)
        sc.tube(add(top, (0, 0, -2.0)), add(top, (0, 0, 2.5)), 4.6, 5.6, "#2b3234", sides=12, layer=1, bias=-0.3)
        tip = self.P(a, b, z0 + height)
        sc.limb(top, tip, 0.9, 0.6, col, sw=0.4, layer=1)
        ya, yb = self.P(a, b - 6, z0 + height * 0.9), self.P(a, b + 6, z0 + height * 0.9)
        sc.line(ya, yb, col, 1.1, layer=1)
        return tip

    def stay(self, p, q):
        self.sc.line(p, q, "#2a2520", 0.6, op=0.7, layer=1, bias=2.0)

    def boat(self, a, b, z, length=14.0, r=2.6, col="#e4dfcf"):
        """A ship's boat chocked on the deck."""
        p, q = self.P(a, b, z + r), self.P(a + length, b, z + r)
        self.sc.limb(p, q, r, r * 0.55, col, sw=0.5, layer=1)

    def ensign(self, a=3, w=30, h=20, pole_h=30, wave=1.0):
        """A staff at the stern with national cloth streaming aft."""
        sc = self.sc
        if getattr(self.m, "sink_depth", 0.0) > 0.4:
            return  # the stern is under: the staff goes with it
        foot = self.P(a, self.B / 2, self.hh)
        top = self.P(a, self.B / 2, self.hh + pole_h)
        sc.limb(foot, top, 0.8, 0.6, "#3a2c1e", ink=None, layer=1, bias=-1.0)
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
        # Individual folds retain their light and shade when the national cloth is recolored.
        for i in range(8):
            cloth = [pts_top[i], pts_top[i+1], pts_bot[i+1], pts_bot[i]]
            sc.poly(cloth, "#f3e8cf", None, two=True, layer=1, bias=-1.2, accent=True)
        sc.line(pts_top[0], pts_bot[0], "#594735", 0.6, layer=1, bias=-1.3)
        mid = mix(pts_top[4], pts_bot[4], 0.5)
        n = unit(self.xf.vec((0.0, 1.0, 0.0)))
        sc.disk(mid, n, h * 0.3, "#e8dcc0", ink=None, sides=14, layer=1, bias=-4.0)

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
        dying = m.clip == "death"
        self.plumes()
        if m.steam is not None and self.tops:
            u = m.steam
            top = self.tops[0][0]
            for j in range(4):
                age = span(u, 0.1 + j * 0.12, 0.6 + j * 0.12)
                if 0 < age < 1:
                    p = add(top, (0.0, 0.0, 6.0 + 20.0 * age))
                    sc.draw(p, lambda svg, x, y, age=age: billow(svg, x - 2, y, 4.0 + 6.0 * age, 0.9 * (1 - age), tone=0.95), layer=3)
        if m.clip == "attack":
            self.gun_smoke()
        if dying:
            self.magazine()

    def plumes(self):
        """Funnel smoke: puffs cast off each mouth, rising and leaning aft; under way they stream out into one long
        dark plume.  The puffs are evenly spaced in age and shift by one place over a loop, so it repeats seamlessly."""
        sc, m = self.sc, self.m
        aft = self.ground.vec((-1.0, 0.0, 0.0))
        dying = m.clip == "death"
        run = m.speed
        loop = CLIPS[m.clip][2]
        n = 10 if run else 8
        for i, (top, w) in enumerate(self.tops):
            phase = m.t if loop else m.t * 2.0
            if dying:
                phase = m.t * 2.0
            for j in range(n):
                age = ((j + phase) / n + i * 0.37) % 1.0
                trail = (30.0 + 80.0 * run) * (1.0 + 0.3 * (1 if dying else 0))
                lift = (28.0 - 12.0 * run) * (1.3 if dying else 1.0)
                sway = math.sin(age * 7.0 + i * 2.0 + j) * 2.5 * age
                p = add(add(top, mul(aft, trail * age ** 0.9)), (0.0, 0.0, 4.0 + lift * age ** 0.8 + sway))
                r = (w * 0.42 + (5.5 + 5.0 * run) * age) * (0.95 + 0.2 * m.smoke)
                op = 0.9 * (1 - age) ** 1.1 * min(1.0, 0.65 + 0.35 * m.smoke)
                sc.draw(p, lambda svg, x, y, r=r, op=op, j=j, age=age: billow(svg, x, y, r, op, tone=0.18 + 0.45 * age, seed=j + i * 3), layer=3, bias=-age)

    def gun_smoke(self):
        """The glare at each muzzle as a broadside goes off, and the smoke that hangs about the guns after."""
        sc, m = self.sc, self.m
        out = self.xf.vec((0.0, -1.0, 0.0))  # starboard, in the sprite's own frame
        for idx, (p, w) in enumerate(self.muzzles):
            if m.glare > 0:
                r = (6.0 + 6.0 * w) * (0.5 + 0.5 * m.glare)
                q = add(p, mul(out, 3.0))
                sc.draw(q, lambda svg, x, y, r=r, g=m.glare: glare(svg, x, y, r, g), layer=3.2)
            if m.fire is not None:
                x = m.fire
                q = add(add(p, mul(out, 5.0 + 11.0 * x)), (0.0, 0.0, 2.0 + 7.0 * x))
                r = (5.0 + 6.0 * w) + 6.0 * x
                sc.draw(q, lambda svg, px, py, r=r, x=x, idx=idx: billow(svg, px, py, r, 0.62 * (1 - x) ** 1.1, tone=0.55, seed=idx), layer=3)

    def magazine(self):
        """The magazine blows: fire along the hull, a white flash at the heart, two columns of flame and black smoke
        leaning apart, and the white disc of foam where she goes under."""
        sc, m = self.sc, self.m
        rng = random.Random(7)
        mid = self.P(self.L * 0.5, self.B / 2, self.hh)
        # flames licking over the deck
        if m.flames > 0:
            for k in range(7):
                a = self.L * (0.22 + 0.09 * k)
                p = self.P(a, self.B / 2 + rng.uniform(-7, 7), self.hh + 2)
                if p[2] < -1.0:
                    continue
                flick = 0.75 + 0.25 * math.sin(m.t * 37.0 + k * 1.7)
                size = (5.5 + 3.5 * rng.random()) * m.flames * flick

                def fire(svg, x, y, size=size):
                    blur = svg.blur(1.2)
                    svg.ellipse(x, y - size * 0.6, size * 0.75, size * 1.15, "#e8742a", op=0.9, blur=blur)
                    svg.ellipse(x, y - size * 0.5, size * 0.42, size * 0.75, "#ffd25e", op=0.95, blur=blur)
                sc.draw(p, fire, layer=2.5)
        # the explosion
        if m.blast is not None:
            age = m.blast
            if age < 0.45:
                k = age / 0.45
                sc.draw(add(mid, (0.0, 0.0, 6.0 + 10.0 * k)), lambda svg, x, y, k=k: fireball(svg, x, y, 12.0 + 30.0 * k, 1.0 - 0.75 * k, k), layer=3.4)
            for side, lean in ((-1, -0.55), (1, 0.55)):
                self.column(mid, side, lean, age)
        # foam where she goes under
        depth = getattr(m, "sink_depth", 0.0)
        if depth > 0.05:
            g = self.ground
            for k in range(14):
                q = g((self.x0 + rng.uniform(0, self.L), rng.uniform(-self.B / 2 - 8, self.B / 2 + 8), 0.0))
                x, y = sc.P(q)
                rx = rng.uniform(5, 10)
                sc.add(sc.depth(q) - 40, lambda svg, x=x, y=y, rx=rx: svg.ellipse(x, y, rx, rx * 0.45, "#f4fbf7", op=min(0.85, 1.4 * depth), blur=svg.blur(1.0)), 2)
        if m.blast is not None:
            k = ease(m.blast)
            gx, gy = self.sc.P(self.ground((0.0, 0.0, 0.0)))
            rx = (30.0 + 74.0 * k) * self.sc.s
            op = 0.8 * min(1.0, k * 2.5) * (1.0 - 0.55 * k)
            soft = self.sc.svg.blur(2.2)
            sc.add(1e6 - 5, lambda svg: svg.ellipse(gx, gy, rx, rx * 0.5, "#eaf6f2", op=op, blur=soft), 2)
            sc.add(1e6 - 6, lambda svg: svg.ellipse(gx, gy, rx * 0.72, rx * 0.36, "#9cc9d0", op=op * 0.7, blur=soft), 2)

    def column(self, base, side, lean, age):
        """One of the two columns an exploding magazine throws up: a stack of puffs, flame at the root and
        black smoke at the top, swaying as it climbs."""
        sc = self.sc
        height = 78.0 * ease(min(1.0, age / 0.42)) * (1.0 - 0.15 * span(age, 0.7, 1.0))
        n = 11
        for j in range(n):
            u = j / (n - 1)
            if u * 78.0 > height:
                break
            hot = max(0.0, 1.0 - (age / 0.55) - u * 0.9)
            x = lean * 26.0 * u * u * 1.6 + math.sin(u * 5.0 + age * 9.0 + side) * 2.0 * u
            p = add(base, (x * (1 if side > 0 else 1), 0.0, 4.0 + height * u))
            p = add(p, (self.ground.vec((0.0, 1.0, 0.0))[0] * 0.0, 0.0, 0.0))
            r = 6.0 + 7.0 * u + 4.0 * age
            op = min(1.0, 0.95 * (1.0 - 0.8 * span(age, 0.55, 1.0))) * (1.0 - 0.3 * u)
            sc.draw(p, lambda svg, px, py, r=r, op=op, hot=hot, u=u, j=j: ember(svg, px, py, r, op, hot, 0.15 + 0.7 * u, seed=j), layer=3.3, bias=-u)


# ---- soft drawings in screen space -------------------------------------------------------------
def billow(svg, x, y, r, op, tone=0.3, seed=0):
    """A puff of smoke: five overlapping soft discs, lit from the upper left.  `tone` runs from soot (0) to pale grey (1)."""
    if op <= 0.01 or r <= 0.2:
        return
    base = [58, 56, 55]
    lo = [int(base[c] + (206 - base[c]) * tone) for c in range(3)]
    dk = tuple(int(v * 0.72) for v in lo)
    lt = tuple(min(255, int(v * 1.22 + 10)) for v in lo)
    col = "#%02x%02x%02x"
    soft = svg.blur(max(0.5, r * 0.16))
    rng = random.Random(seed * 7919 + 13)
    svg.circle(x + r * 0.16, y + r * 0.2, r, col % dk, op=op * 0.92, blur=soft)
    for _ in range(3):
        ox, oy = rng.uniform(-0.5, 0.5) * r, rng.uniform(-0.45, 0.35) * r
        svg.circle(x + ox, y + oy, r * rng.uniform(0.62, 0.82), col % tuple(lo), op=op * 0.9, blur=soft)
    svg.circle(x - r * 0.26, y - r * 0.3, r * 0.5, col % lt, op=op * 0.75, blur=soft)


def glare(svg, x, y, r, strength):
    """The bright flash at a muzzle: a white-hot core in a yellow halo."""
    soft = svg.blur(max(0.5, r * 0.18))
    svg.circle(x, y, r * 1.2, "#ffb43c", op=0.55 * strength, blur=soft)
    svg.circle(x, y, r * 0.8, "#ffe27a", op=0.9 * strength, blur=soft)
    svg.circle(x, y, r * 0.42, "#fffdf0", op=min(1.0, strength), blur=soft)


def fireball(svg, x, y, r, op, heat):
    """A rolling fireball: white-yellow at its heart in the first moments, orange then smouldering red after."""
    soft = svg.blur(max(0.6, r * 0.14))
    outer = "#e0561c" if heat < 0.6 else "#8a2e14"
    svg.circle(x, y, r, outer, op=op * 0.85, blur=soft)
    svg.circle(x - r * 0.05, y - r * 0.08, r * 0.72, "#f59a2b", op=op * 0.9, blur=soft)
    svg.circle(x - r * 0.1, y - r * 0.14, r * 0.42, "#ffe27a", op=op, blur=soft)
    if heat < 0.4:
        svg.circle(x - r * 0.1, y - r * 0.16, r * 0.2, "#fffdf0", op=op, blur=soft)


def ember(svg, x, y, r, op, hot, tone, seed=0):
    """A puff in a column of an explosion: flame while it is hot, black smoke after."""
    if op <= 0.01:
        return
    if hot > 0.05:
        soft = svg.blur(max(0.6, r * 0.16))
        svg.circle(x, y, r * 1.05, "#7a2a14", op=op * 0.9, blur=soft)
        svg.circle(x - r * 0.05, y - r * 0.05, r * 0.82, "#e0561c", op=op * min(1.0, 0.4 + hot), blur=soft)
        svg.circle(x - r * 0.1, y - r * 0.1, r * 0.5, "#ffc04a", op=op * min(1.0, hot * 1.4), blur=soft)
        svg.circle(x - r * 0.12, y - r * 0.12, r * 0.24, "#fff4c4", op=op * min(1.0, hot * 1.4 - 0.2), blur=soft)
    cool = 1.0 - min(1.0, hot * 1.6)
    if cool > 0.02:
        billow(svg, x, y, r, op * cool, tone=min(0.45, tone * 0.5), seed=seed)


def _scene(k, clip, t, s=0.92):
    sc = Scene(W_, H_, ORIGIN, s)
    sc.clip_z = None
    m = Motion(clip, t)
    xf = facing_frame(k)
    return sc, m, xf


LAST = {}  # what the last ship drawn reports: its starboard muzzles, as sprite pixels


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
    LAST["muzzles"] = [(*sc.P(p), w) for p, w in h.muzzles]
    return sc.render()


def _wave(clip):
    """How hard the ensign streams."""
    return 2.0 if clip in ("run", "victory") else 1.0


def ironclad(k, clip, t):
    """Steam ironclad: a short, broad armoured hull, wood below the belt, a long casemate with gun ports, two turrets,
    two squat funnels and a pair of masts."""
    L, B, hh = 184, 38, 12
    sc, h = _ship(k, clip, t, L, B, hh, 44, 2.0, s=0.94)
    ports = [(34 + i * 21, 5, 8.5, 5.5) for i in range(5)]
    h.body(STEEL_DK, DECK, band="#6a2c25", stripe=[("#b9794a", 3.4, hh * 0.45), ("#7f8a8e", hh * 0.45, hh - 0.2)], ports=ports, seams=0, rail=3.5)
    h.box(40, 150, 7, B - 7, hh, hh + 6.5, "#8e969a", windows=False, roof=shade("#a8a08a", 1.0))
    for i in range(5):
        h.sponson(34 + i * 21 + 2.5, hh + 3.2, weight=0.7, reach=6.0, r=1.7)
    h.turret(150, B / 2, hh + 6.5, r=10, barrels=2, reach=27, home=0.0, h=6.0, weight=1.0)
    for a in (110, 76):
        h.funnel(a, hh + 6.5, 30, 9.5, BLACK_STACK, band="#8b3a2c")
    h.turret(36, B / 2, hh + 6.5, r=9, barrels=2, reach=23, home=180.0, h=5.5, weight=0.9)
    h.box(124, 140, 12, B - 12, hh + 6.5, hh + 11.5, "#a9a58f", windows=True)
    tips = [h.mast(a, hh + 0.5, 56 - (a == 164) * 8, yard=9, w=2.4) for a in (14, 92, 164)]
    h.stay(tips[0], tips[1])
    h.stay(tips[1], tips[2])
    h.boat(52, B / 2 + 8, hh + 6.5, length=12, r=2.2)
    h.ensign(a=0, w=30, h=20, pole_h=30, wave=_wave(clip))
    return _done(sc, h, h.m.speed)


def transport(k, clip, t):
    """Troop transport: a long black merchant hull with a red boot-topping, lit portholes, a pale deckhouse amidships,
    one tall funnel, hatches and derricks."""
    L, B, hh = 214, 36, 14
    sc, h = _ship(k, clip, t, L, B, hh, 44, 3.0, s=0.9)
    h.body("#363c41", "#a98e63", band="#8a3028", stripe=("#4a5258", hh * 0.8, hh - 0.2), portholes=list(range(16, L - 52, 12)), seams=0, rail=5)
    h.box(138, 158, 9, B - 9, hh, hh + 2.8, "#8c7b52")
    for i, (a, b) in enumerate([(166, B - 9), (168, B - 14), (164, B - 14)]):
        h.box(a, a + 5, b, b + 5, hh, hh + 4 + i % 2 * 2, "#b08850")
    fore = h.mast(146, hh + 2.8, 46, yard=6, w=2.6)
    h.sc.line(h.P(146, B / 2, hh + 14), h.P(172, B / 2 + 8, hh + 30), "#2a2018", 1.8, layer=1)
    h.stay(fore, h.P(172, B / 2 + 8, hh + 30))
    h.box(78, 126, 7, B - 7, hh, hh + 11, "#e6e0cb", windows=True)
    h.box(94, 120, 10, B - 10, hh + 11, hh + 18, "#f1ecdc", windows=True)
    h.funnel(100, hh + 11, 36, 13, BLACK_STACK, band="#d9d3b8", rake=3.0)
    h.box(44, 66, 9, B - 9, hh, hh + 2.8, "#8c7b52")
    aft = h.mast(74, hh, 46, yard=6, w=2.6)
    h.sc.line(h.P(74, B / 2, hh + 14), h.P(42, B / 2 + 8, hh + 30), "#2a2018", 1.8, layer=1)
    h.stay(aft, h.P(42, B / 2 + 8, hh + 30))
    h.boat(84, B / 2 + 11, hh + 11, length=13, r=2.4)
    h.boat(84, B / 2 - 11, hh + 11, length=13, r=2.4)
    h.figures((10, B - 10))
    for a in (30, 150):
        h.sponson(a, hh - 3, weight=0.45, reach=5.0, r=1.4)
    h.ensign(a=3, w=30, h=20, pole_h=30, wave=_wave(clip))
    return _done(sc, h, h.m.speed * 0.8)


def battleship(k, clip, t):
    """Battleship: a long flush-decked hull, steel-grey under a pale planked deck, five twin turrets (one forward, two
    on the wings, two aft), two tall oval funnels either side of the wings, a tripod foremast and a bridge."""
    L, B, hh, bow = 248, 40, 11, 62
    sc, h = _ship(k, clip, t, L, B, hh, bow, 2.6, s=0.86)
    ports = [(a, 4.5, 7.5, 4) for a in range(30, L - bow - 24, 18)]
    h.body(STEEL, DECK, band=BOOT, stripe=(shade(STEEL, 1.12), 3.4, 5.0), ports=ports, seams=9, rail=4.0, planks=13)
    # (bow to stern, so the blasts of a broadside ripple down the ship in the order the client expects)
    h.turret(204, B / 2, hh, r=10, barrels=2, reach=31, home=0.0, h=6.0, weight=1.15)
    h.box(166, 184, 11, B - 11, hh, hh + 8, "#c4c7c1", windows=True)
    h.box(171, 181, 14, B - 14, hh + 8, hh + 14, "#d4d6d0", windows=True)
    fore = h.tripod(158, hh, 62, spread=8.5)
    h.funnel(138, hh, 46, 12, BLACK_STACK, band="#b9bcb3", rake=2.0)
    h.turret(120, B / 2 - 14, hh, r=8.8, barrels=2, reach=27, home=0.0, h=5.5, weight=1.0)
    h.turret(112, B / 2 + 14, hh, r=8.8, barrels=2, reach=27, home=0.0, h=5.5, weight=1.0)
    h.funnel(98, hh, 46, 12, BLACK_STACK, band="#b9bcb3", rake=2.0)
    aft = h.mast(82, hh, 42, yard=7, top=0.62)
    h.turret(62, B / 2, hh, r=10, barrels=2, reach=29, home=180.0, h=6.0, weight=1.1)
    h.turret(34, B / 2, hh, r=10, barrels=2, reach=29, home=180.0, h=6.0, weight=1.1)
    h.boat(104, B / 2 + 12.5, hh, length=13)
    h.boat(104, B / 2 - 12.5, hh, length=13)
    h.stay(fore, aft)
    h.ensign(a=-2, w=26, h=17, pole_h=38, wave=_wave(clip))
    return _done(sc, h, h.m.speed * 0.9)


def protected_cruiser(k, clip, t):
    """Protected cruiser: a long, slim hull, three funnels, a gun forward and aft and six-inch guns in sponsons along
    both flanks, pole masts."""
    L, B, hh, bow = 232, 28, 9.5, 56
    sc, h = _ship(k, clip, t, L, B, hh, bow, 2.2, s=0.88)
    ports = [(a, 4.2, 6.8, 3.6) for a in range(34, L - bow - 14, 20)]
    h.body("#757d83", DECK, band=BOOT, stripe=(shade("#757d83", 1.14), 3.4, 4.8), ports=ports, seams=0, rail=3.8)
    h.turret(196, B / 2, hh, r=8.4, barrels=1, reach=24, home=0.0, h=5.0, weight=0.95)
    for a in (166, 150):
        h.sponson(a, hh - 2.2, weight=0.5, reach=5.5)
    h.box(114, 138, 6.5, B - 6.5, hh, hh + 7.5, "#cfd1c9", windows=True)
    h.box(120, 132, 9, B - 9, hh + 7.5, hh + 13, "#dcdcd2", windows=True)
    fore = h.mast(126, hh + 13, 36, yard=7, top=0.58)
    for a in (110, 86, 62):
        h.funnel(a, hh, 34, 9.6, BLACK_STACK, band="#8d6f3c", rake=1.5)
    for a in (104, 80):
        h.sponson(a, hh - 2.2, weight=0.5, reach=5.5)
    aft = h.mast(40, hh, 34, yard=6)
    h.turret(26, B / 2, hh, r=8.4, barrels=1, reach=22, home=180.0, h=5.0, weight=0.95)
    h.boat(70, B / 2 + 6.5, hh, length=11, r=2.2)
    h.stay(fore, aft)
    h.ensign(a=1, w=28, h=19, pole_h=26, wave=_wave(clip))
    return _done(sc, h, h.m.speed)


def torpedo_boat(k, clip, t):
    """Torpedo boat: long, thin and pale, a turtleback bow, three slim funnels, a light gun forward and torpedo tubes aft."""
    L, B, hh, bow = 168, 15, 7.5, 58
    sc, h = _ship(k, clip, t, L, B, hh, bow, 3.0, s=1.02)
    h.body("#b9c0c3", "#a89268", band="#7a2e26", seams=0, rail=3, planks=18)
    sc2 = h.sc
    tb = [h.P(L - bow - 16, 2.4, hh + 0.6), h.P(L - bow, 2.4, hh + 0.6), h.P(L - 6, B / 2, hh + 0.6), h.P(L - bow, B - 2.4, hh + 0.6), h.P(L - bow - 16, B - 2.4, hh + 0.6)]
    sc2.poly(tb, "#8f8d82", None, two=True, layer=0.8)
    h.turret(138, B / 2, hh, r=4.2, barrels=1, reach=14, home=0.0, h=3.4, weight=0.7)
    h.box(100, 120, 3.5, B - 3.5, hh, hh + 6.5, "#8b9094", windows=True)
    h.box(104, 117, 4.8, B - 4.8, hh + 6.5, hh + 9.5, "#a0a5a8")
    mast = h.mast(96, hh, 34, yard=6, w=1.8)
    for a in (82, 62, 42):
        h.funnel(a, hh, 24, 6.2, BLACK_STACK, rake=1.0)
    h.box(8, 26, B / 2 - 5, B / 2 + 5, hh, hh + 1.8, "#6b7378")
    for off in (-2.6, 2.6):
        s0, e0 = h.P(5, B / 2 + off, hh + 3.6), h.P(28, B / 2 + off, hh + 3.6)
        sc2.tube(s0, e0, 2.0, 2.0, "#222a2d", sides=10, layer=1)
    for a in (74, 54):
        h.sponson(a, hh - 1.6, weight=0.4, reach=4.5, r=1.1)
    h.stay(mast, h.P(8, B / 2, hh + 14))
    h.ensign(a=0, w=14, h=9, pole_h=14, wave=_wave(clip))
    return _done(sc, h, h.m.speed * 1.5)


BUILDERS = {
    "ironclad": ironclad,
    "transport": transport,
    "battleship": battleship,
    "protected-cruiser": protected_cruiser,
    "torpedo-boat": torpedo_boat,
}


def muzzle_table(name):
    """Where each design's guns fire in a broadside, as sprite pixels: for each of the eight facings a list of
    `[x, y, weight]` (the weight is a gun's size, 0.5 for a six-inch gun and about 1 for a main turret)."""
    table = []
    for k in range(8):
        BUILDERS[name](k, "attack", FIRE_FRAME / (CLIPS["attack"][0] - 1))
        table.append([[round(x, 1), round(y, 1), round(w, 2)] for x, y, w in LAST["muzzles"]])
    return table
