#!/usr/bin/env python3
"""Convert Civ3 GOG assets into runtime formats (RGBA PNG, OGG, WAV).

The game never reads PCX, FLC, or MP3. Run from civ3-clone/:
    python3 tools/prep_assets.py [stage ...]
Stages: terrain units cities cityscreen splash audio fonts features
improvements unitbuttons fog (default: all)

Engine color rules (verified by probe):
- magenta (255,0,255) and palette index 255: transparent
- pure red (255,0,0): unit shadow, rendered as translucent black
- unit FLC frames are direction-major: 8 directions x (total/8) frames
"""
import configparser
import glob
import json
import os
import shutil
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops

GOG = os.environ.get("CIV3_GOG", "../civ3-gog/app")
OUT = "assets/gen"

UNITS = ["Settler", "warrior", "Worker", "Scout"]
UNIT_SLOTS = ["DEFAULT", "RUN", "FORTIFY", "FIDGET", "BUILD", "ROAD",
              "MINE", "IRRIGATE", "FORTRESS", "JUNGLE", "FOREST", "PLANT"]
UI_SOUNDS = ["Select.wav", "Button OK.wav", "Button Cancel .wav", "Check.wav",
             "EnterTurn.wav", "Hut.wav", "WhatToBuild.wav", "PopupInfo.wav",
             "City View.wav", "Grid.wav", "PaperTurn.wav", "Barbarian Raid.wav"]
MUSIC = [("Diplomusic/DipASEarlyPeace.mp3", "as_early_peace"),
         ("Diplomusic/DipASLatePeace-2.mp3", "as_late_peace"),
         ("Menu/Menu1.mp3", "menu")]

_is255 = [0] * 255 + [255]
_is0 = [255] + [0] * 255
_ge201 = [0] * 201 + [255] * 55
_le99 = [255] * 100 + [0] * 156
_ge240 = [0] * 240 + [255] * 16
_le60 = [255] * 61 + [0] * 195


def to_rgba(im, unit=False, green_clear=False):
    """Apply Civ3 transparency rules, return RGBA image.

    Magenta is transparent everywhere. Shadows are exact red, plus vivid
    green for units (the scout's shadow palette is green, not red).
    Feature sheets (goody huts, terrain buildings) fill outside the tile
    diamond with green ((0,255,0) or (12,252,12)), cleared fuzzily when
    green_clear is set. Verified: no art pixel in those sheets matches.
    """
    rgb = im.convert("RGB")
    r, g, b = rgb.split()
    is_mag = ImageChops.multiply(r.point(_is255),
                                 ImageChops.multiply(g.point(_is0), b.point(_is255)))
    is_red = ImageChops.multiply(r.point(_is255),
                                 ImageChops.multiply(g.point(_is0), b.point(_is0)))
    if unit:
        is_green = ImageChops.multiply(g.point(_ge201), ImageChops.multiply(
            r.point(_le99), b.point(_le99)))
        is_red = ImageChops.add(is_red, is_green)
    black = Image.new("RGB", rgb.size, (0, 0, 0))
    out_rgb = Image.composite(black, rgb, is_red)
    alpha = Image.new("L", rgb.size, 255)
    alpha = Image.composite(Image.new("L", rgb.size, 0), alpha, is_mag)
    shadow_a = Image.new("L", rgb.size, 110)
    alpha = Image.composite(shadow_a, alpha, is_red)
    if green_clear:
        is_eg = ImageChops.multiply(r.point(_le60), ImageChops.multiply(
            g.point(_ge240), b.point(_le60)))
        alpha = Image.composite(Image.new("L", rgb.size, 0), alpha, is_eg)
    out = out_rgb.convert("RGBA")
    out.putalpha(alpha)
    return out


def convert_pcx(src, dst):
    im = Image.open(src)
    im.load()
    to_rgba(im).save(dst)
    return Image.open(src).size


