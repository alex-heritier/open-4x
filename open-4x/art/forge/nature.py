"""Forest and mountain overlay sprites (256 x 224 at 2x; game draws them 128 x 112)."""

import math
import random

from .svgkit import Iso, Svg, lerp, shade, mixc

W, H = 256, 224
CX, CY = 128, 160  # ground diamond centre; half-axes 128 x 64


def in_diamond(x, y, margin=0.0):
    return abs(x - CX) / (128 - margin * 2) + abs(y - CY) / (64 - margin) <= 1.0


# ---- trees ---------------------------------------------------------------------------------------
LEAF_STOPS = [(0.0, "#16301a"), (0.28, "#25481f"), (0.52, "#3a6828"), (0.76, "#5b8a30"), (1.0, "#8fb040")]
FIR_STOPS = [(0.0, "#12291a"), (0.30, "#1f4222"), (0.55, "#325d28"), (0.80, "#4d7a2d"), (1.0, "#78a038")]
JUNGLE_STOPS = [(0.0, "#0c3317"), (0.28, "#145a24"), (0.52, "#23812c"), (0.76, "#46a834"), (1.0, "#8fd048")]
FROST_STOPS = [(0.0, "#10282a"), (0.30, "#1d4236"), (0.55, "#2f5f46"), (0.80, "#557f5d"), (1.0, "#8fb09c")]
AUTUMN = ["#b9702a", "#cf9236", "#a2532a", "#d9a443", "#c4622b"]
LIGHT = (-0.55, -0.83)  # towards the upper-left sun, in screen space


def _ramp(stops, t):
    from .palette import ramp
    import numpy as np
    from .svgkit import hexc
    c = ramp(np.array([t], dtype=np.float32), stops)[0] * 255
    return hexc(c)


def trunk(svg, x, y, h, w=3.0):
    svg.poly([(x - w / 2, y), (x + w / 2, y), (x + w * 0.35, y - h), (x - w * 0.35, y - h)], "#3a2a1c", "#1d130c", 0.4)


def ground_shadow(svg, x, y, r, soft):
    svg.ellipse(x + r * 0.6, y + r * 0.12, r * 1.25, r * 0.42, "#0b1107", op=0.5, blur=soft)


def _dabs(svg, rng, inside, bbox, n, stops, centre, radius, size=(2.3, 1.5), droop=0.0, autumn=0.075):
    """Scatter paint dabs inside a silhouette; tone follows the upper-left light."""
    x0, y0, x1, y1 = bbox
    done = 0
    pts = []
    while done < n:
        px, py = rng.uniform(x0, x1), rng.uniform(y0, y1)
        if inside(px, py):
            pts.append((px, py))
            done += 1
    pts.sort(key=lambda q: q[1])  # lower dabs first so upper ones overlap them
    for px, py in pts:
        nx, ny = (px - centre[0]) / radius, (py - centre[1]) / radius
        t = 0.50 + 0.42 * (nx * LIGHT[0] + ny * LIGHT[1]) + rng.uniform(-0.17, 0.17)
        if rng.random() < autumn and t > 0.45:
            col = rng.choice(AUTUMN)
        else:
            col = _ramp(stops, max(0.0, min(1.0, t)))
        sx = size[0] * rng.uniform(0.8, 1.3)
        sy = size[1] * rng.uniform(0.8, 1.3)
        svg.ellipse(px, py, sx, sy, col, op=0.93, rot=rng.uniform(-35, 35) + droop)


def broadleaf(svg, x, y, h, rng, soft, stops=LEAF_STOPS, autumn=0.075):
    """Round crown of paint dabs: dark rim, sun-struck upper-left, amber flecks."""
    w = h * 0.34
    ground_shadow(svg, x, y, w * 0.9, soft)
    trunk(svg, x, y, h * 0.44, 3.4)
    cx, cy = x, y - h * 0.64
    rx, ry = w, h * 0.34
    lobes = [(0, 0, 1.0), (-0.45, 0.12, 0.62), (0.45, 0.12, 0.62), (-0.2, -0.3, 0.55), (0.25, -0.28, 0.5)]

    def inside(px, py):
        return any(((px - cx - dx * rx) / (rx * r)) ** 2 + ((py - cy - dy * ry * 1.2) / (ry * r)) ** 2 <= 1.0 for dx, dy, r in lobes)

    # dark silhouette first
    for dx, dy, r in lobes:
        svg.ellipse(cx + dx * rx, cy + dy * ry * 1.2, rx * r * 1.04, ry * r * 1.04, "#142a14", stroke="#0e1d0d", sw=0.8)
    n = int(rx * ry * 0.5)
    _dabs(svg, rng, inside, (cx - rx * 1.4, cy - ry * 1.4, cx + rx * 1.4, cy + ry * 1.6), n, stops, (cx, cy), rx * 1.2, (2.6, 1.7), autumn=autumn)


