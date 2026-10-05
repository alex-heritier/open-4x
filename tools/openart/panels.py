"""Interface art: the bottom-right info plaque, the next-turn disc and the
unit action buttons with a pictogram per `#UNIT_ACTIONS` entry.

Drawn at 4x on RGB canvases and reduced; the caller quantises them. Pictograms
are original shapes (hourglass, shield, pickaxe, ...), not copies of any
game's button art.
"""
import math

from PIL import Image, ImageDraw

SS = 4
INK = (60, 36, 22)
PAPER = (240, 228, 196)

STATES = {
    # ring dark, ring light, face edge, face centre, glyph shift
    "norm": ((120, 84, 38), (214, 170, 84), (226, 196, 116), (252, 236, 176), 0.0),
    "over": ((150, 104, 44), (240, 196, 100), (248, 222, 146), (255, 247, 206), 0.0),
    "down": ((74, 50, 26), (150, 114, 62), (176, 146, 90), (212, 188, 128), 1.2),
}


def _mix(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


class G:
    """A drawing surface in logical pixels (glyph frame centred on 0,0)."""

    def __init__(self, im, ox, oy, shift=0.0):
        self.d = ImageDraw.Draw(im)
        self.ox, self.oy = ox + shift, oy + shift

    def p(self, x, y):
        return ((self.ox + x) * SS, (self.oy + y) * SS)

    def poly(self, pts, fill=INK):
        self.d.polygon([self.p(*q) for q in pts], fill=fill)

    def line(self, pts, w=2.0, fill=INK):
        pp = [self.p(*q) for q in pts]
        self.d.line(pp, fill=fill, width=max(1, int(w * SS)), joint="curve")
        r = w * SS / 2.0
        for x, y in (pp[0], pp[-1]):
            self.d.ellipse((x - r, y - r, x + r, y + r), fill=fill)

    def ell(self, box, fill=INK, outline=None, w=1.5):
        x0, y0 = self.p(box[0], box[1])
        x1, y1 = self.p(box[2], box[3])
        self.d.ellipse((x0, y0, x1, y1), fill=fill, outline=outline, width=int(w * SS))

    def ring(self, box, w=1.6, fill=INK):
        self.ell(box, fill=None, outline=fill, w=w)

    def rect(self, x0, y0, x1, y1, fill=INK):
        a, b = self.p(x0, y0), self.p(x1, y1)
        self.d.rectangle((a[0], a[1], b[0], b[1]), fill=fill)

    def arc(self, box, a0, a1, w=1.8, fill=INK):
        x0, y0 = self.p(box[0], box[1])
        x1, y1 = self.p(box[2], box[3])
        self.d.arc((x0, y0, x1, y1), a0, a1, fill=fill, width=int(w * SS))


def _arrow(g, x0, y0, x1, y1, w=2.2, head=4.5):
    g.line([(x0, y0), (x1, y1)], w)
    ang = math.atan2(y1 - y0, x1 - x0)
    pts = [(x1 + math.cos(ang) * head * 0.9, y1 + math.sin(ang) * head * 0.9),
           (x1 + math.cos(ang + 2.5) * head, y1 + math.sin(ang + 2.5) * head),
           (x1 + math.cos(ang - 2.5) * head, y1 + math.sin(ang - 2.5) * head)]
    g.poly(pts)


def _house(g, cx=0, cy=0, s=1.0):
    g.rect(cx - 6 * s, cy - 1 * s, cx + 6 * s, cy + 8 * s)
    g.poly([(cx - 8 * s, cy - 1 * s), (cx, cy - 8 * s), (cx + 8 * s, cy - 1 * s)])
    g.rect(cx - 1.5 * s, cy + 3 * s, cx + 1.5 * s, cy + 8 * s, PAPER)


def _tent(g, cx=0, cy=0):
    g.poly([(cx - 9, cy + 8), (cx, cy - 8), (cx + 9, cy + 8)])
    g.poly([(cx - 2, cy + 8), (cx, cy + 1), (cx + 2, cy + 8)], PAPER)


def _road(g, cx=0):
    g.poly([(cx - 2, -9), (cx + 2, -9), (cx + 9, 9), (cx - 9, 9)])
    for y, w in ((-4, 0.6), (1, 0.9), (6.5, 1.3)):
        g.rect(cx - w / 2, y - 1.2, cx + w / 2, y + 1.2, PAPER)


def _rails(g, cx=0):
    g.line([(cx - 2, -9), (cx - 8, 9)], 1.6)
    g.line([(cx + 2, -9), (cx + 8, 9)], 1.6)
    for y in (-6, -1, 4, 8.5):
        k = (y + 9) / 18.0
        w = 3 + 5 * k
        g.line([(cx - w - 1, y), (cx + w + 1, y)], 1.3)


def _gear(g, cx=0, cy=0, r=6.5, teeth=8):
    for i in range(teeth):
        a = 2 * math.pi * i / teeth
        ca, sa = math.cos(a), math.sin(a)
        g.line([(cx + ca * (r - 1), cy + sa * (r - 1)), (cx + ca * (r + 2.2), cy + sa * (r + 2.2))], 2.6)
    g.ell((cx - r, cy - r, cx + r, cy + r))
    g.ell((cx - 2.6, cy - 2.6, cx + 2.6, cy + 2.6), fill=PAPER)


def _eye(g, bang=False):
    g.poly([(-9, 0), (-5, -4.5), (0, -6), (5, -4.5), (9, 0), (5, 4.5), (0, 6), (-5, 4.5)])
    g.ell((-4.5, -4.5, 4.5, 4.5), fill=PAPER)
    g.ell((-2.4, -2.4, 2.4, 2.4))
    if bang:
        g.rect(8, -9, 10, -3)
        g.rect(8, -1, 10, 1)


def _plane(g, s=1.0, ox=0, oy=0):
    g.line([(ox - 8 * s, oy), (ox + 8 * s, oy)], 2.6 * s)
    g.poly([(ox - 1 * s, oy), (ox - 5 * s, oy - 8 * s), (ox - 2.5 * s, oy - 8 * s), (ox + 3 * s, oy)])
    g.poly([(ox - 1 * s, oy), (ox - 5 * s, oy + 8 * s), (ox - 2.5 * s, oy + 8 * s), (ox + 3 * s, oy)])
    g.poly([(ox - 7 * s, oy), (ox - 9 * s, oy - 3.5 * s), (ox - 7.5 * s, oy - 3.5 * s), (ox - 5 * s, oy)])


def _crosshair(g):
    g.ring((-7, -7, 7, 7), 1.6)
    g.line([(0, -10), (0, -3)], 1.6)
    g.line([(0, 3), (0, 10)], 1.6)
    g.line([(-10, 0), (-3, 0)], 1.6)
    g.line([(3, 0), (10, 0)], 1.6)
    g.ell((-1.2, -1.2, 1.2, 1.2))


def _bomb(g):
    g.ell((-6, -3, 6, 9))
    g.line([(2, -3), (4, -7), (7, -8)], 1.4)
    g.poly([(7, -10), (9, -7), (6, -7)])
    g.ell((-3.5, 0, -1.5, 2), fill=PAPER)


def _flame(g):
    g.poly([(0, -10), (5, -3), (7, 3), (4, 9), (0, 10), (-4, 9), (-7, 3), (-4, -1), (-2, -4)])
    g.poly([(0, 0), (3, 4), (2, 8), (0, 9), (-2, 8), (-3, 4)], PAPER)


def _drop(g):
    g.poly([(0, -10), (6, 2), (5, 7), (0, 10), (-5, 7), (-6, 2)])
    g.ell((-3.5, 1, -1.5, 3.5), fill=PAPER)


def _tree(g, trunk=True):
    g.rect(-1.4, 2, 1.4, 9)
    g.ell((-7, -9, 7, 5))
    g.ell((-4, -5, 0, -1), fill=PAPER)


def _tower(g):
    g.rect(-6, -2, 6, 9)
    for x in (-6, -2, 2):
        g.rect(x, -6, x + 4, -2)
    g.poly([(-1.8, 9), (-1.8, 3), (0, 1.5), (1.8, 3), (1.8, 9)], PAPER)


def _pick(g):
    g.line([(-6, 9), (5, -3)], 2.2)
    g.poly([(-7, -3), (-1, -8), (9, -6), (10, -4), (4, -5), (-3, -1)])


def _axe(g):
    g.line([(-6, 9), (4, -6)], 2.2)
    g.poly([(1, -9), (9, -4), (8, 2), (3, -1)])


def _wrench(g):
    g.line([(-6, 8), (3, -1)], 3.0)
    g.ring((0, -9, 9, 0), 3.0)
    g.poly([(5, -9), (9, -9), (9, -5)], fill=PAPER)


def _reeds(g):
    for x, lean in ((-5, -2), (0, 0), (5, 2)):
        g.line([(x, 8), (x + lean, -8)], 1.6)
        g.ell((x + lean - 1.4, -10, x + lean + 1.4, -4))
    g.line([(-9, 9), (-4, 7), (0, 9), (4, 7), (9, 9)], 1.4)


GLYPHS = {
    0: lambda g: (g.poly([(-6, -9), (6, -9), (1, 0), (6, 9), (-6, 9), (-1, 0)]),
                  g.poly([(-3.4, -7), (3.4, -7), (0, -1.5)], PAPER), g.poly([(-4, 7), (4, 7), (0, 3)], PAPER)),
    1: lambda g: (g.ring((-8, -8, 8, 8), 2.0), g.line([(0, 0), (0, -5)], 1.8), g.line([(0, 0), (4, 2)], 1.8)),
    2: lambda g: (g.poly([(-7, -8), (7, -8), (7, 0), (0, 9), (-7, 0)]),
                  g.line([(0, -6), (0, 5)], 1.4, PAPER), g.line([(-4, -2), (4, -2)], 1.4, PAPER)),
    3: lambda g: (g.line([(-7, -7), (7, 7)], 3.0), g.line([(-7, 7), (7, -7)], 3.0)),
    4: lambda g: (g.line([(-8, 8), (-5, 4)], 1.6), g.line([(-3, 2), (-1, 0)], 1.6), _arrow(g, 1, -2, 8, -8)),
    5: lambda g: (g.ring((-8, -9, 3, 2), 2.0), g.line([(2, 1), (8, 8)], 3.0)),
    6: lambda g: _eye(g),
    7: lambda g: (g.rect(-8, 3, 8, 9), g.rect(-6, 5, 6, 7, PAPER), _arrow(g, 0, -10, 0, 1)),
    8: lambda g: (g.rect(-8, 3, 8, 9), g.rect(-6, 5, 6, 7, PAPER), _arrow(g, 0, 1, 0, -10)),
    9: lambda g: _plane(g),
    10: lambda g: _flame(g),
    11: lambda g: (g.line([(-8, 4), (4, -2)], 4.4), g.ell((-9, 1, -3, 9)),
                   g.line([(7, -5), (10, -8)], 1.4), g.line([(8, 0), (10, 0)], 1.4), g.line([(7, -9), (7, -10)], 1.4)),
    12: lambda g: (g.poly([(-9, -1), (-7, -7), (0, -10), (7, -7), (9, -1), (4, -3), (0, -1), (-4, -3)]),
                   g.line([(-8, -1), (0, 6)], 0.9), g.line([(8, -1), (0, 6)], 0.9), g.ell((-2, 5, 2, 9))),
    13: lambda g: (g.line([(-6, 9), (-6, -9)], 1.4), g.poly([(-6, -9), (7, -6), (-6, -2)]),
                   [g.ell((x - 2, 3, x + 2, 7)) for x in (-1, 4, 9)]),
    14: lambda g: g.poly([(2, -10), (-5, 1), (-1, 1), (-3, 10), (6, -2), (1, -2)]),
    15: lambda g: (g.poly([(-8, 0), (0, -9), (8, 0), (4, 0), (0, -4), (-4, 0)]),
                   g.poly([(-8, 8), (0, -1), (8, 8), (4, 8), (0, 3), (-4, 8)])),
    16: lambda g: (g.poly([(-8, 7), (-8, -4), (-4, 1), (0, -7), (4, 1), (8, -4), (8, 7)]),
                   g.ell((-1.2, -9, 1.2, -6.5))),
    17: lambda g: g.ell((-1.5, -1.5, 1.5, 1.5)),
    18: lambda g: _drop(g),
    19: lambda g: (g.poly([(-2, -9), (2, -9), (2, -3), (8, 7), (-8, 7), (-2, -3)]),
                   g.poly([(-5, 3), (5, 3), (7, 6), (-7, 6)], PAPER), g.ell((-1, -1, 1, 1), fill=PAPER)),
    20: lambda g: _tent(g),
    21: lambda g: (_house(g, -3, 1, 0.8), _house(g, 5, 3, 0.6)),
    22: lambda g: _road(g),
    23: lambda g: _rails(g),
    24: lambda g: _tower(g),
    25: lambda g: _pick(g),
    26: lambda g: (g.poly([(0, -10), (4, -3), (3, 0), (0, 2), (-3, 0), (-4, -3)]),
                   g.line([(-9, 4), (9, 4)], 1.5), g.line([(-9, 8), (9, 8)], 1.5)),
    27: lambda g: (_axe(g), g.rect(2, 6, 9, 9)),
    28: lambda g: _reeds(g),
    29: lambda g: _tree(g),
    30: lambda g: _wrench(g),
    31: lambda g: _gear(g),
    32: lambda g: (_house(g, 2, 2, 0.9), g.line([(-9, -5), (-3, -5)], 1.8), g.line([(-6, -8), (-6, -2)], 1.8)),
    33: lambda g: (g.poly([(-9, 9), (-4, 2), (4, 2), (9, 9)]), g.line([(0, 3), (0, 9)], 1.0, PAPER), _plane(g, 0.5, 0, -5)),
    34: lambda g: (g.line([(0, 9), (0, -3)], 2.0), g.line([(-5, 9), (0, 0), (5, 9)], 1.2),
                   g.arc((-6, -8, 6, 4), 200, 340, 1.6), g.arc((-9, -11, 9, 7), 210, 330, 1.4)),
    35: lambda g: (g.poly([(-9, 9), (-3, 3), (3, 3), (9, 9)]), g.line([(0, 3), (0, -9)], 1.6),
                   g.poly([(0, -9), (8, -6), (0, -3)])),
    36: lambda g: (g.line([(-8, 8), (6, -9)], 2.2), g.line([(8, 8), (-6, -9)], 2.2), g.line([(-9, 9), (9, 9)], 1.8),
                   g.poly([(-9, -10), (-6, -10), (-7.5, -7)]), g.poly([(9, -10), (6, -10), (7.5, -7)])),
    37: lambda g: _bomb(g),
    38: lambda g: (g.rect(-8, -4, 8, 8), g.rect(-4, -7, 3, -4), g.ell((-4, -2, 4, 6), fill=PAPER), g.ell((-2, 0, 2, 4))),
    39: lambda g: (g.poly([(-9, 6), (0, -8), (9, 6), (5, 6), (0, -1), (-5, 6)]),
                   g.poly([(-6, 10), (0, 3), (6, 10), (3, 10), (0, 7), (-3, 10)])),
    40: lambda g: (g.arc((-8, -8, 8, 8), 30, 300, 2.2), g.poly([(7, -9), (9, 0), (1, -3)]), _plane(g, 0.35)),
    41: lambda g: _crosshair(g),
    42: lambda g: _eye(g, True),
    43: lambda g: (_gear(g, -2, 2, 5.5), g.line([(5, -4), (9, -8)], 1.4), g.line([(6, 0), (10, 0)], 1.4)),
    44: lambda g: (_tent(g, -2, 2), _arrow(g, 1, -8, 9, -8, 1.6, 3.0)),
    45: lambda g: (_road(g, -3), _arrow(g, 2, -7, 10, -7, 1.6, 3.0)),
    46: lambda g: (_rails(g, -3), _arrow(g, 2, -7, 10, -7, 1.6, 3.0)),
    47: lambda g: (g.ell((-8, -8, 8, 8)), g.ell((-5.5, -5.5, 5.5, 5.5), fill=PAPER),
                   g.line([(-3, 3), (0, -3), (3, 3)], 1.8), g.line([(-2, 1), (2, 1)], 1.4)),
}


def _disc_layers(im, ox, oy, state):
    ring_dark, ring_light, edge, centre, shift = STATES[state]
    d = ImageDraw.Draw(im)
    cx, cy = (ox + 16) * SS, (oy + 16) * SS
    r0 = 15.3 * SS

    def circle(r, col, dx=0.0, dy=0.0):
        d.ellipse((cx + dx - r, cy + dy - r, cx + dx + r, cy + dy + r), fill=col)
    circle(r0, ring_dark)
    steps = 10
    for i in range(steps):
        t = i / (steps - 1)
        circle(r0 * (0.97 - 0.10 * t) , _mix(ring_light, ring_dark, 0.15 + 0.7 * t), -0.6 * SS * (1 - t), -0.8 * SS * (1 - t))
    face_r = 12.2 * SS
    n = 14
    for i in range(n):
        t = i / (n - 1)
        circle(face_r * (1 - t * 0.92), _mix(edge, centre, t), -3.0 * SS * t, -3.5 * SS * t)
    return shift


def _sheet(state):
    W, H = 257, 320
    im = Image.new("RGB", (W * SS, H * SS), (30, 22, 14))
    for r in range(10):
        for c in range(8):
            ox, oy = c * 32, r * 32
            shift = _disc_layers(im, ox, oy, state)
            g = G(im, ox + 16, oy + 16, shift)
            fn = GLYPHS.get(r * 8 + c)
            if fn is not None:
                fn(g)
    return im.resize((W, H), Image.LANCZOS)


def buttons(state):
    """RGB 257x320 sheet for 'norm', 'over' or 'down'."""
    return _sheet(state)


def button_alpha():
    W, H = 257, 320
    im = Image.new("L", (W * SS, H * SS), 0)
    d = ImageDraw.Draw(im)
    for r in range(10):
        for c in range(8):
            cx, cy = (c * 32 + 16) * SS, (r * 32 + 16) * SS
            rad = 15.3 * SS
            d.ellipse((cx - rad, cy - rad, cx + rad, cy + rad), fill=255)
    return im.resize((W, H), Image.LANCZOS).convert("RGB")


# ----------------------------------------------------------------- plaque

def _rounded(size, box, radius, knob):
    """Supersampled mask of a rounded plaque plus the knob circle."""
    m = Image.new("L", (size[0] * SS, size[1] * SS), 0)
    d = ImageDraw.Draw(m)
    x0, y0, x1, y1 = (v * SS for v in box)
    d.rounded_rectangle((x0, y0, x1, y1), radius * SS, fill=255)
    kx, ky, kr = (v * SS for v in knob)
    d.ellipse((kx - kr, ky - kr, kx + kr, ky + kr), fill=255)
    return m


def plaque():
    """(colour RGB, alpha RGB) for `box right`: 294x137, knob top-left."""
    W, H = 294, 137
    box = (5, 9, 290, 133)
    knob = (24, 15, 23)
    outer = _rounded((W, H), box, 30, knob)
    rim = _rounded((W, H), (box[0] + 4, box[1] + 4, box[2] - 4, box[3] - 4), 26, (knob[0], knob[1], knob[2] - 4))
    face = _rounded((W, H), (box[0] + 7, box[1] + 7, box[2] - 7, box[3] - 7), 23, (knob[0], knob[1], knob[2] - 7))
    col = Image.new("RGB", (W * SS, H * SS), (140, 102, 52))
    # gold rim: vertical gradient inside the outer shape
    rim_img = Image.new("RGB", col.size)
    rd = ImageDraw.Draw(rim_img)
    for y in range(rim_img.height):
        t = y / rim_img.height
        rd.line([(0, y), (rim_img.width, y)], fill=_mix((230, 190, 104), (130, 92, 44), t))
    col.paste(rim_img, mask=outer)
    inner_dark = Image.new("RGB", col.size, (118, 82, 40))
    col.paste(inner_dark, mask=rim)
    # parchment with a soft vignette
    par = Image.new("RGB", col.size)
    pd = ImageDraw.Draw(par)
    for y in range(par.height):
        t = abs(y / par.height - 0.5) * 2
        pd.line([(0, y), (par.width, y)], fill=_mix((240, 228, 192), (222, 204, 160), t * t))
    col.paste(par, mask=face)
    colour = col.resize((W, H), Image.LANCZOS)
    alpha = outer.resize((W, H), Image.LANCZOS).convert("RGB")
    return colour, alpha


def nextturn():
    """(colour, alpha) for the three 47x28 next-turn discs side by side."""
    W, H = 141, 28
    col = Image.new("RGB", (W * SS, H * SS), (100, 72, 36))
    al = Image.new("L", (W * SS, H * SS), 0)
    d = ImageDraw.Draw(col)
    ad = ImageDraw.Draw(al)
    tints = [((248, 214, 110), (190, 140, 50)), ((250, 160, 70), (196, 96, 30)), ((120, 176, 236), (50, 92, 170))]
    for i, (light, dark) in enumerate(tints):
        x0, y0, x1, y1 = (i * 47 + 3) * SS, 2 * SS, (i * 47 + 44) * SS, 26 * SS
        ad.ellipse((x0, y0, x1, y1), fill=255)
        d.ellipse((x0, y0, x1, y1), fill=_mix(dark, (60, 40, 20), 0.55))
        n = 10
        for k in range(n):
            t = k / (n - 1)
            inset = (1.2 + 7.5 * t) * SS
            d.ellipse((x0 + inset * 1.5, y0 + inset * 0.7, x1 - inset * 1.5 * 0.9, y1 - inset * 0.7 * 1.1),
                      fill=_mix(dark, light, t ** 0.8))
        d.ellipse((x0 + 9 * SS, y0 + 4 * SS, x0 + 19 * SS, y0 + 8 * SS), fill=_mix(light, (255, 255, 255), 0.55))
    return (col.resize((W, H), Image.LANCZOS),
            al.resize((W, H), Image.LANCZOS).convert("RGB"))
