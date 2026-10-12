"""The margin of the chart (`margin/`): what lies past the top and bottom of an upright world.

The map is printed like a Victorian atlas plate. Past its last row the camera finds the plate's
border and margin, and further out the edge of the sheet lying on the cartographer's desk:

* `band.png`: the neatline frame, a tile 128 x 120 world units (drawn from 256 x 240) that repeats
  along the edge: a gilt cable, a meander frieze, a chequered degree scale and the inner rule. It
  is authored for the north edge, map side down; the client flips it for the south. Its last eight
  units are a soft shadow cast over the map.
* `edge.png`: the torn, browned end of the sheet and its shadow on the desk, 256 x 64 units, paper
  side down, also flipped for the south.
* `desk.png`: seamless walnut, 512 units square.
* Engravings set on the margin, ink on transparent: `compass.png` (a compass rose), `cartouche.png`
  (the title plaque the client letters with the campaign's name), `ribbon.png` (a motto banner,
  lettered by the client), `ship.png`, `steamer.png` and `serpent.png`.
* `instruments.png`: brass dividers, a reading glass and a quill lying on the desk.

Every strip tiles horizontally, and all light comes from the upper left.
"""

import math
import random

import numpy as np
from PIL import Image

from .chrome import periodic_noise
from .parchment import INK, WASH, _fade, _render, _ripples, _sailing_ship, _steamer
from .svgkit import Svg

PAPER = "#ede0c0"
PAPER_DEEP = "#dccaa0"
GILT = "#c9993f"
GILT_HI = "#f0d58a"
GILT_LO = "#7e5a1e"
TEAL = "#2c4f4d"
CRIMSON = "#9b2f27"
CRIMSON_LO = "#64190f"


# ---- the neatline frame ---------------------------------------------------------------------------
def band():
    W, H = 128, 120
    s = Svg(W, H)
    # Ground of the whole frame: paper.
    s.rect(0, 0, W, 112, PAPER)
    s.rect(0, 0, W, 1.6, INK)
    # A gilt cable on ink: slanted lozenges, eight to a tile.
    s.rect(0, 3.5, W, 24, "#22170e")
    for i in range(-1, 9):
        x = i * 16
        twist = f"M{x + 1},{25} C{x + 6},{24} {x + 13},{9} {x + 17},{6} C{x + 12},{7} {x + 5},{20} {x + 1},{25} Z"
        s.path(twist, GILT, GILT_LO, 0.7)
        s.path(f"M{x + 4},{21} C{x + 8},{17} {x + 11},{12} {x + 14},{9}", "none", GILT_HI, 1.0, 0.8)
    s.rect(0, 3.5, W, 24, "none", stroke=INK, sw=1.0)
    s.rect(0, 29.5, W, 1.2, INK)
    # A meander frieze, gold on teal, four to a tile.
    s.rect(0, 32, W, 34, TEAL)
    for i in range(4):
        x = i * 32
        key = (
            f"M{x},{60} H{x + 26} V{38} H{x + 6} V{54} H{x + 20} V{44} H{x + 12} V{48}"
        )
        s.path(key, "none", "#1a2e2c", 4.6, extra='transform="translate(0.6,0.8)"')
        s.path(key, "none", GILT, 3.0)
        s.path(f"M{x + 26},{60} H{x + 32}", "none", GILT, 3.0)
    s.rect(0, 32, W, 1.0, GILT_HI, op=0.6)
    s.rect(0, 65, W, 1.0, "#10201f", op=0.8)
    s.rect(0, 67.5, W, 1.2, INK)
    # The degree scale: two staggered rows of ink and paper blocks, ticks between them.
    for row, (y, shift) in enumerate(((72, 0), (81, 16))):
        for i in range(-1, 5):
            x = i * 32 + shift
            s.rect(x, y, 16, 9, INK)
    s.rect(0, 72, W, 18, "none", stroke=INK, sw=0.9)
    for i in range(32):
        x = i * 4
        tall = 4.5 if i % 4 == 0 else 2.2
        s.line((x, 90), (x, 90 + tall), INK, 0.6, 0.85)
    # The inner rule: heavy ink, a gilt hairline, a thin ink line, the map's edge.
    s.rect(0, 96, W, 4.2, INK)
    s.rect(0, 101.2, W, 1.4, GILT)
    s.rect(0, 103.6, W, 1.0, INK)
    s.rect(0, 105, W, 7, PAPER)
    s.rect(0, 110.6, W, 1.4, INK)
    im = _render(s, 2 * W, 2 * H)
    # The frame casts a soft shadow onto the map below it.
    a = np.asarray(im, dtype=np.float32).copy()
    top = 2 * 112
    for y in range(top, 2 * H):
        t = (y - top) / (2 * H - top)
        a[y, :, :3] = (12, 8, 4)
        a[y, :, 3] = 255 * 0.42 * (1 - t) ** 2
    return Image.fromarray(a.astype(np.uint8), "RGBA")