def conifer(svg, x, y, h, rng, soft, stops=FIR_STOPS):
    """Fir made of drooping needle dabs in stacked skirts; dark gaps between the tiers."""
    w = h * 0.52
    ground_shadow(svg, x, y, w * 0.6, soft)
    trunk(svg, x, y, h * 0.2)
    tiers = 4
    for i in range(tiers):
        t = i / (tiers - 1)
        ty = y - h * 0.10 - h * 0.74 * t
        tw = w * (1.0 - 0.60 * t)
        th = h * 0.36
        apex = (x, ty - th)
        tri = [apex, (x + tw / 2, ty + 1.2), (x - tw / 2, ty + 1.2)]
        svg.poly(tri, "#10241a", "#0b1a10", 0.7)

        def inside(px, py, apex=apex, ty=ty, tw=tw, th=th):
            if py < apex[1] or py > ty + 2.0:
                return False
            return abs(px - x) <= (py - apex[1]) / th * tw / 2 + 1.2

        n = int(tw * th * 0.5)
        _dabs(svg, rng, inside, (x - tw / 2 - 1, apex[1], x + tw / 2 + 1, ty + 2), n, stops, (x, ty - th * 0.5), max(tw * 0.55, th * 0.6), (2.4, 1.3), droop=18, autumn=0.0 if stops is not FIR_STOPS else 0.075)
        svg.line((x - tw * 0.5, ty + 1.8), (x + tw * 0.5, ty + 2.6), "#0a160d", 1.5, 0.45)


def scatter(rng, n, min_d, margin=6.0, tries=600):
    pts = []
    for _ in range(tries):
        x = rng.uniform(CX - 128 + margin, CX + 128 - margin)
        y = rng.uniform(CY - 64 + margin / 2, CY + 64 - margin / 2)
        if not in_diamond(x, y, margin):
            continue
        if all((x - a) ** 2 + ((y - b) * 1.9) ** 2 > min_d**2 for a, b in pts):
            pts.append((x, y))
        if len(pts) >= n:
            break
    return pts


def palm(svg, x, y, h, rng, soft):
    """Leaning trunk with a crown of arching fronds."""
    ground_shadow(svg, x, y, h * 0.30, soft)
    lean = rng.uniform(-0.22, 0.22) * h
    top = (x + lean, y - h)
    ctrl = (x - lean * 0.3, y - h * 0.55)
    svg.path(f"M{x - 1.6:.1f},{y:.1f} Q{ctrl[0]:.1f},{ctrl[1]:.1f} {top[0]:.1f},{top[1]:.1f}", stroke="#2e2014", sw=4.2)
    svg.path(f"M{x - 1.2:.1f},{y:.1f} Q{ctrl[0]:.1f},{ctrl[1]:.1f} {top[0]:.1f},{top[1]:.1f}", stroke="#8a6a3e", sw=2.8)
    for k in range(1, 6):
        t = k / 6
        px = (1 - t) ** 2 * (x - 1.2) + 2 * (1 - t) * t * ctrl[0] + t**2 * top[0]
        py = (1 - t) ** 2 * y + 2 * (1 - t) * t * ctrl[1] + t**2 * top[1]
        svg.line((px - 2.2, py), (px + 1.6, py - 0.8), "#4d3822", 0.9, 0.7)
    cx, cy = top
    for ang in range(0, 360, 40):
        a = math.radians(ang + rng.uniform(-8, 8))
        length = h * rng.uniform(0.42, 0.56)
        ex, ey = cx + math.cos(a) * length, cy + math.sin(a) * length * 0.5 + length * 0.22
        mx, my = cx + math.cos(a) * length * 0.55, cy + math.sin(a) * length * 0.28 - length * 0.20
        light = 0.5 + 0.4 * (math.cos(a) * LIGHT[0] + math.sin(a) * LIGHT[1]) + rng.uniform(-0.1, 0.1)
        dark = _ramp(JUNGLE_STOPS, max(0.0, min(1.0, light - 0.25)))
        mid = _ramp(JUNGLE_STOPS, max(0.0, min(1.0, light)))
        d = f"M{cx:.1f},{cy:.1f} Q{mx:.1f},{my:.1f} {ex:.1f},{ey:.1f}"
        svg.path(d, stroke=dark, sw=5.4, op=0.95)
        svg.path(d, stroke=mid, sw=3.4, op=0.95)
        for i in range(1, 6):  # leaflets drooping off the rib
            t = i / 6
            qx = (1 - t) ** 2 * cx + 2 * (1 - t) * t * mx + t**2 * ex
            qy = (1 - t) ** 2 * cy + 2 * (1 - t) * t * my + t**2 * ey
            for side in (-1, 1):
                svg.line((qx, qy), (qx + side * 3.4 * (1 - t * 0.5), qy + 2.8), mid, 1.3, 0.9)
    svg.ellipse(cx, cy + 0.8, 2.4, 2.0, "#4d3822")


