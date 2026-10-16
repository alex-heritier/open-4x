from PIL import Image
from forge import scenes, emblems, chrome
from preview_chrome import tiled

if __name__ == "__main__":
    sc = scenes.render_all()
    paper = tiled(chrome.parchment(), 1400, 900)
    sheet = paper.copy()
    for k, (name, im) in enumerate(sc.items()):
        x, y = 20 + (k % 3) * 360, 20 + (k // 3) * 170
        sheet.alpha_composite(im, (x, y))
    ic = emblems.render_all()
    for k, (name, im) in enumerate(ic.items()):
        sheet.alpha_composite(im, (20 + k * 100, 720))
    sheet.save("preview/scenes.png")
