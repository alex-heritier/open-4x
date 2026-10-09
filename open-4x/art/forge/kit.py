"""Round and organic isometric primitives for the city skins: drums, cones, domes, trees, huts.

`buildings.py` holds the box-and-roof kit. This module adds what boxes cannot do (round walls,
conical roofs, domes, onion domes, spires, palms, palisades, stilts) in the same lighting: the key
light is upper left, so every curved surface runs light on the left to dark on the right.
Positions are grid coordinates of an `Iso`; heights are screen pixels.
"""

import math
import random

from .buildings import building, outline, puffs, roof
from .svgkit import Iso, Svg, lerp, mixc, shade

K = 16.0
SQ2 = math.sqrt(2.0)


def radii(r):
    """Screen radii (horizontal, vertical) of a circle of grid radius `r` on the ground."""
    return r * K * SQ2, r * K * SQ2 / 2


def drop_shadow(svg: Svg, iso: Iso, x, y, r, h, op=0.3):
    """Soft shadow of a round thing, thrown to the lower right."""
    rx, ry = radii(r)
    cx, cy = iso.p(x, y, 0)
    svg.ellipse(cx + h * 0.42 + rx * 0.2, cy + h * 0.1 + ry * 0.15, rx * 1.05 + h * 0.34, ry * 1.0 + h * 0.07, "#0d1009", op=op, blur=svg.blur(2.4))


def _side_fill(svg: Svg, col):
    return svg.lin([(0, shade(col, 1.2)), (0.28, shade(col, 1.03)), (0.7, shade(col, 0.7)), (1, shade(col, 0.5))], 0, 0, 1, 0)


def cylinder(svg, iso, x, y, r, h, col, z0=0.0, shadow=True, top=True, bands=(), ink=0.42, grain=0, seed=0):
    """A round wall. `bands` is [(from, to, colour)] as fractions of the height."""
    cx, cy = iso.p(x, y, z0)
    rx, ry = radii(r)
    if shadow:
        drop_shadow(svg, iso, x, y, r, h + z0)
    body = (
        f"M{cx - rx:.2f},{cy - h:.2f} L{cx - rx:.2f},{cy:.2f} A{rx:.2f},{ry:.2f} 0 0 0 {cx + rx:.2f},{cy:.2f} "
        f"L{cx + rx:.2f},{cy - h:.2f} A{rx:.2f},{ry:.2f} 0 0 0 {cx - rx:.2f},{cy - h:.2f} Z"
    )
    svg.path(body, _side_fill(svg, col), stroke=shade(col, ink), sw=1.0)
    for a, b, c in bands:
        y0, y1 = cy - h * b, cy - h * a
        d = (
            f"M{cx - rx:.2f},{y0:.2f} L{cx - rx:.2f},{y1:.2f} A{rx:.2f},{ry:.2f} 0 0 0 {cx + rx:.2f},{y1:.2f} "
            f"L{cx + rx:.2f},{y0:.2f} A{rx:.2f},{ry:.2f} 0 0 0 {cx - rx:.2f},{y0:.2f} Z"
        )
        svg.path(d, _side_fill(svg, c), op=0.95)
    if grain:
        rng = random.Random(seed)
        for _ in range(grain):
            u = rng.uniform(-0.95, 0.95)
            gx = cx + u * rx
            gy = cy + ry * math.sqrt(max(0.0, 1 - u * u)) - rng.uniform(0.06, 0.95) * h
            svg.ellipse(gx, gy, rng.uniform(1.2, 2.8), rng.uniform(0.6, 1.3), shade(col, rng.uniform(0.62, 1.3)), op=0.25)
    if top:
        svg.ellipse(cx, cy - h, rx, ry, shade(col, 1.14), stroke=shade(col, ink), sw=0.8)
    return cx, cy - h


