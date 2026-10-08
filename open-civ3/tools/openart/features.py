"""Ground-level feature sheets: rivers, roads, irrigation, huts, buildings.

Contracts (all cells 128x64 unless noted; the diamond's centre is (64, 32)):

  * rivers  4x4, cell = corner mask, bit0 NW, bit1 NE, bit2 SW, bit3 SE. The
    cell is centred on a tile *corner*; a spoke runs from the middle to the
    midpoint of the diamond edge on that side and a little beyond, so
    neighbouring cells join.
  * roads   16x16, cell = 8-bit neighbour mask, bit i the map neighbour
    N, NE, E, SE, S, SW, W, NW = screen upper-right, right, lower-right,
    down, lower-left, left, upper-left, up. Edge neighbours exit through the
    edge midpoint, diagonal ones through the diamond vertex.
  * irrigation 4x4 per soil, cell = edge mask, bit0 NW, bit1 NE, bit2 SW,
    bit3 SE (map W, N, S, E).
"""
import math

from PIL import Image, ImageChops, ImageDraw, ImageFilter

from . import gfx, iso
from .gfx import KEY, mix
from .overlays import _sampler, sheet

S = 3
W, H = 128, 64
C = (64, 32)

SPOKE = {0: (-1, -1), 1: (1, -1), 2: (-1, 1), 3: (1, 1)}      # NW NE SW SE


def _mask_canvas():
    return Image.new("L", (W * S, H * S), 0)


def _pt(p):
    return (p[0] * S, p[1] * S)


def _bez(a, c, b, n=14):
    return [((1 - t) ** 2 * a[0] + 2 * t * (1 - t) * c[0] + t * t * b[0],
             (1 - t) ** 2 * a[1] + 2 * t * (1 - t) * c[1] + t * t * b[1]) for t in [i / n for i in range(n + 1)]]


def _stroke(draw, pts, width, fill=255):
    draw.line([_pt(p) for p in pts], fill=fill, width=max(1, int(width * S)), joint="curve")
    r = width * S / 2
    for p in (pts[0], pts[-1]):
        x, y = _pt(p)
        draw.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def _down(m):
    return m.resize((W, H), Image.Resampling.LANCZOS)


# ---------------------------------------------------------------- rivers

RIVER = {
    "delta": dict(bank=(88, 108, 66), deep=(40, 98, 170), shallow=(98, 164, 220), foam=(214, 238, 248), wb=8.0, ww=5.6),
    "mtn": dict(bank=(96, 92, 84), deep=(66, 128, 196), shallow=(142, 198, 234), foam=(246, 250, 252), wb=5.6, ww=3.6),
}


def river_cell(mask, style):
    st = RIVER[style]
    bank, water, rapids = _mask_canvas(), _mask_canvas(), _mask_canvas()
    bd, wd, rd = ImageDraw.Draw(bank), ImageDraw.Draw(water), ImageDraw.Draw(rapids)
    for bit, (sx, sy) in SPOKE.items():
        if not mask >> bit & 1:
            continue
        end = (C[0] + sx * 38, C[1] + sy * 19)
        d = (end[0] - C[0], end[1] - C[1])
        n = (-d[1] / 43.0, d[0] / 43.0)
        amp = (1 if bit % 2 else -1) * 3.0
        mid = (C[0] + d[0] * 0.5 + n[0] * amp * 2, C[1] + d[1] * 0.5 + n[1] * amp * 2)
        pts = _bez(C, mid, end)
        _stroke(bd, pts, st["wb"])
        _stroke(wd, pts, st["ww"])
        # a thin bright streak along the lit bank
        off = [(x - 0.9, y - 0.7) for x, y in pts[2:-2]]
        _stroke(rd, off, 1.1, 255 if style == "mtn" else 150)
    if mask:
        x, y = _pt(C)
        r = (st["wb"] / 2) * S
        bd.ellipse([x - r, y - r, x + r, y + r], fill=255)
        r = (st["ww"] / 2) * S
        wd.ellipse([x - r, y - r, x + r, y + r], fill=255)
    bank, water, rapids = _down(bank), _down(water), _down(rapids)
    rgb = Image.new("RGB", (W, H), KEY)
    body = _sampler(70 + mask, 6, 2)
    img = Image.new("RGB", (W, H), st["bank"])
    px = img.load()
    wm = water.load()
    for y in range(H):
        for x in range(W):
            if wm[x, y] > 40:
                t = body(x * 1.3, y * 2.0)
                px[x, y] = mix(st["deep"], st["shallow"], 0.25 + 0.7 * t)
    img.paste(Image.new("RGB", (W, H), st["foam"]), mask=ImageChops.multiply(rapids, water).point([min(255, i * 2) for i in range(256)]))
    # dark inner edge so the channel reads as cut into the ground
    edge = ImageChops.subtract(water, water.filter(ImageFilter.MinFilter(3)))
    img.paste(Image.new("RGB", (W, H), gfx.shade_rgb(st["deep"], 0.62)), mask=edge)
    alpha = bank.point([255 if i >= 110 else 0 for i in range(256)])
    # the bank is a darker, ground-coloured fringe; keep it thin on the shadow side
    rgb.paste(img, mask=alpha)
    return rgb, alpha, Image.new("L", (W, H), 0)


