"""Map-view city sprites for the five culture groups, four eras, three sizes
(plus the walled variants), built on `figures.Scene`.

The model's `r` and `f` axes run along the tile diamond's edges, so houses sit
square to the grid like in an isometric city; the cell centre is the tile
centre. Everything is generated from boxes, roofs and balls.
"""
import random
import zlib

from PIL import Image

from . import figures as F

CELL = (167, 95)
GREEN = (84, 130, 58)
DOOR = (72, 48, 34)
GLASS = (104, 156, 196)

# (walls, roofs, trim, accent) per culture, medieval baseline.
CULTURES = [
    # AMER
    dict(walls=[(238, 234, 224), (180, 100, 80), (128, 148, 168)], roofs=[(96, 82, 74), (150, 64, 54)],
         trim=(250, 248, 240), accent=(60, 110, 80), special="church"),
    # EURO
    dict(walls=[(232, 214, 178), (176, 172, 166), (214, 196, 160)], roofs=[(188, 88, 58), (90, 98, 112)],
         trim=(110, 80, 56), accent=(150, 40, 40), special="keep"),
    # ROMAN
    dict(walls=[(242, 232, 210), (216, 166, 116), (228, 204, 170)], roofs=[(198, 100, 64), (176, 84, 54)],
         trim=(252, 248, 236), accent=(130, 40, 60), special="temple"),
    # MIDEAST
    dict(walls=[(224, 192, 142), (244, 238, 226), (208, 172, 120)], roofs=[(222, 192, 142), (240, 232, 214)],
         trim=(250, 240, 220), accent=(60, 150, 150), special="mosque"),
    # ASIAN
    dict(walls=[(182, 64, 52), (236, 224, 196), (160, 58, 50)], roofs=[(84, 124, 108), (70, 78, 96)],
         trim=(60, 40, 34), accent=(210, 170, 60), special="pagoda"),
]
THATCH = [(196, 164, 92), (176, 142, 78)]
MUD = [(204, 170, 124), (188, 150, 108), (214, 186, 140)]
BRICK = [(158, 86, 68), (138, 80, 66), (172, 110, 84)]
SLATE = [(84, 90, 104), (70, 76, 90)]
CONCRETE = [(204, 206, 210), (186, 190, 196), (224, 224, 226)]
FLAT = [(150, 152, 156), (130, 134, 140)]


def _pick(rng, seq):
    return seq[rng.randrange(len(seq))]


def _tri(sc, a, b, c, normal, rgb):
    sc.quad([a, b, c, c], normal, rgb)


def block(sc, c, hx, hy, h, rgb, windows=0, door=True, top=None):
    """A straight-walled box standing on the ground at (r, f) = c[:2]."""
    cr, cf = c
    sc.box((cr, cf, h / 2.0), (hx, hy, h / 2.0), rgb, top=top)
    if windows:
        wz = [h * (0.35 + 0.28 * k) for k in range(windows)]
        for z in wz:
            for off in (-hy * 0.5, hy * 0.5):
                sc.box((cr + hx + 0.1, cf + off, z), (0.15, 0.9, 1.1), GLASS)
            for off in (-hx * 0.5, hx * 0.5):
                sc.box((cr + off, cf + hy + 0.1, z), (0.9, 0.15, 1.1), GLASS)
    if door:
        sc.box((cr + hx * 0.4, cf + hy + 0.1, 1.6), (1.0, 0.15, 1.6), DOOR)


def gable(sc, c, hx, hy, z0, rh, rgb, wall, ridge="f", over=1.0):
    cr, cf = c
    ex, ey = hx + over, hy + over
    if ridge == "f":
        for side in (-1, 1):
            n = F._norm((side * rh, 0.0, ex))
            sc.quad([(cr + side * ex, cf - ey, z0), (cr + side * ex, cf + ey, z0),
                     (cr, cf + ey, z0 + rh), (cr, cf - ey, z0 + rh)], n, rgb)
        for side in (-1, 1):
            _tri(sc, (cr - hx, cf + side * hy, z0), (cr + hx, cf + side * hy, z0),
                 (cr, cf + side * hy, z0 + rh), (0, side, 0), wall)
    else:
        for side in (-1, 1):
            n = F._norm((0.0, side * rh, ey))
            sc.quad([(cr - ex, cf + side * ey, z0), (cr + ex, cf + side * ey, z0),
                     (cr + ex, cf, z0 + rh), (cr - ex, cf, z0 + rh)], n, rgb)
        for side in (-1, 1):
            _tri(sc, (cr + side * hx, cf - hy, z0), (cr + side * hx, cf + hy, z0),
                 (cr + side * hx, cf, z0 + rh), (side, 0, 0), wall)