def cone(svg, cx, cy, R, H, col, ridges=14, ink=0.4, seed=0, texture=True, lean=0.0, eave=True):
    """A conical roof whose base ellipse is centred at screen (cx, cy)."""
    ry = R / 2
    apex = (cx + lean, cy - H)
    d = f"M{cx - R:.2f},{cy:.2f} L{apex[0]:.2f},{apex[1]:.2f} L{cx + R:.2f},{cy:.2f} A{R:.2f},{ry:.2f} 0 0 1 {cx - R:.2f},{cy:.2f} Z"
    svg.path(d, _side_fill(svg, col), stroke=shade(col, ink), sw=1.0)
    if texture:
        rng = random.Random(seed)
        for i in range(ridges):
            th = math.pi * (i + 0.5) / ridges  # from the right edge round the front to the left
            base = (cx + R * math.cos(th), cy + ry * math.sin(th))
            a = lerp(apex, base, rng.uniform(0.05, 0.45))
            f = 0.5 + 0.62 * (th / math.pi)
            svg.line(a, base, shade(col, f * rng.uniform(0.7, 1.0)), 0.9, 0.5)
    if eave:
        svg.path(f"M{cx - R:.2f},{cy:.2f} A{R:.2f},{ry:.2f} 0 0 0 {cx + R:.2f},{cy:.2f}", stroke=shade(col, 0.5), sw=1.6, op=0.55)
    return apex


def dome(svg, cx, cy, R, H, col, ink=0.4, ring=True, ribs=0):
    """A hemisphere (or flattened cap) whose base ellipse is centred at screen (cx, cy)."""
    ry = R / 2
    d = f"M{cx - R:.2f},{cy:.2f} A{R:.2f},{H:.2f} 0 0 1 {cx + R:.2f},{cy:.2f} A{R:.2f},{ry:.2f} 0 0 1 {cx - R:.2f},{cy:.2f} Z"
    fill = svg.rad([(0, shade(col, 1.42)), (0.4, shade(col, 1.05)), (1, shade(col, 0.5))], 0.36, 0.3, 0.82)
    svg.path(d, fill, stroke=shade(col, ink), sw=1.0)
    for i in range(ribs):
        t = (i + 1) / (ribs + 1)
        bx = cx - R + 2 * R * t
        by = cy + ry * math.sqrt(max(0.0, 1 - ((bx - cx) / R) ** 2))
        svg.path(f"M{cx:.2f},{cy - H:.2f} Q{(cx + bx) / 2 + (bx - cx) * 0.25:.2f},{(cy - H + by) / 2 - H * 0.12:.2f} {bx:.2f},{by:.2f}", stroke=shade(col, 0.62), sw=0.8, op=0.45)
    if ring:
        svg.path(f"M{cx - R:.2f},{cy:.2f} A{R:.2f},{ry:.2f} 0 0 0 {cx + R:.2f},{cy:.2f}", stroke=shade(col, 0.5), sw=1.5, op=0.5)
    svg.ellipse(cx - R * 0.4, cy - H * 0.58, R * 0.17, H * 0.1, shade(col, 1.75), op=0.55, rot=-28, blur=svg.blur(0.8))
    return (cx, cy - H)


def finial(svg, top, h=8, col="#d6b25e", crescent=False, cross=False):
    x, y = top
    svg.line((x, y), (x, y - h), shade(col, 0.55), 1.7)
    svg.line((x, y), (x, y - h), col, 1.0)
    if cross:
        svg.line((x - 3.2, y - h * 0.62), (x + 3.2, y - h * 0.62), shade(col, 0.55), 1.7)
        svg.line((x - 3.2, y - h * 0.62), (x + 3.2, y - h * 0.62), col, 1.0)
        svg.line((x - 2, y - h * 0.3 + 1.2), (x + 2, y - h * 0.3 - 1.2), col, 0.9)
    elif crescent:
        svg.path(f"M{x - 2.6:.2f},{y - h - 2:.2f} A3.2,3.2 0 1 0 {x + 2.6:.2f},{y - h - 2:.2f} A2.4,2.4 0 1 1 {x - 2.6:.2f},{y - h - 2:.2f} Z", col, stroke=shade(col, 0.55), sw=0.5)
    else:
        svg.circle(x, y - h - 0.5, 1.7, col, stroke=shade(col, 0.55), sw=0.5)


