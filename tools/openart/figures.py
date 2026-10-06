"""Posable little 3D figures, rendered to 8-facing unit sprites.

A figure is a list of parts (ellipsoids, capsule limbs, boxes, discs, flat
polygons) placed in *model* space: `r` to the figure's right, `f` forward,
`z` up, in output pixels. A `Scene` turns the model by one of the eight
facings, projects it with a 30 degree camera, depth-sorts the parts, shades
them in a few cel bands, and `finish` maps the result onto the unit palette:
parts flagged `team` land on the recolourable team ramp, everything else on
the nearest art colour, with a dark outline and a soft red shadow.

Everything here is generated from primitives; nothing is traced from game art.
"""
import math

from PIL import Image, ImageDraw

SS = 4                                   # supersampling of the working canvas
SZ = 0.866                               # vertical foreshortening (cos 30)
SY = 0.5                                 # ground depth foreshortening (sin 30)
VIEW = (0.0, SZ, SY)                     # towards the camera (X, Y, Z)


def _norm(v):
    n = math.sqrt(sum(a * a for a in v)) or 1.0
    return tuple(a / n for a in v)


LIGHT = _norm((-0.55, 0.40, 0.75))
# Facing d0..d7 as a ground vector (X right, Y towards the viewer): the unit
# looks at the camera, then turns round the compass.
FACING = [(0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1)]


def tone(rgb, k):
    """Darken (k < 1) or lighten towards white (k > 1)."""
    if k <= 1.0:
        return tuple(max(0, min(255, int(c * k))) for c in rgb)
    t = min(1.0, (k - 1.0) * 1.4)
    return tuple(max(0, min(255, int(c + (255 - c) * t))) for c in rgb)


def shade_k(n):
    """Cel band factor for a unit normal in world space."""
    d = n[0] * LIGHT[0] + n[1] * LIGHT[1] + n[2] * LIGHT[2]
    return 0.52 + 0.62 * max(0.0, d)