def rivers(style):
    return sheet(4, 4, W, H, lambda i: river_cell(i, style))


# ---------------------------------------------------------------- roads

# bit -> (screen vector from the centre, thickness class)
ROAD_END = {
    0: ((32, -16), 5.0), 1: ((64, 0), 4.2), 2: ((32, 16), 5.0), 3: ((0, 32), 6.4),
    4: ((-32, 16), 5.0), 5: ((-64, 0), 4.2), 6: ((-32, -16), 5.0), 7: ((0, -32), 6.4),
}
ROAD = dict(base=(176, 148, 104), light=(200, 174, 128), edge=(118, 92, 62), pebble=(138, 114, 82))


def road_cell(mask):
    body, rim = _mask_canvas(), _mask_canvas()
    bd, rd = ImageDraw.Draw(body), ImageDraw.Draw(rim)
    for bit, ((dx, dy), wid) in ROAD_END.items():
        if not mask >> bit & 1:
            continue
        end = (C[0] + dx * 1.04, C[1] + dy * 1.04)
        n = math.hypot(dx, dy)
        mid = (C[0] + dx * 0.5 + (-dy / n) * 1.6, C[1] + dy * 0.5 + (dx / n) * 1.6)
        pts = _bez(C, mid, end, 10)
        _stroke(rd, pts, wid + 2.2)
        _stroke(bd, pts, wid)
    # the hub, which is all there is when nothing connects
    x, y = _pt(C)
    rx, ry = (7.5 if mask else 11.0) * S, (3.9 if mask else 5.6) * S
    rd.ellipse([x - rx - 2 * S, y - ry - S, x + rx + 2 * S, y + ry + S], fill=255)
    bd.ellipse([x - rx, y - ry, x + rx, y + ry], fill=255)
    body, rim = _down(body), _down(rim)
    tex = _sampler(80 + (mask & 15), 7, 3)
    img = Image.new("RGB", (W, H), ROAD["edge"])
    px = img.load()
    bm = body.load()
    for yy in range(H):
        for xx in range(W):
            if bm[xx, yy] > 60:
                t = tex(xx, yy * 2)
                c = mix(ROAD["base"], ROAD["light"], 0.2 + 0.8 * t)
                if t < 0.22:
                    c = mix(c, ROAD["pebble"], 0.7)
                px[xx, yy] = c
    alpha = rim.point([255 if i >= 110 else 0 for i in range(256)])
    rgb = Image.new("RGB", (W, H), KEY)
    rgb.paste(img, mask=alpha)
    return rgb, alpha, Image.new("L", (W, H), 0)


def roads():
    return sheet(16, 16, W, H, road_cell)


# ---------------------------------------------------------------- irrigation

