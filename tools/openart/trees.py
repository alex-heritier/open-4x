"""Tree cover: broadleaf woods, jungle, pines, drawn as lit canopy clumps.

Contract (src/blend.rs `cover_sprite`): a forest sheet is 1000x884, ten rows
of 88.4 px, 128 px columns. A sprite is the cell inset 2 px (124 x ~84) with
the tile centre at (62, 48):

    rows 0-1  jungle, joined to a neighbour  (4 columns)
    rows 2-3  jungle, isolated               (6)
    rows 4-5  forest, joined                 (4)
    rows 6-7  forest, isolated               (5)
    rows 8-9  pine                           (6)

"Joined" cells are big canopies meant to overlap neighbours so woods read as
one mass; isolated ones are a few trees. Everything is drawn at 3x and
reduced, then outlined, so it survives the 1-bit alpha of a palette image.
"""
import math
import random
import zlib

from PIL import Image, ImageChops, ImageDraw, ImageFilter

from . import gfx
from .gfx import KEY

S = 3                      # supersampling
CW, CH = 124, 84           # usable cell (inset by 2)
CX, CY = 62, 48            # tile centre inside the cell

PALETTES = {
    "grass": dict(dark=(30, 74, 38), mid=(58, 116, 50), light=(112, 168, 70), hi=(170, 205, 96), trunk=(88, 62, 42), rim=(18, 44, 26)),
    "plains": dict(dark=(52, 84, 36), mid=(88, 124, 52), light=(140, 168, 74), hi=(196, 208, 104), trunk=(96, 70, 44), rim=(34, 52, 24)),
    "tundra": dict(dark=(26, 58, 46), mid=(48, 92, 68), light=(96, 144, 108), hi=(226, 238, 244), trunk=(84, 64, 48), rim=(14, 34, 30)),
    "jungle": dict(dark=(14, 66, 44), mid=(26, 110, 58), light=(70, 166, 70), hi=(150, 214, 96), trunk=(82, 58, 40), rim=(8, 38, 26)),
}


def _p(v):
    return v * S


def _blob(d, x, y, r, col):
    d.ellipse([_p(x - r), _p(y - r * 0.9), _p(x + r), _p(y + r * 0.9)], fill=col + (255,))


def _broadleaf(d, x, y, h, rnd, pal):
    """A round-crowned tree, base at (x, y), height h."""
    tw = max(1.4, h * 0.07)
    d.rectangle([_p(x - tw), _p(y - h * 0.34), _p(x + tw), _p(y)], fill=pal["trunk"] + (255,))
    d.rectangle([_p(x + tw * 0.2), _p(y - h * 0.34), _p(x + tw), _p(y)], fill=gfx.shade_rgb(pal["trunk"], 0.7) + (255,))
    r = h * 0.34
    cy = y - h * 0.62
    lobes = [(0, 0, 1.0), (-0.62, 0.22, 0.72), (0.62, 0.2, 0.74), (-0.18, -0.46, 0.7), (0.28, -0.4, 0.62), (0.0, 0.34, 0.7)]
    for dx, dy, k in lobes:                       # shadow side, whole mass
        _blob(d, x + dx * r + 0.5, cy + dy * r + 0.6, r * k, pal["dark"])
    for dx, dy, k in lobes:                       # body, shifted toward the light
        _blob(d, x + dx * r - 0.6, cy + dy * r - 0.7, r * k * 0.86, pal["mid"])
    for dx, dy, k in lobes[:5]:                   # lit tops
        _blob(d, x + dx * r - 1.5, cy + dy * r - 1.8, r * k * 0.52, pal["light"])
    for _ in range(4):                            # leaf sparkle
        a = rnd.uniform(math.pi, 1.9 * math.pi)
        rr = rnd.uniform(0.15, 0.7) * r
        _blob(d, x + math.cos(a) * rr - 1, cy + math.sin(a) * rr - 1.5, r * 0.16, pal["hi"])


