"""Oblique height-field renderer for hills and mountains.

A cell is a 128x64 ground diamond seen from the south at 2:1. Ground point
(x, g) (g = 0 at the far corner, 63 at the near one) with elevation e lands on
screen row `ground_top + g - e`, so peaks rise above the diamond into the
extra rows a hill or mountain cell carries. Painting far to near with the
span between consecutive samples filled gives exact occlusion for this
projection without any 3D machinery, and the silhouette is a true profile
of the surface, not a drawn outline.

`render` returns (RGB, alpha, shadow): the shadow mask is a soft contact
shadow on the ground beside the object, meant for the exact-red index that
Civ3's loaders turn into a translucent dark.
"""
import math

from PIL import Image, ImageChops, ImageFilter

# x to the right, g toward the viewer, z up. Light from upper left, front.
LIGHT = (-0.52, -0.40, 0.75)
_LEN = math.sqrt(sum(c * c for c in LIGHT))
LIGHT = tuple(c / _LEN for c in LIGHT)


def inside_diamond(x, g, shrink=1.0):
    """True when ground point (x, g) is inside the (shrunk) tile diamond."""
    return abs(x + 0.5 - 64) / 64.0 + abs(g + 0.5 - 32) / 32.0 <= shrink


def render(size, ground_top, elev, colour, sx=0.9, sg=1.5, shadow_offset=(5, 3)):
    """Render `elev(x, g)` (None = outside, else elevation in px >= 0).

    `colour(x, g, e, lam, slope)` returns an (r, g, b) triple: `lam` is the
    Lambert term 0..1 and `slope` the gradient magnitude."""
    w, h = size
    grid = [[elev(x, g) for x in range(w)] for g in range(64)]

    def at(x, g):
        if 0 <= x < w and 0 <= g < 64:
            v = grid[g][x]
            return 0.0 if v is None else v
        return 0.0

    img = Image.new("RGB", size, (255, 0, 255))
    alpha = Image.new("L", size, 0)
    ipx, apx = img.load(), alpha.load()
    base = Image.new("L", size, 0)        # where the object meets the ground
    bpx = base.load()
    for x in range(w):
        prev_y = None
        for g in range(64):
            e = grid[g][x]
            if e is None:
                prev_y = None
                continue
            dx = (at(x + 1, g) - at(x - 1, g)) * 0.5 * sx
            dg = (at(x, g + 1) - at(x, g - 1)) * 0.5 * sg
            nl = math.sqrt(dx * dx + dg * dg + 1.0)
            lam = max(0.0, (-dx * LIGHT[0] - dg * LIGHT[1] + LIGHT[2]) / nl)
            rgb = colour(x, g, e, lam, math.sqrt(dx * dx + dg * dg))
            y = int(round(ground_top + g - e))
            lo, hi = (y, y) if prev_y is None else (min(y, prev_y), max(y, prev_y))
            if prev_y is not None and y > prev_y:
                lo = prev_y + 1             # gentle slope: just the new rows
            for yy in range(max(0, lo), min(h, hi + 1)):
                ipx[x, yy] = rgb
                apx[x, yy] = 255
            gy = ground_top + g
            if e < 1.5 and 0 <= gy < h:
                bpx[x, gy] = 255
            prev_y = y
    # contact shadow: the ground footprint pushed away from the light,
    # minus the object itself
    foot = base.filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.GaussianBlur(1.2))
    sh = ImageChops.offset(foot, shadow_offset[0], shadow_offset[1])
    sh = sh.point([min(255, int(i * 1.6)) for i in range(256)])
    sh = ImageChops.multiply(sh, ImageChops.invert(alpha))
    return img, alpha, sh
