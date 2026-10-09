"""UI chrome in the reference's style: brass-framed near-black navy panels, cream parchment,
charcoal nameplates, bevelled brass buttons, bars, flags and a seamless parchment texture.

All nine-slice sources are drawn at 2x (the game shows them at half size).  `NINE_SLICE`
lists the border insets, in source pixels, that keep corner ornaments undistorted.
"""

import math

import numpy as np
from PIL import Image, ImageFilter

from . import palette as P
from .noise import fbm
from .paint import render_sprite
from .svgkit import Svg, shade
from .util import downsample, rasterize, to_array, to_image

BR_L, BR_M, BR_D = "#ecd8a4", "#bca06e", "#7a653d"
EDGE = "#241a10"

NINE_SLICE = {
    "panel_navy": 44,
    "panel_parchment": 44,
    "button_brass": 28,
    "button_brass_pressed": 28,
    "button_dark": 36,
    "bar_frame": 14,
    "plate_name": 56,
}


def _stud(svg: Svg, x, y, r=9):
    g = svg.rad([(0, "#fff3c8"), (0.35, BR_L), (0.75, BR_M), (1, BR_D)], 0.35, 0.3, 0.85)
    svg.circle(x + 0.8, y + 1.2, r, "#000", op=0.35, blur=svg.blur(1.3))
    svg.circle(x, y, r, g, EDGE, 1.2)
    svg.circle(x, y, r * 0.5, "none", shade(BR_D, 0.8), 0.8)
    svg.circle(x - r * 0.3, y - r * 0.32, r * 0.2, "#fffbe8", op=0.9)


def _brass_band(svg: Svg, x, y, w, h, rx=10, width=8):
    g = svg.lin([(0, "#f4e3b2"), (0.35, BR_M), (0.7, "#a98f5c"), (1, BR_D)], 0, 0, 1, 1)
    svg.rect(x, y, w, h, g, rx=rx, stroke=EDGE, sw=2)
    svg.rect(x + 2.2, y + 2.2, w - 4.4, h - 4.4, "none", rx=rx - 2, stroke="#fff3c8", sw=1.0, op=0.55)  # sunlit bevel
    svg.rect(x + width, y + width, w - 2 * width, h - 2 * width, "#20170d", rx=max(2, rx - width + 2))


def panel_svg(size=192, fill="navy") -> Svg:
    s = size
    svg = Svg(s, s)
    svg.rect(5, 7, s - 8, s - 8, "#000", rx=12, op=0.4, stroke=None)  # soft contact shadow
    _brass_band(svg, 3, 3, s - 6, s - 6)
    inner = (12, 12, s - 24, s - 24)
    if fill == "navy":
        g = svg.lin([(0, "#17303b"), (0.5, "#0f1f29"), (1, "#0a141b")], 0.1, 0, 0.8, 1)
    else:
        g = svg.lin([(0, "#f7e7bd"), (1, "#ecd6a2")], 0, 0, 1, 1)
    svg.rect(*inner, g, rx=5)
    if fill == "navy":
        svg.rect(17.5, 17.5, s - 35, s - 35, "none", rx=3, stroke="#b49a62", sw=1.2, op=0.75)
        v = svg.rad([(0.55, "#000", 0.0), (1, "#000", 0.55)], 0.5, 0.5, 0.75)
        svg.rect(*inner, v, rx=5)
    else:
        svg.rect(16, 16, s - 32, s - 32, "none", rx=2, stroke="#8a6e3c", sw=1.2, op=0.6)
    for cx, cy in ((13, 13), (s - 13, 13), (13, s - 13), (s - 13, s - 13)):
        _stud(svg, cx, cy, 9.5)
    return svg


def _plate(svg: Svg, x, y, w, h, tint="#f4e9cf"):
    """Cream square tile with a dark rim, used at both ends of a nameplate."""
    svg.rect(x, y, w, h, "#15110c", rx=2)
    g = svg.lin([(0, shade(tint, 1.08)), (1, shade(tint, 0.86))], 0, 0, 0.6, 1)
    svg.rect(x + 3, y + 3, w - 6, h - 6, g, rx=1.5, stroke="#7a6a4a", sw=1.0)
    svg.rect(x + 5, y + 5, w - 10, h - 10, "none", stroke="#fff", sw=0.8, op=0.5)


