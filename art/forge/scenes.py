"""Landscape vignettes for the terrain panel (`ui/chrome/scene_*.png`): a small painted view of
each kind of ground, 340 x 150 and shown at 170 x 75. Edges fade into the paper.

The panel picks one by what is on the square: mountains, hills, a cover (forest, jungle, marsh),
else the ground itself (see `scene_for` in the client).
"""

import math
import random

import numpy as np
from PIL import Image

from .paint import paint
from .svgkit import Svg, mixc, shade
from .util import downsample, rasterize, to_array, to_image

W, H = 680, 300  # drawn at 2x the PNG's 340 x 150, then painted and shrunk


def ridge(s: Svg, y, amp, fill, seed, x0=-20, x1=W + 20, bottom=H + 10, step=22, waves=3, edge=None):
    """A band of rolling ground whose top edge is a sum of sines."""
    rng = random.Random(seed)
    ph = [rng.uniform(0, math.tau) for _ in range(waves)]
    fr = [rng.uniform(0.006, 0.02) * (k + 1) for k in range(waves)]
    pts = []
    x = x0
    while x <= x1:
        yy = y + sum(amp / (k + 1) * math.sin(x * fr[k] + ph[k]) for k in range(waves))
        pts.append((x, yy))
        x += step
    pts += [(x1, bottom), (x0, bottom)]
    s.poly(pts, fill, edge, 1.5 if edge else 1.0)
    return pts


def sky(s: Svg, top, bottom, to=170):
    s.rect(0, 0, W, H, s.lin([(0, top), (to / H, bottom), (1, bottom)], 0, 0, 0, 1))


def clouds(s: Svg, seed, n=5, y0=20, y1=90, col="#ffffff", op=0.75):
    rng = random.Random(seed)
    for _ in range(n):
        cx, cy = rng.uniform(40, W - 40), rng.uniform(y0, y1)
        for _ in range(5):
            s.ellipse(cx + rng.uniform(-50, 50), cy + rng.uniform(-8, 8), rng.uniform(30, 60), rng.uniform(8, 16), col, op=op * rng.uniform(0.5, 1), blur=s.blur(6))


def pine(s: Svg, x, y, h, col, dark=None):
    """A fir: a trunk and three boughs, widest at the bottom, the right half in shade."""
    w = h * 0.36
    dark = dark or shade(col, 0.6)
    s.rect(x - h * 0.03, y - h * 0.1, h * 0.06, h * 0.12, "#3a2814")
    for top, base, wide in ((1.0, 0.62, 0.5), (0.8, 0.36, 0.72), (0.58, 0.1, 0.95)):
        s.poly([(x, y - h * top), (x + w * wide, y - h * base), (x - w * wide, y - h * base)], col)
        s.poly([(x, y - h * top), (x + w * wide, y - h * base), (x, y - h * base)], dark, op=0.45)


def round_tree(s: Svg, x, y, r, col):
    s.rect(x - r * 0.1, y - r * 0.8, r * 0.2, r * 0.9, "#4a3220")
    for dx, dy, rr in ((0, -r * 1.5, r), (-r * 0.6, -r * 1.1, r * 0.7), (r * 0.6, -r * 1.1, r * 0.7)):
        s.circle(x + dx, y + dy, rr, shade(col, 0.82))
    for dx, dy, rr in ((-r * 0.15, -r * 1.65, r * 0.7), (r * 0.5, -r * 1.3, r * 0.4)):
        s.circle(x + dx, y + dy, rr, shade(col, 1.15), op=0.9)


def palm(s: Svg, x, y, h, lean=0.2):
    top = (x + h * lean, y - h)
    s.path(f"M{x},{y} Q{x + h * lean * 0.2},{y - h * 0.6} {top[0]},{top[1]}", "none", "#5a3e22", h * 0.07)
    for a in (-160, -125, -90, -55, -20, 15):
        r = math.radians(a)
        ex, ey = top[0] + math.cos(r) * h * 0.55, top[1] + math.sin(r) * h * 0.4 + h * 0.12
        s.path(f"M{top[0]},{top[1]} Q{(top[0] + ex) / 2},{top[1] - h * 0.2} {ex},{ey}", "none", "#2f6a2a", h * 0.09)
        s.path(f"M{top[0]},{top[1]} Q{(top[0] + ex) / 2},{top[1] - h * 0.2} {ex},{ey}", "none", "#5f9a44", h * 0.035, op=0.8)


