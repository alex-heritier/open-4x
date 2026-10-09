"""Scratch: render the heightfield mountain on a grass tile."""
import time
from PIL import Image
from forge import relief
from forge.paint import paint
from forge.util import downsample

t = time.time()
layer = relief.build_mountain_layer()
print("rendered", f"{time.time()-t:.1f}s", layer.size)
layer.save("preview/mountain_ss.png")
im = downsample(paint(layer, radius=1, grain=0.10, strokes=0.05, grade_kw={"sat": 1.1, "contrast": 1.08, "key": 0.10}), 256, 224)
im.save("preview/mountain.png")
grass = Image.open("preview/atlas.png").crop((0, 0, 256, 128))
tile = Image.new("RGBA", (256, 224), (0, 0, 0, 0))
tile.alpha_composite(grass, (0, 96))
tile.alpha_composite(im)
bg = Image.new("RGBA", (512, 448), (24, 60, 74, 255))
bg.alpha_composite(tile.resize((512, 448), Image.Resampling.LANCZOS))
bg.save("preview/mountain_2x.png")
