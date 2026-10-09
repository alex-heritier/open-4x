"""Scratch: contact sheet of one unit's clip, rows = facings (SW, S, SE, E, NE, N, NW, W), columns = frames.

    python preview_anim.py infantry idle [out.png] [--paint]
"""
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from PIL import Image

from forge import fleet, troops
from forge.paint import render_sprite
from forge.util import rasterize

BUILDERS = {**troops.BUILDERS, **fleet.BUILDERS}


def frame(args):
    name, clip, k, i, n, w, h, painted = args
    clips = fleet.CLIPS if name in fleet.BUILDERS else troops.CLIPS
    t = i / n if clips[clip][2] else i / max(1, n - 1)
    svg = BUILDERS[name](k, clip, t).render()
    if painted:
        return render_sprite(svg, w, h, ss=2, seed=2, radius=1, rim=0.6)
    return rasterize(svg, w, h)


def main():
    name, clip = sys.argv[1], sys.argv[2]
    out = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].startswith("--") else f"preview/{name}_{clip}.png"
    painted = "--paint" in sys.argv
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    w, h = (256, 192) if name in fleet.BUILDERS else (160, 160)
    n = (fleet.CLIPS if name in fleet.BUILDERS else troops.CLIPS)[clip][0]
    jobs = [(name, clip, k, i, n, w, h, painted) for k in range(8) for i in range(n)]
    with ProcessPoolExecutor(4) as ex:
        ims = list(ex.map(frame, jobs))
    sheet = Image.new("RGBA", (w * n, h * 8), (120, 140, 90, 255))
    for (_, _, k, i, *_), im in zip(jobs, ims):
        sheet.alpha_composite(im, (i * w, k * h))
    sheet.save(out)
    print(out)


if __name__ == "__main__":
    main()