# ---- the edge of the sheet ------------------------------------------------------------------------
def edge():
    """The torn end of the sheet: browning that deepens toward a deckled rim, then a shadow."""
    W, H = 512, 128
    seam = 64  # rows below this only brown the paper tiles under the strip
    rng = np.random.default_rng(23)
    x = np.arange(W)
    rim = np.zeros(W)
    for k, amp in ((2, 5.0), (5, 3.2), (11, 2.0), (23, 1.4), (47, 0.9), (97, 0.6)):
        rim += amp * np.sin(2 * math.pi * k * x / W + rng.uniform(0, 2 * math.pi))
    rim = 36 + rim  # the row where the paper ends, per column
    yy = np.arange(H, dtype=np.float32)[:, None] * np.ones((1, W), np.float32)
    inside = yy >= rim[None, :]
    paper = np.array((229, 212, 172), np.float32)
    brown = np.array((128, 90, 46), np.float32)
    burn = np.exp(-(yy - rim[None, :]).clip(0, None) / 7.0)
    texture = 1 + 0.035 * periodic_noise(W, 6, 5)[:H] + 0.02 * periodic_noise(W, 1.0, 6)[:H]
    mix = (0.3 + 0.7 * burn)[..., None]
    solid = (paper * (1 - mix) + brown * mix) * texture[..., None]
    fade = 0.3 * np.clip((H - yy) / (H - seam), 0, 1) ** 1.5
    out = np.zeros((H, W, 4), np.float32)
    out[..., :3] = np.where((yy < seam)[..., None], solid, brown)
    out[..., 3] = np.where(yy < seam, 255.0, fade * 255)
    # Past the rim: the sheet's shadow on the desk, offset away from the light.
    shadow = np.clip(1 - (rim[None, :] + 3 - yy) / 18.0, 0, 1) ** 2 * 0.65
    out[..., :3] = np.where(inside[..., None], out[..., :3], 8)
    out[..., 3] = np.where(inside, out[..., 3], shadow * 255)
    return Image.fromarray(np.clip(out, 0, 255).astype(np.uint8), "RGBA")


# ---- the desk -------------------------------------------------------------------------------------
def desk(size=512):
    """Walnut: long, straight grain running across, figured by broad bands, with pores. It tiles."""
    rng = np.random.default_rng(36)
    # Grain is a function of the row, bent gently by broad noise, so streaks run long and straight.
    bend = periodic_noise(size, 110, 31) * 16 + periodic_noise(size, 30, 32) * 2.5
    yy = np.arange(size)[:, None] + bend
    # Streaks: a 1-D noise down the rows (periodic), sampled along the bent rows.
    rows = np.fft.ifft(np.fft.fft(rng.standard_normal(size)) * np.exp(-((np.fft.fftfreq(size) * size / 60) ** 2))).real
    rows = rows / (rows.std() + 1e-9)
    fine = np.fft.ifft(np.fft.fft(rng.standard_normal(size)) * np.exp(-((np.fft.fftfreq(size) * size / 200) ** 2))).real
    fine = fine / (fine.std() + 1e-9)
    idx = np.mod(yy, size)
    i0 = np.floor(idx).astype(int) % size
    t = idx - np.floor(idx)
    streak = rows[i0] * (1 - t) + rows[(i0 + 1) % size] * t
    hair = fine[i0] * (1 - t) + fine[(i0 + 1) % size] * t
    figure = 0.5 + 0.5 * np.sin(2 * math.pi * (5 * yy / size) + periodic_noise(size, 60, 33) * 0.8)
    pores = np.clip(periodic_noise(size, 0.7, 35) - 1.8, 0, None) * 0.4
    dark = np.array((30, 18, 11), np.float32)
    light = np.array((96, 60, 35), np.float32)
    v = 0.42 + 0.14 * streak + 0.07 * hair + 0.16 * figure + 0.05 * periodic_noise(size, 80, 34) - pores
    v = np.clip(v, 0, 1)[..., None]
    rgb = (dark + (light - dark) * v) * 0.8
    out = np.dstack([np.clip(rgb, 0, 255), np.full((size, size), 255.0)])
    return Image.fromarray(out.astype(np.uint8), "RGBA")


