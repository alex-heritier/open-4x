"""Map overlays that sit on top of the terrain: the fog of war and the culture-border dashes.

Both are sheets of 2:1 tile diamonds (256 x 128 at 2x, drawn 128 x 64), like the terrain cells.

* `fog_sheet`: a 9 x 9 grid of cells, black with the fog's opacity in the alpha channel. A tile's
  four vertices (north, east, south, west) are each 0 never seen, 1 remembered or 2 in sight, and the
  cell for `(vN, vE, vS, vW)` sits at row `3 * vW + vN`, column `3 * vS + vE`. The opacity runs
  smoothly across the diamond (bilinear between its vertices), so the edge of the explored world and
  of what you can see this turn is a soft gradient, not a tile staircase. Neighbouring tiles share
  the vertices of their common edge, so they agree there. The masks are half-open, so adjacent
  diamonds partition the plane exactly: no overlap, no gaps, and nearest-neighbour sampling never
  shows a seam.
* `border_sheet`: four diamonds side by side, each carrying a dashed line along one edge just inside
  it, in the order up-right, down-right, down-left, up-left. The dashes are white so the client can
  tint them with the owner's colour; a darker rim keeps them legible on any ground.
"""

import numpy as np
from PIL import Image


CELL_W, CELL_H = 256, 128
# Opacity of black at a vertex that is never seen, remembered, or in sight.
FOG_ALPHA = (255, 172, 0)


def fog_cell(v_n, v_e, v_s, v_w) -> np.ndarray:
    """One tile diamond, RGBA uint8: black, with opacity interpolated between the vertices."""
    ys, xs = np.mgrid[0:CELL_H, 0:CELL_W].astype(np.float64)
    px, py = xs + 0.5, ys + 0.5
    # Position in the tile's own axes: `a` runs north -> east, `b` north -> west.
    q, r = py / (CELL_H / 2), (px - CELL_W / 2) / (CELL_W / 2)
    a, b = (q + r) / 2, (q - r) / 2
    inside = (a >= 0) & (a < 1) & (b >= 0) & (b < 1)
    n, e, s, w = (FOG_ALPHA[v] for v in (v_n, v_e, v_s, v_w))
    alpha = (1 - a) * (1 - b) * n + a * (1 - b) * e + a * b * s + (1 - a) * b * w
    out = np.zeros((CELL_H, CELL_W, 4), dtype=np.uint8)
    out[..., 3] = np.where(inside, np.rint(alpha), 0).astype(np.uint8)
    return out


def fog_sheet() -> Image.Image:
    sheet = np.zeros((9 * CELL_H, 9 * CELL_W, 4), dtype=np.uint8)
    for v_w in range(3):
        for v_n in range(3):
            for v_s in range(3):
                for v_e in range(3):
                    row, col = 3 * v_w + v_n, 3 * v_s + v_e
                    sheet[row * CELL_H : (row + 1) * CELL_H, col * CELL_W : (col + 1) * CELL_W] = fog_cell(v_n, v_e, v_s, v_w)
    return Image.fromarray(sheet, "RGBA")


def border_sheet() -> Image.Image:
    """The same eight-bead paint as relief borders, on a flat diamond."""
    from .relief import border_masks
    ss = 4
    ys, xs = np.mgrid[0:CELL_H*ss, 0:CELL_W*ss].astype(np.float32)
    q = (ys + 0.5) / (CELL_H * ss / 2)
    r = (xs + 0.5 - CELL_W * ss / 2) / (CELL_W * ss / 2)
    u, v = (q+r)/2, (q-r)/2
    inside = (u >= 0) & (u <= 1) & (v >= 0) & (v <= 1)
    sheet = Image.new("RGBA", (4 * CELL_W, CELL_H))
    for edge, (col, alpha) in enumerate(border_masks(u, v)):
        pixels = np.dstack((col, alpha * inside))
        layer = Image.fromarray((np.clip(pixels, 0, 1)*255).astype(np.uint8), "RGBA")
        sheet.paste(layer.resize((CELL_W, CELL_H), Image.Resampling.LANCZOS), (edge * CELL_W, 0))
    return sheet