def nameplate_svg(w=288, h=56) -> Svg:
    svg = Svg(w, h)
    svg.rect(1, 3, w - 2, h - 3, "#000", rx=2, op=0.35, blur=svg.blur(1.6))
    g = svg.lin([(0, "#34322a"), (0.45, "#24231c"), (1, "#16150f")], 0, 0, 0, 1)
    svg.rect(0, 0, w, h, g, rx=3, stroke="#0b0a07", sw=1.5)
    svg.line((2, 2.5), (w - 2, 2.5), "#6b6650", 1.0, 0.6)
    _plate(svg, 0, 0, h, h)
    _plate(svg, w - h, 0, h, h)
    return svg


def button_svg(size=96, pressed=False) -> Svg:
    s = size
    svg = Svg(s, s)
    svg.rect(2, 4, s - 3, s - 3, "#000", rx=8, op=0.45, blur=svg.blur(1.6))
    top, bot = ("#cdb47c", "#a78e5c") if pressed else ("#e5cf9c", "#b09560")
    svg.rect(1.5, 1.5, s - 3, s - 3, svg.lin([(0, top), (1, bot)], 0, 0, 0.8, 1), rx=8, stroke="#1d150c", sw=2)
    # bevel: bright upper-left lip, dark lower-right lip (reversed when pressed)
    lt, dk = ("#6d5b38", "#fff0c0") if pressed else ("#fff0c0", "#6d5b38")
    svg.path(f"M6,{s-8} L6,8 Q6,6 8,6 L{s-8},6", stroke=lt, sw=2.2, op=0.9)
    svg.path(f"M{s-6},8 L{s-6},{s-8} Q{s-6},{s-6} {s-8},{s-6} L8,{s-6}", stroke=dk, sw=2.2, op=0.7)
    svg.rect(10, 10, s - 20, s - 20, "none", rx=4, stroke="#7a643a", sw=1.0, op=0.6)
    return svg


def dark_button_svg(w=192, h=72) -> Svg:
    svg = Svg(w, h)
    svg.rect(2, 5, w - 3, h - 4, "#000", rx=8, op=0.45, blur=svg.blur(1.8))
    _brass_band(svg, 2, 1.5, w - 4, h - 4, rx=8, width=5)
    g = svg.lin([(0, "#4a3626"), (0.5, "#2f2218"), (1, "#1f160f")], 0, 0, 0, 1)
    svg.rect(7, 6.5, w - 14, h - 14, g, rx=4)
    svg.line((10, 9), (w - 10, 9), "#a78a5a", 1.0, 0.5)
    return svg


def bar_frame_svg(w=256, h=32) -> Svg:
    svg = Svg(w, h)
    svg.rect(0, 1.5, w, h - 2, "#000", rx=5, op=0.35, blur=svg.blur(1.2))
    g = svg.lin([(0, "#cdb47c"), (0.5, BR_M), (1, BR_D)], 0, 0, 0, 1)
    svg.rect(0.8, 0.8, w - 1.6, h - 2, g, rx=5, stroke=EDGE, sw=1.4)
    svg.rect(4, 4, w - 8, h - 9, svg.lin([(0, "#0a1115"), (1, "#1a262c")], 0, 0, 0, 1), rx=3)
    return svg


def bar_fill_svg(kind="green", w=64, h=32) -> Svg:
    cols = {"green": ("#7fe36a", "#1fae3c", "#0c6a22"), "red": ("#f08a76", "#c3342a", "#6d1511"), "gold": ("#ffe58f", "#d3a22e", "#7a5612")}[kind]
    svg = Svg(w, h)
    g = svg.lin([(0, cols[0]), (0.45, cols[1]), (1, cols[2])], 0, 0, 0, 1)
    svg.rect(0, 0, w, h, g)
    svg.rect(0, 2, w, 4, "#fff", op=0.28)
    svg.rect(0, h - 3, w, 3, "#000", op=0.3)
    return svg


