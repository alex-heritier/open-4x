"""Tiny SVG scene builder plus colour and isometric helpers used by every sprite module."""

import math
from dataclasses import dataclass, field


# --- colour -----------------------------------------------------------------------------------
def rgb(code: str):
    code = code.lstrip("#")
    return tuple(int(code[i : i + 2], 16) for i in (0, 2, 4))


def hexc(t) -> str:
    return "#%02x%02x%02x" % tuple(max(0, min(255, int(round(c)))) for c in t)


def shade(code: str, f: float) -> str:
    """f < 1 darkens (keeping a warm/cool bias), f > 1 lightens toward cream."""
    r, g, b = rgb(code)
    if f <= 1:
        # darkening shifts slightly toward blue-violet shadow, as in the painted references
        return hexc((r * f * 0.98, g * f * 0.99, b * f * 1.04))
    t = min(1.0, f - 1.0)
    return hexc((r + (255 - r) * t * 0.9, g + (246 - g) * t * 0.9, b + (214 - b) * t * 0.9))


def mixc(a: str, b: str, t: float) -> str:
    ra, rb = rgb(a), rgb(b)
    return hexc(tuple(x * (1 - t) + y * t for x, y in zip(ra, rb)))


def fmt(p):
    return f"{p[0]:.2f},{p[1]:.2f}"


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


# --- scene ------------------------------------------------------------------------------------
@dataclass
class Svg:
    w: int
    h: int
    defs: list = field(default_factory=list)
    items: list = field(default_factory=list)
    _n: int = 0

    def uid(self, prefix="d"):
        self._n += 1
        return f"{prefix}{self._n}"

    def raw(self, s):
        self.items.append(s)

    def lin(self, stops, x1=0, y1=0, x2=0, y2=1):
        """linearGradient in bounding-box space; stops are [(offset, '#hex'[, opacity])]."""
        i = self.uid("lg")
        st = "".join(
            f'<stop offset="{o}" stop-color="{c}"' + (f' stop-opacity="{rest[0]}"' if rest else "") + "/>"
            for o, c, *rest in stops
        )
        self.defs.append(f'<linearGradient id="{i}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">{st}</linearGradient>')
        return f"url(#{i})"

    def rad(self, stops, cx=0.5, cy=0.5, r=0.5, fx=None, fy=None):
        i = self.uid("rg")
        st = "".join(
            f'<stop offset="{o}" stop-color="{c}"' + (f' stop-opacity="{rest[0]}"' if rest else "") + "/>"
            for o, c, *rest in stops
        )
        f = f' fx="{fx}" fy="{fy}"' if fx is not None else ""
        self.defs.append(f'<radialGradient id="{i}" cx="{cx}" cy="{cy}" r="{r}"{f}>{st}</radialGradient>')
        return f"url(#{i})"

    def blur(self, sd):
        i = self.uid("bl")
        self.defs.append(
            f'<filter id="{i}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{sd}"/></filter>'
        )
        return f"url(#{i})"

    def clip(self, pts):
        i = self.uid("cp")
        self.defs.append(f'<clipPath id="{i}"><polygon points="{" ".join(fmt(p) for p in pts)}"/></clipPath>')
        return f"url(#{i})"

    def poly(self, pts, fill, stroke=None, sw=1.0, op=1.0, extra=""):
        s = f' stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round"' if stroke else ""
        o = f' opacity="{op}"' if op != 1.0 else ""
        self.items.append(f'<polygon points="{" ".join(fmt(p) for p in pts)}" fill="{fill}"{s}{o} {extra}/>')

    def path(self, d, fill="none", stroke=None, sw=1.0, op=1.0, extra=""):
        s = f' stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"' if stroke else ""
        o = f' opacity="{op}"' if op != 1.0 else ""
        self.items.append(f'<path d="{d}" fill="{fill}"{s}{o} {extra}/>')

    def line(self, a, b, stroke, sw=1.0, op=1.0, cap="round"):
        o = f' opacity="{op}"' if op != 1.0 else ""
        self.items.append(
            f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" stroke="{stroke}" '
            f'stroke-width="{sw}" stroke-linecap="{cap}"{o}/>'
        )

    def ellipse(self, cx, cy, rx, ry, fill, op=1.0, blur=None, stroke=None, sw=1.0, rot=0):
        o = f' opacity="{op}"' if op != 1.0 else ""
        f = f' filter="{blur}"' if blur else ""
        s = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ""
        t = f' transform="rotate({rot} {cx:.2f} {cy:.2f})"' if rot else ""
        self.items.append(f'<ellipse cx="{cx:.2f}" cy="{cy:.2f}" rx="{rx:.2f}" ry="{ry:.2f}" fill="{fill}"{o}{f}{s}{t}/>')

    def circle(self, cx, cy, r, fill, op=1.0, stroke=None, sw=1.0, blur=None):
        self.ellipse(cx, cy, r, r, fill, op, blur, stroke, sw)

    def rect(self, x, y, w, h, fill, rx=0, stroke=None, sw=1.0, op=1.0, blur=None):
        s = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ""
        o = f' opacity="{op}"' if op != 1.0 else ""
        f = f' filter="{blur}"' if blur else ""
        self.items.append(
            f'<rect x="{x:.2f}" y="{y:.2f}" width="{w:.2f}" height="{h:.2f}" rx="{rx}" fill="{fill}"{s}{o}{f}/>'
        )

    def begin(self, extra=""):
        self.items.append(f"<g {extra}>")

    def end(self):
        self.items.append("</g>")

    def group(self, inner, extra=""):
        self.items.append(f"<g {extra}>{inner}</g>")

    def render(self) -> str:
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.w}" height="{self.h}" '
            f'viewBox="0 0 {self.w} {self.h}">\n<defs>{"".join(self.defs)}</defs>\n' + "\n".join(self.items) + "\n</svg>\n"
        )


# --- isometric helpers ---------------------------------------------------------------------------
@dataclass
class Iso:
    """2:1 isometric projection.  x runs to the lower right, y to the lower left, z up (pixels)."""

    ox: float
    oy: float
    k: float = 16.0

    def p(self, x, y, z=0.0):
        return (self.ox + (x - y) * self.k, self.oy + (x + y) * self.k / 2 - z)

    def ground(self, x, y, dx, dy, z=0.0):
        return [self.p(x, y, z), self.p(x + dx, y, z), self.p(x + dx, y + dy, z), self.p(x, y + dy, z)]


def wall_style(base: str):
    """(lit left face, shaded right face, top) colours for a wall material."""
    return shade(base, 1.04), shade(base, 0.64), shade(base, 1.18)


def face_gradient(svg: Svg, col: str, top=1.12, bottom=0.82):
    return svg.lin([(0, shade(col, top)), (1, shade(col, bottom))], 0, 0, 0.25, 1)