def waves(s: Svg, y0, y1, seed, n=24, col="#ffffff", op=0.55, x0=0, x1=W):
    rng = random.Random(seed)
    for i in range(n):
        t = rng.random()
        y = y0 + (y1 - y0) * t * t
        x = rng.uniform(x0, x1)
        w = 14 + 70 * t
        s.path(f"M{x},{y} q{w * 0.25},{-4 - 6 * t} {w * 0.5},0 t{w * 0.5},0", "none", col, 1.2 + 2.4 * t, op=op * (0.4 + 0.6 * t))


def sun_glow(s: Svg, x, y, r, col="#fff2c0", op=0.7):
    s.circle(x, y, r, s.rad([(0, col, op), (1, col, 0.0)], 0.5, 0.5, 0.5), 1.0)


# --- the scenes ----------------------------------------------------------------------------------------
def coast(s):
    sky(s, "#7fb0d4", "#f3e8cc", 120)
    clouds(s, 1, 6, 12, 80)
    sun_glow(s, 300, 124, 140)
    s.rect(0, 122, W, H, s.lin([(0, "#5aa0b4"), (0.35, "#2f7e9a"), (1, "#18506c")], 0, 0, 0, 1))
    waves(s, 130, 290, 3, 38, "#ffffff", 0.6, 0, 520)
    # the headland on the right
    pts = [(330, 300), (372, 238), (420, 214), (470, 160), (520, 96), (560, 60), (588, 84), (612, 74), (650, 112), (680, 104), (680, 300)]
    s.poly(pts, s.lin([(0, "#8a6a48"), (0.6, "#5a4028"), (1, "#2c1e14")], 0, 0, 0.7, 1), "#2a1c12", 2)
    s.poly([(470, 160), (520, 96), (560, 60), (574, 120), (540, 190), (440, 240)], "#b99468", op=0.55)
    s.poly([(560, 60), (588, 84), (600, 150), (574, 120)], "#2a1c12", op=0.35)
    s.poly([(612, 74), (650, 112), (680, 104), (680, 190), (630, 170)], "#a07e58", op=0.5)
    for y in (140, 176, 214):
        s.path(f"M{380 + (y - 140) * 0.1},{y} L{640},{y - 16}", "none", "#2a1c12", 1.6, op=0.3)
    # surf at its foot
    for x, y, rx in ((350, 296, 70), (430, 270, 50), (500, 292, 70), (330, 268, 36)):
        s.ellipse(x, y, rx, 9, "#ffffff", op=0.75, blur=s.blur(3))
    for x, y in ((360, 262), (410, 276), (300, 284)):
        s.ellipse(x, y, 16, 8, "#3a2a1c", stroke="#1c120a", sw=1.2)


def sea(s, ocean=False):
    top, mid = ("#6d8fb0", "#cfd8dc") if ocean else ("#86bad6", "#f2ecd6")
    sky(s, top, mid, 120)
    clouds(s, 2 if ocean else 4, 5, 14, 90, op=0.8)
    sun_glow(s, 470, 126, 120)
    deep = ("#1c4a78", "#12385e", "#0b2644") if ocean else ("#4aa6b8", "#2a8aa2", "#176a86")
    s.rect(0, 122, W, H, s.lin([(0, deep[0]), (0.4, deep[1]), (1, deep[2])], 0, 0, 0, 1))
    waves(s, 128, 292, 7 if ocean else 9, 70, "#ffffff", 0.45 if ocean else 0.55)
    s.rect(0, 118, W, 14, "#ffffff", op=0.25, blur=s.blur(5))


