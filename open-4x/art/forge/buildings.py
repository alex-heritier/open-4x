"""Isometric building primitives and the city sprite (256 x 224 at 2x; game draws it 128 x 112)."""

import math
import random

from .svgkit import Iso, Svg, lerp, mixc, shade, wall_style
from .palette import INK

W, H = 256, 224
CX, CY = 128, 160  # centre of the ground diamond inside the sprite canvas

PLASTER = "#c9b58d"
CONCRETE = "#a7a396"
BRICK = "#a8693f"
OCHRE_WALL = "#bb8f56"
GREY_STUCCO = "#9b988a"
TIMBER = "#6b4a36"
ROOF_TILE = "#a1401f"
ROOF_BROWN = "#6f4230"
ROOF_SLATE = "#454f56"
ROOF_ORANGE = "#b5582c"


def outline(col, f=0.52):
    return shade(col, f)


def _shadow(svg: Svg, iso: Iso, x, y, dx, dy, h, op=0.34):
    """Soft cast shadow falling to the lower right (light from the upper left)."""
    reach = 0.35 + h / 26.0
    pts = [iso.p(x + 0.1, y + dy, 0), iso.p(x + dx, y + dy, 0), iso.p(x + dx + reach, y + dy + reach * 0.1, 0), iso.p(x + dx + reach, y + 0.2, 0), iso.p(x + dx, y, 0), iso.p(x + 0.3, y + dy + 0.3, 0)]
    svg.poly(pts, "#0d1009", op=op, extra=f'filter="{svg.blur(2.6)}"')


def _quad(iso, side, x, y, dx, dy, z0, h, s0, s1, t0, t1):
    """Quad on a wall: side 'L' = the lit face (plane y+dy, s along x); 'R' = shaded face (plane x+dx)."""
    if side == "L":
        pt = lambda s, t: iso.p(x + s * dx, y + dy, z0 + t * h)
    else:
        pt = lambda s, t: iso.p(x + dx, y + dy - s * dy, z0 + t * h)
    return [pt(s0, t0), pt(s1, t0), pt(s1, t1), pt(s0, t1)]


def speckle(svg, pt, rng, n, base, lo=0.72, hi=1.22, size=(0.035, 0.05), op=0.38):
    """Scatter small tonal patches over a surface; `pt(s, t)` maps unit coords to the screen."""
    for _ in range(n):
        s0, t0 = rng.random(), rng.random()
        ds, dt = rng.uniform(*size), rng.uniform(*size) * 1.2
        q = [pt(s0, t0), pt(min(1, s0 + ds), t0), pt(min(1, s0 + ds), min(1, t0 + dt)), pt(s0, min(1, t0 + dt))]
        svg.poly(q, shade(base, rng.uniform(lo, hi)), op=op)


def _arch(iso, side, x, y, dx, dy, z0, h, s0, s1, t0, t1):
    """A window outline with a round head, on a wall face."""
    if side == "L":
        pt = lambda s, t: iso.p(x + s * dx, y + dy, z0 + t * h)
    else:
        pt = lambda s, t: iso.p(x + dx, y + dy - s * dy, z0 + t * h)
    sc, sr = (s0 + s1) / 2, (s1 - s0) / 2
    spring = t0 + 0.62 * (t1 - t0)
    pts = [pt(s0, t0), pt(s1, t0), pt(s1, spring)]
    for i in range(1, 6):
        th = math.pi * i / 6
        pts.append(pt(sc + sr * math.cos(th), spring + (t1 - spring) * math.sin(th)))
    pts += [pt(s0, spring)]
    return pts