def onion(svg, cx, cy, R, H, col, cross=True, ink=0.38):
    """A bulbous Orthodox dome with its tip drawn out to a point."""
    n = R * 0.58
    d = (
        f"M{cx - n:.2f},{cy:.2f} C{cx - R * 1.38:.2f},{cy - H * 0.3:.2f} {cx - R * 0.5:.2f},{cy - H * 0.64:.2f} {cx:.2f},{cy - H:.2f} "
        f"C{cx + R * 0.5:.2f},{cy - H * 0.64:.2f} {cx + R * 1.38:.2f},{cy - H * 0.3:.2f} {cx + n:.2f},{cy:.2f} A{n:.2f},{n / 2:.2f} 0 0 1 {cx - n:.2f},{cy:.2f} Z"
    )
    fill = svg.rad([(0, shade(col, 1.5)), (0.38, shade(col, 1.08)), (1, shade(col, 0.52))], 0.34, 0.4, 0.78)
    svg.path(d, fill, stroke=shade(col, ink), sw=1.0)
    svg.ellipse(cx - R * 0.42, cy - H * 0.36, R * 0.14, H * 0.1, shade(col, 1.8), op=0.55, rot=-35, blur=svg.blur(0.7))
    if cross:
        finial(svg, (cx, cy - H + 1), h=min(14, H * 0.5), cross=True)
    return (cx, cy - H)


def minaret(svg, iso, x, y, h, wall, cap, r=0.3):
    cylinder(svg, iso, x, y, r, h * 0.6, wall, top=False)
    cylinder(svg, iso, x, y, r * 1.7, 2.6, shade(wall, 0.92), z0=h * 0.6 - 1.2, top=True)
    cylinder(svg, iso, x, y, r * 0.8, h * 0.28, wall, z0=h * 0.6 + 1.0, shadow=False, top=False)
    cx, cy = iso.p(x, y, h * 0.88 + 1.0)
    apex = cone(svg, cx, cy, radii(r)[0] * 0.95, h * 0.2, cap, texture=False, eave=False)
    finial(svg, apex, h=7, crescent=True)


def palm(svg, iso, x, y, h=46, lean=7, seed=0, leaf="#3f7a35", trunk="#7a5a3a"):
    rng = random.Random(seed)
    bx, by = iso.p(x, y, 0)
    top = (bx + lean, by - h)
    svg.ellipse(bx + 7, by + 1, 11, 4.2, "#0d1009", op=0.26, blur=svg.blur(2.0))
    ctl = (bx - lean * 0.2 - 2, by - h * 0.55)
    svg.path(f"M{bx:.2f},{by:.2f} Q{ctl[0]:.2f},{ctl[1]:.2f} {top[0]:.2f},{top[1]:.2f}", stroke=shade(trunk, 0.55), sw=4.4)
    svg.path(f"M{bx - 0.4:.2f},{by:.2f} Q{ctl[0] - 0.4:.2f},{ctl[1]:.2f} {top[0] - 0.4:.2f},{top[1]:.2f}", stroke=trunk, sw=2.8)
    for i in range(1, 7):
        t = i / 7
        q = ((1 - t) ** 2 * bx + 2 * (1 - t) * t * ctl[0] + t * t * top[0], (1 - t) ** 2 * by + 2 * (1 - t) * t * ctl[1] + t * t * top[1])
        svg.line((q[0] - 2.2, q[1]), (q[0] + 2.2, q[1] - 0.8), shade(trunk, 0.6), 0.7, 0.7)
    for a in (160, 195, 228, 262, 296, 330, 20, 118):
        th = math.radians(a + rng.uniform(-6, 6))
        length = h * rng.uniform(0.42, 0.55)
        end = (top[0] + math.cos(th) * length, top[1] + math.sin(th) * length * 0.55 + length * 0.2)
        mid = (top[0] + math.cos(th) * length * 0.55, top[1] + math.sin(th) * length * 0.5 - 7)
        svg.path(f"M{top[0]:.2f},{top[1]:.2f} Q{mid[0]:.2f},{mid[1]:.2f} {end[0]:.2f},{end[1]:.2f}", stroke=shade(leaf, 0.6), sw=4.0, op=0.96)
        svg.path(f"M{top[0]:.2f},{top[1] - 0.6:.2f} Q{mid[0]:.2f},{mid[1] - 1.0:.2f} {end[0]:.2f},{end[1] - 0.6:.2f}", stroke=shade(leaf, 1.25), sw=1.5, op=0.9)
    svg.circle(top[0] + 1.5, top[1] + 2.5, 2.3, "#6b4a2a")
    svg.circle(top[0] - 1.5, top[1] + 3, 2.0, "#7a5632")


