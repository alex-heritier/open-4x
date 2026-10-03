#!/usr/bin/env python3
"""Convert Civ3 GOG assets into runtime formats (RGBA PNG, OGG, WAV).

The game never reads PCX, FLC, or MP3. Run from the repo root:
    python3 tools/prep_assets.py [stage ...]
Stages: terrain units cities cityscreen splash audio fonts features
improvements unitbuttons fog borders cursor leaders diplomacy wonders
(default: all)

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
import struct
import re
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops

GOG = os.environ.get("CIV3_GOG", "civ3/civ3-gog/app")
OUT = "assets/gen"

def roster_units():
    """`Art/Units` folders of the units the game plays with, read off the
    generated roster (`src/rules_data.rs`, from `biq/examples/gen_game_rules.rs`),
    so a unit is converted as soon as the roster gives it art."""
    with open(os.path.join(os.path.dirname(__file__), "..", "src", "rules_data.rs")) as f:
        text = f.read()
    seen = []
    for m in re.finditer(r'UnitRow \{ name: "[^"]*", art: "([^"]+)"', text):
        if m.group(1) not in seen:
            seen.append(m.group(1))
    return seen


UNITS = roster_units()
UNIT_SLOTS = ["DEFAULT", "RUN", "FORTIFY", "FIDGET", "BUILD", "ROAD",
              "MINE", "IRRIGATE", "FORTRESS", "JUNGLE", "FOREST", "PLANT",
              "ATTACK1", "ATTACK2", "ATTACK3", "DEFEND", "DEATH", "VICTORY",
              "CAPTURE"]
# Slots whose sounds the combat sequencer plays (the rest keep their
# hard-coded effects in audio.rs).
SOUND_SLOTS = {"ATTACK1", "ATTACK2", "ATTACK3", "DEFEND", "DEATH", "VICTORY",
               "CAPTURE"}
UI_SOUNDS = ["Select.wav", "Button OK.wav", "Button Cancel .wav", "Check.wav",
             "EnterTurn.wav", "Hut.wav", "WhatToBuild.wav", "PopupInfo.wav",
             "City View.wav", "Grid.wav", "PaperTurn.wav", "Barbarian Raid.wav",
             "Wonder.wav"]
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


def flc_ms(path):
    """Milliseconds per frame from the FLC header (`speed`, dword at 16)."""
    with open(path, "rb") as f:
        head = f.read(20)
    magic, speed = struct.unpack_from("<H", head, 4)[0], struct.unpack_from("<I", head, 16)[0]
    assert magic == 0xAF12, f"{path}: not an FLC (magic {magic:#x})"
    return speed


def _cstr(data, at):
    end = data.index(b"\0", at)
    return data[at:end].decode("latin-1"), end + 1


def _vlq(data, p):
    v = 0
    while True:
        b = data[p]
        p += 1
        v = (v << 7) | (b & 0x7F)
        if not b & 0x80:
            return v, p


def parse_amb(path):
    """Sound schedule of a Civ3 `.amb`: a list of `(seconds, wav name)`.

    An `.amb` is a sampler bank (`prgm` programs, `kmap` key maps pointing at
    `.wav` files, one `glbl`) followed by a raw Standard MIDI File. Each
    MIDI track fires one program's note; its first note-on tick, converted
    with the file's tempo and division, is when that sample starts relative
    to the animation (`WarriorAttackA.amb`: Grunt at 0 s, Slash at 0.44 s
    of a 1.245 s clip). Verified against the on-disk bytes; the engine's
    own reader is not disassembled.
    """
    data = open(path, "rb").read()
    programs = {}   # program id -> sample name
    kmaps = {}      # sample name -> wav
    p = 0
    while data[p:p + 4] in (b"prgm", b"kmap", b"glbl"):
        tag = data[p:p + 4]
        n = struct.unpack_from("<I", data, p + 4)[0]
        body = data[p + 8:p + 8 + n]
        if tag == b"prgm":
            pid = struct.unpack_from("<i", body, 0)[0]
            programs[pid] = _cstr(body, 28)[0]
        elif tag == b"kmap":
            name, q = _cstr(body, 12)
            kmaps[name] = _cstr(body, q + 20)[0]
        p += 8 + n
    assert data[p:p + 4] == b"MThd", f"{path}: no MIDI after the sampler bank"
    division = struct.unpack_from(">H", data, p + 12)[0]
    p += 8 + struct.unpack_from(">I", data, p + 4)[0]
    tempo = 500000
    out = []
    while data[p:p + 4] == b"MTrk":
        end = p + 8 + struct.unpack_from(">I", data, p + 4)[0]
        q, tick, status, program, first = p + 8, 0, 0, None, None
        while q < end:
            delta, q = _vlq(data, q)
            tick += delta
            if data[q] >= 0x80:
                status = data[q]
                q += 1
            if status == 0xFF:
                kind = data[q]
                ln, q = _vlq(data, q + 1)
                if kind == 0x51:
                    tempo = int.from_bytes(data[q:q + 3], "big")
                q += ln
            elif status in (0xF0, 0xF7):
                ln, q = _vlq(data, q)
                q += ln
            elif status & 0xF0 in (0xC0, 0xD0):
                if status & 0xF0 == 0xC0:
                    program = data[q]
                q += 1
            else:
                if status & 0xF0 == 0x90 and data[q + 1] > 0 and first is None:
                    first = tick
                q += 2
        if first is not None and program in programs:
            wav = kmaps.get(programs[program])
            if wav:
                out.append((round(first / division * tempo / 1e6, 3), wav))
        p = end
    return sorted(out)


def slot_sounds(cfg, locate, slot):
    """`[Sound Effects] <slot>` as a list of `(seconds, wav)`; `.amb` files
    expand to their schedule, a bare `.wav` starts with the clip."""
    if "Sound Effects" not in cfg:
        return []
    name = cfg["Sound Effects"].get(slot, "").strip()
    if not name:
        return []
    if name.lower().endswith(".amb"):
        return parse_amb(locate(name))
    return [(0.0, name)]


def unit_dirs(unit):
    """The unit's art folders, Conquests first: it often holds only the INI
    and sounds of a unit whose animations live in the base game's folder."""
    return [d for d in (os.path.join(GOG, "Conquests", "Art", "Units", unit),
                        os.path.join(GOG, "Art", "Units", unit))
            if os.path.isdir(d)]