def stage_terrain():
    d = os.path.join(OUT, "terrain", "sheets")
    os.makedirs(d, exist_ok=True)
    files = sorted(glob.glob(os.path.join(GOG, "Art", "Terrain", "*.pcx")) +
                   glob.glob(os.path.join(GOG, "Art", "Terrain", "*.PCX")))
    for f in files:
        stem = os.path.splitext(os.path.basename(f))[0]
        size = convert_pcx(f, os.path.join(d, stem + ".png"))
        print(f"  terrain/{stem}: {size}")
    print(f"terrain: {len(files)} sheets")
    crop_terrain()


def water_pixels(im):
    """Count teal water-like pixels inside the diamond of a 128x64 tile.

    Water aqua has blue+green far above red (e.g. 156,239,198);
    land, ice, and ice shadows do not.
    """
    rgb = im.convert("RGB")
    px = rgb.load()
    n = 0
    for y in range(64):
        for x in range(128):
            if abs(x - 64) / 64 + abs(y - 32) / 32 > 1:
                continue
            r, g, b = px[x, y]
            if b > 120 and (b + g - 2 * r) > 90:
                n += 1
    return n


def auto_picks(sheet, metric, cols=9, rows=9, count=3):
    """Pick the purest cells of a sheet by a pixel metric (lower wins)."""
    im = Image.open(os.path.join(OUT, "terrain", "sheets", sheet + ".png"))
    scored = []
    for row in range(rows):
        for col in range(cols):
            cell = im.crop((col * 128, row * 64, (col + 1) * 128,
                            (row + 1) * 64))
            scored.append((metric(cell), col, row))
    scored.sort()
    # spread picks across the sheet: greedily take best cells far apart
    chosen = []
    for score, col, row in scored:
        if all(abs(col - c) + abs(row - r) >= 2 for _, c, r in chosen):
            chosen.append((score, col, row))
        if len(chosen) == count:
            break
    print(f"  auto {sheet}: {[(c, r, s) for s, c, r in chosen]}")
    return [(col, row) for _, col, row in chosen]


def diamond_mask(im):
    """Zero alpha outside the bottom-anchored 128x64 tile diamond."""
    w, h = im.size
    cy = h - 32
    a = im.split()[3]
    px = a.load()
    # only the diamond rows; art above (peaks) keeps source alpha
    for y in range(max(0, h - 64), h):
        for x in range(w):
            if abs(x - w / 2) / 64 + abs(y - cy) / 32 > 1:
                px[x, y] = 0
    out = im.copy()
    out.putalpha(a)
    # bleed opaque edge pixels 1px outward so tile seams never sample
    # transparent texels (the GPU rounds UVs at diamond boundaries)
    rgb = out.convert("RGB").load()
    alpha = out.split()[3].load()
    bleed = []
    for y in range(h):
        for x in range(w):
            if alpha[x, y] != 0:
                continue
            for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                if 0 <= nx < w and 0 <= ny < h and alpha[nx, ny] == 255:
                    bleed.append((x, y, rgb[nx, ny]))
                    break
    opx = out.load()
    for x, y, c in bleed:
        opx[x, y] = c + (255,)
    return out


# (output name, sheet, col, row, cell_w, cell_h). Water cells verified pure
# by eye; land and ice cells are auto-picked for purity at crop time.
AUTO_SPECS = [
    ("grassland", "xggc", water_pixels, 9, 9),
    ("desert", "xdpc", water_pixels, 9, 9),
    ("plains", "xpgc", water_pixels, 9, 9),
    ("tundra", "xtgc", water_pixels, 9, 9),
    ("ice", "polarICEcaps-final", water_pixels, 8, 4),
]