def tree(svg, iso, x, y, h=36, kind="round", seed=0, leaf="#4f7a2e", trunk="#5e4430"):
    rng = random.Random(seed)
    bx, by = iso.p(x, y, 0)
    svg.ellipse(bx + h * 0.35, by + 1, h * 0.38, h * 0.1, "#0d1009", op=0.26, blur=svg.blur(2.4))
    if kind == "pine":
        svg.line((bx, by), (bx, by - h * 0.25), trunk, 3.0)
        for i in range(4):
            w = h * (0.34 - 0.065 * i)
            ty = by - h * (0.16 + 0.22 * i)
            svg.poly([(bx - w, ty), (bx + w, ty), (bx, ty - h * 0.36)], shade(leaf, 0.7 + 0.12 * i), stroke=shade(leaf, 0.4), sw=0.8)
            svg.poly([(bx - w, ty), (bx, ty), (bx, ty - h * 0.36)], shade(leaf, 1.05 + 0.08 * i), op=0.85)
        return
    if kind == "baobab":
        svg.path(
            f"M{bx - 7:.2f},{by:.2f} C{bx - 6:.2f},{by - h * 0.35:.2f} {bx - 3.2:.2f},{by - h * 0.55:.2f} {bx - 3.6:.2f},{by - h * 0.7:.2f} L{bx + 3.6:.2f},{by - h * 0.7:.2f} "
            f"C{bx + 3.2:.2f},{by - h * 0.55:.2f} {bx + 6:.2f},{by - h * 0.35:.2f} {bx + 7:.2f},{by:.2f} Z",
            svg.lin([(0, shade("#9a8466", 1.2)), (0.5, "#8a7556"), (1, shade("#8a7556", 0.55))], 0, 0, 1, 0),
            stroke="#3c3022", sw=0.9,
        )
        for dx, dy, r in ((-9, -h * 0.82, 7), (0, -h * 0.9, 8), (9, -h * 0.8, 7), (-3, -h * 0.76, 6)):
            svg.line((bx + dx * 0.3, by - h * 0.7), (bx + dx, by + dy + 4), "#6b5a42", 2.0)
            svg.circle(bx + dx, by + dy, r, shade(leaf, 0.85), op=0.95)
            svg.circle(bx + dx - 1.5, by + dy - 1.8, r * 0.62, shade(leaf, 1.25), op=0.9)
        return
    if kind == "acacia":
        svg.path(f"M{bx:.2f},{by:.2f} Q{bx - 2:.2f},{by - h * 0.4:.2f} {bx + 1:.2f},{by - h * 0.62:.2f}", stroke=shade(trunk, 0.6), sw=3.0)
        cy = by - h * 0.7
        for dx, dy, rx, ry in ((-10, 2, 11, 4.6), (9, 3, 11, 4.4), (0, -2, 13, 5.4), (-3, -5, 9, 4.0)):
            svg.ellipse(bx + dx + 1, cy + dy, rx, ry, shade(leaf, 0.78))
            svg.ellipse(bx + dx, cy + dy - 1.2, rx * 0.86, ry * 0.7, shade(leaf, 1.18), op=0.95)
        return
    # round canopy; "birch" is pale-trunked and yellow-green
    if kind == "birch":
        svg.line((bx, by), (bx + 0.6, by - h * 0.55), "#e4e0d2", 2.6)
        for k in range(3):
            svg.line((bx + 0.3, by - h * (0.15 + 0.15 * k)), (bx + 1.4, by - h * (0.15 + 0.15 * k)), "#2a2a2a", 0.9)
        leaf = "#8fa84a"
    else:
        svg.path(f"M{bx:.2f},{by:.2f} L{bx + 0.4:.2f},{by - h * 0.5:.2f}", stroke=shade(trunk, 0.7), sw=3.6)
    for _ in range(6):
        dx, dy = rng.uniform(-h * 0.2, h * 0.2), rng.uniform(-h * 0.14, h * 0.1)
        r = h * rng.uniform(0.17, 0.26)
        svg.circle(bx + dx + 1.2, by - h * 0.64 + dy + 1.4, r, shade(leaf, 0.62), op=0.95)
    for _ in range(7):
        dx, dy = rng.uniform(-h * 0.2, h * 0.17), rng.uniform(-h * 0.16, h * 0.08)
        r = h * rng.uniform(0.11, 0.19)
        svg.circle(bx + dx - 1.0, by - h * 0.66 + dy - 1.0, r, shade(leaf, rng.uniform(0.92, 1.3)), op=0.96)