# ---- engravings ---------------------------------------------------------------------------------
def _star_point(s: Svg, cx, cy, ang, length, half, light, dark):
    """One point of a rose: a kite split down its spine, lit on one side."""
    ca, sa = math.cos(ang), math.sin(ang)
    tip = (cx + ca * length, cy + sa * length)
    left = (cx + math.cos(ang - math.pi / 2) * half, cy + math.sin(ang - math.pi / 2) * half)
    right = (cx + math.cos(ang + math.pi / 2) * half, cy + math.sin(ang + math.pi / 2) * half)
    s.poly([(cx, cy), left, tip], light, INK, 0.9)
    s.poly([(cx, cy), right, tip], dark, INK, 0.9)


def compass():
    W = 280
    s = Svg(W, W)
    c = W / 2
    s.circle(c, c, 118, PAPER, op=0.55)
    s.circle(c, c, 118, "none", stroke=INK, sw=2.2)
    s.circle(c, c, 112, "none", stroke=INK, sw=0.8)
    for i in range(128):
        a = 2 * math.pi * i / 128
        r0 = 112 - (6 if i % 4 == 0 else 3)
        s.line((c + math.cos(a) * r0, c + math.sin(a) * r0), (c + math.cos(a) * 112, c + math.sin(a) * 112), INK, 0.7)
    s.circle(c, c, 100, "none", stroke=GILT, sw=3.2)
    s.circle(c, c, 100, "none", stroke=INK, sw=0.6)
    s.circle(c, c, 64, "none", stroke=INK, sw=0.7, op=0.6)
    # Sixteen fine points, then eight, then the four cardinals.
    for i in range(16):
        a = -math.pi / 2 + math.pi / 16 + 2 * math.pi * i / 16
        _star_point(s, c, c, a, 70, 7, PAPER, WASH)
    for i in range(4):
        a = -math.pi / 4 + math.pi / 2 * i
        _star_point(s, c, c, a, 84, 11, PAPER, TEAL)
    for i in range(4):
        a = -math.pi / 2 + math.pi / 2 * i
        _star_point(s, c, c, a, 108, 15, GILT_HI if i else "#d65a46", GILT_LO if i else CRIMSON_LO)
    s.circle(c, c, 9, GILT, stroke=INK, sw=1.2)
    s.circle(c - 2, c - 2, 3, GILT_HI, op=0.9)
    # A fleur-de-lis over north.
    fx, fy = c, c - 118
    lis = (
        f"M{fx},{fy - 22} C{fx + 5},{fy - 14} {fx + 6},{fy - 6} {fx},{fy + 2} C{fx - 6},{fy - 6} {fx - 5},{fy - 14} {fx},{fy - 22} Z "
        f"M{fx + 2},{fy} C{fx + 10},{fy - 4} {fx + 18},{fy - 14} {fx + 12},{fy - 20} C{fx + 16},{fy - 10} {fx + 10},{fy - 2} {fx + 3},{fy + 3} Z "
        f"M{fx - 2},{fy} C{fx - 10},{fy - 4} {fx - 18},{fy - 14} {fx - 12},{fy - 20} C{fx - 16},{fy - 10} {fx - 10},{fy - 2} {fx - 3},{fy + 3} Z"
    )
    s.path(lis, CRIMSON, INK, 1.0)
    s.rect(fx - 9, fy + 1, 18, 3.5, GILT, stroke=INK, sw=0.8)
    for label, (dx, dy) in (("E", (1, 0)), ("S", (0, 1)), ("W", (-1, 0))):
        s.raw(
            f'<text x="{c + dx * 126}" y="{c + dy * 126 + 6}" font-family="Georgia, serif" font-size="17" '
            f'font-weight="bold" text-anchor="middle" fill="{INK}">{label}</text>'
        )
    return _render(s, 2 * W, 2 * W)