SOIL = {
    "grass": dict(soil=(110, 82, 52), crop=(96, 154, 62), every=1),
    "plains": dict(soil=(134, 102, 60), crop=(212, 182, 80), every=1),
    "desert": dict(soil=(192, 158, 102), crop=(122, 152, 74), every=2),
    "tundra": dict(soil=(88, 82, 70), crop=(128, 146, 104), every=2),
}
WATER = dict(deep=(52, 116, 190), shallow=(120, 186, 232), bank=(78, 62, 44))


def irrigation_cell(mask, soil):
    st = SOIL[soil]
    cv = iso.Canvas((W, H), S)
    h = 24.0
    outer = [iso.P(C[0], C[1], -h, -h, 0), iso.P(C[0], C[1], h, -h, 0), iso.P(C[0], C[1], h, h, 0), iso.P(C[0], C[1], -h, h, 0)]
    cv.poly(outer, gfx.shade_rgb(st["soil"], 0.62))
    inner = [iso.P(C[0], C[1], -h + 1.6, -h + 1.6, 0), iso.P(C[0], C[1], h - 1.6, -h + 1.6, 0),
             iso.P(C[0], C[1], h - 1.6, h - 1.6, 0), iso.P(C[0], C[1], -h + 1.6, h - 1.6, 0)]
    cv.poly(inner, st["soil"])
    k = 0
    v = -h + 4.0
    while v < h - 3:
        k += 1
        col = st["crop"] if k % st["every"] == 0 else gfx.shade_rgb(st["soil"], 0.82)
        cv.line([iso.P(C[0], C[1], -h + 3, v, 0), iso.P(C[0], C[1], h - 3, v, 0)], col, 1.5)
        if k % st["every"] == 0:
            cv.line([iso.P(C[0], C[1], -h + 4, v - 0.9, 0), iso.P(C[0], C[1], h - 5, v - 0.9, 0)], gfx.shade_rgb(col, 1.25), 0.7)
        v += 4.6
    # channels
    for bit, (sx, sy) in SPOKE.items():
        if mask >> bit & 1:
            end = (C[0] + sx * 38, C[1] + sy * 19)
            cv.line([C, end], WATER["bank"], 5.2)
            cv.line([C, end], WATER["deep"], 3.4)
            cv.line([(C[0] - sx * -0.8, C[1] - 0.7), (end[0] - sx * -0.8, end[1] - 0.7)], WATER["shallow"], 0.9)
    cv.ellipse((C[0] - 11, C[1] - 5.6, C[0] + 11, C[1] + 5.6), WATER["bank"])
    cv.ellipse((C[0] - 9, C[1] - 4.5, C[0] + 9, C[1] + 4.5), WATER["deep"])
    cv.ellipse((C[0] - 6.5, C[1] - 3.4, C[0] + 3, C[1] + 0.8), WATER["shallow"])
    img, _ = cv.done()
    alpha = img.getchannel("A").point([255 if i >= 128 else 0 for i in range(256)])
    rgb = Image.new("RGB", (W, H), KEY)
    rgb.paste(img.convert("RGB"), mask=alpha)
    return rgb, alpha, Image.new("L", (W, H), 0)


def irrigation(soil):
    return sheet(4, 4, W, H, lambda i: irrigation_cell(i, soil))


# ---------------------------------------------------------------- finishing

def finish(cv, rim, size=(W, H)):
    """Reduce a canvas to a cell: (rgb, alpha, shadow) with a coloured rim."""
    img, sh = cv.done(size)
    img = gfx.outline(img, rim, 1)
    alpha = img.getchannel("A").point([255 if i >= 110 else 0 for i in range(256)])
    sh = sh.filter(ImageFilter.GaussianBlur(0.8))
    sh = ImageChops.multiply(sh, ImageChops.invert(alpha))
    rgb = Image.new("RGB", size, KEY)
    rgb.paste(img.convert("RGB"), mask=alpha)
    return rgb, alpha, sh


STRAW = (206, 178, 96)
ADOBE = (214, 180, 132)
STONE = (150, 146, 138)
WOOD = (122, 88, 56)
DARK = (44, 32, 26)
RIM = (46, 34, 24)