def build_forest_svg(seed=11, trees=34, conifers=0.55, leaf=LEAF_STOPS, fir=FIR_STOPS, palms=0.0, floor="#16210c", size=(38, 54), spacing=17) -> Svg:
    """A stand of trees on the tile.  `conifers` and `palms` are shares of the trees; the rest are
    broadleaf.  The palettes turn it into temperate forest, boreal pine or jungle."""
    rng = random.Random(seed)
    svg = Svg(W, H)
    soft = svg.blur(1.8)
    pts = sorted(scatter(rng, trees, spacing, margin=8), key=lambda q: q[1])
    # dark forest floor so the ground below reads as shade between crowns
    svg.ellipse(CX, CY + 4, 108, 48, floor, op=0.55, blur=svg.blur(9))
    for x, y in pts:
        depth = (y - (CY - 64)) / 128  # nearer trees are larger
        h = rng.uniform(*size) * (0.88 + 0.26 * depth)
        roll = rng.random()
        if roll < palms:
            palm(svg, x, y, h * 0.9, rng, soft)
        elif roll < palms + conifers:
            conifer(svg, x, y, h, rng, soft, fir)
        else:
            broadleaf(svg, x, y, h, rng, soft, leaf, 0.0 if leaf is not LEAF_STOPS else 0.075)
    return svg


def build_pine_svg(seed=13) -> Svg:
    """Boreal pine: only firs, paler and bluer, a little sparser than the mixed forest."""
    return build_forest_svg(seed, trees=32, conifers=1.0, fir=FROST_STOPS, floor="#11221d", size=(40, 58), spacing=18)


def build_jungle_svg(seed=17) -> Svg:
    """Dense, saturated canopy with palms standing out of it."""
    svg = build_forest_svg(seed, trees=44, conifers=0.0, leaf=JUNGLE_STOPS, palms=0.28, floor="#0b2410", size=(40, 56), spacing=14)
    rng = random.Random(seed + 5)
    soft = svg.blur(1.4)
    for _ in range(26):  # undergrowth: bright ferns between the trunks
        x, y = rng.uniform(CX - 100, CX + 100), rng.uniform(CY - 40, CY + 56)
        if in_diamond(x, y, 10):
            svg.ellipse(x, y, rng.uniform(4, 8), rng.uniform(2, 3.4), _ramp(JUNGLE_STOPS, rng.uniform(0.5, 1.0)), op=0.55, rot=rng.uniform(-30, 30), blur=soft)
    return svg