TERRAIN_PICKS = [
    ("ocean_0", "wOOO", 2, 2, 128, 64),
    ("ocean_1", "wOOO", 5, 5, 128, 64),
    ("ocean_2", "wOOO", 7, 1, 128, 64),
    ("sea_0", "wSSS", 2, 2, 128, 64),
    ("sea_1", "wSSS", 5, 5, 128, 64),
    ("sea_2", "wSSS", 7, 1, 128, 64),
    ("coast_0", "wCSO", 2, 0, 128, 64),
    ("coast_1", "wCSO", 5, 0, 128, 64),
    ("coast_2", "wCSO", 3, 1, 128, 64),
    ("hill_0", "xhills", 0, 0, 128, 72),
    ("hill_1", "xhills", 1, 0, 128, 72),
    ("hill_2", "xhills", 2, 0, 128, 72),
    ("mtn_0", "Mountains", 0, 0, 128, 88),
    ("mtn_1", "Mountains", 1, 0, 128, 88),
    ("mtnsnow_0", "Mountains-snow", 0, 0, 128, 88),
    ("mtnsnow_1", "Mountains-snow", 1, 0, 128, 88),
]

# Canopy sprites: connected opaque components inside a y band of a sheet.
CANOPY_PICKS = [
    ("forest_0", "grassland forests", (395, 605), 0),
    ("forest_1", "grassland forests", (395, 605), 1),
    ("jungle_0", "grassland forests", (0, 205), 0),
    ("jungle_1", "grassland forests", (0, 205), 1),
    ("pine_0", "tundra forests", (745, 884), 0),
    ("pine_1", "tundra forests", (745, 884), 1),
]