def _hut_round(cv, cx, cy, wall, roof, r=12, h=9):
    iso.cylinder(cv, cx, cy, r, 0, h, wall)
    iso.cone(cv, cx, cy + 0.0, r + 2.5, h - 1, 13, roof)
    cv.poly([(cx - 3, cy + r * 0.5 + 0.2), (cx + 3, cy + r * 0.5 + 0.2), (cx + 3, cy + r * 0.5 - 7), (cx - 3, cy + r * 0.5 - 7)], DARK)


def _tepee(cv, cx, cy, skin, r=11, h=22):
    iso.cone(cv, cx, cy, r, 0, h, skin)
    cv.poly([(cx - 3.2, cy + r * 0.5 - 0.4), (cx + 3.2, cy + r * 0.5 - 0.4), (cx, cy + r * 0.5 - 11)], DARK)
    for dx in (-3, 0, 3):
        cv.line([(cx + dx * 0.4, cy - h + 1), (cx + dx * 0.7, cy - h - 5)], WOOD, 0.9)


def _adobe(cv, cx, cy, wall, roof):
    iso.box(cv, cx, cy, 11, 9, 13, wall)
    iso.box(cv, cx, cy, 12, 10, 1.6, tuple(min(255, int(c * 0.9)) for c in wall), z0=13)
    ux, uy = iso.P(cx, cy, 2, 9, 0)
    cv.poly([(ux - 3, uy), (ux + 3, uy - 0.0), (ux + 3, uy - 9), (ux - 3, uy - 9)], DARK)


def _longhouse(cv, cx, cy, wall, roof):
    iso.box(cv, cx, cy, 17, 8, 8, wall)
    iso.gable(cv, cx, cy, 17, 8, 8, 10, roof, "u", overhang=1.8)
    ux, uy = iso.P(cx, cy, 4, 8, 0)
    cv.poly([(ux - 2.6, uy), (ux + 2.6, uy), (ux + 2.6, uy - 7), (ux - 2.6, uy - 7)], DARK)


def _fence(cv, cx, cy, r):
    for i in range(10):
        t = math.pi * (0.1 + 0.8 * i / 9)
        x, y = cx + r * math.cos(t), cy + r * 0.5 * math.sin(t)
        cv.line([(x, y), (x, y - 5)], WOOD, 0.9)
    cv.line([(cx - r * 0.93, cy + 1.3 - 3.2), (cx + r * 0.93, cy + 1.3 - 3.2)], WOOD, 0.6)


def _fire(cv, x, y):
    cv.ellipse((x - 3.2, y - 1.4, x + 3.2, y + 1.4), (70, 60, 56))
    cv.poly([(x - 2, y), (x, y - 6), (x + 2, y)], (240, 140, 40))
    cv.poly([(x - 1, y), (x, y - 3.4), (x + 1, y)], (255, 226, 120))


def hut_cell(i):
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 38
    cv.shadow_ellipse(cx + 5, cy + 7, 20, 6)
    k = i % 8
    if k == 0:
        _hut_round(cv, cx, cy, ADOBE, STRAW)
    elif k == 1:
        _fence(cv, cx, cy + 3, 27)
        _hut_round(cv, cx, cy, (190, 160, 118), (176, 150, 84), 11, 8)
    elif k == 2:
        _tepee(cv, cx, cy, (226, 204, 160))
    elif k == 3:
        _tepee(cv, cx - 15, cy - 2, (214, 186, 140), 9, 18)
        _tepee(cv, cx + 14, cy + 3, (232, 214, 172), 10, 20)
        _fire(cv, cx, cy + 9)
    elif k == 4:
        _adobe(cv, cx, cy + 1, (226, 192, 142), STONE)
    elif k == 5:
        _hut_round(cv, cx - 14, cy - 1, STONE, (110, 130, 84), 9, 7)
        _hut_round(cv, cx + 13, cy + 3, (176, 168, 150), STRAW, 10, 8)
    elif k == 6:
        _longhouse(cv, cx, cy, (168, 124, 84), (150, 108, 62))
    else:
        _hut_round(cv, cx, cy, (176, 130, 90), (120, 150, 80), 11, 8)
        cv.line([(cx + 20, cy + 9), (cx + 20, cy - 16)], WOOD, 1.3)
        cv.poly([(cx + 20, cy - 16), (cx + 28, cy - 13), (cx + 20, cy - 8)], (206, 62, 54))
    return finish(cv, RIM)