def windows(svg, iso, rng, x, y, dx, dy, z0, h, side, cols, rows, lit=0.18, tall=0.5, skip_base=0.22, arch=False):
    span = dx if side == "L" else dy
    lit_col, dark_col = "#f0c46a", "#2b2824"
    for r in range(rows):
        for c in range(cols):
            s0 = (c + 0.26) / cols
            s1 = (c + 0.74) / cols
            t0 = skip_base + (r + 0.18) / rows * (0.94 - skip_base)
            t1 = t0 + tall / rows * (0.94 - skip_base)
            q = _quad(iso, side, x, y, dx, dy, z0, h, s0, s1, t0, t1)
            if arch:
                q = _arch(iso, side, x, y, dx, dy, z0, h, s0, s1, t0, t1)
            svg.poly(q, shade("#e6dcc2", 0.8 if side == "R" else 1.0), op=0.95)
            ins = 0.035 / cols * 6
            if arch:
                inner = _arch(iso, side, x, y, dx, dy, z0, h, s0 + ins, s1 - ins, t0 + 0.012, t1 - 0.012)
            else:
                inner = _quad(iso, side, x, y, dx, dy, z0, h, s0 + ins, s1 - ins, t0 + 0.012, t1 - 0.012)
            glow = rng.random() < lit
            col = lit_col if glow else dark_col
            if side == "R":
                col = shade(col, 0.7)
            svg.poly(inner, col, op=0.96 if glow else 0.92)
            if not glow and not arch:  # cheap glass glint
                svg.poly([inner[3], lerp(inner[3], inner[2], 0.45), lerp(inner[0], inner[1], 0.45), inner[0]], "#7c8a94", op=0.18)


def door(svg, iso, x, y, dx, dy, side="L", s=0.5, w=0.22, hgt=0.38, z0=0, h=20):
    s0, s1 = s - w / 2, s + w / 2
    q = _quad(iso, side, x, y, dx, dy, z0, h, s0, s1, 0.0, hgt)
    svg.poly(q, "#2a1c14")
    svg.poly(q[:2] + [lerp(q[1], q[2], 0.35), lerp(q[0], q[3], 0.35)], "#4a3220", op=0.6)


def _tile_lines(svg, a, b, c, d, n, col, op=0.5, w=0.9):
    for i in range(1, n):
        t = i / n
        svg.line(lerp(a, d, t), lerp(b, c, t), col, w, op)


