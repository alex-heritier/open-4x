"""Painterly finishing for rasterised SVG art: oil-paint smoothing, paint grain, soft rim light."""

import numpy as np
from PIL import Image

from .noise import flat_noise
from .util import downsample, rasterize, to_array, to_image


def _window_mean(a, y0, y1, x0, x1):
    """Mean of a over the offset window [y0..y1] x [x0..x1] around each pixel (edge-padded)."""
    r = max(abs(y0), abs(y1), abs(x0), abs(x1))
    pad = np.pad(a, ((r + 1, r), (r + 1, r)) + ((0, 0),) * (a.ndim - 2), mode="edge")
    integral = pad.cumsum(0).cumsum(1)
    h, w = a.shape[:2]
    ys, xs = np.arange(h), np.arange(w)

    def at(dy, dx):
        return integral[(ys + dy + r + 1)[:, None], (xs + dx + r + 1)[None, :]]

    area = (y1 - y0 + 1) * (x1 - x0 + 1)
    return (at(y1, x1) - at(y0 - 1, x1) - at(y1, x0 - 1) + at(y0 - 1, x0 - 1)) / area


def kuwahara(arr: np.ndarray, radius: int = 2) -> np.ndarray:
    """Edge-preserving oil-paint filter on premultiplied RGBA floats."""
    r = radius
    lum = arr[..., :3] @ np.array([0.3, 0.59, 0.11], dtype=np.float32)
    best_var = np.full(lum.shape, np.inf, dtype=np.float32)
    out = np.zeros_like(arr)
    for y0, y1, x0, x1 in ((-r, 0, -r, 0), (-r, 0, 0, r), (0, r, -r, 0), (0, r, 0, r)):
        mean = _window_mean(arr, y0, y1, x0, x1)
        var = _window_mean(lum * lum, y0, y1, x0, x1) - _window_mean(lum, y0, y1, x0, x1) ** 2
        pick = var < best_var
        best_var = np.where(pick, var, best_var)
        out = np.where(pick[..., None], mean, out)
    return out


def grade(rgb, alpha_shape, sat=1.14, contrast=1.1, key=0.13):
    """Colour grade: richer saturation, a touch more contrast, and the shared upper-left key light
    (brighter top-left, falling off to the lower right) so every sprite agrees with the terrain."""
    h, w = rgb.shape[:2]
    lum = (rgb @ np.array([0.3, 0.59, 0.11], dtype=np.float32))[..., None]
    rgb = lum + (rgb - lum) * sat
    rgb = (rgb - 0.42) * contrast + 0.42
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    ramp = 1.0 + key * (0.5 - (0.55 * xx / w + 0.45 * yy / h)) * 2.0
    return rgb * ramp[..., None]


def rim_light(rgb, alpha, sigma=5.0, strength=0.55):
    """Pseudo-3D shading from the silhouette alone: edges facing the upper-left sun brighten and
    edges facing away darken, so flat vector shapes pick up a rounded, lit volume."""
    from PIL import ImageFilter

    a = Image.fromarray((np.clip(alpha[..., 0], 0, 1) * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(sigma))
    h = np.asarray(a, dtype=np.float32) / 255.0
    gy, gx = np.gradient(h)
    lit = (-gx * 0.62 - gy * 0.78) * sigma * strength * 2.2  # light from the upper left
    return rgb * (1.0 + np.clip(lit, -0.45, 0.45))[..., None]


def paint(im: Image.Image, seed=1, radius=2, grain=0.13, strokes=0.07, grade_kw=None, rim=0.0) -> Image.Image:
    """Take a (supersampled) RGBA render and give it a hand-painted surface.  Alpha-safe."""
    arr = to_array(im)
    a = arr[..., 3:4]
    pm = arr.copy()
    pm[..., :3] *= a  # premultiply so smoothing never bleeds colour out of the silhouette
    if radius:
        pm = kuwahara(pm, radius)
    alpha = pm[..., 3:4]
    rgb = np.where(alpha > 1e-4, pm[..., :3] / np.maximum(alpha, 1e-4), 0.0)
    h, w = rgb.shape[:2]
    if grain:
        fine = flat_noise(w, h, max(24, w // 18), 3, seed=seed)
        rgb = rgb * (1.0 + grain * np.clip(fine * 1.6, -1, 1))[..., None]
    if strokes:
        broad = flat_noise(w, h, 6, 3, seed=seed + 5)
        rgb = rgb * (1.0 + strokes * np.clip(broad * 1.8, -1, 1))[..., None]
    if rim:
        rgb = rim_light(rgb, alpha, strength=rim)
    if grade_kw is not None:
        rgb = grade(rgb, alpha, **grade_kw)
    out = np.dstack([np.clip(rgb, 0, 1), np.clip(alpha, 0, 1)])
    return to_image(out)


def render_sprite(svg_text: str, width: int, height: int, ss: int = 4, seed=1, **paint_kw) -> Image.Image:
    """SVG -> supersample -> painterly -> shrink to the final pixel grid."""
    big = rasterize(svg_text, width * ss, height * ss)
    paint_kw.setdefault('grade_kw', {})
    big = paint(big, seed=seed, **paint_kw)
    return downsample(big, width, height)