def huts():
    return sheet(3, 3, W, H, lambda i: hut_cell(i) if i < 8 else (Image.new("RGB", (W, H), KEY), Image.new("L", (W, H), 0), Image.new("L", (W, H), 0)))


# ---------------------------------------------------------------- terrain buildings (512x256: 4 cols x 4 eras)

ERA_WALL = [(138, 100, 62), (156, 152, 144), (150, 120, 96), (132, 136, 140)]


def _wall_ring(cv, cx, cy, half, thick, height, colour, towers, gate=True, crenel=False):
    """Four wall runs around the centre with optional corner towers."""
    t, hh = thick, half
    runs = [  # (u0, v0, hu, hv, back?)
        (0, -hh, hh, t, True), (-hh, 0, t, hh, True), (0, hh, hh, t, False), (hh, 0, t, hh, False)]
    runs.sort(key=lambda r: r[0] + r[1])
    for u0, v0, hu, hv, back in runs:
        x, y = iso.P(cx, cy, u0, v0, 0)
        hgt = height * (1 if back else 0.72)
        iso.box(cv, x, y, hu, hv, hgt, colour)
        if crenel and back:
            n = int(max(hu, hv) / 4)
            for i in range(-n + 1, n, 2):
                if hu > hv:
                    px, py = iso.P(cx, cy, u0 + i * 2.0, v0, hgt)
                    iso.box(cv, px, py, 1.4, t, 2.0, colour)
                else:
                    px, py = iso.P(cx, cy, u0, v0 + i * 2.0, hgt)
                    iso.box(cv, px, py, t, 1.4, 2.0, colour)
    if towers:
        for u0, v0 in sorted([(-hh, -hh), (hh, -hh), (-hh, hh), (hh, hh)], key=lambda p: p[0] + p[1]):
            x, y = iso.P(cx, cy, u0, v0, 0)
            iso.cylinder(cv, x, y, 5.2, 0, height + 5, gfx.shade_rgb(colour, 1.08))
            iso.cone(cv, x, y, 6.3, height + 4, 6, (150, 70, 52))


def fortress_cell(era):
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 34
    cv.shadow_ellipse(cx + 6, cy + 6, 44, 14)
    col = ERA_WALL[era]
    if era == 0:      # palisade
        _wall_ring(cv, cx, cy, 20, 1.2, 9, col, False)
        for u0, v0 in ((-20, -20), (20, -20), (-20, 20), (20, 20)):
            x, y = iso.P(cx, cy, u0, v0, 0)
            iso.box(cv, x, y, 2.2, 2.2, 14, col)
    elif era == 1:    # stone wall and towers
        _wall_ring(cv, cx, cy, 21, 2.2, 10, col, True, crenel=True)
    elif era == 2:    # earthwork star with cannon
        _wall_ring(cv, cx, cy, 21, 3.4, 7, col, False)
        x, y = iso.P(cx, cy, 6, 6, 0)
        iso.cylinder(cv, x, y, 3.6, 0, 3.2, (60, 60, 64))
        cv.line([(x - 2, y - 4), (x + 8, y + 0)], (40, 40, 44), 2.4)
    else:             # concrete bunkers
        _wall_ring(cv, cx, cy, 20, 3.0, 6, col, False)
        for u0, v0 in ((-12, -12), (12, 12), (-12, 12), (12, -12)):
            x, y = iso.P(cx, cy, u0, v0, 0)
            iso.box(cv, x, y, 4.2, 4.2, 6.5, (118, 122, 126))
            iso.box(cv, x, y, 3.0, 3.0, 2.2, (84, 88, 92), z0=6.5)
    return finish(cv, RIM)