def _pine(d, x, y, h, rnd, pal, snow=True):
    """A conifer: three overlapping tiers on a short trunk."""
    d.rectangle([_p(x - 1.2), _p(y - h * 0.18), _p(x + 1.2), _p(y)], fill=pal["trunk"] + (255,))
    tiers = 3
    for i in range(tiers):
        base_y = y - h * (0.14 + 0.2 * i)
        top_y = base_y - h * (0.46 - 0.06 * i)
        half = h * (0.30 - 0.06 * i)
        pts = [(x, top_y), (x + half, base_y), (x - half, base_y)]
        d.polygon([(_p(a), _p(b)) for a, b in pts], fill=pal["dark"] + (255,))
        lit = [(x, top_y), (x - half * 0.05, base_y), (x - half, base_y)]
        d.polygon([(_p(a), _p(b)) for a, b in lit], fill=pal["mid"] + (255,))
        lit2 = [(x - half * 0.1, top_y + h * 0.1), (x - half * 0.1, base_y), (x - half * 0.7, base_y)]
        d.polygon([(_p(a), _p(b)) for a, b in lit2], fill=pal["light"] + (255,))
        if snow:
            cap = [(x, top_y), (x + half * 0.34, top_y + h * 0.15), (x - half * 0.34, top_y + h * 0.15)]
            d.polygon([(_p(a), _p(b)) for a, b in cap], fill=pal["hi"] + (255,))


def _jungle(d, x, y, h, rnd, pal):
    """A dense tropical tree: a broad crown with drooping fronds."""
    tw = max(1.4, h * 0.06)
    d.polygon([(_p(x - tw), _p(y)), (_p(x - tw * 0.6), _p(y - h * 0.5)), (_p(x + tw * 0.6), _p(y - h * 0.5)), (_p(x + tw), _p(y))],
              fill=pal["trunk"] + (255,))
    cy = y - h * 0.62
    r = h * 0.40
    for ring, col, shift in ((1.0, pal["dark"], 0.6), (0.8, pal["mid"], -0.4), (0.55, pal["light"], -1.4)):
        for i in range(9):
            a = math.tau * i / 9 + rnd.uniform(-0.15, 0.15) + (0.2 if ring < 1 else 0)
            lx, ly = x + math.cos(a) * r * ring * 0.9 + shift, cy + math.sin(a) * r * ring * 0.5 + shift * 0.5
            leaf = [(x + shift, cy + shift * 0.5), (lx + math.cos(a + 0.5) * r * 0.35, ly + math.sin(a + 0.5) * r * 0.2 - r * 0.08),
                    (lx + math.cos(a) * r * 0.25, ly + math.sin(a) * r * 0.16 + r * 0.14),
                    (lx + math.cos(a - 0.5) * r * 0.35, ly + math.sin(a - 0.5) * r * 0.2 - r * 0.08)]
            d.polygon([(_p(a_), _p(b_)) for a_, b_ in leaf], fill=col + (255,))
        _blob(d, x + shift, cy + shift * 0.5, r * 0.45 * ring, col)
    for _ in range(3):
        _blob(d, x + rnd.uniform(-0.4, 0.2) * r - 1, cy + rnd.uniform(-0.5, 0.1) * r - 1.5, r * 0.14, pal["hi"])


DRAW = {"forest": _broadleaf, "jungle": _jungle, "pine": _pine}


def _positions(rnd, count, spread_x, spread_y, cx=CX, cy=CY + 6):
    """Jittered points over a diamond-ish footprint, back to front."""
    pts = []
    tries = 0
    while len(pts) < count and tries < 400:
        tries += 1
        x = cx + rnd.uniform(-spread_x, spread_x)
        y = cy + rnd.uniform(-spread_y, spread_y)
        if abs(x - cx) / spread_x + abs(y - cy) / spread_y > 1.0:
            continue
        if all(abs(x - px) > 11 or abs(y - py) > 6 for px, py in pts):
            pts.append((x, y))
    return sorted(pts, key=lambda p: p[1])