def roof(svg, iso, x, y, dx, dy, zb, kind, col, rh=12, o=0.28, tiles=6, ridge_caps=True):
    """Draw a roof over footprint (x,y,dx,dy) sitting at height zb."""
    lit = shade(col, 1.02)
    dark = shade(col, 0.58)
    ink = outline(col, 0.45)
    xo0, xo1, yo0, yo1 = x - o, x + dx + o, y - o, y + dy + o
    xm, ym = x + dx / 2, y + dy / 2
    p = iso.p
    if kind == "flat":
        top = [p(xo0, yo0, zb), p(xo1, yo0, zb), p(xo1, yo1, zb), p(xo0, yo1, zb)]
        svg.poly(top, shade(col, 1.12), ink, 0.8)
        inner = [p(x + 0.25, y + 0.25, zb + 0.4), p(x + dx - 0.25, y + 0.25, zb + 0.4), p(x + dx - 0.25, y + dy - 0.25, zb + 0.4), p(x + 0.25, y + dy - 0.25, zb + 0.4)]
        svg.poly(inner, shade(col, 0.9), op=0.6)
        return
    if kind == "parapet":  # flat roof behind a low wall; `col` is the wall colour
        wl = 3.4
        floor = [p(x, y, zb), p(x + dx, y, zb), p(x + dx, y + dy, zb), p(x, y + dy, zb)]
        svg.poly(floor, shade(col, 0.78), ink, 0.8)
        svg.poly([p(x, y, zb), p(x + dx, y, zb), p(x + dx, y, zb + wl), p(x, y, zb + wl)], shade(col, 0.95), ink, 0.7)
        svg.poly([p(x, y, zb), p(x, y + dy, zb), p(x, y + dy, zb + wl), p(x, y, zb + wl)], shade(col, 0.82), ink, 0.7)
        svg.poly([p(x, y + dy, zb), p(x + dx, y + dy, zb), p(x + dx, y + dy, zb + wl), p(x, y + dy, zb + wl)], shade(col, 1.12), ink, 0.8)
        svg.poly([p(x + dx, y + dy, zb), p(x + dx, y, zb), p(x + dx, y, zb + wl), p(x + dx, y + dy, zb + wl)], shade(col, 0.62), ink, 0.8)
        svg.line(p(x, y + dy, zb + wl), p(x + dx, y + dy, zb + wl), shade(col, 1.3), 0.9, 0.7)
        return
    if kind == "gable_x":  # ridge parallel to x; lit slope faces +y; gable end at +x
        zr = zb + rh
        slope = [p(xo0, yo1, zb), p(xo1, yo1, zb), p(xo1, ym, zr), p(xo0, ym, zr)]
        gable = [p(xo1, yo1, zb), p(xo1, yo0, zb), p(xo1, ym, zr)]
        svg.poly(gable, shade(col, 0.5), ink, 0.8)
        svg.poly(slope, svg.lin([(0, shade(col, 1.12)), (1, shade(col, 0.88))]), ink, 0.9)
        _tile_lines(svg, slope[0], slope[1], slope[2], slope[3], tiles, shade(col, 0.62))
        svg.line(slope[2], slope[3], shade(col, 1.25), 1.4, 0.9)
    elif kind == "gable_y":  # ridge parallel to y; shaded slope faces +x; gable end at +y
        zr = zb + rh
        slope = [p(xo1, yo0, zb), p(xo1, yo1, zb), p(xm, yo1, zr), p(xm, yo0, zr)]
        gable = [p(xo0, yo1, zb), p(xo1, yo1, zb), p(xm, yo1, zr)]
        svg.poly(gable, shade(col, 0.82), ink, 0.8)
        svg.poly(slope, svg.lin([(0, shade(col, 0.72)), (1, shade(col, 0.5))]), ink, 0.9)
        _tile_lines(svg, slope[0], slope[1], slope[2], slope[3], tiles, shade(col, 0.4), 0.55)
        svg.line(slope[2], slope[3], shade(col, 0.95), 1.2, 0.9)
    elif kind == "hip":
        apex = p(xm, ym, zb + rh)
        left = [p(xo0, yo1, zb), p(xo1, yo1, zb), apex]
        right = [p(xo1, yo1, zb), p(xo1, yo0, zb), apex]
        svg.poly(right, svg.lin([(0, shade(col, 0.7)), (1, shade(col, 0.5))], 0, 1, 1, 0), ink, 0.9)
        svg.poly(left, svg.lin([(0, shade(col, 1.12)), (1, shade(col, 0.86))], 0, 1, 1, 0), ink, 0.9)
        for i in range(1, tiles):
            t = i / tiles
            svg.line(lerp(left[0], apex, t), lerp(left[1], apex, t), shade(col, 0.62), 0.9, 0.45)
            svg.line(lerp(right[0], apex, t), lerp(right[1], apex, t), shade(col, 0.4), 0.9, 0.45)
    elif kind == "hip_curved":  # East Asian hall: a hip roof whose eaves sweep up at the corners
        lift = 5.0
        w_, s_, e_, n_ = p(xo0, yo1, zb + lift), p(xo1, yo1, zb + lift * 0.6), p(xo1, yo0, zb + lift), p(xo0, yo0, zb + lift)
        r0, r1 = p(x + dx * 0.28, ym, zb + rh), p(x + dx * 0.72, ym, zb + rh)
        sag = lambda a, b: ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2 + 3.2)
        left = f"M{w_[0]:.2f},{w_[1]:.2f} Q{sag(w_, s_)[0]:.2f},{sag(w_, s_)[1] + 2:.2f} {s_[0]:.2f},{s_[1]:.2f} L{r1[0]:.2f},{r1[1]:.2f} L{r0[0]:.2f},{r0[1]:.2f} Z"
        right = f"M{s_[0]:.2f},{s_[1]:.2f} Q{sag(s_, e_)[0]:.2f},{sag(s_, e_)[1] + 2:.2f} {e_[0]:.2f},{e_[1]:.2f} L{r1[0]:.2f},{r1[1]:.2f} Z"
        end_l = f"M{w_[0]:.2f},{w_[1]:.2f} L{r0[0]:.2f},{r0[1]:.2f} L{n_[0]:.2f},{n_[1]:.2f} Z"
        svg.path(end_l, shade(col, 0.82), stroke=ink, sw=0.9)
        svg.path(right, svg.lin([(0, shade(col, 0.7)), (1, shade(col, 0.5))], 0, 1, 1, 0), stroke=ink, sw=0.9)
        svg.path(left, svg.lin([(0, shade(col, 1.14)), (1, shade(col, 0.86))], 0, 1, 1, 0), stroke=ink, sw=0.9)
        for i in range(1, tiles + 2):
            t = i / (tiles + 2)
            a = lerp(w_, s_, t)
            b = lerp(r0, r1, t)
            svg.line((a[0], a[1] + 3.2 * math.sin(math.pi * t)), b, shade(col, 0.6), 0.8, 0.4)
        svg.line(r0, r1, "#c9a85c", 1.8, 0.95)
    elif kind == "pyramid_curved":  # pagoda eaves: slightly upswept corners
        apex = p(xm, ym, zb + rh)
        lift = 3.2
        w_, s_, e_, n_ = p(xo0, yo1, zb + lift), p(xo1, yo1, zb), p(xo1, yo0, zb + lift), p(xo0, yo0, zb + lift)
        sag_l = lerp(w_, s_, 0.5)
        sag_r = lerp(s_, e_, 0.5)
        left = [w_, (sag_l[0], sag_l[1] + 1.6), s_, apex]
        right = [s_, (sag_r[0], sag_r[1] + 1.6), e_, apex]
        svg.poly(right, shade(col, 0.55), ink, 0.9)
        svg.poly(left, svg.lin([(0, shade(col, 1.1)), (1, shade(col, 0.82))], 0, 1, 1, 0), ink, 0.9)