def build_marsh_svg(seed=19) -> Svg:
    """Waterlogged ground: dark wet earth, still pools, tussocks, reeds and cattails."""
    rng = random.Random(seed)
    svg = Svg(W, H)
    soft = svg.blur(2.2)
    svg.ellipse(CX, CY + 2, 112, 54, "#2b3b25", op=0.55, blur=svg.blur(9))
    pools = []
    for _ in range(40):
        if len(pools) >= 7:
            break
        x, y = rng.uniform(CX - 100, CX + 100), rng.uniform(CY - 44, CY + 46)
        r = rng.uniform(15, 28)
        if in_diamond(x, y, 14) and all(((x - a) / (r + c)) ** 2 + ((y - b) * 1.7 / (r + c)) ** 2 > 0.9 for a, b, c in pools):
            pools.append((x, y, r))
    pools = [(x, y, r) for x, y, r in pools]
    for x, y, r in pools:
        svg.ellipse(x, y + 1, r + 3, r * 0.5 + 2.2, "#2d3a1f", op=0.75, blur=soft)  # muddy rim
        svg.ellipse(x, y, r, r * 0.5, svg.rad([(0, "#6fb7b1"), (0.6, "#3f8f8f"), (1, "#2f6e72")], 0.4, 0.35, 0.75), stroke="#1f4a4a", sw=0.8)
        svg.ellipse(x - r * 0.25, y - r * 0.12, r * 0.45, r * 0.12, "#cdeae4", op=0.45)  # sky glint
    for _ in range(26):  # tussocks of sedge
        x, y = rng.uniform(CX - 105, CX + 105), rng.uniform(CY - 48, CY + 52)
        if in_diamond(x, y, 10):
            svg.ellipse(x, y, rng.uniform(5, 10), rng.uniform(2.4, 4.2), _ramp([(0, "#3d5a22"), (0.6, "#5f7e2e"), (1, "#8c9e45")], rng.uniform(0.2, 0.9)), op=0.9, rot=rng.uniform(-20, 20))
    for _ in range(30):  # reed clumps, nearer clumps drawn later and taller
        x, y = rng.uniform(CX - 100, CX + 100), rng.uniform(CY - 44, CY + 50)
        if not in_diamond(x, y, 12):
            continue
        h = rng.uniform(14, 26) * (0.85 + 0.3 * (y - (CY - 64)) / 128)
        for k in range(rng.randint(5, 9)):
            lean = rng.uniform(-6, 6)
            bx = x + rng.uniform(-3.5, 3.5)
            col = rng.choice(["#6f8a32", "#8da23f", "#a4a74c", "#58742b"])
            svg.path(f"M{bx:.1f},{y:.1f} Q{bx + lean * 0.4:.1f},{y - h * 0.55:.1f} {bx + lean:.1f},{y - h * rng.uniform(0.7, 1.0):.1f}", stroke=col, sw=rng.uniform(0.9, 1.5), op=0.95)
        if rng.random() < 0.45:  # cattail
            bx = x + rng.uniform(-2, 2)
            svg.path(f"M{bx:.1f},{y:.1f} L{bx + 1:.1f},{y - h * 1.05:.1f}", stroke="#6b5a32", sw=0.9)
            svg.ellipse(bx + 1, y - h * 1.12, 1.6, 4.4, "#4b3420", stroke="#2a1c10", sw=0.4)
    return svg


# ---- mountains -----------------------------------------------------------------------------------
ROCK_L, ROCK_M, ROCK_D = "#9a8f7b", "#756d5f", "#4b463f"
SNOW_L, SNOW_M, SNOW_D = "#f6f3e8", "#cfd5dd", "#9aa6b8"