def split_wide_box(fg, w, box, band):
    """Split boxes wider than one sprite at fully empty columns."""
    x0, y0, x1, y1 = box
    if x1 - x0 <= 220:
        return [box]
    y_lo, y_hi = max(y0, band[0]), min(y1, band[1])
    gaps = [x for x in range(x0, x1)
            if all(fg[y * w + x] == 0 for y in range(y_lo, y_hi))]
    cuts, run = [], []
    for x in gaps:
        if run and x == run[-1] + 1:
            run.append(x)
        else:
            if len(run) >= 3:
                cuts.append((run[0] + run[-1]) // 2)
            run = [x]
    if len(run) >= 3:
        cuts.append((run[0] + run[-1]) // 2)
    edges, prev = [], x0
    for c in cuts:
        edges.append((prev, c))
        prev = c + 1
    edges.append((prev, x1))
    return [(a0, y0, a1, y1) for a0, a1 in edges if a1 - a0 > 20]


def canopy_fg(im):
    """Foreground mask for canopy sheets: opaque and not green outline."""
    rgb = im.convert("RGB").load()
    a = im.split()[3].load()
    w, h = im.size
    fg = bytearray(w * h)
    for y in range(h):
        for x in range(w):
            if a[x, y] > 0 and rgb[x, y] != (0, 255, 0):
                fg[y * w + x] = 1
    return fg


def opaque_components(im, band, xmax=None):
    w, h = im.size
    xmax = xmax or w
    fg = canopy_fg(im)
    a = im.split()[3].load()
    seen = bytearray(w * h)
    boxes = []
    for y in range(band[0], min(band[1], h)):
        for x in range(min(w, xmax)):
            if not fg[y * w + x] or seen[y * w + x]:
                continue
            stack = [(x, y)]
            seen[y * w + x] = 1
            x0 = x1 = x
            y0 = y1 = y
            while stack:
                cx, cy = stack.pop()
                if cx < x0:
                    x0 = cx
                elif cx > x1:
                    x1 = cx
                if cy < y0:
                    y0 = cy
                elif cy > y1:
                    y1 = cy
                for nx, ny in ((cx + 1, cy), (cx - 1, cy),
                               (cx, cy + 1), (cx, cy - 1)):
                    if 0 <= nx < xmax and band[0] <= ny < min(band[1], h):
                        if fg[ny * w + nx] and not seen[ny * w + nx]:
                            seen[ny * w + nx] = 1
                            stack.append((nx, ny))
            if x1 - x0 > 20 and y1 - y0 > 20:
                boxes.append((x0, y0, x1 + 1, y1 + 1))
    split = []
    for box in boxes:
        split.extend(split_wide_box(fg, w, box, band))
    split.sort(key=lambda b: (b[2] - b[0]) * (b[3] - b[1]), reverse=True)
    return split


def crop_terrain():
    sheets = os.path.join(OUT, "terrain", "sheets")
    outdir = os.path.join(OUT, "terrain")
    manifest = {}
    picks = list(TERRAIN_PICKS)
    for name, sheet, metric, cols, rows in AUTO_SPECS:
        for i, (col, row) in enumerate(auto_picks(sheet, metric, cols, rows)):
            picks.append((f"{name}_{i}", sheet, col, row, 128, 64))
    for name, sheet, col, row, cw, ch in picks:
        im = Image.open(os.path.join(sheets, sheet + ".png"))
        tile = im.crop((col * cw, row * ch, (col + 1) * cw, (row + 1) * ch))
        diamond_mask(tile).save(os.path.join(outdir, name + ".png"))
        # anchor: pixel in the tile that sits on the tile diamond center
        manifest[name] = {"file": name + ".png", "size": [cw, ch],
                          "anchor": [cw // 2, ch - 32]}
    for name, sheet, band, which in CANOPY_PICKS:
        im = Image.open(os.path.join(sheets, sheet + ".png"))
        boxes = opaque_components(im, band, xmax=850)
        print(f"  canopy {name}: {len(boxes)} pieces, take #{which} {boxes[which] if len(boxes) > which else None}")
        assert len(boxes) > which, f"{name}: only {len(boxes)} components"
        x0, y0, x1, y1 = boxes[which]
        pad = 2
        crop = im.crop((max(0, x0 - pad), max(0, y0 - pad),
                        x1 + pad, y1 + pad))
        # drop green outline fringe caught by the crop
        px = crop.load()
        for yy in range(crop.height):
            for xx in range(crop.width):
                r, g, b, _ = px[xx, yy]
                if (r, g, b) == (0, 255, 0):
                    px[xx, yy] = (0, 0, 0, 0)
        crop.save(os.path.join(outdir, name + ".png"))
        w, h = crop.size
        manifest[name] = {"file": name + ".png", "size": [w, h],
                          "anchor": [w // 2, h - 16]}
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"cropped {len(picks)} tiles + {len(CANOPY_PICKS)} canopies")


def ffmpeg_frames(src, pattern):
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src, pattern],
                   check=True)


def stage_units():
    art = os.path.join(GOG, "Art", "Units")
    for unit in UNITS:
        udir = os.path.join(art, unit)
        ini = next(glob.iglob(os.path.join(udir, "*.ini")), None)
        if ini is None:
            ini = next(glob.iglob(os.path.join(udir, "*.INI")), None)
        if ini is None:
            print(f"  {unit}: no INI, skipped")
            continue
        cfg = configparser.ConfigParser()
        cfg.optionxform = str
        cfg.read(ini, encoding="latin-1")
        anims = dict(cfg["Animations"]) if "Animations" in cfg else {}
        outdir = os.path.join(OUT, "units", unit)
        os.makedirs(outdir, exist_ok=True)
        manifest = {}
        with tempfile.TemporaryDirectory() as tmp:
            for slot in UNIT_SLOTS:
                flc = anims.get(slot, "").strip()
                if not flc:
                    continue
                pat = os.path.join(tmp, "f_%04d.png")
                for old in glob.glob(os.path.join(tmp, "f_*.png")):
                    os.remove(old)
                ffmpeg_frames(os.path.join(udir, flc), pat)
                frames = sorted(glob.glob(os.path.join(tmp, "f_*.png")))
                n = len(frames)
                assert n % 8 == 0, f"{unit}/{slot}: {n} frames not divisible by 8"
                fpd = n // 8
                w, h = Image.open(frames[0]).size
                for direction in range(8):
                    strip = Image.new("RGBA", (w * fpd, h), (0, 0, 0, 0))
                    for i in range(fpd):
                        fim = Image.open(frames[direction * fpd + i])
                        fim.load()
                        strip.paste(to_rgba(fim, unit=True), (i * w, 0))
                    strip.save(os.path.join(outdir, f"{slot}_d{direction}.png"))
                manifest[slot] = {"frame": [w, h], "frames": fpd, "dirs": 8}
                print(f"  {unit}/{slot}: {w}x{h} x{fpd}f x8d")
        with open(os.path.join(outdir, "manifest.json"), "w") as f:
            json.dump(manifest, f, indent=1)
        adir = os.path.join(OUT, "audio", "units", unit)
        os.makedirs(adir, exist_ok=True)
        for wav in glob.glob(os.path.join(udir, "*.wav")):
            shutil.copy(wav, adir)
    print(f"units: {len(UNITS)} converted")


def stage_cities():
    d = os.path.join(OUT, "cities", "sheets")
    os.makedirs(d, exist_ok=True)
    for name in ["rASIAN", "ASIANWALL", "city icons"]:
        src = os.path.join(GOG, "Art", "Cities", name + ".PCX")
        if not os.path.exists(src):
            src = os.path.join(GOG, "Art", "Cities", name + ".pcx")
        size = convert_pcx(src, os.path.join(d, name + ".png"))
        print(f"  cities/{name}: {size}")
    crop_cities()


def crop_cities():
    outdir = os.path.join(OUT, "cities")
    sheets = os.path.join(OUT, "cities", "sheets")
    # ancient Asian column of rASIAN: rows 1-3 are town/city/metro
    manifest = {}
    sheet = Image.open(os.path.join(sheets, "rASIAN.png"))
    for name, row in [("town", 1), ("city", 2), ("metro", 3)]:
        crop = sheet.crop((0, row * 95, 167, (row + 1) * 95))
        crop.save(os.path.join(outdir, name + ".png"))
        # The town's footprint is centered in the 167x95 cell, so the cell
        # center sits on the tile center (a bottom anchor drew cities one
        # tile north of their square).
        manifest[name] = {"file": name + ".png", "size": [167, 95],
                          "anchor": [83, 47]}
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    # X close buttons: three states in the top-left of XandView
    xv = Image.open(os.path.join(OUT, "cityscreen", "XandView.png"))
    boxes = [b for b in opaque_components(xv, (52, 100), xmax=140)
             if b[2] - b[0] < 60]
    boxes.sort()
    assert len(boxes) >= 3, f"X buttons: {len(boxes)}"
    uid = os.path.join(OUT, "ui")
    for i, (x0, y0, x1, y1) in enumerate(boxes[:3]):
        xv.crop((x0, y0, x1, y1)).save(os.path.join(uid, f"x_{i}.png"))
    # production button states: thirds of ProdButton
    pb = Image.open(os.path.join(OUT, "cityscreen", "ProdButton.png"))
    w = pb.width // 3
    for i in range(3):
        pb.crop((i * w, 0, (i + 1) * w, pb.height)).save(
            os.path.join(uid, f"prod_{i}.png"))
    # unit icons for the production list
    udir = os.path.join(GOG, "Art", "civilopedia", "icons", "units")
    for src, name in [("00Settlersmall", "settler"), ("01Workersmall", "worker"),
                      ("02scoutsmall", "scout"), ("06warriorsmall", "warrior")]:
        convert_pcx(os.path.join(udir, src + ".pcx"),
                    os.path.join(uid, f"uniticon_{name}.png"))
    # content Asian citizen head (row 0, col 4 of popHeads)
    heads = Image.open(os.path.join(GOG, "Art", "SmallHeads", "popHeads.pcx"))
    heads.load()
    to_rgba(heads.crop((200, 0, 250, 50))).save(
        os.path.join(uid, "citizen.png"))
    # entertainer (jester, row 16 col 1): idle citizens on the city screen
    to_rgba(heads.crop((50, 800, 100, 850))).save(
        os.path.join(uid, "entertainer.png"))
    print("cities cropped: sprites + x/prod buttons + icons + citizen + entertainer")


def stage_cityscreen():
    d = os.path.join(OUT, "cityscreen")
    os.makedirs(d, exist_ok=True)
    files = sorted(glob.glob(os.path.join(GOG, "Art", "city screen", "*.pcx")))
    for f in files:
        stem = os.path.splitext(os.path.basename(f))[0]
        convert_pcx(f, os.path.join(d, stem + ".png"))
    print(f"cityscreen: {len(files)} files")


def stage_splash():
    d = os.path.join(OUT, "splash")
    os.makedirs(d, exist_ok=True)
    size = convert_pcx(os.path.join(GOG, "Art", "leaderheads", "TO.pcx"),
                       os.path.join(d, "tokugawa.png"))
    print(f"splash/tokugawa: {size}")
    convert_pcx(os.path.join(GOG, "Art", "Units", "units_32.pcx"),
                os.path.join(OUT, "ui", "units_32.png"))
    print("ui/units_32 converted")


def stage_audio():
    os.makedirs(os.path.join(OUT, "audio", "ui"), exist_ok=True)
    for wav in UI_SOUNDS:
        src = os.path.join(GOG, "Sounds", wav)
        if os.path.exists(src):
            shutil.copy(src, os.path.join(OUT, "audio", "ui"))
    md = os.path.join(OUT, "audio", "music")
    os.makedirs(md, exist_ok=True)
    manifest = {}
    for src_rel, stem in MUSIC:
        src = os.path.join(GOG, "Sounds", src_rel)
        dst = os.path.join(md, stem + ".ogg")
        r = subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src,
                            "-c:a", "vorbis", "-strict", "experimental",
                            "-q:a", "4", dst])
        if r.returncode != 0 or os.path.getsize(dst) == 0:
            if os.path.exists(dst):
                os.remove(dst)
            dst = os.path.join(md, stem + ".wav")
            subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src, dst],
                           check=True)
        manifest[stem] = os.path.basename(dst)
        print(f"  music/{stem}: {os.path.basename(dst)}")
    with open(os.path.join(md, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)


def stage_fonts():
    d = os.path.join(OUT, "fonts")
    os.makedirs(d, exist_ok=True)
    shutil.copy(os.path.join(GOG, "LSANS.TTF"), os.path.join(d, "lsans.ttf"))
    print("fonts/lsans.ttf")


# Row-major order of Art/resources.pcx, verified against the named
# civilopedia icons. Used as canonical GOOD ids until BIQ framing lands.
RESOURCES = ["horse", "diamonds", "saltpetre", "coal", "oil", "iron",
             "aluminum", "uranium", "wine", "furs", "dye", "incense",
             "spice", "ivory", "silk", "rubber", "whales", "game",
             "fish", "cattle", "wheat", "gold"]


BTN_SHEETS = [("NormButtons.PCX", "unitbtns_norm.png"),
              ("rolloverbuttons.PCX", "unitbtns_over.png"),
              ("highlightedbuttons.PCX", "unitbtns_down.png")]


def stage_unitbuttons():
    """Unit action buttons: gold discs, 32-px cells in an 8x10 grid.

    The Conquests sheets override the base game's, and their cell order is the
    `#UNIT_ACTIONS` order of `Conquests/Text/labels.txt` row-major (checked
    against in-game screenshots: a Warrior's seven buttons are cells 0-6).
    Magenta is transparent, the black surround is opaque: the disc shape comes
    from ButtonAlpha.pcx, so alpha = sheet alpha * ButtonAlpha.
    """
    d = os.path.join(OUT, "ui")
    os.makedirs(d, exist_ok=True)
    alpha = Image.open(os.path.join(
        GOG, "Conquests", "Art", "interface", "ButtonAlpha.pcx")).convert("L")
    # Hard alpha: the sheet's own edge pixels are anti-aliased against its
    # black surround, which glows against the bar's parchment background.
    alpha = alpha.point(lambda v: 255 if v > 160 else 0)
    for src, dst in BTN_SHEETS:
        im = Image.open(os.path.join(
            GOG, "Conquests", "Art", "interface", src))
        im.load()
        im = to_rgba(im)
        im.putalpha(ImageChops.multiply(im.getchannel("A"), alpha))
        im.save(os.path.join(d, dst))
        print(f"  ui/{dst}: {im.size}")
    print(f"unitbuttons: {len(BTN_SHEETS)} sheets")


def stage_hud():
    """Civ3's map HUD: the bottom-right unit box and the next-turn disc.

    `box right color.pcx` (294x137) is the unit/status panel; its alpha twin
    holds the shape. `nextturn states color.pcx` is three 47x28 discs (gold
    idle, orange rollover, blue highlighted) that sit on the box's top-left
    knob; Civ3 blinks it when no unit needs orders.
    """
    d = os.path.join(OUT, "ui")
    os.makedirs(d, exist_ok=True)
    iface = os.path.join(GOG, "Art", "interface")

    def with_alpha(stem):
        color = Image.open(os.path.join(iface, stem + " color.pcx")).convert("RGBA")
        alpha = Image.open(os.path.join(iface, stem + " alpha.pcx")).convert("L")
        color.putalpha(alpha)
        return color

    with_alpha("box right").save(os.path.join(d, "box_right.png"))
    turn = with_alpha("nextturn states")
    w = turn.width // 3
    for i in range(3):
        turn.crop((i * w, 0, (i + 1) * w, turn.height)).save(
            os.path.join(d, f"nextturn_{i}.png"))
    print("hud: box_right + 3 nextturn states")


def stage_features():
    outdir = os.path.join(OUT, "features")
    os.makedirs(outdir, exist_ok=True)
    manifest = {}
    # goody huts: 3x3 grid of 128x64 cells, last cell empty
    huts = Image.open(os.path.join(GOG, "Art", "Terrain", "goodyhuts.pcx"))
    huts.load()
    huts = to_rgba(huts, green_clear=True)
    n = 0
    for row in range(3):
        for col in range(3):
            if row == 2 and col == 2:
                continue
            cell = huts.crop((col * 128, row * 64, (col + 1) * 128,
                              (row + 1) * 64))
            cell.save(os.path.join(outdir, f"hut_{n}.png"))
            manifest[f"hut_{n}"] = {"file": f"hut_{n}.png",
                                    "size": [128, 64], "anchor": [64, 32]}
            n += 1
    # barbarian camp: palisade hut at col 2, row 0 of TerrainBuildings
    tb = Image.open(os.path.join(GOG, "Art", "Terrain", "TerrainBuildings.PCX"))
    tb.load()
    camp = to_rgba(tb, green_clear=True).crop((256, 0, 384, 64))
    camp.save(os.path.join(outdir, "camp.png"))
    manifest["camp"] = {"file": "camp.png", "size": [128, 64],
                        "anchor": [64, 32]}
    # resources: 50px grid with 1px dark-magenta separators; crop inset.
    # Shadows (resources_shadows.pcx) skipped: opaque blobs, needs softening.
    res = Image.open(os.path.join(GOG, "Art", "resources.pcx"))
    res.load()
    res = to_rgba(res)
    for i, name in enumerate(RESOURCES):
        c, r = i % 6, i // 6
        cell = res.crop((c * 50 + 1, r * 50 + 1,
                         (c + 1) * 50 - 1, (r + 1) * 50 - 1))
        cell.save(os.path.join(outdir, f"res_{name}.png"))
        manifest[f"res_{name}"] = {"file": f"res_{name}.png",
                                   "size": [48, 48], "anchor": [24, 24]}
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"features: {n} huts + camp + {len(RESOURCES)} resources")