def select_svg(w=256, h=128) -> Svg:
    """Golden tile marker: glow, double rule, corner notches."""
    svg = Svg(w, h)
    pts = lambda inset: [(w / 2, inset * 0.5 + 1), (w - inset - 1, h / 2), (w / 2, h - inset * 0.5 - 1), (inset + 1, h / 2)]
    svg.poly(pts(6), "none", "#ffd873", 7, op=0.45, extra=f'filter="{svg.blur(4)}"')
    svg.poly(pts(7), "none", "#1d150a", 4.5, op=0.85)
    svg.poly(pts(7), "none", "#ffd877", 2.6)
    svg.poly(pts(15), "none", "#fff1c0", 1.0, op=0.75)
    for (x, y) in pts(7):
        svg.circle(x, y, 3.2, "#fff1c0", "#6d5420", 0.9)
    return svg


def flag_svg(kind="dawn", w=96, h=64) -> Svg:
    """Banner with a gentle wave; Dawn = cream with a rising sun, League = navy with a pale star."""
    svg = Svg(w, h)
    n = 12
    amp = 3.2

    def wave(i, off):
        return math.sin(i / n * math.pi * 2.2 + off) * amp * (0.25 + i / n)

    top = [(6 + i * (w - 12) / n, 6 + wave(i, 0)) for i in range(n + 1)]
    bot = [(6 + i * (w - 12) / n, h - 6 + wave(i, 0.0)) for i in range(n + 1)]
    pts = top + bot[::-1]
    cloth = "#f2e6cb" if kind == "dawn" else "#233b64"
    shadow_pts = [(x + 2.5, y + 3.5) for x, y in pts]
    svg.poly(shadow_pts, "#000", op=0.35, extra=f'filter="{svg.blur(2)}"')
    svg.poly(pts, svg.lin([(0, shade(cloth, 1.1)), (0.5, cloth), (1, shade(cloth, 0.72))], 0, 0, 1, 0.3), "#1b140c", 1.6)
    cx, cy = w * 0.5, h * 0.52 + wave(n // 2, 0)
    if kind == "dawn":
        r = h * 0.26
        svg.path(f"M{cx - r:.1f},{cy + r*0.5:.1f} A{r:.1f},{r:.1f} 0 0 1 {cx + r:.1f},{cy + r*0.5:.1f} Z", "#b3261e", "#6d130e", 1.0)
        for k in range(-3, 4):
            a = math.pi * (0.5 + k * 0.2)
            svg.line((cx + math.cos(a) * r * 1.2, cy + r * 0.5 - math.sin(a) * r * 1.2), (cx + math.cos(a) * r * 1.75, cy + r * 0.5 - math.sin(a) * r * 1.75), "#b3261e", 2.4)
        svg.line((cx - r * 1.9, cy + r * 0.62), (cx + r * 1.9, cy + r * 0.62), "#6d130e", 1.2)
    else:
        r = h * 0.30
        star = []
        for k in range(16):
            rr = r if k % 2 == 0 else r * 0.28
            if k % 4 in (1, 3):
                rr = r * 0.2
            a = k / 16 * math.tau - math.pi / 2
            star.append((cx + math.cos(a) * rr, cy + math.sin(a) * rr))
        svg.poly(star, "#eef0f2", "#0e1626", 1.0)
        svg.line((8, h * 0.8 + wave(2, 0)), (w - 8, h * 0.8 + wave(n - 2, 0)), "#9fb4d8", 2.0, 0.9)
    svg.line(top[0], bot[0], "#241a10", 2.4)
    return svg


def parchment_texture(size=512, seed=9) -> Image.Image:
    """Seamless mottled parchment: fibres, blotches, speckle."""
    ys, xs = np.mgrid[0:size, 0:size].astype(np.float32)
    a, b = xs / size, ys / size
    low = fbm(a, b, 3, 4, seed=seed, warp=0.2)
    mid = fbm(a, b, 12, 3, seed=seed + 1)
    fibre = fbm(a, b, 6, 3, seed=seed + 2, aniso=7.0, warp=0.05)
    fine = fbm(a, b, 90, 1, seed=seed + 3)
    t = np.clip(0.5 + 0.9 * low + 0.35 * mid + 0.22 * fibre + 0.1 * fine, 0, 1)
    col = P.ramp(t, [(0, "#e0c995"), (0.35, "#ebd8ab"), (0.62, "#f3e2ba"), (1.0, "#fcf1d2")])
    stains = P.smoothstep(0.55, 0.9, np.abs(fbm(a, b, 4, 3, seed=seed + 4)) * 2.0)
    col = P.mix(col, P.hx("#c9ad74"), stains * 0.18)
    speck = P.smoothstep(0.7, 0.9, fbm(a, b, 140, 1, seed=seed + 5) * 1.6) * 0.10
    col = col * (1.0 - speck)[..., None]
    return to_image(np.dstack([np.clip(col, 0, 1), np.ones((size, size), np.float32)]))


def textured_panel(kind: str) -> Image.Image:
    """Render the frame, then give the inner field its surface (parchment grain or navy noise)."""
    svg = panel_svg(192, fill="navy" if kind == "navy" else "parchment")
    img = to_array(render_sprite(svg.render(), 192, 192, ss=4, seed=4, radius=0, grain=0.05, strokes=0.0, grade_kw=None))
    mask_svg = Svg(192, 192)
    mask_svg.rect(12, 12, 168, 168, "#fff", rx=5)
    mask = to_array(downsample(rasterize(mask_svg.render(), 768, 768), 192, 192))[..., 3]
    if kind == "parchment":
        tex = to_array(parchment_texture(192, seed=21))[..., :3]
        img[..., :3] = img[..., :3] * (tex / 0.95)[..., :3] * mask[..., None] + img[..., :3] * (1 - mask[..., None])
        blur = np.asarray(Image.fromarray((mask * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(9)), dtype=np.float32) / 255.0
        edge = np.clip(mask - blur, 0, 1) * 0.9  # darker, scorched rim like the reference's parchment
        img[..., :3] *= (1.0 - 0.38 * edge * mask)[..., None]
    else:
        n = fbm(np.mgrid[0:192, 0:192][1] / 192.0, np.mgrid[0:192, 0:192][0] / 192.0, 24, 2, seed=30)
        img[..., :3] *= (1.0 + 0.05 * n * mask)[..., None]
    return to_image(np.clip(img, 0, 1))


def battle_svg(size=160) -> Svg:
    """Muzzle-flash and powder smoke marker for an engagement."""
    from .buildings import puffs

    svg = Svg(size, size)
    cx, cy = size / 2, size * 0.58
    svg.ellipse(cx, cy + 14, 46, 14, "#000", op=0.3, blur=svg.blur(5))
    for i, (dx, dy, sd) in enumerate([(-26, -6, 3), (22, -14, 4), (0, 6, 5)]):
        puffs(svg, (cx + dx, cy + dy), seed=sd, n=5, drift=(2.5, -7), size=11, op=0.85, dark=True)
    spikes = []
    for k in range(18):
        a = k / 18 * math.tau
        r = 30 if k % 2 == 0 else 13
        spikes.append((cx + math.cos(a) * r * 1.0, cy - 12 + math.sin(a) * r * 0.78))
    svg.poly(spikes, svg.rad([(0, "#fffbe0"), (0.4, "#ffd35a"), (1, "#e2561b")], 0.5, 0.5, 0.6), "#7a2a0e", 1.0)
    svg.circle(cx, cy - 12, 8, "#fffef0", blur=svg.blur(1.4))
    for k in range(9):
        a = (k * 0.7 + 0.3)
        r0, r1 = 34, 50 + (k % 3) * 6
        svg.line((cx + math.cos(a) * r0, cy - 12 + math.sin(a) * r0 * 0.8), (cx + math.cos(a) * r1, cy - 12 + math.sin(a) * r1 * 0.8), "#ffcf5a", 2.2)
    return svg