def colony_cell(era):
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 36
    cv.shadow_ellipse(cx + 5, cy + 6, 30, 9)
    wall = [(180, 134, 90), (214, 190, 150), (196, 150, 120), (176, 178, 184)][era]
    roof = [(140, 98, 54), (170, 86, 62), (120, 72, 64), (96, 100, 108)][era]
    iso.box(cv, cx - 12, cy + 2, 10, 8, 9, wall)
    iso.gable(cv, cx - 12, cy + 2, 10, 8, 9, 9, roof, "u", overhang=1.4)
    iso.box(cv, cx + 12, cy - 2, 8, 7, 8, gfx.shade_rgb(wall, 0.94))
    iso.gable(cv, cx + 12, cy - 2, 8, 7, 8, 8, gfx.shade_rgb(roof, 1.1), "v", overhang=1.3)
    iso.flag(cv, cx + 1, cy + 8, 24, (214, 64, 56))
    return finish(cv, RIM)


def camp_cell():
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 36
    cv.shadow_ellipse(cx + 5, cy + 7, 34, 10)
    for i in range(14):
        t = math.tau * i / 14
        x, y = cx + 28 * math.cos(t), cy + 14 * math.sin(t)
        cv.poly([(x - 1.5, y), (x + 1.5, y), (x, y - 14 - (i % 3) * 2)], (112, 80, 52))
    _tepee(cv, cx - 12, cy - 3, (150, 52, 44), 10, 18)
    _tepee(cv, cx + 12, cy + 2, (116, 44, 40), 11, 20)
    _fire(cv, cx, cy + 9)
    iso.flag(cv, cx + 24, cy + 6, 20, (30, 28, 30))
    return finish(cv, RIM)


def mine_cell():
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 38
    cv.shadow_ellipse(cx + 6, cy + 7, 30, 9)
    # spoil heap
    cv.poly([(cx - 30, cy + 8), (cx - 6, cy - 14), (cx + 16, cy - 12), (cx + 34, cy + 8), (cx, cy + 15)], (138, 122, 100))
    cv.poly([(cx - 6, cy - 14), (cx + 16, cy - 12), (cx + 34, cy + 8), (cx + 8, cy + 3)], (100, 90, 76))
    # headframe
    cv.poly([(cx - 9, cy + 3), (cx - 3, cy - 20), (cx + 3, cy - 20), (cx + 9, cy + 3)], (116, 82, 52))
    cv.poly([(cx - 5, cy + 3), (cx, cy - 2), (cx + 5, cy + 3), (cx, cy + 6)], (24, 20, 20))
    cv.line([(cx - 4, cy - 10), (cx + 4, cy - 10)], (86, 60, 40), 1.2)
    cv.ellipse((cx - 3, cy - 24, cx + 3, cy - 18), (90, 90, 96))
    return finish(cv, RIM)


def barricade_cell(era):
    cv = iso.Canvas((W, H), S)
    cx, cy = 64, 36
    cv.shadow_ellipse(cx + 5, cy + 6, 32, 9)
    if era == 0:      # sharpened stakes
        for i in range(-5, 6):
            x, y = iso.P(cx, cy, i * 4.2, i * 2.6 - 3, 0)
            cv.poly([(x - 1.6, y), (x + 1.6, y), (x + 0.4, y - 13 - (i % 2) * 2.4)], (118, 82, 52))
            cv.line([(x - 0.2, y - 1), (x + 0.2, y - 12)], (170, 130, 86), 0.5)
    elif era == 1:    # timber and stone
        for i in range(-3, 4):
            x, y = iso.P(cx, cy, i * 6, -i * 3, 0)
            iso.box(cv, x, y, 3.4, 3.4, 8 + (i % 2) * 2, (146, 140, 130))
    elif era == 2:    # sandbags
        for row in range(3):
            for i in range(-4 + row % 2, 5 - row % 2):
                x, y = iso.P(cx, cy, i * 5, i * 1.2, row * 3.2)
                iso.box(cv, x, y, 3.0, 2.2, 3.0, (196, 176, 130))
    else:             # concrete teeth
        for i in range(-3, 4):
            x, y = iso.P(cx, cy, i * 6, -i * 2.4, 0)
            iso.pyramid(cv, x, y, 3.8, 3.8, 0, 9, (138, 142, 146))
    return finish(cv, RIM)