class Scene:
    def __init__(self, w, h, direction, gy=None, k=1.0):
        self.w, self.h = w, h
        self.k = k                                    # model px -> output px
        fx, fy = FACING[direction % 8]
        n = math.hypot(fx, fy)
        self.fx, self.fy = fx / n, fy / n
        self.rx, self.ry = -self.fy, self.fx          # the figure's right hand
        self.cx = w / 2.0
        self.gy = float(h - 8 if gy is None else gy)
        self.pitch = 0.0                              # fall backwards (radians)
        self.span = 17.0                              # half the model height, for the fall
        self.lift = 0.0                               # airborne height
        self.items = []
        self._seq = 0
        self.shadows = []                             # (cx, cy, rx, ry) in px
        self.foam = []
        size = (w * SS, h * SS)
        self.rgb = Image.new("RGB", size, (0, 0, 0))
        self.alpha = Image.new("L", size, 0)
        self.team = Image.new("L", size, 0)           # team luminance
        self.tmask = Image.new("L", size, 0)          # 255 where a team part shows
        self.d = [ImageDraw.Draw(im) for im in (self.rgb, self.alpha, self.team, self.tmask)]

    # ------------------------------------------------------------ geometry
    def world(self, p):
        r, f, z = p
        if self.pitch:
            c, s = math.cos(self.pitch), math.sin(self.pitch)
            f, z = f * c - z * s, f * s + z * c
            f += self.span * math.sin(self.pitch)         # keep the fallen body near the centre
        return (r * self.rx + f * self.fx, r * self.ry + f * self.fy, z + self.lift)

    def wnorm(self, n):
        r, f, z = n
        if self.pitch:
            c, s = math.cos(self.pitch), math.sin(self.pitch)
            f, z = f * c - z * s, f * s + z * c
        return (r * self.rx + f * self.fx, r * self.ry + f * self.fy, z)

    def scr(self, w3):
        x, y, z = w3
        k = self.k
        return ((self.cx + x * k) * SS, (self.gy + (SY * y - SZ * z) * k) * SS)

    @staticmethod
    def depth(w3, bias=0.0):
        return SZ * w3[1] + SY * w3[2] + bias

    def _push(self, depth, fn):
        self._seq += 1
        self.items.append((depth, self._seq, fn))

    # ------------------------------------------------------------- raster
    def _poly(self, pts, rgb, team, lum):
        self.d[0].polygon(pts, fill=rgb)
        self.d[1].polygon(pts, fill=255)
        if team:
            self.d[2].polygon(pts, fill=int(max(0, min(255, lum * 255))))
            self.d[3].polygon(pts, fill=255)
        else:
            self.d[3].polygon(pts, fill=0)

    def _ell_poly(self, c, rxs, rys, ang, k=1.0, shift=(0, 0), n=26):
        cs, sn = math.cos(ang), math.sin(ang)
        ox, oy = c[0] + shift[0], c[1] + shift[1]
        return [(ox + rxs * k * math.cos(t) * cs - rys * k * math.sin(t) * sn,
                 oy + rxs * k * math.cos(t) * sn + rys * k * math.sin(t) * cs)
                for t in (2 * math.pi * i / n for i in range(n))]

    # ---------------------------------------------------------- primitives
    def ball(self, c, rad, rgb, team=False, bias=0.0):
        """Ellipsoid with semi-axes (right, forward, up) around `c`."""
        w3 = self.world(c)
        a, b, h = rad
        # the 2x3 image of the unit sphere under the projection
        p = [[a * self.rx, b * self.fx, 0.0], [a * SY * self.ry, b * SY * self.fy, -SZ * h]]
        sxx = sum(v * v for v in p[0])
        syy = sum(v * v for v in p[1])
        sxy = sum(u * v for u, v in zip(p[0], p[1]))
        ang = 0.5 * math.atan2(2 * sxy, sxx - syy)
        tr, det = sxx + syy, sxx * syy - sxy * sxy
        disc = math.sqrt(max(0.0, tr * tr / 4 - det))
        l1, l2 = tr / 2 + disc, max(0.01, tr / 2 - disc)
        e1, e2 = math.sqrt(l1) * SS * self.k, math.sqrt(l2) * SS * self.k
        ex, ey = math.sqrt(sxx) * SS * self.k, math.sqrt(syy) * SS * self.k
        cen = self.scr(w3)

        def go():
            for scale, sh, k in ((1.0, (0, 0), 0.58), (0.88, (-0.07, -0.10), 0.80),
                                 (0.68, (-0.15, -0.21), 1.0), (0.38, (-0.24, -0.31), 1.22)):
                pts = self._ell_poly(cen, e1, e2, ang, scale, (sh[0] * ex, sh[1] * ey))
                self._poly(pts, tone(rgb, k), team, 0.62 * k)
        self._push(self.depth(w3, bias), go)

    def limb(self, a, b, r0, r1, rgb, team=False, bias=0.0):
        """Capsule from model point `a` to `b` (radii in px at each end)."""
        wa, wb = self.world(a), self.world(b)
        pa, pb = self.scr(wa), self.scr(wb)
        dx, dy = pb[0] - pa[0], pb[1] - pa[1]
        ln = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / ln, dx / ln
        mid = ((wa[0] + wb[0]) / 2, (wa[1] + wb[1]) / 2, (wa[2] + wb[2]) / 2)

        def go():
            for scale, sh, k in ((1.0, 0.0, 0.60), (0.78, 0.16, 0.90), (0.48, 0.30, 1.18)):
                lx, ly = -0.62, -0.78
                kk = SS * self.k
                s0, s1 = r0 * kk * scale, r1 * kk * scale
                ox, oy = lx * sh * r0 * kk, ly * sh * r0 * kk
                qa = (pa[0] + ox, pa[1] + oy)
                qb = (pb[0] + ox * r1 / max(r0, 0.01), pb[1] + oy * r1 / max(r0, 0.01))
                poly = [(qa[0] + nx * s0, qa[1] + ny * s0), (qb[0] + nx * s1, qb[1] + ny * s1),
                        (qb[0] - nx * s1, qb[1] - ny * s1), (qa[0] - nx * s0, qa[1] - ny * s0)]
                col = tone(rgb, k)
                self._poly(poly, col, team, 0.62 * k)
                for q, s in ((qa, s0), (qb, s1)):
                    self._poly(self._ell_poly(q, s, s, 0, 1.0, (0, 0), 14), col, team, 0.62 * k)
        self._push(self.depth(mid, bias), go)

    def quad(self, pts, normal, rgb, team=False, double=False, bias=0.0):
        """One flat face; `normal` is in model space (outward)."""
        wn = _norm(self.wnorm(normal))
        vis = wn[0] * VIEW[0] + wn[1] * VIEW[1] + wn[2] * VIEW[2]
        if vis <= 0.0:
            if not double:
                return
            wn = (-wn[0], -wn[1], -wn[2])
        w3 = [self.world(p) for p in pts]
        sp = [self.scr(q) for q in w3]
        k = shade_k(wn)
        mid = tuple(sum(q[i] for q in w3) / len(w3) for i in range(3))
        col = tone(rgb, k)

        def go():
            self._poly(sp, col, team, 0.62 * k)
        self._push(self.depth(mid, bias), go)

    def facet(self, pts, inside, rgb, team=False, bias=0.0):
        """A planar face whose outward side is the one away from `inside`."""
        a = [pts[1][i] - pts[0][i] for i in range(3)]
        b = [pts[2][i] - pts[0][i] for i in range(3)]
        n = _norm((a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]))
        mid = [sum(p[i] for p in pts) / len(pts) for i in range(3)]
        if sum(n[i] * (mid[i] - inside[i]) for i in range(3)) < 0:
            n = tuple(-q for q in n)
        self.quad(pts, n, rgb, team, False, bias)

    def box(self, c, size, rgb, team=False, yaw=0.0, bias=0.0, top=None):
        """Axis box (right, forward, up half-sizes in `size`), turned by `yaw`
        about the model's vertical through its centre."""
        hx, hy, hz = size
        cs, sn = math.cos(yaw), math.sin(yaw)

        def P(u, v, w):
            return (c[0] + u * cs - v * sn, c[1] + u * sn + v * cs, c[2] + w)

        def N(u, v, w):
            return (u * cs - v * sn, u * sn + v * cs, w)

        faces = (
            ((-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1), (0, 0, 1)),
            ((1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1), (1, 0, 0)),
            ((-1, 1, -1), (-1, -1, -1), (-1, -1, 1), (-1, 1, 1), (-1, 0, 0)),
            ((1, 1, -1), (-1, 1, -1), (-1, 1, 1), (1, 1, 1), (0, 1, 0)),
            ((-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1), (0, -1, 0)),
        )
        for *corners, nrm in faces:
            pts = [P(u * hx, v * hy, w * hz) for u, v, w in corners]
            col = top if (top is not None and nrm == (0, 0, 1)) else rgb
            self.quad(pts, N(*nrm), col, team, False, bias)

    def disc(self, c, radius, axis, rgb, team=False, thick=0.0, rim=None, bias=0.0):
        """Round plate (or short cylinder) facing along model vector `axis`."""
        ax = _norm(axis)
        ref = (0, 0, 1) if abs(ax[2]) < 0.9 else (1, 0, 0)
        u = _norm((ax[1] * ref[2] - ax[2] * ref[1], ax[2] * ref[0] - ax[0] * ref[2],
                   ax[0] * ref[1] - ax[1] * ref[0]))
        v = (ax[1] * u[2] - ax[2] * u[1], ax[2] * u[0] - ax[0] * u[2], ax[0] * u[1] - ax[1] * u[0])
        wn = _norm(self.wnorm(ax))
        facing = wn[0] * VIEW[0] + wn[1] * VIEW[1] + wn[2] * VIEW[2]
        steps = [-0.5, 0.0, 0.5] if thick else [0.0]
        order = sorted(steps, key=lambda s: s * facing)      # the near face last
        w3c = self.world(c)

        def ring(off, rad):
            return [self.scr(self.world(tuple(
                c[i] + ax[i] * off + (u[i] * math.cos(t) + v[i] * math.sin(t)) * rad
                for i in range(3)))) for t in (2 * math.pi * j / 22 for j in range(22))]

        def go():
            k = shade_k(wn if facing >= 0 else tuple(-q for q in wn))
            for s in order:
                off = s * thick
                col_rim = rim if rim is not None else tone(rgb, 0.55)
                near = (s == order[-1])
                self._poly(ring(off, radius), tone(col_rim, 0.9 if near else 0.7), team and rim is None, 0.4)
                if near:
                    self._poly(ring(off, radius * 0.82), tone(rgb, k), team, 0.62 * k)
        self._push(self.depth(w3c, bias), go)

    def poly(self, pts, rgb, team=False, bias=0.0):
        """Free flat polygon (sail, banner), lit as two-sided cloth."""
        w3 = [self.world(p) for p in pts]
        a = [w3[1][i] - w3[0][i] for i in range(3)]
        b = [w3[2][i] - w3[0][i] for i in range(3)]
        n = _norm((a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]))
        if n[0] * VIEW[0] + n[1] * VIEW[1] + n[2] * VIEW[2] < 0:
            n = tuple(-q for q in n)
        k = shade_k(n)
        sp = [self.scr(q) for q in w3]
        mid = tuple(sum(q[i] for q in w3) / len(w3) for i in range(3))
        col = tone(rgb, k)

        def go():
            self._poly(sp, col, team, 0.62 * k)
        self._push(self.depth(mid, bias), go)

    def rod(self, a, b, width, rgb, team=False, bias=0.0):
        """A thin straight pole or blade drawn as a flat-shaded line."""
        self.limb(a, b, width / 2.0, width / 2.0, rgb, team, bias)

    # ------------------------------------------------------------- finish
    def ground_shadow(self, rx, ry, dx=0.0, dy=0.0):
        k = self.k
        self.shadows.append((self.cx + dx * k, self.gy + dy * k, rx * k, ry * k))

    def ground_foam(self, rx, ry):
        self.foam.append((self.cx, self.gy, rx * self.k, ry * self.k))

    def render(self):
        for _, _, fn in sorted(self.items, key=lambda t: (t[0], t[1])):
            fn()

    def rgba(self, outline=None):
        """The rendered scene as an RGBA image (crisp alpha), with an optional
        one-pixel outline colour around it. Call after `render()`."""
        w, h = self.w, self.h
        size = (w, h)
        alpha = self.alpha.resize(size, Image.BOX)
        rgb = self.rgb.resize(size, Image.BOX)
        ap, rp = alpha.load(), rgb.load()
        out = Image.new("RGBA", size, (0, 0, 0, 0))
        op = out.load()
        solid = [[ap[x, y] >= 128 for x in range(w)] for y in range(h)]
        for y in range(h):
            for x in range(w):
                if solid[y][x]:
                    a = ap[x, y]
                    r, g, b = rp[x, y]
                    if a < 255:
                        k = 255.0 / max(1, a)
                        r, g, b = min(255, int(r * k)), min(255, int(g * k)), min(255, int(b * k))
                    op[x, y] = (r, g, b, 255)
                elif outline is not None:
                    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        nx, ny = x + dx, y + dy
                        if 0 <= nx < w and 0 <= ny < h and solid[ny][nx]:
                            op[x, y] = (*outline, 255)
                            break
        return out

    def finish(self, quant):
        """Return the frame as a `P` image of the unit palette."""
        self.render()
        w, h = self.w, self.h
        size = (w, h)
        alpha = self.alpha.resize(size, Image.BOX)
        rgb = self.rgb.resize(size, Image.BOX)
        tm = self.tmask.resize(size, Image.BOX)
        tl = self.team.resize(size, Image.BOX)
        ap, rp, mp, lp = alpha.load(), rgb.load(), tm.load(), tl.load()
        solid = [[ap[x, y] >= 128 for x in range(w)] for y in range(h)]
        out = Image.new("P", size, quant.BG)
        out.putpalette(quant.pal)
        op = out.load()
        # shadows underneath
        sh = Image.new("L", size, 0)
        sd = ImageDraw.Draw(sh)
        for cx, cy, rx, ry in self.shadows:
            sd.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), fill=255)
        shp = sh.load()
        foam = Image.new("L", size, 0)
        fd = ImageDraw.Draw(foam)
        for cx, cy, rx, ry in self.foam:
            fd.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), outline=255, width=1)
        fp = foam.load()
        for y in range(h):
            for x in range(w):
                if shp[x, y] >= 128:
                    op[x, y] = quant.SHADOW
                if fp[x, y] >= 128 and not solid[y][x]:
                    op[x, y] = quant.nearest((236, 244, 250))
        # outline: any empty pixel next to a solid one
        for y in range(h):
            for x in range(w):
                if solid[y][x]:
                    continue
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and solid[ny][nx]:
                        op[x, y] = quant.OUTLINE
                        break
        for y in range(h):
            for x in range(w):
                if not solid[y][x]:
                    continue
                a = ap[x, y]
                r, g, b = rp[x, y]
                if a < 255:
                    k = 255.0 / max(1, a)
                    r, g, b = min(255, int(r * k)), min(255, int(g * k)), min(255, int(b * k))
                if mp[x, y] * 2 >= a:
                    lum = lp[x, y] / max(1, mp[x, y])
                    op[x, y] = quant.team_index(lum)
                else:
                    op[x, y] = quant.nearest((r, g, b))
        return out