def post(svg, iso, x, y, h=8, col="#6b4a2c", w=1.7, top="round"):
    px, py = iso.p(x, y, 0)
    svg.rect(px - w, py - h, w * 2, h, svg.lin([(0, shade(col, 1.2)), (1, shade(col, 0.6))], 0, 0, 1, 0), rx=1.0 if top == "round" else 0, stroke=shade(col, 0.4), sw=0.5)


def fence_posts(points, spacing=0.36):
    """Posts along a polyline of grid points, as (x, y)."""
    out = []
    for (x0, y0), (x1, y1) in zip(points, points[1:]):
        n = max(1, int(math.hypot(x1 - x0, y1 - y0) / spacing))
        for i in range(n + 1):
            t = i / n
            out.append((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t))
    return out


def arc_points(cx, cy, r, a0, a1, n=16):
    return [(cx + r * math.cos(math.radians(a0 + (a1 - a0) * i / n)), cy + r * math.sin(math.radians(a0 + (a1 - a0) * i / n))) for i in range(n + 1)]


def stilts(svg, iso, x, y, dx, dy, lift, col="#5b4026"):
    """Four posts under a raised floor, with a slab on top."""
    for px, py in ((x + dx - 0.12, y + 0.05), (x + 0.05, y + 0.05), (x + dx - 0.12, y + dy - 0.12), (x + 0.05, y + dy - 0.12)):
        a, b = iso.p(px, py, 0), iso.p(px, py, lift)
        svg.line(a, b, shade(col, 0.55), 3.4)
        svg.line((a[0] - 0.5, a[1]), (b[0] - 0.5, b[1]), col, 2.0)


def earth(svg, iso, rng, rim="#5f5237", mid="#85744e", core="#9b8a62", stone="#a39370", n=46, spots=0.5, radius=3.95):
    """The worn ground a town sits on: a dark-rimmed irregular disc with a scatter of stones."""
    p = iso.p
    pts = []
    for i in range(36):
        th = i / 36 * math.tau
        rr = radius * (1 + 0.035 * math.sin(th * 5 + 1) + 0.025 * math.sin(th * 9))
        pts.append(p(4 + math.cos(th) * rr, 4 + math.sin(th) * rr, 0))
    svg.poly(pts, "#0b0e07", op=0.42, extra=f'filter="{svg.blur(5)}" transform="translate(5 6)"')
    svg.poly(pts, svg.rad([(0, core), (0.7, mid), (1, rim)], 0.45, 0.4, 0.62), shade(rim, 0.75), 1.4)
    for _ in range(n):
        a, b = rng.uniform(0.7, 7.3), rng.uniform(0.7, 7.3)
        q = p(a, b, 0)
        svg.ellipse(q[0], q[1], rng.uniform(2.2, 4.2), rng.uniform(1.1, 2.0), shade(stone, rng.uniform(0.8, 1.15)), op=spots)


def path_strip(svg, iso, pts, col, width=0.7, op=0.55):
    """A trodden road along grid points."""
    d = "M" + " L".join(f"{a:.2f},{b:.2f}" for a, b in (iso.p(x, y, 0.1) for x, y in pts))
    svg.path(d, stroke=shade(col, 0.8), sw=width * K * 1.2, op=op * 0.7)
    svg.path(d, stroke=col, sw=width * K * 0.9, op=op)


def tuft(svg, iso, x, y, col="#5f7a2c"):
    bx, by = iso.p(x, y, 0)
    for dx in (-3, 0, 3):
        svg.line((bx + dx, by), (bx + dx * 1.5, by - 5 - abs(dx) * 0.2), shade(col, 0.8 + abs(dx) * 0.05), 1.1, 0.9)


