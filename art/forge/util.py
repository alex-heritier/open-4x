"""Paths, SVG rasterising (rsvg-convert) and image helpers."""

import io
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image

ART = Path(__file__).resolve().parents[1]
PACK = ART.parent / "assets" / "packs" / "base"
SVG_DIR = ART / "svg"


def write_svg(name: str, svg: str) -> Path:
    SVG_DIR.mkdir(parents=True, exist_ok=True)
    path = SVG_DIR / f"{name}.svg"
    path.write_text(svg)
    return path


def rasterize(svg: str, width: int, height: int) -> Image.Image:
    """Render SVG text to an RGBA image of exactly width x height pixels."""
    out = subprocess.run(
        ["rsvg-convert", "-w", str(width), "-h", str(height), "-f", "png"],
        input=svg.encode(),
        capture_output=True,
        check=True,
    ).stdout
    return Image.open(io.BytesIO(out)).convert("RGBA")


def to_array(im: Image.Image) -> np.ndarray:
    return np.asarray(im, dtype=np.float32) / 255.0


def to_image(arr: np.ndarray) -> Image.Image:
    return Image.fromarray((np.clip(arr, 0, 1) * 255.0 + 0.5).astype(np.uint8), "RGBA")


def downsample(im: Image.Image, width: int, height: int) -> Image.Image:
    """Alpha-correct high quality shrink (premultiplied so edges never fringe dark/light)."""
    return im.convert("RGBa").resize((width, height), Image.Resampling.LANCZOS).convert("RGBA")


def save_png(im: Image.Image, rel: str) -> Path:
    path = PACK / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, optimize=True)
    return path


def indexed(im: Image.Image, colors: int = 256) -> Image.Image:
    """Quantize to a palette without dithering. The same pixel always maps to the same entry, so
    cells that share an edge still agree on it; the big terrain sheets shrink about eightfold."""
    return im.quantize(colors=colors, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)