def hip(sc, c, hx, hy, z0, rh, rgb, over=1.0):
    cr, cf = c
    ex, ey = hx + over, hy + over
    apex = (cr, cf, z0 + rh)
    corners = [(cr - ex, cf - ey, z0), (cr + ex, cf - ey, z0), (cr + ex, cf + ey, z0), (cr - ex, cf + ey, z0)]
    norms = [(0, -1, ey / rh), (1, 0, ex / rh), (0, 1, ey / rh), (-1, 0, ex / rh)]
    for k in range(4):
        a, b = corners[k], corners[(k + 1) % 4]
        _tri(sc, a, b, apex, F._norm(norms[k]), rgb)


def spire(sc, c, r, z0, h, rgb):
    sc.limb((c[0], c[1], z0), (c[0], c[1], z0 + h), r, 0.15, rgb)


def flag(sc, c, z, rgb):
    sc.rod((c[0], c[1], z), (c[0], c[1], z + 9), 0.5, (70, 60, 50))
    sc.poly([(c[0], c[1], z + 9), (c[0] + 5, c[1] + 2, z + 7.5), (c[0], c[1], z + 5.5)], rgb)


def dome(sc, c, rad, z0, rgb):
    sc.ball((c[0], c[1], z0), (rad, rad, rad * 1.05), rgb)


# ------------------------------------------------------------ buildings

def hut(sc, c, pal, rng, era):
    wall = _pick(rng, MUD)
    h = 5.0 + rng.random() * 1.5
    block(sc, c, 4.2, 4.2, h, wall, door=True)
    hip(sc, c, 4.2, 4.2, h, 5.0, _pick(rng, THATCH), over=1.2)


def house(sc, c, pal, rng, era):
    hx, hy = 4.5 + rng.random() * 2.0, 4.5 + rng.random() * 2.0
    h = 6.0 + rng.random() * 3.0
    if era == 2:
        wall, roof = _pick(rng, BRICK), _pick(rng, SLATE)
    else:
        wall, roof = _pick(rng, pal["walls"]), _pick(rng, pal["roofs"])
    flat_roof = pal["special"] == "mosque" and era == 1
    block(sc, c, hx, hy, h, wall, windows=1)
    if flat_roof:
        sc.box((c[0], c[1], h + 0.5), (hx + 0.4, hy + 0.4, 0.5), roof)
        if rng.random() < 0.4:
            dome(sc, c, min(hx, hy) * 0.7, h + 1.0, pal["accent"])
    elif pal["special"] == "pagoda" and era == 1:
        hip(sc, c, hx, hy, h, 4.6, roof, over=2.2)
    else:
        gable(sc, c, hx, hy, h, 4.0 + rng.random() * 2.0, roof, wall, ridge=_pick(rng, "fr"))


def tall(sc, c, pal, rng, era):
    hx, hy = 5.5 + rng.random() * 1.5, 5.5 + rng.random() * 1.5
    h = 13.0 + rng.random() * 6.0
    wall = _pick(rng, BRICK if era == 2 else pal["walls"])
    block(sc, c, hx, hy, h, wall, windows=3)
    if era >= 2:
        sc.box((c[0], c[1], h + 0.6), (hx + 0.3, hy + 0.3, 0.6), _pick(rng, FLAT))
    else:
        hip(sc, c, hx, hy, h, 6.0, _pick(rng, pal["roofs"]), over=1.4)