def _laurel(s: Svg, x0, y0, x1, y1, bow, flip, leaves=9):
    """A laurel spray along a quadratic stem from (x0, y0) to (x1, y1), leaves in pairs."""
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2 + bow
    s.path(f"M{x0},{y0} Q{mx},{my} {x1},{y1}", "none", "#4b5a2c", 2.0)
    for i in range(1, leaves + 1):
        t = i / (leaves + 1)
        px = (1 - t) ** 2 * x0 + 2 * (1 - t) * t * mx + t * t * x1
        py = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * my + t * t * y1
        tx = 2 * (1 - t) * (mx - x0) + 2 * t * (x1 - mx)
        ty = 2 * (1 - t) * (my - y0) + 2 * t * (y1 - my)
        ang = math.degrees(math.atan2(ty, tx))
        size = 1.0 - 0.4 * t
        for side in (-1, 1):
            a = ang + side * 38
            ox = math.cos(math.radians(a)) * 7 * size
            oy = math.sin(math.radians(a)) * 7 * size
            s.ellipse(px + ox, py + oy, 8.5 * size, 3.4 * size, "#6d7f3c" if side * flip > 0 else "#56662d", stroke=INK, sw=0.6, rot=a)


def cartouche():
    """A gilt plaque with scrolled ends and laurels, left blank for the client to letter."""
    W, H = 512, 200
    s = Svg(W, H)
    cx, cy = W / 2, H / 2 - 4
    # Laurels crossing beneath.
    _laurel(s, cx - 10, cy + 70, cx - 200, cy + 30, 30, 1, 11)
    _laurel(s, cx + 10, cy + 70, cx + 200, cy + 30, 30, -1, 11)
    # Scrolls at either end: a curl of strapwork rolled toward the plaque.
    for side in (-1, 1):
        x = cx + side * 176
        curl = (
            f"M{x - side * 6},{cy - 46} C{x + side * 40},{cy - 60} {x + side * 58},{cy - 18} {x + side * 34},{cy - 4} "
            f"C{x + side * 18},{cy + 6} {x + side * 8},{cy - 16} {x + side * 24},{cy - 20} "
            f"C{x + side * 34},{cy - 22} {x + side * 36},{cy - 8} {x + side * 26},{cy - 6} "
            f"M{x - side * 6},{cy + 46} C{x + side * 40},{cy + 60} {x + side * 58},{cy + 18} {x + side * 34},{cy + 4} "
            f"C{x + side * 18},{cy - 6} {x + side * 8},{cy + 16} {x + side * 24},{cy + 20} "
            f"C{x + side * 34},{cy + 22} {x + side * 36},{cy + 8} {x + side * 26},{cy + 6}"
        )
        s.path(curl, "none", INK, 7.0)
        s.path(curl, "none", GILT, 4.2)
        s.path(curl, "none", GILT_HI, 1.2, 0.8, extra='transform="translate(-0.6,-0.8)"')
    # The plaque: a rectangle with notched corners, framed in ink and gilt.
    def plaque(inset):
        x0, x1, y0, y1 = cx - 180 + inset, cx + 180 - inset, cy - 52 + inset, cy + 52 - inset
        n = 14 - inset * 0.5
        return (
            f"M{x0 + n},{y0} H{x1 - n} Q{x1 - n},{y0 + n} {x1},{y0 + n} V{y1 - n} Q{x1 - n},{y1 - n} {x1 - n},{y1} "
            f"H{x0 + n} Q{x0 + n},{y1 - n} {x0},{y1 - n} V{y0 + n} Q{x0 + n},{y0 + n} {x0 + n},{y0} Z"
        )

    blur = s.blur(4)
    s.path(plaque(0), "#000", op=0.28, extra=f'filter="{blur}" transform="translate(3,5)"')
    s.path(plaque(0), INK)
    s.path(plaque(2.2), GILT)
    s.path(plaque(5.5), INK)
    s.path(plaque(7), PAPER)
    s.path(plaque(7), s.rad([(0, "#f6ecd2"), (1, PAPER_DEEP)], r=0.7))
    s.path(plaque(12), "none", INK, 0.7, 0.7)
    # A rule between the title and the line under it, with a small diamond.
    s.line((cx - 120, cy + 12), (cx - 10, cy + 12), INK, 0.8, 0.8)
    s.line((cx + 10, cy + 12), (cx + 120, cy + 12), INK, 0.8, 0.8)
    s.poly([(cx, cy + 7), (cx + 5, cy + 12), (cx, cy + 17), (cx - 5, cy + 12)], CRIMSON, INK, 0.6)
    # A crest on top: a small gilt globe with its meridians.
    gx, gy = cx, cy - 64
    s.circle(gx, gy, 15, GILT, stroke=INK, sw=1.4)
    s.ellipse(gx, gy, 6, 15, "none", stroke=INK, sw=0.8)
    s.line((gx - 15, gy), (gx + 15, gy), INK, 0.8)
    s.circle(gx - 4, gy - 5, 4, GILT_HI, op=0.8)
    return _render(s, 2 * W, 2 * H)


