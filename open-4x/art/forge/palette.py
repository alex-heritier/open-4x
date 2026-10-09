"""Shared colour vocabulary, picked by eye from the style references.

The references are a warm, saturated, hand-painted look: olive/yellow-green land, tan beaches,
deep teal sea with turquoise shallows, near-black navy UI panels with brass trim, and cream
parchment.  Light comes from the upper left; shade falls to the lower right.
"""

import numpy as np


def hx(code: str) -> np.ndarray:
    """'#rrggbb' -> float rgb in 0..1."""
    code = code.lstrip("#")
    return np.array([int(code[i : i + 2], 16) for i in (0, 2, 4)], dtype=np.float32) / 255.0


def ramp(t, stops):
    """Piecewise-linear colour ramp.  `t` is an array in 0..1, `stops` is [(pos, '#hex'), ...]."""
    t = np.clip(t, 0.0, 1.0)
    pos = np.array([p for p, _ in stops], dtype=np.float32)
    cols = np.stack([hx(c) for _, c in stops])
    return np.stack([np.interp(t, pos, cols[:, ch]) for ch in range(3)], axis=-1)


def mix(a, b, t):
    """Linear blend of colour fields; `t` has one fewer trailing axis than the colours."""
    t = np.asarray(t)[..., None]
    return a * (1.0 - t) + b * t


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


# --- land -----------------------------------------------------------------------------------
GRASS_STOPS = [
    (0.00, "#4d6024"),
    (0.20, "#5f7529"),
    (0.42, "#74872f"),
    (0.62, "#869638"),
    (0.82, "#98a642"),
    (1.00, "#b4bb62"),
]
OCHRE = "#9c8d4c"
SAND_STOPS = [
    (0.00, "#a6934c"),
    (0.35, "#c3ad73"),
    (0.65, "#d7c497"),
    (1.00, "#e9dfc0"),
]
WET_SAND = "#8f7d4a"

# --- sea ------------------------------------------------------------------------------------
WATER_STOPS = [  # by depth: 0 at the shoreline, 1 in open water
    (0.00, "#4cbbb8"),
    (0.14, "#33a0ab"),
    (0.38, "#247f95"),
    (0.70, "#1d6479"),
    (1.00, "#195367"),
]
FOAM = "#eef3e6"

# --- ink / outlines -------------------------------------------------------------------------
INK = "#16130e"
SHADOW = "#10140b"

# --- ui -------------------------------------------------------------------------------------
PANEL_DARK = "#0e1a22"
PANEL_MID = "#162a34"
BRASS_LIGHT = "#e6cf9a"
BRASS = "#bca06e"
BRASS_DARK = "#7c6840"
PARCHMENT = "#f1ddb1"
PARCHMENT_DARK = "#e0c791"
CREAM = "#f6ead0"
CRIMSON = "#b3261e"