def clump(kind, palette, joined, seed):
    """One 124x84 sprite: (RGB, alpha, shadow)."""
    rnd = random.Random(seed)
    pal = PALETTES[palette]
    big = Image.new("RGBA", (CW * S, CH * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    shadow = Image.new("L", (CW * S, CH * S), 0)
    sd = ImageDraw.Draw(shadow)
    if joined:
        count = rnd.randint(11, 15) if kind != "pine" else rnd.randint(13, 18)
        pts = _positions(rnd, count, 54, 25)
    else:
        count = rnd.randint(2, 4)
        pts = _positions(rnd, count, 24, 11)
    draw = DRAW[kind]
    for x, y in pts:
        depth = (y - (CY - 19)) / 50.0                   # 0 far .. 1 near
        h = (24 + 12 * depth) * (rnd.uniform(0.9, 1.15) if joined else rnd.uniform(1.0, 1.3))
        h = min(h, y - 4)                                # keep the crown inside the cell
        w = h * 0.5
        sd.ellipse([_p(x - w * 0.4), _p(y - 2.5), _p(x + w * 1.5), _p(y + 3.2)], fill=255)
        draw(d, x, y, h, rnd, pal) if kind != "pine" else draw(d, x, y, h * 1.12, rnd, pal, snow=palette == "tundra")
    img = big.resize((CW, CH), Image.Resampling.LANCZOS)
    img = gfx.outline(img, pal["rim"], 1)
    alpha = img.getchannel("A").point([255 if i >= 110 else 0 for i in range(256)])
    shadow = shadow.resize((CW, CH), Image.Resampling.LANCZOS).filter(ImageFilter.GaussianBlur(0.8))
    shadow = ImageChops.multiply(shadow, ImageChops.invert(alpha))
    rgb = Image.new("RGB", img.size, KEY)
    rgb.paste(img.convert("RGB"), mask=alpha)
    return rgb, alpha, shadow


# (first row, rows, columns, joined, kind)
BLOCKS = {
    "jungle_joined": (0, 4, True, "jungle"),
    "jungle_alone": (2, 6, False, "jungle"),
    "forest_joined": (4, 4, True, "forest"),
    "forest_alone": (6, 5, False, "forest"),
    "pine": (8, 6, True, "pine"),
}
ROW_H = 884 / 10.0


def cover_sheet(palette, size=(1000, 884), blocks=None):
    """A `* forests` sheet laid out on the 88.4 px grid."""
    rgb = Image.new("RGB", size, KEY)
    alpha = Image.new("L", size, 0)
    shadow = Image.new("L", size, 0)
    for name, (row0, cols, joined, kind) in BLOCKS.items():
        if blocks and name not in blocks:
            continue
        pal = "jungle" if kind == "jungle" else palette
        for r in range(2):
            for c in range(cols):
                a, m, s = clump(kind, pal, joined, zlib.crc32(f"{name}/{palette}/{r}/{c}".encode()) & 0xFFFF)
                x = c * 128 + 2
                y = int(round((row0 + r) * ROW_H)) + 2
                rgb.paste(a, (x, y), m)
                alpha.paste(m, (x, y))
                shadow.paste(s, (x, y))
    return rgb, alpha, shadow


def on_sheet(base, cols, rows, cw, ch, kind, palette, lift):
    """Put a small clump on every cell of a hill or mountain sheet.

    `lift` raises the clump above the diamond centre (negative lowers it);
    anything that would fall outside its own cell is cut off."""
    rgb, alpha, shadow = base
    rgb, alpha, shadow = rgb.copy(), alpha.copy(), shadow.copy()
    for i in range(cols * rows):
        c, r = i % cols, i // cols
        a, m, s = clump(kind, palette, False, 900 + i)
        x = c * cw + 64 - CX
        y = r * ch + (ch - 32) - CY - lift
        box = (c * cw, r * ch, (c + 1) * cw, (r + 1) * ch)
        x0, y0 = max(x, box[0]), max(y, box[1])
        x1, y1 = min(x + CW, box[2]), min(y + CH, box[3])
        if x1 <= x0 or y1 <= y0:
            continue
        crop = (x0 - x, y0 - y, x1 - x, y1 - y)
        a, m, s = a.crop(crop), m.crop(crop), s.crop(crop)
        rgb.paste(a, (x0, y0), m)
        alpha.paste(m, (x0, y0), m)
        shadow.paste(s, (x0, y0))
    return rgb, alpha, shadow