def ribbon():
    """A motto banner: a cream ribbon waving across, its ends folded behind and cut in a swallowtail."""
    W, H = 560, 120
    s = Svg(W, H)
    cy = H / 2
    wave = 10
    x0, x1 = 90, W - 90
    top = lambda x: cy - 20 + wave * math.sin((x - x0) / (x1 - x0) * math.pi * 2 - math.pi / 2) * 0.6
    # The tails behind, darker, cut in a swallowtail.
    for side in (-1, 1):
        inner = x0 if side < 0 else x1
        outer = inner + side * 80
        y = top(inner) + 26
        tail = [(inner - side * 14, y - 18), (outer, y - 22), (outer - side * 18, y + 2), (outer, y + 24), (inner - side * 14, y + 20)]
        s.poly(tail, "#c7b48c", INK, 1.2)
        s.poly([(inner + side * 8, y + 20), (inner - side * 6, y + 26), (inner - side * 6, y + 6)], CRIMSON_LO, INK, 1.0)
    # The face: top and bottom edges follow the same wave.
    pts_top = [(x, top(x)) for x in np.linspace(x0, x1, 40)]
    pts_bot = [(x, y + 40) for x, y in reversed(pts_top)]
    s.poly(pts_top + pts_bot, s.lin([(0, "#f4ead0"), (0.55, PAPER), (1, "#cdb98d")]), INK, 1.4)
    s.path("M" + " L".join(f"{x:.1f},{y + 4:.1f}" for x, y in pts_top), "none", CRIMSON, 1.4, 0.8)
    s.path("M" + " L".join(f"{x:.1f},{y + 36:.1f}" for x, y in pts_top), "none", CRIMSON, 1.4, 0.8)
    return _render(s, 2 * W, 2 * H)


def ship():
    W, H = 420, 230
    s = Svg(W, H)
    rng = random.Random(41)
    _ripples(s, rng, 30, W - 40, 186, 222, 70, op=0.45)
    _sailing_ship(s, 210, 190, 1.15)
    return _fade(_render(s, 2 * W, 2 * H), left=0.08, right=0.08, bottom=0.12)


def steamer():
    W, H = 440, 220
    s = Svg(W, H)
    rng = random.Random(43)
    _ripples(s, rng, 20, W - 30, 176, 212, 70, op=0.45)
    _steamer(s, 236, 180, 1.2)
    return _fade(_render(s, 2 * W, 2 * H), left=0.08, right=0.08, bottom=0.12)


