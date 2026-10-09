"""A tiny 3D puppet renderer that writes SVG, so a unit can be drawn from any of eight headings.

World space: x east, y north (away from the viewer), z up, in sprite pixels at scale 1.  The camera is the game's
2:1 dimetric view: a point projects to `(ox + x, oy - (y / 2 + z))`, and depth along the view is `y - z / 2`
(larger is farther).  Light comes from the upper left, as in every other module.

A `Scene` collects primitives (polygons, tapered limbs, balls, tubes, free-hand screen drawings), each with a depth,
then paints them back to front.  `layer` beats depth: the hull of a ship is layer 0, what stands on its deck layer 1,
so a turret at the far end of a deck is never painted under it.

Models are posed in a local frame (`Xf`): x forward, y to the model's left, z up.  `HEADINGS[k]` is the ground angle of
facing `k`, in the order Civ3 and open-civ3 store their animation strips: SW, S, SE, E, NE, N, NW, W.
"""

import math

from .svgkit import Svg, fmt, shade

INK = "#17130f"
# Facing k looks along ground angle HEADINGS[k] (degrees, 0 east, 90 north, counter-clockwise).
HEADINGS = [225 + 45 * k for k in range(8)]
FACINGS = ["sw", "s", "se", "e", "ne", "n", "nw", "w"]

VIEW = (0.0, -0.894427, 0.447214)  # from the scene toward the camera
LIGHT = (-0.56, -0.30, 0.77)  # toward the light: upper left, a little in front
_n = math.sqrt(sum(c * c for c in LIGHT))
LIGHT = tuple(c / _n for c in LIGHT)


# ---- vectors ------------------------------------------------------------------------------------
def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def mul(a, k):
    return (a[0] * k, a[1] * k, a[2] * k)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def length(a):
    return math.sqrt(dot(a, a))


def unit(a):
    n = length(a) or 1.0
    return (a[0] / n, a[1] / n, a[2] / n)


def mix(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)


def ease(t):
    t = min(1.0, max(0.0, t))
    return t * t * (3 - 2 * t)


def span(t, a, b):
    return min(1.0, max(0.0, (t - a) / (b - a)))


def keys(t, pts):
    """Piecewise smooth interpolation through (time, value) pairs; values may be numbers or tuples."""
    if t <= pts[0][0]:
        return pts[0][1]
    for (t0, v0), (t1, v1) in zip(pts, pts[1:]):
        if t <= t1:
            k = ease((t - t0) / (t1 - t0)) if t1 > t0 else 1.0
            if isinstance(v0, tuple):
                return tuple(a + (b - a) * k for a, b in zip(v0, v1))
            return v0 + (v1 - v0) * k
    return pts[-1][1]


# ---- transforms ---------------------------------------------------------------------------------
class Xf:
    """A rigid transform: rotation matrix `m` (rows) and translation `t`.  Methods return a new transform that applies
    the given local motion first, so `Xf().at(p).yaw(a)` places a model at p turned by a."""

    def __init__(self, m=((1, 0, 0), (0, 1, 0), (0, 0, 1)), t=(0.0, 0.0, 0.0)):
        self.m, self.t = m, t

    def __call__(self, p):
        m = self.m
        return (
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + self.t[0],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + self.t[1],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + self.t[2],
        )

    def vec(self, v):
        m = self.m
        return (
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        )

    def _with(self, r):
        m = self.m
        out = tuple(tuple(sum(m[i][k] * r[k][j] for k in range(3)) for j in range(3)) for i in range(3))
        return Xf(out, self.t)

    def at(self, p):
        return Xf(self.m, self(p))

    def yaw(self, deg):
        c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
        return self._with(((c, -s, 0), (s, c, 0), (0, 0, 1)))

    def pitch(self, deg):
        """Tip the nose (+x) down for positive degrees: a rotation about the local y axis."""
        c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
        return self._with(((c, 0, s), (0, 1, 0), (-s, 0, c)))

    def roll(self, deg):
        """Lean toward the model's right (-y) for positive degrees: a rotation about the local x axis."""
        c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
        return self._with(((1, 0, 0), (0, c, -s), (0, s, c)))

    def scale(self, k):
        return Xf(tuple(tuple(v * k for v in row) for row in self.m), self.t)