def grass(s):
    sky(s, "#86b6d8", "#f1ead0", 130)
    clouds(s, 11, 5, 14, 80)
    ridge(s, 120, 16, "#8aa6a4", 3, waves=2)
    ridge(s, 140, 18, "#6c9a52", 5)
    ridge(s, 176, 20, s.lin([(0, "#78a63e"), (1, "#4f7e2a")], 0, 0, 0, 1), 8)
    for x, y, r in ((120, 168, 14), (210, 160, 11), (560, 150, 10)):
        round_tree(s, x, y, r, "#4f8a34")
    ridge(s, 232, 16, s.lin([(0, "#8cb84a"), (1, "#5a8a2e")], 0, 0, 0, 1), 12)
    rng = random.Random(3)
    for _ in range(46):
        x, y = rng.uniform(0, W), rng.uniform(244, 296)
        s.circle(x, y, rng.uniform(1.4, 2.6), rng.choice(["#f6e17a", "#ffffff", "#e87f6a"]), op=0.85)


def plains(s):
    sky(s, "#8ab8d6", "#f4ecd2", 140)
    clouds(s, 21, 4, 14, 90)
    ridge(s, 150, 6, "#a7a16a", 2, waves=2)
    ridge(s, 172, 8, s.lin([(0, "#d2bb62"), (1, "#a8903a")], 0, 0, 0, 1), 4)
    round_tree(s, 470, 190, 12, "#6f8a3a")
    ridge(s, 220, 10, s.lin([(0, "#dcc56c"), (1, "#b5993e")], 0, 0, 0, 1), 6)
    rng = random.Random(8)
    for _ in range(190):
        x, y = rng.uniform(0, W), rng.uniform(176, 298)
        h = 6 + (y - 170) * 0.16
        s.path(f"M{x},{y} q{rng.uniform(-3, 3)},{-h * 0.5} {rng.uniform(-6, 6)},{-h}", "none", rng.choice(["#efd988", "#c4a444", "#f6e6a4", "#98802e"]), 1.6, op=0.85)


def desert(s):
    sky(s, "#9cc4da", "#f8e7bd", 150)
    sun_glow(s, 420, 130, 150, "#fff0b0", 0.8)
    clouds(s, 31, 2, 20, 70, op=0.5)
    ridge(s, 150, 12, "#d0a868", 3, waves=2)
    for k, (y, c0, c1) in enumerate(((176, "#e6c27e", "#c99a52"), (216, "#efcd88", "#d2a458"), (262, "#f4d894", "#d9b062"))):
        pts = ridge(s, y, 22, s.lin([(0, c0), (1, c1)], 0, 0, 0, 1), 10 + k, waves=2, step=14)
        shadow = [(x + 18, yy + 6) for x, yy in pts[::2]]
        s.path("M" + " L".join(f"{x:.1f},{yy:.1f}" for x, yy in pts[: len(pts) - 2]), "none", "#fff4c8", 2.2, op=0.6)
        s.poly(pts[:-2] + [(pts[-3][0], H + 10), (pts[0][0], H + 10)], "#8a5a26", op=0.0)


def tundra(s):
    sky(s, "#a9bccb", "#eceff0", 150)
    clouds(s, 41, 5, 16, 100, op=0.85)
    ridge(s, 140, 20, "#c8d3dc", 5, waves=2)
    ridge(s, 160, 14, s.lin([(0, "#f0f4f6"), (1, "#c9d4dc")], 0, 0, 0, 1), 4)
    ridge(s, 200, 12, s.lin([(0, "#f6f8f8"), (1, "#c2ced8")], 0, 0, 0, 1), 7)
    ridge(s, 250, 10, s.lin([(0, "#fbfcfc"), (1, "#d3dde5")], 0, 0, 0, 1), 9)
    rng = random.Random(5)
    for _ in range(26):
        x, y = rng.uniform(0, W), rng.uniform(190, 290)
        r = 3 + (y - 190) * 0.05
        s.ellipse(x, y, r * 2.2, r, "#5c6a5a", op=0.8)
        s.ellipse(x - 1, y - 1, r * 1.5, r * 0.6, "#8a9a7c", op=0.8)
    for x, y in ((90, 240), (300, 266), (560, 230)):
        s.ellipse(x, y, 60, 8, "#b4c4d2", op=0.45, blur=s.blur(4))