def serpent():
    """A sea serpent in three coils above the swell, its head reared and its tail curled."""
    W, H = 440, 220
    s = Svg(W, H)
    rng = random.Random(47)
    sea = 160
    _ripples(s, rng, 20, W - 30, sea - 4, sea + 50, 90, op=0.45)
    body = "#55705f"
    belly = "#c9c49a"
    # Coils: arches rising from the water and plunging back.
    for x, w, h in ((150, 60, 52), (240, 56, 44), (322, 46, 34)):
        arch = f"M{x - w / 2},{sea} C{x - w / 2},{sea - h * 1.3} {x + w / 2},{sea - h * 1.3} {x + w / 2},{sea}"
        s.path(arch, "none", INK, 25)
        s.path(arch, "none", body, 21)
        s.path(arch, "none", belly, 6, 0.85, extra='transform="translate(0,7)"')
        # Scales: short arcs along the coil, and a row of fin spines.
        for i in range(1, 12):
            t = i / 12
            px = (1 - t) ** 3 * (x - w / 2) + 3 * (1 - t) ** 2 * t * (x - w / 2) + 3 * (1 - t) * t * t * (x + w / 2) + t**3 * (x + w / 2)
            py = (1 - t) ** 3 * sea + 3 * (1 - t) ** 2 * t * (sea - h * 1.3) + 3 * (1 - t) * t * t * (sea - h * 1.3) + t**3 * sea
            s.path(f"M{px - 3},{py - 2} q3,4 6,0", "none", INK, 0.7, 0.7)
            if 2 <= i <= 10:
                s.poly([(px - 3, py - 9), (px + 1, py - 22 + abs(t - 0.5) * 14), (px + 4, py - 9)], CRIMSON, INK, 0.7)
        for ox in (-w / 2, w / 2):
            s.ellipse(x + ox, sea + 1, 16, 3, "#f4ecd6", stroke=INK, sw=0.7)
    # The tail, curled.
    s.path(f"M375,{sea} C380,{sea - 30} 410,{sea - 34} 408,{sea - 14} C406,{sea - 2} 392,{sea - 6} 396,{sea - 16}", "none", INK, 10)
    s.path(f"M375,{sea} C380,{sea - 30} 410,{sea - 34} 408,{sea - 14} C406,{sea - 2} 392,{sea - 6} 396,{sea - 16}", "none", body, 7)
    s.poly([(396, sea - 16), (384, sea - 30), (402, sea - 26)], CRIMSON, INK, 0.8)
    # Neck and head, reared up out of the sea, jaws open.
    neck = f"M110,{sea} C100,{sea - 50} 80,{sea - 90} 58,{sea - 104}"
    s.path(neck, "none", INK, 26)
    s.path(neck, "none", body, 22)
    s.path(f"M116,{sea} C108,{sea - 48} 92,{sea - 84} 70,{sea - 98}", "none", belly, 6, 0.85)
    hx, hy = 52, sea - 108
    head = f"M{hx + 22},{hy - 6} C{hx + 10},{hy - 22} {hx - 20},{hy - 20} {hx - 40},{hy - 6} L{hx - 18},{hy - 2} Z"
    jaw = f"M{hx + 18},{hy + 8} C{hx + 2},{hy + 18} {hx - 22},{hy + 16} {hx - 36},{hy + 14} L{hx - 16},{hy + 4} Z"
    s.path(jaw, body, INK, 1.2)
    s.path(f"M{hx - 16},{hy - 2} L{hx - 36},{hy + 13} L{hx - 40},{hy - 5} Z", CRIMSON_LO, op=0.9)
    s.path(head, body, INK, 1.2)
    for i in range(5):
        tx = hx - 34 + i * 4.5
        s.poly([(tx, hy - 4), (tx + 1.6, hy + 1), (tx + 3.2, hy - 4)], "#f4ecd6", INK, 0.4)
    s.circle(hx - 6, hy - 10, 3.4, "#e8c25a", stroke=INK, sw=0.8)
    s.circle(hx - 5.4, hy - 10, 1.2, INK)
    # Its crest and a forked tongue.
    for i in range(5):
        bx = hx + 4 + i * 6
        s.poly([(bx, hy - 12 + i * 2), (bx + 8, hy - 30 + i * 3), (bx + 6, hy - 8 + i * 2)], CRIMSON, INK, 0.7)
    s.path(f"M{hx - 38},{hy + 4} l-14,2 l-6,-4 M{hx - 52},{hy + 6} l-6,5", "none", CRIMSON, 1.4)
    return _fade(_render(s, 2 * W, 2 * H), left=0.05, right=0.05, bottom=0.1)