def two_bone(a, target, l1, l2, pole):
    """Elbow or knee for a limb from `a` of lengths l1, l2 reaching toward `target`, bent toward `pole` (a direction)."""
    d = sub(target, a)
    dist = min(length(d), (l1 + l2) * 0.999)
    dist = max(dist, abs(l1 - l2) + 1e-3)
    dirv = unit(d)
    # law of cosines: distance from a along dirv to the foot of the elbow, and the elbow's height off the line
    x = (l1 * l1 - l2 * l2 + dist * dist) / (2 * dist)
    h = math.sqrt(max(0.0, l1 * l1 - x * x))
    side = sub(pole, mul(dirv, dot(pole, dirv)))
    if length(side) < 1e-6:
        side = (0.0, 0.0, 1.0)
    side = unit(side)
    return add(add(a, mul(dirv, x)), mul(side, h))


# ---- scene --------------------------------------------------------------------------------------
class Scene:
    def __init__(self, w, h, origin, s=1.0):
        self.svg = Svg(w, h)
        self.ox, self.oy = origin
        self.s = s
        self.items = []
        self.clip_z = None  # when set, polygons are cut at this height (ships going under)

    # projection
    def P(self, p):
        return (self.ox + self.s * p[0], self.oy - self.s * (0.5 * p[1] + p[2]))

    @staticmethod
    def depth(p):
        return p[1] - 0.5 * p[2]

    def add(self, depth, draw, layer=0):
        self.items.append((layer, -depth, len(self.items), draw))

    def render(self):
        for _, _, _, draw in sorted(self.items, key=lambda it: (it[0], it[1], it[2])):
            draw(self.svg)
        self.items = []
        return self.svg

    # lighting
    @staticmethod
    def lit(n, lo=0.52, hi=1.22):
        k = max(0.0, dot(n, LIGHT))
        back = max(0.0, -dot(n, LIGHT)) * 0.08
        return lo + (hi - lo) * k - back

    def userlin(self, stops, a, b):
        """A linear gradient between two screen points."""
        svg = self.svg
        i = svg.uid("ul")
        st = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops)
        svg.defs.append(
            f'<linearGradient id="{i}" gradientUnits="userSpaceOnUse" x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}">{st}</linearGradient>'
        )
        return f"url(#{i})"

    # primitives ---------------------------------------------------------------------------
    def _clip(self, pts):
        z = self.clip_z
        if z is None:
            return pts
        out = []
        for i, p in enumerate(pts):
            q = pts[(i + 1) % len(pts)]
            pin, qin = p[2] >= z, q[2] >= z
            if pin:
                out.append(p)
            if pin != qin:
                t = (z - p[2]) / (q[2] - p[2])
                out.append(mix(p, q, t))
        return out

    def poly(self, pts, col, ink=INK, sw=0.8, two=False, op=1.0, layer=0, bias=0.0, flat=None, lit=True):
        """A planar polygon, Lambert-shaded and culled when it faces away (unless `two`)."""
        pts = self._clip(list(pts))
        if len(pts) < 3:
            return
        n = (0.0, 0.0, 0.0)
        for i, p in enumerate(pts):  # Newell's method
            q = pts[(i + 1) % len(pts)]
            n = add(n, ((p[1] - q[1]) * (p[2] + q[2]), (p[2] - q[2]) * (p[0] + q[0]), (p[0] - q[0]) * (p[1] + q[1])))
        if length(n) < 1e-9:
            return
        n = unit(n)
        if dot(n, VIEW) < 0:
            if not two:
                return
            n = mul(n, -1)
        fill = flat if flat else (shade(col, self.lit(n)) if lit else col)
        scr = [self.P(p) for p in pts]
        d = sum(self.depth(p) for p in pts) / len(pts) + bias
        self.add(d, lambda svg: svg.poly(scr, fill, ink, sw, op), layer)

    def strip(self, ring_a, ring_b, col, ink=INK, sw=0.7, layer=0, bias=0.0, closed=True, outline=True):
        """Quads between two rings of points, smoothly shaded across each quad (a tube or a hull side)."""
        n = len(ring_a)
        rng = range(n) if closed else range(n - 1)
        for i in rng:
            j = (i + 1) % n
            quad = [ring_a[i], ring_a[j], ring_b[j], ring_b[i]]
            quad = self._clip(quad)
            if len(quad) < 3:
                continue
            nrm = (0.0, 0.0, 0.0)
            for k, p in enumerate(quad):
                q = quad[(k + 1) % len(quad)]
                nrm = add(nrm, ((p[1] - q[1]) * (p[2] + q[2]), (p[2] - q[2]) * (p[0] + q[0]), (p[0] - q[0]) * (p[1] + q[1])))
            if length(nrm) < 1e-9 or dot(unit(nrm), VIEW) <= 0:
                continue
            nrm = unit(nrm)
            # smooth shading: the normals at the two long edges are the neighbours' averages
            def edge_normal(k):
                prev = (k - 1) % n
                nxt = (k + 1) % n
                t = sub(ring_a[nxt], ring_a[prev])
                along = sub(ring_b[k], ring_a[k])
                e = cross(t, along)
                return unit(e) if length(e) > 1e-9 else nrm
            ni, nj = edge_normal(i), edge_normal(j)
            if dot(ni, nrm) < 0:
                ni = mul(ni, -1)
            if dot(nj, nrm) < 0:
                nj = mul(nj, -1)
            ci, cj = shade(col, self.lit(ni)), shade(col, self.lit(nj))
            scr = [self.P(p) for p in quad]
            a = self.P(mix(ring_a[i], ring_b[i], 0.5))
            b = self.P(mix(ring_a[j], ring_b[j], 0.5))
            if math.hypot(b[0] - a[0], b[1] - a[1]) < 0.3:
                fill_args = ("flat", shade(col, self.lit(nrm)))
            else:
                fill_args = ("grad", ci, cj, a, b)
            d = sum(self.depth(p) for p in quad) / len(quad) + bias

            def draw(svg, scr=scr, fill_args=fill_args):
                if fill_args[0] == "flat":
                    fill = fill_args[1]
                else:
                    _, c0, c1, a, b = fill_args
                    fill = self.userlin([(0, c0), (1, c1)], a, b)
                # a hairline in the fill colour hides the seams between neighbouring quads
                svg.poly(scr, fill, None)
                svg.poly(scr, "none", fill if fill_args[0] == "flat" else fill_args[1], 0.35)
            self.add(d, draw, layer)
        if outline and ink:
            # silhouette edges: where a visible quad meets one facing away, an ink line
            self._silhouette(ring_a, ring_b, ink, sw, layer, bias, closed)

    def _silhouette(self, ring_a, ring_b, ink, sw, layer, bias, closed):
        n = len(ring_a)

        def facing(i):
            j = (i + 1) % n
            quad = [ring_a[i], ring_a[j], ring_b[j], ring_b[i]]
            nrm = cross(sub(quad[1], quad[0]), sub(quad[3], quad[0]))
            if length(nrm) < 1e-9:
                nrm = cross(sub(quad[2], quad[1]), sub(quad[0], quad[1]))
            return dot(nrm, VIEW) > 0

        count = n if closed else n - 1
        vis = [facing(i) for i in range(count)]
        lines = []
        for i in range(count):
            nxt = (i + 1) % n if closed else i + 1
            if nxt >= count and not closed:
                if vis[i]:
                    lines.append((ring_a[nxt], ring_b[nxt]))
                continue
            if vis[i] != vis[nxt % count]:
                lines.append((ring_a[nxt], ring_b[nxt]))
        if not closed and vis and vis[0]:
            lines.append((ring_a[0], ring_b[0]))
        for p, q in lines:
            if self.clip_z is not None and (p[2] < self.clip_z or q[2] < self.clip_z):
                if p[2] < self.clip_z and q[2] < self.clip_z:
                    continue
                lo, hi = (p, q) if p[2] < q[2] else (q, p)
                t = (self.clip_z - lo[2]) / (hi[2] - lo[2])
                p, q = mix(lo, hi, t), hi
            a, b = self.P(p), self.P(q)
            d = self.depth(mix(p, q, 0.5)) + bias - 0.01
            self.add(d, lambda svg, a=a, b=b: svg.line(a, b, ink, sw), layer)

    def tube(self, a, b, ra, rb, col, side=None, squash=1.0, sides=12, caps=True, ink=INK, sw=0.7, layer=0, bias=0.0):
        """A tapered tube from a to b with an elliptical section: radius r across `side`, r * squash across the other axis."""
        axis = unit(sub(b, a))
        if side is None:
            side = (0, 0, 1) if abs(axis[2]) < 0.9 else (1, 0, 0)
        u = unit(sub(side, mul(axis, dot(side, axis))))
        w = cross(axis, u)
        ring = lambda c, r: [add(c, add(mul(u, r * math.cos(t)), mul(w, r * squash * math.sin(t)))) for t in (i / sides * math.tau for i in range(sides))]
        A, B = ring(a, ra), ring(b, rb)
        self.strip(A, B, col, ink, sw, layer, bias)
        if caps:
            self.poly(A[::-1], shade(col, 1.0), ink, sw * 0.8, layer=layer, bias=bias)
            self.poly(B, shade(col, 1.0), ink, sw * 0.8, layer=layer, bias=bias)

    def box(self, xf, x0, x1, y0, y1, z0, z1, col, ink=INK, sw=0.8, layer=0, bias=0.0, top=None):
        c = [xf((x, y, z)) for z in (z0, z1) for y in (y0, y1) for x in (x0, x1)]
        faces = [
            ((0, 2, 3, 1), col),  # bottom
            ((4, 5, 7, 6), top or col),  # top
            ((0, 1, 5, 4), col),  # -y side
            ((2, 6, 7, 3), col),  # +y side
            ((0, 4, 6, 2), col),  # -x end
            ((1, 3, 7, 5), col),  # +x end
        ]
        for idx, fc in faces:
            self.poly([c[i] for i in idx], fc, ink, sw, layer=layer, bias=bias)

    def limb(self, a, b, ra, rb, col, ink=INK, sw=0.8, layer=0, bias=0.0, hi=1.22, lo=0.66):
        """A tapered, round-ended limb lit from the upper left (what the 2D figures used, now in 3D)."""
        if self.clip_z is not None and min(a[2], b[2]) < self.clip_z:
            return
        A, B = self.P(a), self.P(b)
        wa, wb = ra * 2 * self.s, rb * 2 * self.s
        dx, dy = B[0] - A[0], B[1] - A[1]
        n = math.hypot(dx, dy)
        if n < 1e-6:
            nx, ny = 1.0, 0.0
        else:
            nx, ny = -dy / n, dx / n
        # put the lit edge toward the upper left of the screen
        if nx + ny > 0:
            nx, ny = -nx, -ny
        pts = [(A[0] + nx * wa / 2, A[1] + ny * wa / 2), (B[0] + nx * wb / 2, B[1] + ny * wb / 2), (B[0] - nx * wb / 2, B[1] - ny * wb / 2), (A[0] - nx * wa / 2, A[1] - ny * wa / 2)]
        w = max(wa, wb) / 2
        mid = ((A[0] + B[0]) / 2, (A[1] + B[1]) / 2)
        g0 = (mid[0] + nx * w, mid[1] + ny * w)
        g1 = (mid[0] - nx * w, mid[1] - ny * w)
        d = (self.depth(a) + self.depth(b)) / 2 + bias

        def draw(svg):
            g = self.userlin([(0, shade(col, hi)), (0.55, col), (1, shade(col, lo))], g0, g1)
            if ink:
                svg.circle(A[0], A[1], wa / 2, g, stroke=ink, sw=sw)
                svg.circle(B[0], B[1], wb / 2, g, stroke=ink, sw=sw)
            else:
                svg.circle(A[0], A[1], wa / 2, g)
                svg.circle(B[0], B[1], wb / 2, g)
            svg.poly(pts, g, None)
            if ink:
                svg.line(pts[0], pts[1], ink, sw)
                svg.line(pts[3], pts[2], ink, sw)
        self.add(d, draw, layer)

    def ball(self, c, r, col, ink=INK, sw=0.8, squash=1.0, layer=0, bias=0.0, op=1.0):
        if self.clip_z is not None and c[2] < self.clip_z:
            return
        x, y = self.P(c)
        rx, ry = r * self.s, r * self.s * squash
        d = self.depth(c) + bias

        def draw(svg):
            g = svg.rad([(0, shade(col, 1.3)), (0.6, col), (1, shade(col, 0.6))], 0.36, 0.32, 0.75)
            svg.ellipse(x, y, rx, ry, g, op=op, stroke=ink, sw=sw)
        self.add(d, draw, layer)

    def line(self, a, b, col, w=1.0, op=1.0, layer=0, bias=0.0):
        if self.clip_z is not None and min(a[2], b[2]) < self.clip_z:
            return
        A, B = self.P(a), self.P(b)
        d = (self.depth(a) + self.depth(b)) / 2 + bias
        self.add(d, lambda svg: svg.line(A, B, col, w * self.s, op), layer)

    def disk(self, c, n, r, col, ink=INK, sw=0.8, sides=20, layer=0, bias=0.0, two=True):
        n = unit(n)
        u = unit(cross(n, (0, 0, 1))) if abs(n[2]) < 0.95 else (1.0, 0.0, 0.0)
        w = cross(n, u)
        pts = [add(c, add(mul(u, r * math.cos(t)), mul(w, r * math.sin(t)))) for t in (i / sides * math.tau for i in range(sides))]
        self.poly(pts, col, ink, sw, two=two, layer=layer, bias=bias)

    def draw(self, p, fn, layer=0, bias=0.0):
        """Free-hand screen drawing at a 3D point: fn(svg, x, y) with (x, y) its projection."""
        x, y = self.P(p)
        self.add(self.depth(p) + bias, lambda svg: fn(svg, x, y), layer)