# Irrigation sheet per base terrain (verified by average hue).
IRRIGATION_SHEETS = [("grass", "irrigation"), ("plains", "irrigation PLAINS"),
                     ("desert", "irrigation DESETT"),
                     ("tundra", "irrigation TUNDRA")]


def stage_improvements():
    outdir = os.path.join(OUT, "improvements")
    os.makedirs(outdir, exist_ok=True)
    manifest = {}
    # roads: 16x16 neighbor-mask table, row-major, 128x64 cells. Bit order
    # (verified by stub tiles): screen-clockwise from map-E.
    roads = Image.open(os.path.join(GOG, "Art", "Terrain", "roads.pcx"))
    roads.load()
    roads = to_rgba(roads, green_clear=True)
    for mask in range(256):
        c, r = mask % 16, mask // 16
        cell = roads.crop((c * 128, r * 64, (c + 1) * 128, (r + 1) * 64))
        cell.save(os.path.join(outdir, f"road_{mask}.png"))
        manifest[f"road_{mask}"] = {"file": f"road_{mask}.png",
                                    "size": [128, 64], "anchor": [64, 32]}
    # irrigation: 4x4 edge-mask table per base terrain. Bit order
    # (verified by edge-contact + 2x2 continuity): 0=W 1=E 2=S 3=N.
    for base, sheet in IRRIGATION_SHEETS:
        im = Image.open(os.path.join(GOG, "Art", "Terrain", sheet + ".pcx"))
        im.load()
        im = to_rgba(im, green_clear=True)
        for mask in range(16):
            c, r = mask % 4, mask // 4
            cell = im.crop((c * 128, r * 64, (c + 1) * 128, (r + 1) * 64))
            cell.save(os.path.join(outdir, f"irr_{base}_{mask}.png"))
            manifest[f"irr_{base}_{mask}"] = {
                "file": f"irr_{base}_{mask}.png",
                "size": [128, 64], "anchor": [64, 32]}
    # mine: shaft mound at col 2, row 1 of TerrainBuildings (128x64 cells;
    # the barbarian camp in stage_features is col 2, row 0 of the same sheet)
    tb = Image.open(os.path.join(GOG, "Art", "Terrain", "TerrainBuildings.PCX"))
    tb.load()
    mine = to_rgba(tb, green_clear=True).crop((256, 64, 384, 128))
    mine.save(os.path.join(outdir, "mine.png"))
    manifest["mine"] = {"file": "mine.png", "size": [128, 64], "anchor": [64, 32]}
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"improvements: 256 roads + {16 * len(IRRIGATION_SHEETS)} irrigation + mine")