def instruments():
    """Brass dividers, a reading glass and a quill lying on the desk, with their shadows."""
    W, H = 640, 320
    s = Svg(W, H)
    blur = s.blur(6)
    brass, brass_hi, brass_lo = "#b88a3c", "#f1d38a", "#6b4a18"

    def shadowed(draw):
        s.raw(f'<g filter="{blur}" opacity="0.55" transform="translate(10,12)">')
        draw("#000", "#000", "#000", True)
        s.raw("</g>")
        draw(brass, brass_hi, brass_lo, False)

    # Dividers: two long legs from a hinge, steel points.
    def dividers(c, hi, lo, shadow):
        hx, hy = 120, 70
        for tx, ty in ((70, 290), (210, 280)):
            s.line((hx, hy), (tx, ty), "#000" if shadow else INK, 11)
            if not shadow:
                s.line((hx, hy), (tx, ty), c, 8)
                s.line((hx - 1.5, hy - 1), (tx - 1.5, ty - 6), hi, 2, 0.8)
                ux, uy = (tx - hx) / 220, (ty - hy) / 220
                s.line((tx - ux * 40, ty - uy * 40), (tx, ty), "#8d949a", 5)
        s.circle(hx, hy, 16, c, stroke=None if shadow else INK, sw=1.5)
        if not shadow:
            s.circle(hx, hy, 6, lo, stroke=INK, sw=1)
            s.circle(hx - 5, hy - 5, 4, hi, op=0.8)
        s.line((hx, hy - 16), (hx, hy - 40), "#000" if shadow else INK, 6)

    # The reading glass: a brass ring round a pale lens, on a turned wooden handle.
    def glass(c, hi, lo, shadow):
        gx, gy, r = 380, 140, 82
        s.line((gx + r * 0.7, gy + r * 0.7), (gx + r * 0.7 + 120, gy + r * 0.7 + 90), "#000" if shadow else INK, 26)
        if not shadow:
            s.line((gx + r * 0.7, gy + r * 0.7), (gx + r * 0.7 + 120, gy + r * 0.7 + 90), "#4a2a16", 22)
            s.line((gx + r * 0.7, gy + r * 0.7 - 6), (gx + r * 0.7 + 116, gy + r * 0.7 + 82), "#7e5332", 5, 0.8)
            s.line((gx + r * 0.7 + 6, gy + r * 0.7 + 4), (gx + r * 0.7 + 30, gy + r * 0.7 + 22), c, 24)
            s.circle(gx, gy, r, s.rad([(0, "#e9f2ee", 0.35), (1, "#9fb8b0", 0.25)]))
            s.circle(gx, gy, r, "none", stroke=INK, sw=16)
            s.circle(gx, gy, r, "none", stroke=c, sw=12)
            s.circle(gx, gy, r - 4, "none", stroke=hi, sw=2, op=0.7)
            s.path(f"M{gx - r * 0.55},{gy - r * 0.25} A{r * 0.6},{r * 0.6} 0 0 1 {gx - r * 0.1},{gy - r * 0.62}", "none", "#ffffff", 7, 0.55)
        else:
            s.circle(gx, gy, r, "none", stroke="#000", sw=16)

    # A quill, its vane laid across the desk.
    def quill(c, hi, lo, shadow):
        x0, y0, x1, y1 = 470, 300, 630, 40
        vane = f"M{x0 + 20},{y0 - 40} C{x0 + 70},{y0 - 160} {x1 - 40},{y1 + 10} {x1},{y1} C{x1 - 30},{y1 + 60} {x0 + 70},{y0 - 70} {x0 + 30},{y0 - 30} Z"
        s.path(vane, "#000" if shadow else "#efe8d8", None if shadow else INK, 1.0)
        if not shadow:
            for i in range(1, 16):
                t = i / 16
                bx, by = x0 + 20 + (x1 - x0 - 20) * t, y0 - 40 + (y1 - y0 + 40) * t
                s.line((bx, by), (bx + 18 - 8 * t, by + 14 - 4 * t), WASH, 0.8, 0.7)
            s.line((x0, y0), (x1, y1), INK, 2.2)
            s.line((x0, y0), (x0 + 22, y0 - 36), "#3a2a1a", 3)

    shadowed(quill)
    shadowed(glass)
    shadowed(dividers)
    return _render(s, 2 * W, 2 * H)


def render_all():
    return {
        "band": band(),
        "edge": edge(),
        "desk": desk(),
        "compass": compass(),
        "cartouche": cartouche(),
        "ribbon": ribbon(),
        "ship": ship(),
        "steamer": steamer(),
        "serpent": serpent(),
        "instruments": instruments(),
    }