class Quantizer:
    """Maps colours onto the shared unit palette (`pal` is its 768 bytes)."""

    def __init__(self, pal, team=(2, 30), art=(64, 254), bg=255, shadow=254, outline_rgb=(28, 24, 34)):
        self.pal = pal
        self.BG, self.SHADOW = bg, shadow
        self.team0, self.teamn = team
        self.art = [(i, tuple(pal[i * 3:i * 3 + 3])) for i in range(art[0], art[1])]
        self._cache = {}
        self.OUTLINE = self.nearest(outline_rgb)

    def nearest(self, rgb):
        key = (rgb[0] >> 2, rgb[1] >> 2, rgb[2] >> 2)
        hit = self._cache.get(key)
        if hit is None:
            r, g, b = key[0] * 4 + 2, key[1] * 4 + 2, key[2] * 4 + 2
            hit = min(self.art, key=lambda e: (e[1][0] - r) ** 2 * 3 + (e[1][1] - g) ** 2 * 4 + (e[1][2] - b) ** 2 * 2)[0]
            self._cache[key] = hit
        return hit

    def team_index(self, lum):
        """Bright parts take the ramp's light end (index 2), dark the other."""
        t = 1.0 - max(0.0, min(1.0, (lum - 0.20) / 0.78))
        return self.team0 + int(round(t * (self.teamn - 1)))