def rack(svg, iso, x, y, w=1.4, h=13, col="#6b4a2c", hang="#b4573a"):
    """A drying rack: two A-frames and a crossbar with strips hanging from it."""
    a0, a1 = iso.p(x, y, 0), iso.p(x + w, y + w * 0.0, 0)
    for base in (a0, a1):
        svg.line((base[0] - 3, base[1]), (base[0], base[1] - h), col, 1.6)
        svg.line((base[0] + 3, base[1]), (base[0], base[1] - h), shade(col, 0.7), 1.6)
    svg.line((a0[0], a0[1] - h), (a1[0], a1[1] - h), shade(col, 0.8), 1.8)
    for i in range(1, 6):
        t = i / 6
        sx, sy = a0[0] + (a1[0] - a0[0]) * t, a0[1] + (a1[1] - a0[1]) * t - h
        svg.line((sx, sy), (sx + 0.4, sy + 6 + (i % 3) * 1.4), hang if i % 2 else shade(hang, 0.8), 1.4, 0.95)


def awning(svg, iso, x, y, dx, dy, z, cols=("#b3261e", "#efe3c4"), stripes=6):
    """A striped market canopy over a stall."""
    p = iso.p
    for i in range(stripes):
        t0, t1 = i / stripes, (i + 1) / stripes
        quad = [p(x + dx * t0, y, z + 2), p(x + dx * t1, y, z + 2), p(x + dx * t1, y + dy, z - 1), p(x + dx * t0, y + dy, z - 1)]
        svg.poly(quad, cols[i % 2] if i % 2 == 0 else cols[1], stroke=shade(cols[0], 0.5), sw=0.4)
    svg.poly([p(x, y + dy, z - 1), p(x + dx, y + dy, z - 1), p(x + dx, y + dy, z - 3), p(x, y + dy, z - 3)], shade(cols[0], 0.8))


def horse(svg, x, y, s=1.0, col="#7a4e2c", facing=1, seed=0):
    """A tiny standing horse in screen space."""
    f = facing
    body = [(x - 7 * s, y - 7 * s), (x + 5 * s, y - 7 * s), (x + 7 * s, y - 5 * s), (x + 5 * s, y - 2 * s), (x - 6 * s, y - 2 * s)]
    svg.poly([(a if f > 0 else 2 * x - a, b) for a, b in body], col, stroke=shade(col, 0.45), sw=0.6)
    neck = [(x + 4 * s, y - 7 * s), (x + 8 * s, y - 12 * s), (x + 11 * s, y - 11 * s), (x + 7 * s, y - 5 * s)]
    svg.poly([(a if f > 0 else 2 * x - a, b) for a, b in neck], shade(col, 1.08), stroke=shade(col, 0.45), sw=0.6)
    for lx in (-5, -2.5, 3, 5):
        a = x + lx * s * f
        svg.line((a, y - 2.5 * s), (a, y + 2 * s), shade(col, 0.55), 1.3)
    svg.line((x - 6.5 * s * f, y - 6.5 * s), (x - 9 * s * f, y - 2 * s), shade(col, 0.4), 1.4)


def canoe(svg, x, y, length=40, col="#7a4a2a", float_col="#8a5a34"):
    """An outrigger canoe lying on the sand, in screen space."""
    hull = f"M{x - length / 2:.2f},{y - 1:.2f} Q{x:.2f},{y - 7:.2f} {x + length / 2:.2f},{y - 4:.2f} Q{x:.2f},{y + 8:.2f} {x - length / 2:.2f},{y - 1:.2f} Z"
    svg.path(hull, svg.lin([(0, shade(col, 1.2)), (1, shade(col, 0.6))], 0, 0, 1, 0), stroke=shade(col, 0.4), sw=0.8)
    fx, fy = x - 2, y + 13
    svg.path(f"M{fx - length * 0.36:.2f},{fy:.2f} Q{fx:.2f},{fy + 4:.2f} {fx + length * 0.36:.2f},{fy - 2:.2f} Q{fx:.2f},{fy:.2f} {fx - length * 0.36:.2f},{fy:.2f} Z", float_col, stroke=shade(float_col, 0.4), sw=0.6)
    for k in (-0.2, 0.2):
        svg.line((x + length * k, y + 1.5), (fx + length * k * 0.9, fy), "#4a3220", 1.2)