def hills(s):
    sky(s, "#82b2d6", "#f1ead0", 130)
    clouds(s, 51, 5, 14, 80)
    ridge(s, 112, 18, "#8ea6b0", 3, waves=2)
    ridge(s, 136, 26, s.lin([(0, "#6f9e48"), (1, "#4a7a30")], 0, 0, 0, 1), 5)
    ridge(s, 184, 30, s.lin([(0, "#86b04a"), (1, "#587f2e")], 0, 0, 0, 1), 7)
    for x, y, r in ((520, 190, 10), (580, 184, 9), (110, 204, 11)):
        round_tree(s, x, y, r, "#4a7a2e")
    pts = ridge(s, 238, 22, s.lin([(0, "#98bd54"), (1, "#648f34")], 0, 0, 0, 1), 9)
    s.path("M-10,296 C160,260 280,282 420,250 C520,228 600,236 700,210", "none", "#c9b07a", 14, op=0.8)
    s.path("M-10,296 C160,260 280,282 420,250 C520,228 600,236 700,210", "none", "#e8d7a2", 5, op=0.7)


def mountains(s):
    sky(s, "#7aa6cc", "#efe8d2", 150)
    clouds(s, 61, 4, 18, 80)

    def peak(cx, base, h, w, col_lit, col_shade, snow=True):
        left = [(cx - w, base), (cx - w * 0.3, base - h * 0.55), (cx - w * 0.12, base - h * 0.78), (cx, base - h)]
        s.poly(left + [(cx + w * 0.02, base)], col_lit)
        s.poly([(cx, base - h), (cx + w * 0.22, base - h * 0.62), (cx + w * 0.4, base - h * 0.5), (cx + w, base), (cx + w * 0.02, base)], col_shade)
        if snow:
            s.poly([(cx, base - h), (cx - w * 0.12, base - h * 0.78), (cx - w * 0.2, base - h * 0.66), (cx - w * 0.06, base - h * 0.7), (cx + w * 0.04, base - h * 0.62), (cx + w * 0.22, base - h * 0.62)], "#fbfcfd")
            s.poly([(cx, base - h), (cx + w * 0.22, base - h * 0.62), (cx + w * 0.1, base - h * 0.58), (cx + w * 0.02, base - h * 0.64)], "#c9d3e0")

    s.rect(0, 180, W, 80, s.lin([(0, "#a9b8c0"), (1, "#5f7f58")], 0, 0, 0, 1))
    peak(120, 190, 100, 160, "#8a96a6", "#5a6678")
    peak(560, 190, 120, 190, "#8c97a6", "#566074")
    peak(330, 230, 190, 230, "#a29a90", "#5e5a60")
    ridge(s, 238, 12, s.lin([(0, "#4f7a3a"), (1, "#355a28")], 0, 0, 0, 1), 3, waves=2)
    for k in range(18):
        pine(s, 20 + k * 38 + (k % 3) * 8, 262 + (k % 4) * 8, 40 + (k % 5) * 6, "#2f5a30")


def forest(s):
    sky(s, "#9cc0d8", "#e9e6cf", 80)
    ridge(s, 118, 14, "#7a9a8a", 2, waves=2)
    rng = random.Random(7)
    rows = ((130, 40, "#2f5a34", 26), (166, 56, "#2a5230", 24), (210, 74, "#244a2c", 22), (262, 96, "#1d3f26", 20))
    for y, h, col, n in rows:
        s.rect(0, y - h * 0.4, W, 14, "#d8e4d0", op=0.25, blur=s.blur(8))
        xs = sorted(rng.uniform(-10, W + 10) for _ in range(n))
        for x in xs:
            pine(s, x, y + rng.uniform(-6, 6), h * rng.uniform(0.85, 1.15), shade(col, rng.uniform(0.9, 1.15)))
    s.rect(0, 280, W, 30, "#16301e", op=0.7)