def _peak(svg, rng, cx, base_y, h, w, snow=0.42, rock=(ROCK_L, ROCK_M, ROCK_D)):
    """One snow-capped Fuji-style peak: concave flanks, ridge/gully strokes, ragged snowline."""
    L, M, D = rock
    top = (cx - w * 0.04, base_y - h)
    left_foot = (cx - w / 2, base_y)
    right_foot = (cx + w / 2, base_y)
    ctrl_l = (cx - w * 0.2, base_y - h * 0.28)
    ctrl_r = (cx + w * 0.22, base_y - h * 0.30)
    body = (
        f"M{left_foot[0]:.1f},{left_foot[1]:.1f} Q{ctrl_l[0]:.1f},{ctrl_l[1]:.1f} {top[0]:.1f},{top[1]:.1f} "
        f"Q{ctrl_r[0]:.1f},{ctrl_r[1]:.1f} {right_foot[0]:.1f},{right_foot[1]:.1f} "
        f"Q{cx:.1f},{base_y + h * 0.10:.1f} {left_foot[0]:.1f},{left_foot[1]:.1f} Z"
    )
    clip_id = svg.uid("mc")
    svg.defs.append(f'<clipPath id="{clip_id}"><path d="{body}"/></clipPath>')
    g = svg.lin([(0, L), (0.45, M), (1, D)], 0.0, 0.2, 1.0, 0.9)
    svg.path(body, g, shade(D, 0.45), 1.4)
    svg.begin(f'clip-path="url(#{clip_id})"')
    # shaded east half: from the summit down the right flank
    svg.poly([top, (cx + w * 0.08, base_y - h * 0.4), (cx + w * 0.02, base_y + 4), (cx + w * 0.7, base_y + 4), (cx + w * 0.7, base_y - h * 0.2)], D, op=0.55)
    # gullies and ridges radiating from the summit
    for i in range(15):
        t = (i + rng.uniform(-0.2, 0.2)) / 14
        bx = cx - w * 0.55 + w * 1.1 * t
        shadowy = t > 0.42
        col = shade(D, 0.75) if shadowy else shade(M, 0.7)
        mid = lerp(top, (bx, base_y), rng.uniform(0.35, 0.55))
        mid = (mid[0] + rng.uniform(-4, 4), mid[1])
        svg.path(f"M{top[0]:.1f},{top[1] + 6:.1f} Q{mid[0]:.1f},{mid[1]:.1f} {bx:.1f},{base_y:.1f}", stroke=col, sw=rng.uniform(1.1, 2.4), op=0.55)
        if not shadowy:
            svg.path(f"M{top[0] - 2:.1f},{top[1] + 8:.1f} Q{mid[0] - 2:.1f},{mid[1]:.1f} {bx - 3:.1f},{base_y:.1f}", stroke=shade(L, 1.2), sw=1.0, op=0.40)
    # rock speckle
    for _ in range(110):
        t = rng.random()
        yy = top[1] + (base_y - top[1]) * (t**0.8)
        half = (yy - top[1]) / h * w * 0.5 * 0.95
        xx = top[0] + rng.uniform(-half, half)
        svg.ellipse(xx, yy, rng.uniform(1.5, 4.0), rng.uniform(0.8, 1.8), shade(L if xx < cx else D, rng.uniform(0.7, 1.25)), op=0.32, rot=rng.uniform(-30, 30))
    svg.end()
    # snow cap with a ragged lower edge following the gullies
    snow_y = top[1] + h * snow
    edge = []
    n = 13
    for i in range(n + 1):
        t = i / n
        yy = snow_y + math.sin(t * 17.0 + 1) * h * 0.035 + (0.08 * h if i % 2 else -0.03 * h) * rng.uniform(0.4, 1.0)
        half = (yy - top[1]) / h * w * 0.5 * 0.96
        edge.append((top[0] - half + 2 * half * t, yy))
    snow_poly = [top] + [edge[0]] + edge + [edge[-1]]
    clip_snow = svg.uid("sc")
    svg.defs.append(f'<clipPath id="{clip_snow}"><path d="{body}"/></clipPath>')
    svg.begin(f'clip-path="url(#{clip_snow})"')
    svg.poly(snow_poly, svg.lin([(0, SNOW_L), (0.6, SNOW_M), (1, SNOW_D)], 0.1, 0.1, 0.95, 0.8), shade(SNOW_D, 0.7), 0.9)
    right_half = [top] + [e for e in edge if e[0] >= top[0]] + [(edge[-1][0], edge[-1][1])]
    svg.poly(right_half, SNOW_D, op=0.55)
    for i in range(7):
        t = (i + 0.5) / 7
        e = lerp(edge[1], edge[-2], t)
        svg.path(f"M{top[0] + (t - 0.5) * 6:.1f},{top[1] + 5:.1f} L{e[0]:.1f},{e[1]:.1f}", stroke="#8794a8", sw=1.1, op=0.4)
    svg.end()
    return top


def build_mountain_svg(seed=5) -> Svg:
    rng = random.Random(seed)
    svg = Svg(W, H)
    soft = svg.blur(4)
    # broad foothill apron and cast shadow
    svg.ellipse(CX + 14, CY + 22, 112, 38, "#0b0f08", op=0.5, blur=soft)
    apron = [(CX - 118, CY + 14), (CX - 60, CY - 14), (CX + 6, CY - 22), (CX + 70, CY - 14), (CX + 120, CY + 12), (CX + 60, CY + 52), (CX - 10, CY + 62), (CX - 80, CY + 48)]
    svg.poly(apron, svg.rad([(0, "#6d7a36"), (0.75, "#4f5e27"), (1, "#3a461e")], 0.5, 0.4, 0.7), "#2e3a18", 1.0, op=0.95)
    for _ in range(46):
        x, y = rng.uniform(CX - 105, CX + 105), rng.uniform(CY - 10, CY + 54)
        if in_diamond(x, y, 6):
            svg.ellipse(x, y, rng.uniform(3, 7), rng.uniform(1.5, 3), shade("#72803a", rng.uniform(0.75, 1.3)), op=0.5)
    _peak(svg, rng, CX - 62, CY + 34, 64, 100, 0.40)
    _peak(svg, rng, CX + 58, CY + 38, 74, 112, 0.42)
    _peak(svg, rng, CX - 2, CY + 44, 120, 160, 0.46)
    # scrub and a few firs on the lower slopes, in front of the rock
    soft2 = svg.blur(1.6)
    for x, y, h in [(CX - 70, CY + 52, 26), (CX - 44, CY + 58, 30), (CX + 16, CY + 62, 28), (CX + 62, CY + 56, 26), (CX + 92, CY + 44, 22), (CX - 98, CY + 38, 22)]:
        conifer(svg, x, y, h, rng, soft2)
    return svg