def stage_units():
    for unit in UNITS:
        dirs = unit_dirs(unit)

        def locate(name, dirs=dirs):
            for d in dirs:
                p = os.path.join(d, name)
                if os.path.exists(p):
                    return p
            return os.path.join(dirs[0], name)

        ini = next((p for d in dirs for p in glob.glob(os.path.join(d, "*.[iI][nN][iI]"))), None)
        if ini is None:
            print(f"  {unit}: no INI, skipped")
            continue
        cfg = configparser.ConfigParser(strict=False)
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
                ffmpeg_frames(locate(flc), pat)
                frames = sorted(glob.glob(os.path.join(tmp, "f_*.png")))
                n = len(frames)
                if n == 0 or n % 8 != 0:
                    print(f"  {unit}/{slot}: {n} frames not divisible by 8, skipped")
                    continue
                fpd = n // 8
                w, h = Image.open(frames[0]).size
                feet = []
                for direction in range(8):
                    strip = Image.new("RGBA", (w * fpd, h), (0, 0, 0, 0))
                    for i in range(fpd):
                        fim = Image.open(frames[direction * fpd + i])
                        fim.load()
                        rgba = to_rgba(fim, unit=True)
                        strip.paste(rgba, (i * w, 0))
                        if i == 0:
                            # Rows of padding under the lowest solid pixel
                            # (the shadow is translucent and does not
                            # count). The game lifts each clip by the
                            # difference to DEFAULT so feet stay put.
                            solid = rgba.getchannel("A").point(
                                lambda v: 255 if v > 200 else 0)
                            box = solid.getbbox()
                            feet.append(h - box[3] if box else 0)
                    strip.save(os.path.join(outdir, f"{slot}_d{direction}.png"))
                entry = {"frame": [w, h], "frames": fpd, "dirs": 8,
                         "ms": flc_ms(locate(flc)), "feet": feet}
                if slot in SOUND_SLOTS:
                    entry["sounds"] = [list(s) for s in slot_sounds(cfg, locate, slot)]
                manifest[slot] = entry
                print(f"  {unit}/{slot}: {w}x{h} x{fpd}f x8d {entry['ms']}ms")
        with open(os.path.join(outdir, "manifest.json"), "w") as f:
            json.dump(manifest, f, indent=1)
        adir = os.path.join(OUT, "audio", "units", unit)
        os.makedirs(adir, exist_ok=True)
        for d in reversed(dirs):
            for wav in glob.glob(os.path.join(d, "*.wav")):
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
    # unit icons for the production list: the Conquests sheet, indexed by
    # PRTO.icon (14 columns of 32-px cells on a 33-px grid)
    convert_pcx(os.path.join(GOG, "Conquests", "Art", "Units", "units_32.pcx"),
                os.path.join(uid, "unit_icons.png"))
    # content Asian citizen head (row 0, col 4 of popHeads)
    heads = Image.open(os.path.join(GOG, "Art", "SmallHeads", "popHeads.pcx"))
    heads.load()
    to_rgba(heads.crop((200, 0, 250, 50))).save(
        os.path.join(uid, "citizen.png"))
    # entertainer (jester, row 16 col 1): idle citizens on the city screen
    to_rgba(heads.crop((50, 800, 100, 850))).save(
        os.path.join(uid, "entertainer.png"))
    # City-panel top bar buttons: previous/next city, close, in
    # normal/hover/pressed states. `cityMgmtButtons.pcx` is a 4x3 grid of
    # 49x60 cells (prev, next, eye, X), with the dev's own layout notes
    # under it.
    mgmt = Image.open(os.path.join(OUT, "cityscreen", "cityMgmtButtons.png"))
    for row in range(3):
        # The sheet's third column (the small eye) is unused: the panel's
        # city view is always on screen, so it has no view toggle.
        for col, name in enumerate(["prev", "next", "x"]):
            # 47 wide: the cells' last two columns carry the next button's
            # edge, which would show as a sliver on the top bar.
            mgmt.crop((col * 49, row * 60, col * 49 + 47, row * 60 + 60)).save(
                os.path.join(uid, f"mgmt_{name}_{row}.png"))
    # Scrollbar pieces from `Art/scroll.pcx`: the small arrow set at the top
    # of the sheet (18x16 cells, green then orange for idle/hover) and the
    # ladder track tile.
    sc = to_rgba(Image.open(os.path.join(GOG, "Art", "scroll.pcx")))
    for name, box in [
        ("scroll_up_0", (112, 14, 130, 30)),
        ("scroll_up_1", (128, 14, 146, 30)),
        ("scroll_down_0", (112, 0, 130, 16)),
        ("scroll_down_1", (128, 0, 146, 16)),
        ("scroll_track", (14, 138, 28, 158)),
    ]:
        sc.crop(box).save(os.path.join(uid, name + ".png"))
    # Fade bars: the game masks the bar art with its Alpha sheet, so bake
    # the mask into the alpha channel here.
    for name in ["TopFadeBar", "BottomFadeBar"]:
        art = Image.open(os.path.join(OUT, "cityscreen", name + ".png")).convert("RGB")
        mask = Image.open(os.path.join(OUT, "cityscreen", name + "Alpha.png")).convert("L")
        faded = art.convert("RGBA")
        faded.putalpha(mask)
        faded.save(os.path.join(uid, name + ".png"))
    print("cities cropped: sprites + buttons + icons + citizen + entertainer")


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


