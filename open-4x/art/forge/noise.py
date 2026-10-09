"""Noise that tiles on the isometric diamond lattice.

A terrain cell is a 2:1 diamond.  Neighbouring cells are offset by lattice vectors
(+-W/2, +-H/2).  Writing a cell-local pixel as lattice coordinates

    a = (u + v) / 2,   b = (u - v) / 2      with u, v in [-1, 1] across the diamond

a shift to a neighbouring cell changes `a` or `b` by exactly 1.  Any function that is periodic
with period 1 in both `a` and `b` therefore evaluates identically on shared cell borders, so
every cell of the atlas can reuse the same texture fields and the seams still vanish.
"""

import numpy as np


def lattice_coords(width: int, height: int):
    ys, xs = np.mgrid[0:height, 0:width].astype(np.float32)
    u = (xs + 0.5 - width / 2) / (width / 2)
    v = (ys + 0.5 - height / 2) / (height / 2)
    return u, v, (u + v) / 2, (u - v) / 2


def _fade(t):
    return t * t * t * (t * (t * 6 - 15) + 10)


def _gradients(ka, kb, seed):
    ang = np.random.default_rng(seed).random((ka, kb)) * (2 * np.pi)
    return np.cos(ang).astype(np.float32), np.sin(ang).astype(np.float32)


def gradient_noise(a, b, ka, kb=None, seed=0):
    """Periodic Perlin-style noise, period 1 in `a` and `b`, ka x kb gradient lattice, ~[-1, 1]."""
    kb = ka if kb is None else kb
    gx, gy = _gradients(ka, kb, seed)
    x = (a % 1.0) * ka
    y = (b % 1.0) * kb
    x0 = np.floor(x).astype(np.int64)
    y0 = np.floor(y).astype(np.int64)
    fx, fy = x - x0, y - y0
    x0 %= ka
    y0 %= kb
    x1, y1 = (x0 + 1) % ka, (y0 + 1) % kb

    def corner(ix, iy, dx, dy):
        return gx[ix, iy] * dx + gy[ix, iy] * dy

    n00 = corner(x0, y0, fx, fy)
    n10 = corner(x1, y0, fx - 1, fy)
    n01 = corner(x0, y1, fx, fy - 1)
    n11 = corner(x1, y1, fx - 1, fy - 1)
    u, v = _fade(fx), _fade(fy)
    top = n00 + u * (n10 - n00)
    bot = n01 + u * (n11 - n01)
    return (top + v * (bot - top)) * 1.4


def fbm(a, b, base, octaves, seed=0, gain=0.5, aniso=1.0, warp=0.0):
    """Sum of periodic octaves starting at `base` cells per tile.  `aniso` stretches the
    lattice along `a` (>1 = more detail across `a`, i.e. streaks along it).  `warp` bends the
    sampling position by a lower-frequency copy of itself (still periodic)."""
    if warp:
        wa = gradient_noise(a, b, max(2, base // 2), seed=seed + 101)
        wb = gradient_noise(a, b, max(2, base // 2), seed=seed + 202)
        a = a + wa * warp
        b = b + wb * warp
    total = np.zeros_like(a, dtype=np.float32)
    amp, norm = 1.0, 0.0
    for o in range(octaves):
        k = base * (2**o)
        total += amp * gradient_noise(a, b, int(k * aniso), k, seed + o * 17)
        norm += amp
        amp *= gain
    return total / norm


def unit(x):
    """Roughly [-1, 1] noise -> [0, 1]."""
    return np.clip(0.5 + 0.5 * x, 0.0, 1.0)


def flat_noise(width, height, base, octaves, seed=0, gain=0.5):
    """Non-tiling noise over a plain rectangle (for sprites, parchment, ...) -> ~[-1, 1].
    Implemented by evaluating the periodic generator on an enlarged torus."""
    ys, xs = np.mgrid[0:height, 0:width].astype(np.float32)
    s = max(width, height)
    return fbm(xs / s * 0.97, ys / s * 0.97, base, octaves, seed=seed, gain=gain)