def facing_frame(k, at=(0.0, 0.0, 0.0)):
    """The model frame for facing k standing at `at`."""
    return Xf().at(at).yaw(HEADINGS[k])


def ground_shadow(scene, at, rx, ry, op=0.45, layer=-1):
    soft = scene.svg.blur(2.4)
    x, y = scene.P(at)
    scene.add(1e6, lambda svg: svg.ellipse(x + rx * 0.12, y, rx * scene.s, ry * scene.s, "#0b1007", op=op, blur=soft), layer)


def oval_shadow(scene, xf, ax, ay, op=0.45, layer=-1):
    """A soft shadow on the ground: an oval `ax` long along the model's x axis and `ay` across it."""
    soft = scene.svg.blur(2.4)
    pts = [scene.P(xf((ax * math.cos(a), ay * math.sin(a), 0.0))) for a in (i / 24 * math.tau for i in range(24))]
    pts = [(x + 4.0, y) for x, y in pts]
    scene.add(1e6, lambda svg: svg.poly(pts, "#0b1007", op=op, extra=f'filter="{soft}"'), layer)


def smoke(svg, x, y, r, op, dark=False, seed=0):
    """A soft cloud of three overlapping puffs."""
    lo, mid, hi = ("#8d8a85", "#5a5855", "#383736") if dark else ("#e6e2d8", "#b8b5ad", "#8a8984")
    soft = svg.blur(max(0.6, r * 0.12))
    k = (seed * 0.37) % 1.0
    svg.circle(x + r * 0.18, y + r * 0.2, r, hi, op=op * 0.9, blur=soft)
    svg.circle(x - r * 0.1 * k, y, r * 0.9, mid, op=op, blur=soft)
    svg.circle(x - r * 0.25, y - r * 0.28, r * 0.6, lo, op=op, blur=soft)


__all__ = [
    "Scene",
    "Xf",
    "HEADINGS",
    "FACINGS",
    "facing_frame",
    "ground_shadow",
    "oval_shadow",
    "smoke",
    "two_bone",
    "add",
    "sub",
    "mul",
    "dot",
    "cross",
    "unit",
    "mix",
    "ease",
    "span",
    "keys",
    "fmt",
]