def stage_cursor():
    """Civ3's selection marker: `Art/Animations/Cursor/Cursor.flc`.

    The dashed white ellipse drawn around the selected unit is not in the
    interface art — the game keeps it in the animation folder as a 31
    frame FLC on a 93x46 canvas (art inset 1 px), the dashes crawling
    around the ellipse. `Cursor.ini` gives 175 ms per frame. Slides convert
    like unit art: magenta out, the red ring under the dashes is Civ3's
    shadow (`to_rgba`), so it lands as a translucent dark outline.
    """
    src = os.path.join(GOG, "Art", "Animations", "Cursor", "Cursor.flc")
    outdir = os.path.join(OUT, "cursor")
    os.makedirs(outdir, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        pat = os.path.join(tmp, "f_%04d.png")
        ffmpeg_frames(src, pat)
        frames = sorted(glob.glob(os.path.join(tmp, "f_*.png")))
        w, h = Image.open(frames[0]).size
        strip = Image.new("RGBA", (w * len(frames), h), (0, 0, 0, 0))
        for i, f in enumerate(frames):
            fim = Image.open(f)
            fim.load()
            strip.paste(to_rgba(fim), (i * w, 0))
        strip.save(os.path.join(outdir, "ring.png"))
        with open(os.path.join(outdir, "manifest.json"), "w") as f:
            json.dump({"ring": {"file": "ring.png", "frame": [w, h],
                                 "frames": len(frames), "ms": 175}}, f, indent=1)
        print(f"  cursor/ring.png: {w}x{h} x{len(frames)}f")
    print("cursor: selection ring from Cursor.flc")


def stage_borders():
    """Civ3's cultural border ribbon: `Art/Terrain/Territory.pcx`.

    The sheet is 256x288 = 2 columns x 4 rows of 128x72 cells (the exe
    slices it at 0x4C6A4B with x += 128, y += 72). Each cell is one tile
    diamond, inset 4 px top and bottom so its center is the cell center,
    with the ribbon drawn along one edge: row 0 the upper-left edge, 1
    upper-right, 2 lower-left, 3 lower-right. Column 1 is a variant whose
    ribbon bulges into the tile (purpose unresolved); the game's own
    screenshot shows the straight column-0 ribbon on every edge, so only
    those four cells ship.

    The gray diamond (index 1) and the purple surround (255) are
    transparent: the loader ends by making indices 0 and 1 transparent
    (0x4C6A8F..0x4C6AAA calls 0x5FFED0 with 0 and 1), so the gray is a
    template, not art. What is left is a chain of beads on a thread. The
    game paints it in the owner's color: the bead core (64) is the color
    and the rod rim (65) a darker shade of it, so both bake to a gray
    level for the runtime tint. The two magenta pinks (249, 252) are the
    shadow thread and the bead's soft edge, which the game draws dark and
    nearly neutral (they read desaturated over terrain), so they bake to
    black. Alphas are eyeballed from the blurred reference screenshot,
    not measured.
    """
    outdir = os.path.join(OUT, "borders")
    os.makedirs(outdir, exist_ok=True)
    src = Image.open(os.path.join(GOG, "Art", "Terrain", "Territory.pcx"))
    src.load()
    # `tobytes` on a P image is the raw palette index per pixel; `convert`
    # would go through RGB and lose the index.
    raw = src.tobytes()
    alpha = {1: 0, 255: 0, 64: 255, 65: 255, 249: 110, 252: 70}
    ink = {64: 255, 65: 200}  # civ color, and its dark rim, via the tint
    lut = lambda table: bytes(table.get(i, 0) for i in range(256))
    lum = Image.frombytes("L", src.size, bytes(lut(ink)[b] for b in raw))
    mask = Image.frombytes("L", src.size, bytes(lut(alpha)[b] for b in raw))
    body = Image.merge("RGBA", (lum, lum, lum, mask))
    # Sheet row -> the map neighbor the edge faces. `map::tile_to_world`
    # is in Bevy world space, y up: (x+1, y) lands at (+64, -32), which is
    # down-right on screen, (x, y+1) down-left, (x-1, y) up-left and
    # (x, y-1) up-right. So row 0 (upper-left edge) faces `xm`, row 1
    # `ym`, row 2 `yp` and row 3 `xp`. The ribbon must land in that
    # quadrant of the cell (image space, y down); a vertical flip here puts
    # every ribbon on the mirrored edge and breaks the outline into
    # scattered dashes.
    for row, (side, (ux, uy)) in enumerate(
            zip(["xm", "ym", "yp", "xp"], [(0, 0), (1, 0), (0, 1), (1, 1)])):
        cell = body.crop((0, row * 72, 128, row * 72 + 72))
        cell.save(os.path.join(outdir, f"border_{side}.png"))
        box = cell.getchannel("A").getbbox()
        assert box, f"border_{side}.png is empty"
        cx, cy = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2
        assert (cx > 64) == bool(ux) and (cy > 36) == bool(uy), \
            f"border_{side}.png ribbon sits at {cx:.0f},{cy:.0f}"
        print(f"  borders/border_{side}.png: {cell.size}")
    print("borders: 4 edges from Territory.pcx")


# --- leaders, diplomacy screens and wonders --------------------------------

# `RACE.era_art` of conquests.biq (forward clips, one per era: ancient,
# middle, industrial, modern; the second four paths are the reverse clips).
# Checked against the file: Japan is row 9, Rome 1, Egypt 2, China 7.
LEADER_CLIPS = {
    "Japan": ["To_A01", "To_01", "To_C01", "To_D01"],
    "Rome": ["Ce_01", "Ce_B01", "Ce_C01", "Ce_D01"],
    "Egypt": ["Cl_01", "Cl_B01", "Cl_C01", "Cl_D01"],
    "China": ["Mo_A01", "Mo_B01", "Mo_C01", "Mo_01"],
}
LEADER_COLS = 11  # 121 frames fill an 11 x 11 grid of 200 x 240 cells
# The FLC header speed is 71 ms in Mo_01 and the Japan/Rome base clips but 0
# or 20 in the rest (it is not what the game plays by); 71 ms is the clone's
# rate for all.
LEADER_MS = 71

# `BLDG` name -> `civilopedia_entry` of every wonder row; the splash art is
# `PediaIcons.txt #WON_SPLASH_<entry>`.
WONDER_ENTRIES = {
    "The Pyramids": "BLDG_Pyramids",
    "The Hanging Gardens": "BLDG_Hanging_Gardens",
    "The Colossus": "BLDG_Colossus",
    "The Great Lighthouse": "BLDG_Lighthouse",
    "The Great Library": "BLDG_Great_Library",
    "The Oracle": "BLDG_Oracle",
    "The Great Wall": "BLDG_Great_Wall",
    "Sun Tzu's Art of War": "BLDG_Art_of_War",
    "Sistine Chapel": "BLDG_Sistine_Chapel",
    "Magellan's Voyage": "BLDG_Circumnavigation",
    "Copernicus' Observatory": "BLDG_Solar_System",
    "Shakespeare's Theater": "BLDG_Great_Playhouse",
    "Leonardo's Workshop": "BLDG_Inventor's_Workshop",
    "JS Bach's Cathedral": "BLDG_Grand_Cathedral",
    "Newton's University": "BLDG_Great_University",
    "Smith's Trading Company": "BLDG_Trading_Company",
    "Universal Suffrage": "BLDG_Universal_Suffrage",
    "Hoover Dam": "BLDG_Hoover_Dam",
    "Theory of Evolution": "BLDG_Theory_of_Evolution",
    "The United Nations": "BLDG_United_Nations",
    "The Manhattan Project": "BLDG_Manhattan_Project",
    "Cure for Cancer": "BLDG_Cure_for_Cancer",
    "Longevity": "BLDG_Longevity",
    "SETI program": "BLDG_SETI_Program",
    "Heroic Epic": "BLDG_Epic",
    "Iron Works": "BLDG_Great_Ironworks",
    "Forbidden Palace": "BLDG_Forbidden_Palace",
    "Military Academy": "BLDG_Military_Academy",
    "The Pentagon": "BLDG_Pentagon",
    "Wall Street": "BLDG_Wall_Street",
    "Apollo Program": "BLDG_Apollo_Project",
    "Strategic Missile Defense": "BLDG_SDI",
    "Intelligence Agency": "BLDG_Intelligence_Center",
    "Battlefield Medicine": "BLDG_Battlefield_Medicine",
    "The Internet": "BLDG_Internet",
    "The Temple of Artemis": "BLDG_Artemis",
    "The Statue of Zeus": "BLDG_Zeus",
    "The Mausoleum of Mausollos": "BLDG_Mausoleum",
    "Knights Templar": "BLDG_Knights_Templar",
    "Secret Police HQ": "BLDG_Secret_Police_HQ",
}

# The card of the Wonders window shows the art in a 190 x 132 well.
WONDER_THUMB = (190, 132)


def slug(name):
    """`Sun Tzu's Art of War` -> `sun_tzu_s_art_of_war`; `src/wonders.rs`
    computes the same."""
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")


def find_ci(bases, rel):
    """The file `rel` (Windows separators, any case) under the first of
    `bases` that has it."""
    parts = rel.replace("\\", "/").split("/")
    for base in bases:
        here = base
        for part in parts:
            if not os.path.isdir(here):
                here = None
                break
            hit = next((e for e in os.listdir(here) if e.lower() == part.lower()), None)
            if hit is None:
                here = None
                break
            here = os.path.join(here, hit)
        if here is not None and os.path.exists(here):
            return here
    return None


def conquests_first():
    return [os.path.join(GOG, "Conquests"), GOG]


def decode_flc(path, tmp):
    """Decoded frames of an FLC; the last is the ring frame that returns to
    the first."""
    for old in glob.glob(os.path.join(tmp, "f_*.png")):
        os.remove(old)
    ffmpeg_frames(path, os.path.join(tmp, "f_%04d.png"))
    frames = []
    for f in sorted(glob.glob(os.path.join(tmp, "f_*.png"))):
        im = Image.open(f)
        im.load()
        frames.append(im.convert("RGB"))
    return frames


def stage_leaders():
    """The leaderhead animations of the diplomacy screens (`Art/Flics`).

    Each civ has one clip per era (ancient, middle, industrial, modern),
    200 x 240, 120 or 121 frames. The clip does not loop on its own: it
    drifts away from its first frame the whole way (no cuts), and the `_02`
    twin is the same frames in reverse (checked here: all but a few pixels
    of a few frames), so the pair played back to back is a ping-pong over
    one forward clip. Only the forward clips ship, one grid sheet each. The
    ring frame FLC adds at the end (equal to the first) is dropped.
    """
    outdir = os.path.join(OUT, "leaders")
    os.makedirs(outdir, exist_ok=True)
    flics = os.path.join(GOG, "Art", "Flics")
    manifest = {}
    with tempfile.TemporaryDirectory() as tmp:
        for civ, clips in LEADER_CLIPS.items():
            manifest[civ] = []
            for era, stem in enumerate(clips):
                fwd = find_ci([flics], stem + ".flc")
                rev = find_ci([flics], stem[:-2] + "02.flc")
                assert fwd and rev, f"{civ}: no clip {stem}"
                frames = decode_flc(fwd, tmp)
                header_ms = flc_ms(fwd)
                ring = frames.pop()
                assert ring.tobytes() == frames[0].tobytes(), f"{stem}: ring frame is not frame 0"
                back = decode_flc(rev, tmp)
                back.pop()
                assert len(back) == len(frames), f"{stem}: the _02 clip is {len(back)} frames"
                # Not always byte for byte: Mo_B02 differs from the reversed
                # Mo_B01 in 6 frames, by a few pixels near (55, 170).
                odd = sum(a.tobytes() != b.tobytes() for a, b in zip(back, reversed(frames)))
                assert odd * 10 <= len(frames), f"{stem}: the _02 clip is not the reverse ({odd})"
                ms = LEADER_MS
                w, h = frames[0].size
                n = len(frames)
                rows = -(-n // LEADER_COLS)
                sheet = Image.new("RGB", (w * LEADER_COLS, h * rows), (0, 0, 0))
                for i, f in enumerate(frames):
                    sheet.paste(f, ((i % LEADER_COLS) * w, (i // LEADER_COLS) * h))
                name = f"{civ.lower()}_{era}.png"
                sheet.save(os.path.join(outdir, name), optimize=False)
                manifest[civ].append({"file": name, "frame": [w, h], "cols": LEADER_COLS,
                                      "frames": n, "ms": ms, "header_ms": header_ms})
                print(f"  leaders/{name}: {w}x{h} x{n}f ({stem}, header says {header_ms} ms, "
                      f"{odd} frames of the _02 clip differ)")
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print("leaders: 4 civs x 4 eras")


def stage_diplomacy():
    """The diplomacy screen frames and the leaders' speech.

    `Art/Diplomacy/{talk_offer,consider,counter}.pcx` are 1024 x 768 with
    the map showing through (magenta). All three hold the leader's frame, a
    200 x 240 hole at (411, 59); `text/diplomacy.txt` is what they say
    (Windows-1252 on disk, UTF-8 here).
    """
    d = os.path.join(OUT, "diplomacy")
    os.makedirs(d, exist_ok=True)
    for stem in ("talk_offer", "consider", "counter", "uparrow", "downarrow"):
        size = convert_pcx(os.path.join(GOG, "Art", "Diplomacy", stem + ".pcx"),
                           os.path.join(d, stem + ".png"))
        print(f"  diplomacy/{stem}: {size}")
    td = os.path.join(OUT, "text")
    os.makedirs(td, exist_ok=True)
    src = find_ci(conquests_first(), "Text/diplomacy.txt")
    with open(src, "rb") as f:
        text = f.read().decode("cp1252", errors="replace")
    with open(os.path.join(td, "diplomacy.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    print("diplomacy: frames + text/diplomacy.txt")


def wonder_art_paths():
    """`BLDG` name -> splash `.pcx` path, through `PediaIcons.txt`."""
    pedia = find_ci(conquests_first(), "Text/PediaIcons.txt")
    with open(pedia, "rb") as f:
        lines = f.read().decode("cp1252", errors="replace").splitlines()
    by_entry = {}
    for i, line in enumerate(lines):
        if line.startswith("#WON_SPLASH_"):
            by_entry[line[len("#WON_SPLASH_"):].strip()] = lines[i + 1].strip()
    out = {}
    for name, entry in WONDER_ENTRIES.items():
        rel = by_entry.get(entry)
        out[name] = find_ci(conquests_first(), rel) if rel else None
    return out


def stage_wonders():
    """The wonder splash and the Wonders of the World window.

    `Art/Wonder Splash/wonderBackground.pcx`: 1024 x 768, a 320 x 320 hole
    at (351, 109) for the wonder's picture (`<name>.pcx`, 320 x 320) and the
    text below it. `Art/Advisors/wonders_background.pcx` is the F7 window,
    `wondersBOX.pcx` the 370 x 200 card for one wonder, and
    `wondersBOXoverlay.pcx` a copy of the card's picture well (190 x 132 at
    (162, 47), the top right 66 x 47 cut out for the eye button) drawn over
    the picture of a wonder that is not built. `wondersEye.pcx` is the eye
    button, three 66 x 47 states stacked at x = 1.
    """
    d = os.path.join(OUT, "wonders")
    sd = os.path.join(d, "splash")
    td = os.path.join(d, "thumb")
    for p in (sd, td):
        os.makedirs(p, exist_ok=True)
    pairs = [(os.path.join(GOG, "Art", "Wonder Splash", "wonderBackground.pcx"), "frame"),
             (os.path.join(GOG, "Art", "Advisors", "wonders_background.pcx"), "window"),
             (os.path.join(GOG, "Art", "Advisors", "wondersBOX.pcx"), "card"),
             (os.path.join(GOG, "Art", "Advisors", "wondersBOXoverlay.pcx"), "card_hidden")]
    for src, name in pairs:
        print(f"  wonders/{name}: {convert_pcx(src, os.path.join(d, name + '.png'))}")
    eye = Image.open(os.path.join(GOG, "Art", "interface", "wondersEye.pcx"))
    eye.load()
    eye = to_rgba(eye)
    # The button is rounded: the corners of each 66 x 47 cell are pure green.
    eye.putdata([(r, g, b, 0) if (r, g, b) == (0, 255, 0) else (r, g, b, a)
                 for r, g, b, a in eye.getdata()])
    eye.save(os.path.join(d, "eye.png"))
    done = 0
    missing = []
    for name, src in wonder_art_paths().items():
        if src is None:
            missing.append(name)
            continue
        im = Image.open(src)
        im.load()
        assert im.size == (320, 320), f"{src}: {im.size}"
        art = to_rgba(im)
        art.save(os.path.join(sd, slug(name) + ".png"))
        # The well is 190 x 132: the middle band of the picture, scaled.
        tw, th = WONDER_THUMB
        band = round(320 * th / tw)
        top = (320 - band) // 2
        thumb = art.convert("RGB").crop((0, top, 320, top + band)).resize(
            (tw, th), Image.LANCZOS)
        thumb.save(os.path.join(td, slug(name) + ".png"))
        done += 1
    print(f"wonders: frame, window, card, eye and {done} pictures"
          + (f" (no art: {', '.join(missing)})" if missing else ""))


STAGES = {"terrain": stage_terrain, "units": stage_units,
          "cities": stage_cities, "cityscreen": stage_cityscreen,
          "splash": stage_splash, "audio": stage_audio, "fonts": stage_fonts,
          "features": stage_features, "improvements": stage_improvements,
          "unitbuttons": stage_unitbuttons, "hud": stage_hud,
          "fog": stage_fog, "borders": stage_borders,
          "cursor": stage_cursor, "leaders": stage_leaders,
          "diplomacy": stage_diplomacy, "wonders": stage_wonders}


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