def jungle(s):
    sky(s, "#b4cfd0", "#eef0d4", 80)
    ridge(s, 120, 20, "#5f8a6a", 4, waves=2)
    rng = random.Random(12)
    for y, r, col in ((150, 40, "#2c6a3a"), (200, 52, "#225c30"), (262, 64, "#1a4c28")):
        for x in range(-30, W + 60, int(r * 1.3)):
            dx, dy = rng.uniform(-10, 10), rng.uniform(-10, 10)
            s.circle(x + dx, y + dy, r * rng.uniform(0.8, 1.15), shade(col, rng.uniform(0.85, 1.2)))
            s.circle(x + dx - r * 0.25, y + dy - r * 0.3, r * 0.5, shade(col, 1.35), op=0.5)
    for x, y, h in ((120, 232, 120), (330, 244, 140), (560, 228, 110)):
        palm(s, x, y, h, 0.15 if x < 400 else -0.18)
    s.rect(0, 150, W, 80, "#e8f2dc", op=0.18, blur=s.blur(14))


def marsh(s):
    sky(s, "#9fbcd0", "#efe6cc", 120)
    clouds(s, 71, 5, 14, 80)
    ridge(s, 130, 8, "#7f9a80", 2, waves=2)
    s.rect(0, 140, W, H, s.lin([(0, "#6f9a8c"), (0.5, "#587f6a"), (1, "#3f6350")], 0, 0, 0, 1))
    rng = random.Random(4)
    for _ in range(8):
        x, y = rng.uniform(0, W), rng.uniform(160, 280)
        s.ellipse(x, y, rng.uniform(40, 90), rng.uniform(8, 16), "#b8d4e0", op=0.55)
    ridge(s, 262, 10, s.lin([(0, "#6a8a3e"), (1, "#3f5e2a")], 0, 0, 0, 1), 3, waves=2)
    for _ in range(240):
        x, y = rng.uniform(0, W), rng.uniform(150, 296)
        h = 10 + (y - 150) * 0.22
        c = rng.choice(["#8a8a3c", "#a29a48", "#5f7a34", "#7e9a40"])
        s.path(f"M{x},{y} q{rng.uniform(-4, 4)},{-h * 0.6} {rng.uniform(-7, 7)},{-h}", "none", c, 1.8, op=0.9)
        if rng.random() < 0.12:
            s.ellipse(x, y - h - 3, 2.6, 6, "#6a4a2a")


SCENES = {
    "coast": coast,
    "sea": lambda s: sea(s, False),
    "ocean": lambda s: sea(s, True),
    "grass": grass,
    "plains": plains,
    "desert": desert,
    "tundra": tundra,
    "hills": hills,
    "mountains": mountains,
    "forest": forest,
    "jungle": jungle,
    "marsh": marsh,
}


def fade_mask(w, h):
    """Opaque on the right, melting into the paper along the left and bottom edges."""
    x = np.linspace(0, 1, w)[None, :]
    y = np.linspace(0, 1, h)[:, None]
    left = np.clip(x / 0.42, 0, 1) ** 1.4
    bottom = np.clip((1 - y) / 0.2, 0, 1)
    top = np.clip(y / 0.06, 0, 1)
    right = np.clip((1 - x) / 0.02, 0, 1)
    return left * bottom * np.sqrt(top) * np.sqrt(right)


def render(name, seed=1):
    s = Svg(W, H)
    SCENES[name](s)
    big = rasterize(s.render(), W, H)
    big = paint(big, seed=seed, radius=2, grain=0.05, strokes=0.035, grade_kw={"sat": 1.06, "contrast": 1.04, "key": 0.04})
    img = downsample(big, W // 2, H // 2)
    arr = to_array(img)
    arr[..., 3] = fade_mask(arr.shape[1], arr.shape[0])
    return to_image(arr)


def render_all():
    return {f"scene_{name}": render(name, k + 1) for k, name in enumerate(SCENES)}