def building(svg, iso, rng, x, y, dx, dy, h, wall, roof_kind, roof_col, rh=12, cols=None, rows=None, z0=0, door_on=True, lit=0.16, o=0.28, arch=False, tiles=6, course=5.0, course_op=0.18, shadow=True):
    if shadow:
        _shadow(svg, iso, x, y, dx, dy, h + (rh if roof_kind not in ("flat", "parapet") else 0))
    lw, rw, tp = wall_style(wall)
    p = iso.p
    left = [p(x, y + dy, z0), p(x + dx, y + dy, z0), p(x + dx, y + dy, z0 + h), p(x, y + dy, z0 + h)]
    right = [p(x + dx, y + dy, z0), p(x + dx, y, z0), p(x + dx, y, z0 + h), p(x + dx, y + dy, z0 + h)]
    ink = outline(wall, 0.42)
    svg.poly(left, svg.lin([(0, shade(wall, 1.10)), (0.65, shade(wall, 0.98)), (1, shade(wall, 0.78))], 0, 0, 0.3, 1), ink, 1.0)
    svg.poly(right, svg.lin([(0, shade(wall, 0.72)), (1, shade(wall, 0.48))], 0, 0, 0.3, 1), ink, 1.0)
    # ground-line grime and soot under the eaves give the weathered painted feel
    svg.poly([left[0], left[1], lerp(left[1], left[2], 0.12), lerp(left[0], left[3], 0.12)], "#2b2118", op=0.25)
    svg.poly([right[0], right[1], lerp(right[1], right[2], 0.12), lerp(right[0], right[3], 0.12)], "#1a140e", op=0.3)
    svg.poly([left[3], left[2], lerp(left[2], left[1], 0.10), lerp(left[3], left[0], 0.10)], "#2b2118", op=0.16)
    face_l = lambda s_, t_: iso.p(x + s_ * dx, y + dy, z0 + t_ * h)
    face_r = lambda s_, t_: iso.p(x + dx, y + dy - s_ * dy, z0 + t_ * h)
    speckle(svg, face_l, rng, 34, wall, 0.7, 1.18)
    speckle(svg, face_r, rng, 26, shade(wall, 0.6), 0.6, 1.25, op=0.4)
    for i in range(1, int(h // course)):  # faint masonry courses (or timber logs)
        t_ = i * course / h
        svg.line(face_l(0, t_), face_l(1, t_), shade(wall, 0.6), 0.5 if course > 4 else 1.0, course_op)
        svg.line(face_r(0, t_), face_r(1, t_), shade(wall, 0.35), 0.5 if course > 4 else 1.0, course_op + 0.02)
    cols_l = cols if cols is not None else max(1, round(dx * 1.1))
    cols_r = max(1, round(dy * 1.1))
    rows = rows if rows is not None else max(1, round(h / 11))
    windows(svg, iso, rng, x, y, dx, dy, z0, h, "L", cols_l, rows, lit, arch=arch)
    windows(svg, iso, rng, x, y, dx, dy, z0, h, "R", cols_r, rows, lit * 0.6, arch=arch)
    if door_on:
        door(svg, iso, x, y, dx, dy, "L", s=0.5 if cols_l % 2 else 0.35, w=0.5 / cols_l, hgt=min(0.4, 11 / h), z0=z0, h=h)
    svg.line(left[3], left[0], shade(wall, 1.3), 1.0, 0.55)  # sunlit west edge
    svg.line(left[2], left[3], shade(wall, 1.2), 0.8, 0.35)
    roof(svg, iso, x, y, dx, dy, z0 + h, roof_kind, roof_col, rh, o, tiles=tiles)


def chimney(svg, iso, x, y, h, brick="#74483a", w=0.52, z0=0, smoke=True, seed=0):
    _shadow(svg, iso, x, y, w, w, h, op=0.3)
    p = iso.p
    lw, rw = shade(brick, 1.12), shade(brick, 0.6)
    ink = outline(brick, 0.4)
    svg.poly([p(x, y + w, z0), p(x + w, y + w, z0), p(x + w, y + w, z0 + h), p(x, y + w, z0 + h)], svg.lin([(0, shade(brick, 1.25)), (1, shade(brick, 0.85))], 0, 0, 1, 0), ink, 0.9)
    svg.poly([p(x + w, y + w, z0), p(x + w, y, z0), p(x + w, y, z0 + h), p(x + w, y + w, z0 + h)], rw, ink, 0.9)
    svg.poly([p(x, y + w, z0), p(x + w * 0.28, y + w, z0), p(x + w * 0.28, y + w, z0 + h), p(x, y + w, z0 + h)], shade(brick, 1.35), op=0.35)
    for i in range(1, int(h // 6)):  # brick courses
        z = z0 + i * 6
        svg.line(p(x, y + w, z), p(x + w, y + w, z), shade(brick, 0.7), 0.6, 0.5)
    cap = [p(x - 0.08, y - 0.08, z0 + h + 3), p(x + w + 0.08, y - 0.08, z0 + h + 3), p(x + w + 0.08, y + w + 0.08, z0 + h + 3), p(x - 0.08, y + w + 0.08, z0 + h + 3)]
    svg.poly([p(x - 0.08, y + w + 0.08, z0 + h), p(x + w + 0.08, y + w + 0.08, z0 + h), p(x + w + 0.08, y + w + 0.08, z0 + h + 3), p(x - 0.08, y + w + 0.08, z0 + h + 3)], shade(brick, 0.95), ink, 0.8)
    svg.poly([p(x + w + 0.08, y + w + 0.08, z0 + h), p(x + w + 0.08, y - 0.08, z0 + h), p(x + w + 0.08, y - 0.08, z0 + h + 3), p(x + w + 0.08, y + w + 0.08, z0 + h + 3)], shade(brick, 0.55), ink, 0.8)
    svg.poly(cap, "#2a1f19", ink, 0.8)
    if smoke:
        top = p(x + w / 2, y + w / 2, z0 + h + 3)
        puffs(svg, top, seed, size=8.5, dark=(seed % 2 == 0))


def puffs(svg, base, seed=0, n=6, drift=(8, -13), size=7.5, op=0.8, dark=False):
    rng = random.Random(seed)
    lo, mid, hi = ("#8d8a85", "#5a5855", "#383736") if dark else ("#dcd7cb", "#b0ada5", "#807f7a")
    soft = svg.blur(0.9)
    for i in range(n):
        t = i / max(1, n - 1)
        cx = base[0] + drift[0] * (i + 0.5) + rng.uniform(-2.5, 2.5)
        cy = base[1] + drift[1] * (i + 0.5) + rng.uniform(-2.5, 2.5)
        r = size * (0.55 + 0.95 * t) + rng.uniform(-0.8, 0.8)
        a = op * (1.0 - 0.72 * t)
        svg.circle(cx + r * 0.15, cy + r * 0.2, r, hi, op=a * 0.9, blur=soft)
        svg.circle(cx, cy, r * 0.92, mid, op=a, blur=soft)
        svg.circle(cx - r * 0.22, cy - r * 0.25, r * 0.62, lo, op=a, blur=soft)


def pagoda(svg, iso, rng, x, y, size, tiers=3, wall="#7d4d3a", roof_col="#575660", h0=21, step=0.8, rh=7):
    """Tiered tower: a dark timber landmark in the spirit of the reference's temple."""
    cx, cy = x + size / 2, y + size / 2
    z = 0.0
    sz = size
    _shadow(svg, iso, x, y, size, size, tiers * h0 + 14, op=0.38)
    # stone plinth
    lw, rw, _ = wall_style("#9a9484")
    plinth = (x - 0.15, y - 0.15, size + 0.3, size + 0.3, 4)
    px, py, pdx, pdy, ph = plinth
    p = iso.p
    svg.poly([p(px, py + pdy, 0), p(px + pdx, py + pdy, 0), p(px + pdx, py + pdy, ph), p(px, py + pdy, ph)], lw, outline("#9a9484", 0.5), 0.9)
    svg.poly([p(px + pdx, py + pdy, 0), p(px + pdx, py, 0), p(px + pdx, py, ph), p(px + pdx, py + pdy, ph)], rw, outline("#9a9484", 0.5), 0.9)
    svg.poly([p(px, py, ph), p(px + pdx, py, ph), p(px + pdx, py + pdy, ph), p(px, py + pdy, ph)], shade("#9a9484", 1.15), outline("#9a9484", 0.5), 0.8)
    z = ph
    for i in range(tiers):
        bx, by = cx - sz / 2, cy - sz / 2
        h = h0 * (1.0 - 0.06 * i)
        building(svg, iso, rng, bx + 0.0, by + 0.0, sz, sz, h, wall, "pyramid_curved", roof_col, rh=rh, cols=2, rows=1, z0=z, door_on=(i == 0), lit=0.4, o=0.5)
        z += h + rh * 0.35
        sz *= step
    # finial spire
    top = iso.p(cx, cy, z + rh * 0.4)
    svg.line(top, (top[0], top[1] - 20), "#2a2018", 1.8)
    for i, rr in enumerate((3.2, 2.5, 1.9)):
        svg.ellipse(top[0], top[1] - 5 - i * 4.2, rr, rr * 0.45, "#c9a85c", stroke="#5a4520", sw=0.5)
    return (top[0], top[1] - 22)


def flag(svg, x, y, w=24, h=15, wave=2.2, cloth="#f3e8cf", emblem="#b3261e", pole_h=0, pole=True):
    """Wind-blown banner with the rising-sun emblem (original; not any national flag)."""
    if pole and pole_h:
        svg.line((x, y), (x, y + pole_h), "#3a2c1e", 1.6)
        svg.circle(x, y - 0.5, 1.5, "#c9a85c")
    top = [(x + i * w / 8, y + 1 + math.sin(i * 0.9) * wave) for i in range(9)]
    bot = [(x + i * w / 8, y + 1 + h + math.sin(i * 0.9 + 0.35) * wave) for i in range(9)]
    pts = top + bot[::-1]
    svg.poly(pts, svg.lin([(0, shade(cloth, 1.04)), (0.5, cloth), (1, shade(cloth, 0.8))], 0, 0, 1, 0.4), shade(cloth, 0.45), 0.8)
    cx = x + w * 0.46
    cy = y + 1 + h * 0.55 + math.sin(4 * 0.9) * wave * 0.8
    svg.path(f"M{cx - h*0.30:.2f},{cy:.2f} A{h*0.30:.2f},{h*0.30:.2f} 0 0 1 {cx + h*0.30:.2f},{cy:.2f} Z", emblem)
    for k in range(-2, 3):
        a = math.pi * (0.5 + k * 0.24)
        svg.line((cx + math.cos(a) * h * 0.36, cy - math.sin(a) * h * 0.36), (cx + math.cos(a) * h * 0.52, cy - math.sin(a) * h * 0.52), emblem, 1.1)


def rail(svg, iso, pts_xy, z=0.2, ties=True):
    """Narrow-gauge railway on the ground along a polyline given in grid coordinates."""
    path = [iso.p(x, y, z) for x, y in pts_xy]
    d = "M" + " L".join(f"{a:.2f},{b:.2f}" for a, b in path)
    svg.path(d, stroke="#2a2018", sw=4.2, op=0.55)
    for (x0, y0), (x1, y1) in zip(pts_xy, pts_xy[1:]):
        n = int(math.hypot(x1 - x0, y1 - y0) / 0.28)
        for i in range(n):
            t = (i + 0.5) / n
            x, y = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            ang = math.atan2(y1 - y0, x1 - x0)
            nx, ny = -math.sin(ang) * 0.22, math.cos(ang) * 0.22
            svg.line(iso.p(x - nx, y - ny, z), iso.p(x + nx, y + ny, z), "#5a3f2a", 1.5, 0.95)
    svg.path(d, stroke="#8e8a80", sw=0.9, op=0.9)


# ---- the city ------------------------------------------------------------------------------------
def build_city_svg(seed=7) -> Svg:
    rng = random.Random(seed)
    svg = Svg(W, H)
    iso = Iso(CX, CY - 64, 16)  # 8 x 8 grid exactly fills the 256 x 128 ground diamond
    p = iso.p

    # worn earth footprint, slightly irregular, with a darker rim that sells depth
    pts = []
    for i in range(36):
        th = i / 36 * math.tau
        rr = 3.95 * (1 + 0.035 * math.sin(th * 5 + 1) + 0.025 * math.sin(th * 9))
        pts.append(p(4 + math.cos(th) * rr, 4 + math.sin(th) * rr, 0))
    svg.poly(pts, "#0b0e07", op=0.42, extra=f'filter="{svg.blur(5)}" transform="translate(5 6)"')
    svg.poly(pts, svg.rad([(0, "#9b8a62"), (0.7, "#85744e"), (1, "#5f5237")], 0.45, 0.4, 0.62), "#4a3f2a", 1.4)
    # cobbles and trodden paths
    for _ in range(46):
        a, b = rng.uniform(0.7, 7.3), rng.uniform(0.7, 7.3)
        q = p(a, b, 0)
        svg.ellipse(q[0], q[1], rng.uniform(2.2, 4.2), rng.uniform(1.1, 2.0), shade("#a39370", rng.uniform(0.8, 1.15)), op=0.5)
    # a rail spur running to the lower edge for the future railroad overlay
    rail(svg, iso, [(0.7, 4.4), (2.4, 4.7), (4.2, 5.4), (7.3, 6.9)])

    # lots: (x, y, dx, dy, height, wall, roof kind, roof colour, roof height); no footprint overlaps
    B = [
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
    drawables = [(b[0] + b[2] / 2 + b[1] + b[3] / 2, "b", b) for b in B]
    drawables += [(2.0, "c", (4.9, 0.55, 78)), (2.3, "c", (7.35, 0.7, 64)), (4.7, "c", (2.55, 2.55, 52))]
    drawables += [(4.3, "pagoda", (3.3, 1.2, 1.5))]
    flag_at = None
    for _, kind, d in sorted(drawables, key=lambda t: t[0]):
        if kind == "b":
            building(svg, iso, rng, *d[:5], d[5], d[6], d[7], rh=d[8])
        elif kind == "c":
            chimney(svg, iso, d[0], d[1], d[2], seed=int(d[0] * 10))
        else:
            flag_at = pagoda(svg, iso, rng, d[0], d[1], d[2])
    if flag_at:
        flag(svg, flag_at[0], flag_at[1] - 8, pole_h=14)
    return svg