def buildings():
    """TerrainBuildings: 4 columns (fortress, colony, camp/mine, barricade) x 4 eras."""
    def cell(i):
        col, row = i % 4, i // 4
        if col == 0:
            return fortress_cell(row)
        if col == 1:
            return colony_cell(row)
        if col == 2:
            return camp_cell() if row == 0 else mine_cell() if row == 1 else (
                Image.new("RGB", (W, H), KEY), Image.new("L", (W, H), 0), Image.new("L", (W, H), 0))
        return barricade_cell(row)
    return sheet(4, 4, W, H, cell)


# ---------------------------------------------------------------- territory ribbon

# row -> (start, end) of the diamond edge it hugs, in a 128x72 cell whose
# diamond is inset 4 px top and bottom (centre (64, 36)).
RIBBON_EDGES = [((0, 36), (64, 4)), ((64, 4), (128, 36)), ((0, 36), (64, 68)), ((64, 68), (128, 36))]


def territory(palette):
    """`Territory.pcx`: 2 columns x 4 rows of 128x72; the engine tints index
    64 (core) and 65 (rim) with the owner's colour, draws 249 and 252 as dark
    shadow and halo, and treats 1 and 255 as clear. Column 1 repeats column 0
    (the engine ships only the straight ribbon)."""
    cw, ch, sc = 128, 72, 4
    out = Image.new("P", (cw * 2, ch * 4), 255)
    out.putpalette(palette)
    for row, (a, b) in enumerate(RIBBON_EDGES):
        mid = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
        n = math.hypot(b[0] - a[0], b[1] - a[1])
        nx, ny = 64 - mid[0], 36 - mid[1]
        nn = math.hypot(nx, ny)
        nx, ny = nx / nn, ny / nn
        off = 3.6
        p0 = (a[0] + nx * off, a[1] + ny * off)
        p1 = (b[0] + nx * off, b[1] + ny * off)

        def layer(width, shift=(0.0, 0.0), beads=0.0):
            im = Image.new("L", (cw * sc, ch * sc), 0)
            d = ImageDraw.Draw(im)
            q0 = ((p0[0] + shift[0]) * sc, (p0[1] + shift[1]) * sc)
            q1 = ((p1[0] + shift[0]) * sc, (p1[1] + shift[1]) * sc)
            d.line([q0, q1], fill=255, width=int(width * sc))
            if beads:
                k = int(n / 10.5)
                for i in range(k + 1):
                    t = (i + 0.5) / (k + 1)
                    x = (p0[0] + (p1[0] - p0[0]) * t + shift[0]) * sc
                    y = (p0[1] + (p1[1] - p0[1]) * t + shift[1]) * sc
                    r = beads * sc
                    d.ellipse([x - r, y - r * 0.8, x + r, y + r * 0.8], fill=255)
            return im.resize((cw, ch), Image.Resampling.BOX)

        core = layer(2.0, beads=3.0)
        rim = layer(3.6, beads=4.3)
        halo = layer(5.2, beads=5.4)
        shade = layer(3.6, (1.8, 1.8), beads=4.3)
        idx = Image.new("P", (cw, ch), 255)
        idx.putpalette(palette)
        # the template diamond is index 1, the rest of the cell 255
        diamond = Image.new("L", (cw, ch), 0)
        ImageDraw.Draw(diamond).polygon([(0, 36), (64, 4), (128, 36), (64, 68)], fill=255)
        idx.paste(1, mask=diamond)
        for img, value, cut in ((shade, 249, 60), (halo, 252, 70), (rim, 65, 110), (core, 64, 110)):
            idx.paste(value, mask=img.point([255 if i >= cut else 0 for i in range(256)]))
        for col in range(2):
            out.paste(idx, (col * cw, row * ch))
    return out