def stage_fog():
    """Civ3's fog of war: the 9x9 per-vertex transition sheet over black.

    `Art/Terrain/FogOfWar.pcx` is a grayscale mask whose value is how much
    of the terrain stays visible at that pixel (255 clear, 0 black,
    magenta no fog). The engine picked a cell from the four vertex fog
    states, the same addressing the terrain blend sheets use and that
    `render::fog_cell` repeats. Its corners are 0 where a vertex is
    never-seen, 153 where it is remembered, and clear where it is lit.

    Civ3 blended the mask in display space, where a remembered tile keeps
    60% of its brightness; the renderer blends in linear space, so bake
    the alpha that displays the same, alpha = 1 - (v/255)**2.2.
    """
    src = Image.open(os.path.join(GOG, "Art", "Terrain", "FogOfWar.pcx"))
    im = to_rgba(src)
    alpha = im.getchannel("R").point(
        [round(255 * (1 - (v / 255) ** 2.2)) for v in range(256)])
    out = Image.new("RGBA", im.size, (0, 0, 0, 255))
    out.putalpha(ImageChops.multiply(alpha, im.getchannel("A")))
    out.save(os.path.join(OUT, "terrain", "fog.png"))
    print(f"  terrain/fog: {out.size}")
    print("fog: 9x9 cells from FogOfWar.pcx")


STAGES = {"terrain": stage_terrain, "units": stage_units,
          "cities": stage_cities, "cityscreen": stage_cityscreen,
          "splash": stage_splash, "audio": stage_audio, "fonts": stage_fonts,
          "features": stage_features, "improvements": stage_improvements,
          "unitbuttons": stage_unitbuttons, "hud": stage_hud,
          "fog": stage_fog}


def main():
    os.makedirs(os.path.join(OUT, "ui"), exist_ok=True)
    want = sys.argv[1:] or ["all"]
    if want == ["all"]:
        want = list(STAGES)
    for s in want:
        STAGES[s]()
    print("done ->", OUT)


if __name__ == "__main__":
    main()