def factory(sc, c, pal, rng, era):
    hx, hy = 9.0, 6.0
    block(sc, c, hx, hy, 9.0, _pick(rng, BRICK), windows=1)
    for k in range(3):
        x = c[0] - hx + 3 + k * 6
        _tri(sc, (x - 3, c[1] - hy, 9), (x + 3, c[1] - hy, 9), (x - 3, c[1] - hy, 13), (-1, 0, 0), SLATE[0])
    sc.box((c[0], c[1], 9.4), (hx + 0.3, hy + 0.3, 0.5), SLATE[0])
    for k, dx in enumerate((-5, 3)):
        sc.limb((c[0] + dx, c[1] - 2, 9), (c[0] + dx, c[1] - 2, 27 + k * 3), 1.7, 1.3, (126, 70, 58))
        sc.ball((c[0] + dx + 1, c[1] - 3, 30 + k * 3), (2.2, 2.2, 1.8), (150, 150, 156))
        sc.ball((c[0] + dx + 3, c[1] - 4, 34 + k * 3), (2.8, 2.8, 2.2), (190, 190, 196))


def skyscraper(sc, c, pal, rng, era):
    hx, hy = 4.8 + rng.random() * 1.4, 4.8 + rng.random() * 1.4
    h = 28.0 + rng.random() * 16.0
    block(sc, c, hx, hy, h, _pick(rng, CONCRETE), door=False)
    levels = int(h // 3.4)
    for k in range(levels):
        z = 3.0 + k * 3.4
        sc.box((c[0] + hx + 0.1, c[1], z), (0.12, hy - 0.8, 1.0), GLASS)
        sc.box((c[0], c[1] + hy + 0.1, z), (hx - 0.8, 0.12, 1.0), (92, 142, 182))
    sc.box((c[0], c[1], h + 0.5), (hx + 0.3, hy + 0.3, 0.5), _pick(rng, FLAT))
    if rng.random() < 0.6:
        sc.rod((c[0], c[1], h + 1), (c[0], c[1], h + 9), 0.5, (90, 90, 96))


def apartment(sc, c, pal, rng, era):
    hx, hy = 7.0, 5.2
    h = 12.0 + rng.random() * 5.0
    block(sc, c, hx, hy, h, _pick(rng, CONCRETE), windows=3)
    sc.box((c[0], c[1], h + 0.5), (hx + 0.3, hy + 0.3, 0.5), _pick(rng, FLAT))


def keep(sc, c, pal, rng, era):
    wall = (176, 172, 166)
    block(sc, c, 7.0, 7.0, 20.0, wall, windows=2)
    for dx in (-6, 0, 6):
        for dy in (-6, 6):
            sc.box((c[0] + dx, c[1] + dy, 21.2), (1.4, 1.4, 1.2), wall)
        sc.box((c[0] + dx, c[1] - 6, 21.2), (1.4, 1.4, 1.2), wall)
    hip(sc, (c[0], c[1]), 4.0, 4.0, 20.0, 9.0, pal["roofs"][0], over=0.0)
    flag(sc, c, 29.0, pal["accent"])


def church(sc, c, pal, rng, era):
    wall = (240, 236, 226)
    block(sc, (c[0], c[1] + 3), 6.0, 10.0, 10.0, wall, windows=1)
    gable(sc, (c[0], c[1] + 3), 6.0, 10.0, 10.0, 5.0, pal["roofs"][0], wall, ridge="f")
    block(sc, (c[0] + 1, c[1] - 9), 3.4, 3.4, 18.0, wall, door=False)
    hip(sc, (c[0] + 1, c[1] - 9), 3.4, 3.4, 18.0, 10.0, pal["roofs"][1], over=0.6)
    sc.rod((c[0] + 1, c[1] - 9, 28), (c[0] + 1, c[1] - 9, 33), 0.5, (240, 220, 140))


def temple(sc, c, pal, rng, era):
    stone = (244, 238, 224)
    sc.box((c[0], c[1], 1.0), (9.0, 12.0, 1.0), (210, 204, 190))
    sc.box((c[0], c[1], 2.4), (8.0, 11.0, 0.5), stone)
    for fx in (-8.5, -3.0, 3.0, 8.5):
        sc.limb((c[0] + fx, c[1] + 10, 3.0), (c[0] + fx, c[1] + 10, 13.0), 1.1, 1.0, stone)
        sc.limb((c[0] + fx, c[1] - 10, 3.0), (c[0] + fx, c[1] - 10, 13.0), 1.1, 1.0, stone)
    sc.box((c[0], c[1], 13.5), (9.0, 11.5, 0.6), stone)
    gable(sc, c, 9.0, 11.0, 14.0, 4.5, pal["roofs"][0], stone, ridge="f", over=0.5)
    block(sc, c, 5.5, 8.0, 10.0, (226, 210, 180), door=False)


def mosque(sc, c, pal, rng, era):
    wall = (244, 238, 226)
    block(sc, c, 8.0, 8.0, 11.0, wall, windows=1)
    dome(sc, c, 7.0, 11.0, pal["accent"])
    for dx, dy in ((-9, -9), (9, 9)):
        sc.limb((c[0] + dx, c[1] + dy, 0), (c[0] + dx, c[1] + dy, 24), 1.5, 1.2, wall)
        sc.ball((c[0] + dx, c[1] + dy, 25), (1.8, 1.8, 2.4), pal["accent"])


def pagoda(sc, c, pal, rng, era):
    wall = pal["walls"][0]
    z, sz = 0.0, 7.0
    for tier in range(4):
        h = 6.0 if tier == 0 else 5.0
        sc.box((c[0], c[1], z + h / 2), (sz, sz, h / 2), wall)
        if tier == 0:
            sc.box((c[0] + sz * 0.3, c[1] + sz + 0.1, 1.8), (1.1, 0.15, 1.8), DOOR)
        z += h
        hip(sc, c, sz, sz, z, 3.2, pal["roofs"][0], over=2.6)
        z += 2.0
        sz *= 0.8
    spire(sc, c, 0.8, z + 1.0, 7.0, (230, 190, 80))


def ziggurat(sc, c, pal, rng, era):
    z = 0.0
    sz = 12.0
    for tier in range(3):
        sc.box((c[0], c[1], z + 2.0), (sz, sz, 2.0), _pick(rng, MUD))
        z += 4.0
        sz -= 3.4
    block(sc, c, 2.6, 2.6, 5.0, (226, 196, 150), door=False)
    sc.box((c[0], c[1], z + 5.6), (3.0, 3.0, 0.6), pal["accent"])


SPECIAL = {"church": church, "keep": keep, "temple": temple, "mosque": mosque, "pagoda": pagoda}


# --------------------------------------------------------------- layout

def _lots(n, spread, rng):
    cells = []
    g = 3 if n <= 5 else 4 if n <= 10 else 5
    step = 2.0 * spread / g
    for i in range(g):
        for j in range(g):
            cells.append((-spread + step * (i + 0.5), -spread + step * (j + 0.5)))
    rng.shuffle(cells)
    cells = cells[:n]
    return [(r + rng.uniform(-1.5, 1.5), f + rng.uniform(-1.5, 1.5)) for r, f in cells]


def _build(sc, culture, era, size, seed):
    rng = random.Random(seed)
    pal = CULTURES[culture]
    n = (5, 9, 14)[size]
    spread = (27, 33, 41)[size]
    reach = spread + 6
    lots = _lots(n, spread, rng)
    # the landmark takes the lot nearest the middle
    lots.sort(key=lambda p: p[0] ** 2 + p[1] ** 2)
    for k, (r, f) in enumerate(lots):
        c = (r, f)
        if k == 0 and size >= 1:
            if era == 0:
                ziggurat(sc, c, pal, rng, era)
            elif era == 3:
                skyscraper(sc, c, pal, rng, era)
            elif era == 2 and rng.random() < 0.5:
                factory(sc, c, pal, rng, era)
            else:
                SPECIAL[pal["special"]](sc, c, pal, rng, era)
            continue
        if era == 0:
            (hut if rng.random() < 0.65 else house)(sc, c, pal, rng, era)
        elif era == 1:
            (house if rng.random() < 0.7 else tall)(sc, c, pal, rng, era)
        elif era == 2:
            roll = rng.random()
            (house if roll < 0.45 else tall if roll < 0.8 else factory)(sc, c, pal, rng, era)
        else:
            roll = rng.random()
            (apartment if roll < 0.45 else skyscraper if roll < 0.75 else house)(sc, c, pal, rng, era)
    # bushes
    for _ in range(3 + size * 2):
        r, f = rng.uniform(-reach + 3, reach - 3), rng.uniform(-reach + 3, reach - 3)
        sc.ball((r, f, 1.6), (2.6, 2.6, 2.0), GREEN)


def _walls(sc, culture, era, seed):
    pal = CULTURES[culture]
    wall = (178, 172, 160) if era < 2 else (150, 148, 150)
    cap = (206, 200, 188) if era < 2 else (172, 170, 172)
    S = 45.0
    h = 8.0
    for sgn in (-1, 1):
        for axis in (0, 1):
            if axis == 0:
                sc.box((sgn * S, 0, h / 2), (1.8, S, h / 2), wall)
                sc.box((sgn * S, 0, h + 0.4), (2.2, S, 0.4), cap)
            else:
                if sgn == 1:
                    sc.box((0, S, h / 2), (S, 1.8, h / 2), wall)
                    sc.box((0, S, h + 0.4), (S, 2.2, 0.4), cap)
                else:
                    sc.box((0, -S, h / 2), (S, 1.8, h / 2), wall)
                    sc.box((0, -S, h + 0.4), (S, 2.2, 0.4), cap)
    for r, f in ((-S, -S), (S, -S), (S, S), (-S, S)):
        sc.box((r, f, 7.0), (3.4, 3.4, 7.0), wall)
        hip(sc, (r, f), 3.4, 3.4, 14.0, 5.0, pal["roofs"][0] if era < 2 else SLATE[0], over=0.8)
    sc.box((S + 0.2, 0, 3.4), (0.4, 4.0, 3.4), DOOR)               # gate on the lower-left side


def _plaza(reach, seed, w, h):
    """A rough patch of trodden earth under the city, no outline."""
    rng = random.Random(seed ^ 0x5EED)
    sc = F.Scene(w, h, 1, gy=h / 2.0 + 0.5, k=1.0)
    import math
    for scale, col, bias in ((1.0, (176, 152, 112), -900.0), (0.80, (198, 176, 132), -800.0),
                             (0.52, (212, 192, 150), -700.0)):
        pts = []
        for i in range(28):
            t = 2 * math.pi * i / 28
            c, s_ = math.cos(t), math.sin(t)
            rad = reach * scale * (0.92 + 0.10 * math.sin(3 * t + seed % 7) + rng.uniform(-0.03, 0.03))
            # rounded square
            n = (abs(c) ** 3 + abs(s_) ** 3) ** (1 / 3.0)
            pts.append((c / n * rad, s_ / n * rad, 0.0))
        sc.quad(pts, (0, 0, 1), col, bias=bias)
    sc.render()
    return sc.rgba(outline=None)


def sprite(culture, era, size, walled=False):
    """RGBA 167x95 city cell. `size` 0..2 (town, city, metropolis)."""
    w, h = CELL
    sc = F.Scene(w, h, 1, gy=h / 2.0 + 0.5, k=1.0)
    seed = zlib.crc32(f"city|{culture}|{era}|{size}".encode())
    _build(sc, culture, era, 1 if walled else size, seed)
    if walled:
        _walls(sc, culture, era, seed)
    sc.render()
    body = sc.rgba(outline=(46, 36, 38))
    reach = ((27, 33, 41)[1 if walled else size] + 5.0) if not walled else 50.0
    plaza = _plaza(reach, seed, w, h)
    return Image.alpha_composite(plaza, body)


def sheet(culture):
    """rXXXX.PCX: three sizes across, four eras down."""
    w, h = CELL
    rgb = Image.new("RGB", (w * 3, h * 4), (0, 0, 0))
    alpha = Image.new("L", (w * 3, h * 4), 0)
    for era in range(4):
        for size in range(3):
            cell = sprite(culture, era, size)
            rgb.paste(cell.convert("RGB"), (size * w, era * h))
            alpha.paste(cell.getchannel("A"), (size * w, era * h))
    return rgb, alpha


def wall_sheet(culture):
    w, h = CELL
    rgb = Image.new("RGB", (w, h * 4), (0, 0, 0))
    alpha = Image.new("L", (w, h * 4), 0)
    for era in range(4):
        cell = sprite(culture, era, 1, walled=True)
        rgb.paste(cell.convert("RGB"), (0, era * h))
        alpha.paste(cell.getchannel("A"), (0, era * h))
    return rgb, alpha
