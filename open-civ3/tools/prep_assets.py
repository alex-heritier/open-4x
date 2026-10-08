#!/usr/bin/env python3
"""Convert Civ3 assets into runtime formats (RGBA PNG, OGG, WAV) under
`.cache/<namespace>/` (`docs/civ3-files.md` section 7).

The game never reads PCX, FLC, or MP3. Two ways to run it from the repo root:

    python3 tools/prep_assets.py --request .cache/civ3/request.json
        what the game does when something it draws is missing or stale: the
        request names every item (unit art folders, leader clips, wonder and
        advance pictures) with the Civ3 file it resolved, plus the install
        and search path. Only what is missing or stale is converted.

    python3 tools/prep_assets.py [stage ...]
        no request: convert the stock install in full, for development.
        Stages: units leaders wonders techs, then the interface stages
        terrain cities cityscreen splash audio fonts features advisors
        improvements unitbuttons hud fog borders cursor diplomacy

`.cache/<namespace>/index.json` records, for every item, what it produced, the
size and modification time of every Civ3 file it read, and a hash of this
script, so editing the script reconverts what it produces. The game reads the
index to decide whether to run the script at all.

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
import time

from PIL import Image, ImageChops

sys.stdout.reconfigure(line_buffering=True)

GOG = os.environ.get("CIV3_GOG", "../civ3/civ3-gog/app")
OUT = os.environ.get("CIV3_CACHE", ".cache/civ3")
SELF = os.path.abspath(__file__)

# The match's items. A request fills these; a bare run scans the install.
UNITS = []          # [{"art", "key", "dirs"}]
TEAM_COLORS = [0]   # `ntpNN.pcx` indices
LEADERS = []        # [{"key", "forward", "reverse"}]
WONDERS = []        # [{"key", "path"}]
TECHS = []          # [{"key", "path"}]
SEARCH = []         # Civ3's search order, nearest first (absolute folders)

# ---- the index: what each item produced and read ---------------------------

_READ = None  # sources of the item being converted
_GENERATED_SOURCES = {}  # converted intermediate -> original sources


def script_hash():
    """FNV-1a 64 of this file; the game computes the same."""
    h = 0xCBF29CE484222325
    with open(SELF, "rb") as f:
        for b in f.read():
            h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def rel_source(path):
    """A source as the index keeps it: relative to the install root when it
    is inside, else absolute."""
    root = os.path.abspath(GOG)
    p = os.path.abspath(path)
    return os.path.relpath(p, root) if p.startswith(root + os.sep) else p


def track(path):
    """Note that the current item read `path`."""
    if _READ is not None and path and os.path.exists(path) and not re.match(r"f_\d+\.png$", os.path.basename(path)):
        full = os.path.abspath(path)
        if full.startswith(os.path.abspath(OUT) + os.sep):
            _READ.update(_GENERATED_SOURCES.get(full, []))
        else:
            _READ.add(rel_source(path))
    return path


_open = Image.open


def _tracked_open(fp, *a, **kw):
    if isinstance(fp, (str, os.PathLike)):
        track(os.fspath(fp))
    return _open(fp, *a, **kw)


Image.open = _tracked_open
_copy = shutil.copy


def _tracked_copy(src, dst, *a, **kw):
    track(src)
    return _copy(src, dst, *a, **kw)


shutil.copy = _tracked_copy


def index_path():
    return os.path.join(OUT, "index.json")


def load_index():
    try:
        with open(index_path()) as f:
            idx = json.load(f)
    except (OSError, ValueError):
        idx = {}
    if idx.get("script") != script_hash():
        idx = {"script": script_hash(), "entries": {}}
    idx.setdefault("entries", {})
    return idx


def save_index(idx):
    os.makedirs(OUT, exist_ok=True)
    tmp = index_path() + ".tmp"
    with open(tmp, "w") as f:
        json.dump(idx, f, indent=1, sort_keys=True)
    os.replace(tmp, index_path())


def stat_of(rel):
    full = rel if os.path.isabs(rel) else os.path.join(GOG, rel)
    st = os.stat(full)
    return {"path": rel, "size": st.st_size, "mtime": int(st.st_mtime)}


def fresh(idx, key, params=None, search_dependent=False):
    """Is `key` converted from the files that are there now?"""
    e = idx["entries"].get(key)
    if e is None or e.get("root") != os.path.realpath(GOG):
        return False
    have = e.get("params")
    # A unit's team colors only grow: a copy with more colors will do.
    if have != params and not (isinstance(params, list) and isinstance(have, list) and set(params) <= set(have)):
        return False
    if search_dependent and e.get("search") != list(SEARCH):
        return False
    if any(not os.path.exists(os.path.join(OUT, o)) for o in e["outputs"]):
        return False
    try:
        return all(stat_of(s["path"]) == s for s in e["sources"])
    except OSError:
        return False


def run_item(idx, key, work, params=None, search_dependent=False, scope=""):
    """Convert `key` unless it is fresh; record outputs and sources. The
    outputs are the files under `OUT/scope` that `work` wrote."""
    global _READ
    if fresh(idx, key, params, search_dependent):
        return False
    _READ = set()
    before = snapshot(scope)
    try:
        work()
    finally:
        reads, _READ = sorted(_READ), None
    after = snapshot(scope)
    wrote = sorted(os.path.relpath(p, OUT) for p, m in after.items() if before.get(p) != m)
    entry = {"root": os.path.realpath(GOG), "outputs": wrote, "sources": [stat_of(r) for r in reads], "params": params}
    if search_dependent:
        entry["search"] = list(SEARCH)
    idx["entries"][key] = entry
    for output in wrote:
        _GENERATED_SOURCES[os.path.abspath(os.path.join(OUT, output))] = reads
    save_index(idx)
    return True


def snapshot(scope):
    """`{file: mtime_ns}` of every file under `OUT/scope`."""
    out = {}
    for d, _, files in os.walk(os.path.join(OUT, scope)):
        for f in files:
            if f not in ("index.json", "index.json.tmp", "request.json"):
                full = os.path.join(d, f)
                out[full] = os.stat(full).st_mtime_ns
    return out


def find_in_dirs(dirs, name):
    """`name` in the first of `dirs` that has it, any case. INI files write
    sibling-folder paths with backslashes (`..\\Legionary\\LegionaryRun.amb`),
    which are followed from each dir."""
    parts = name.replace("\\", "/").split("/")
    for d in dirs:
        cur = d
        for part in parts:
            if part == "..":
                cur = os.path.dirname(cur)
                continue
            names = os.listdir(cur) if os.path.isdir(cur) else []
            hit = next((e for e in names if e.lower() == part.lower()), None)
            if hit is None:
                cur = None
                break
            cur = os.path.join(cur, hit)
        if cur is not None and os.path.exists(cur):
            return cur
    return None


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


_is255 = [0] * 255 + [255]
_is0 = [255] + [0] * 255
_ge201 = [0] * 201 + [255] * 55
_le99 = [255] * 100 + [0] * 156
_ge240 = [0] * 240 + [255] * 16
_le60 = [255] * 61 + [0] * 195


def to_rgba(im, unit=False, green_clear=False):
    """Apply Civ3 transparency rules, return RGBA image.

    Magenta is transparent everywhere (for units, also palette index 255).
    Shadows are exact red, plus vivid green for units (the scout's shadow palette is green, not red).
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
        if im.mode == "P":
            # The engine keys sprites on palette index 255, whatever colour a
            # pack gave it (the 6-inch Howitzer's is (238, 0, 237)).
            is_mag = ImageChops.lighter(is_mag, Image.frombytes("L", im.size, im.tobytes()).point(_is255))
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
        if stem in ("deltaRivers", "mtnRivers"):
            to_rgba(Image.open(f), green_clear=True).save(os.path.join(d, stem + ".png"))
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
    track(src)
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src, pattern],
                   check=True)


def flc_ms(path):
    """Milliseconds per frame from the FLC header (`speed`, dword at 16)."""
    track(path)
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
            wav, end = _cstr(body, q + 20)
            kmaps[name] = wav
            # GalleyAttack.amb rounds two declared payload lengths down,
            # omitting a byte of the trailing sampler word after the WAV name.
            n = max(n, end + 4)
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
    path = locate(name)
    if not os.path.exists(path):
        print(f"  no sound {name}, silent")
        return []
    if name.lower().endswith(".amb"):
        return parse_amb(path)
    return [(0.0, os.path.basename(path))]


def team_palettes(colors):
    out = {}
    for n in sorted(set(colors)):
        path = os.path.join(GOG, "Art", "Units", "Palettes", f"ntp{n:02d}.pcx")
        if os.path.exists(path):
            out[n] = Image.open(path).getpalette()[:64 * 3]
        else:
            print(f"  no team color {n} ({path})")
    return out


def recolor(fim, ramp):
    """The frame with palette entries 0-63 replaced by a team ramp."""
    im = fim.copy()
    pal = im.getpalette()
    im.putpalette(ramp + pal[64 * 3:])
    return im


def convert_unit(item):
    """One `Art/Units` folder: strips per slot and direction, a team-colored
    copy per color, the manifest and the sounds. `item["dirs"]` are the
    folders that hold it, nearest first (Conquests often holds only the INI
    and sounds of a unit whose animations live in the base game's folder)."""
    ramps = team_palettes(item["colors"])
    unit, key, dirs = item["art"], item["key"], item["dirs"]

    def locate(name):
        p = find_in_dirs(dirs, name)
        return track(p) if p else os.path.join(dirs[0], name)

    ini = next((p for d in dirs if os.path.isdir(d) for p in sorted(glob.glob(os.path.join(d, "*.[iI][nN][iI]")))), None)
    if ini is None:
        print(f"  {unit}: no INI, skipped")
        return
    track(ini)
    cfg = configparser.ConfigParser(strict=False)
    cfg.optionxform = str
    cfg.read(ini, encoding="latin-1")
    anims = dict(cfg["Animations"]) if "Animations" in cfg else {}
    outdir = os.path.join(OUT, key)
    shutil.rmtree(outdir, ignore_errors=True)
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
                teams = {n: Image.new("RGBA", (w * fpd, h), (0, 0, 0, 0)) for n in ramps}
                for i in range(fpd):
                    fim = Image.open(frames[direction * fpd + i])
                    fim.load()
                    rgba = to_rgba(fim, unit=True)
                    strip.paste(rgba, (i * w, 0))
                    if fim.mode == "P":
                        for n, ramp in ramps.items():
                            teams[n].paste(to_rgba(recolor(fim, ramp), unit=True), (i * w, 0))
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
                for n, team in teams.items():
                    team.save(os.path.join(outdir, f"{slot}_d{direction}_c{n}.png"))
            entry = {"frame": [w, h], "frames": fpd, "dirs": 8,
                     "ms": flc_ms(locate(flc)), "feet": feet}
            if slot in SOUND_SLOTS:
                entry["sounds"] = [list(s) for s in slot_sounds(cfg, locate, slot)]
            manifest[slot] = entry
            print(f"  {unit}/{slot}: {w}x{h} x{fpd}f x8d {entry['ms']}ms")
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    for d in reversed(dirs):
        for wav in glob.glob(os.path.join(d, "*.[wW][aA][vV]")):
            shutil.copy(wav, outdir)
    # A sound the INI takes from another folder (`..\\Legionary\\Death.wav`).
    for name in (cfg["Sound Effects"].values() if "Sound Effects" in cfg else ()):
        if name.strip().lower().endswith(".wav"):
            path = find_in_dirs(dirs, name.strip())
            if path:
                shutil.copy(track(path), outdir)


def stage_cities():
    # A scenario ships its own city sheets (the Middle Ages conquests all
    # draw medieval cities): resolve along the search path, like the game.
    d = os.path.join(OUT, "cities", "sheets")
    os.makedirs(d, exist_ok=True)
    for name in ["rAMER", "rEURO", "rROMAN", "rMIDEAST", "rASIAN",
                 "AMERWALL", "EUROWALL", "ROMANWALL", "MIDEASTWALL", "ASIANWALL",
                 "city icons"]:
        src = track(search_file(f"Art/Cities/{name}.pcx"))
        size = convert_pcx(src, os.path.join(d, name + ".png"))
        print(f"  cities/{name}: {size}")
    crop_cities()


def crop_cities():
    outdir = os.path.join(OUT, "cities")
    sheets = os.path.join(OUT, "cities", "sheets")
    # Each culture sheet is 3 x 4 cells of 167 x 95: a column per size class
    # (town, city, metropolis) and a row per era (ancient, medieval,
    # industrial, modern). RACE.culture_group 0..4 picks the sheet.
    manifest = {}
    for group, sheet_name in enumerate(["rAMER", "rEURO", "rROMAN", "rMIDEAST", "rASIAN"]):
        sheet = Image.open(os.path.join(sheets, sheet_name + ".png"))
        for era in range(4):
            for size, kind in enumerate(["town", "city", "metro"]):
                crop = sheet.crop((size * 167, era * 95, (size + 1) * 167, (era + 1) * 95))
                key = f"{kind}_{group}_{era}"
                crop.save(os.path.join(outdir, key + ".png"))
                # The footprint is centered in the cell, so the cell center
                # sits on the tile center (a bottom anchor drew cities one
                # tile north of their square).
                manifest[key] = {"file": key + ".png", "size": [167, 95],
                                 "anchor": [83, 47]}
        # The walls sheet is one 167 x 95 cell per era, whatever the size.
        wall = Image.open(os.path.join(sheets, sheet_name[1:] + "WALL.png"))
        for era in range(4):
            key = f"wall_{group}_{era}"
            wall.crop((0, era * 95, 167, (era + 1) * 95)).save(
                os.path.join(outdir, key + ".png"))
            manifest[key] = {"file": key + ".png", "size": [167, 95],
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
    # production button states: three 115x95 cells on a 116-px stride
    # (the sheet's own note: "115 x 95, draw @ (905, 516)"); the rest of
    # the sheet is that note.
    pb = Image.open(os.path.join(OUT, "cityscreen", "ProdButton.png"))
    for i in range(3):
        pb.crop((i * 116, 0, i * 116 + 115, pb.height)).save(
            os.path.join(uid, f"prod_{i}.png"))
    # hurry button states: three 28x28 cells split by 1-px magenta lines at
    # x = 0, 29, 58, 87 (the sheet's note: "28 x 28, draw @ (860, 520)").
    hb = Image.open(os.path.join(OUT, "cityscreen", "HurryButton.png"))
    for i in range(3):
        hb.crop((1 + i * 29, 0, 29 + i * 29, 28)).save(
            os.path.join(uid, f"hurry_{i}.png"))
    # unit icons for the production list: the Conquests sheet, indexed by
    # PRTO.icon (14 columns of 32-px cells on a 33-px grid)
    convert_pcx(os.path.join(GOG, "Conquests", "Art", "Units", "units_32.pcx"),
                os.path.join(uid, "unit_icons.png"))
    # content Asian citizen head (row 0, col 4 of popHeads). Cells are 50
    # px with a 1-px (255,87,255) separator on their top and left edges,
    # which is not the transparent magenta, so crop inside it.
    heads = Image.open(os.path.join(GOG, "Art", "SmallHeads", "popHeads.pcx"))
    heads.load()
    to_rgba(heads.crop((201, 1, 250, 50))).save(
        os.path.join(uid, "citizen.png"))
    # entertainer (jester, row 16 col 1): idle citizens on the city screen
    to_rgba(heads.crop((51, 801, 100, 850))).save(
        os.path.join(uid, "entertainer.png"))
    to_rgba(heads.crop((51, 851, 100, 900))).save(
        os.path.join(uid, "tax_collector.png"))
    to_rgba(heads.crop((51, 901, 100, 950))).save(
        os.path.join(uid, "scientist.png"))
    # City-panel top bar buttons: previous/next city, close, in
    # normal/hover/pressed states. `cityMgmtButtons.pcx` is a 4x3 grid of
    # cells (prev, next, eye, X) split by 1-px magenta lines at x = 0, 43,
    # 86, 153, 193 and y = 0, 48, 96, 144, with the dev's own layout notes
    # under it (prev @ (368, 21), next @ (609, 21), X @ (909, 21)).
    mgmt = Image.open(os.path.join(OUT, "cityscreen", "cityMgmtButtons.png"))
    # The third column (the small eye) is unused: the panel's city view is
    # always on screen, so it has no view toggle.
    cols = {"prev": (1, 43), "next": (44, 86), "x": (154, 193)}
    for row in range(3):
        for name, (x0, x1) in cols.items():
            mgmt.crop((x0, row * 48 + 1, x1, row * 48 + 48)).save(
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
    print("cities cropped: sprites + buttons + icons + citizen + specialists")


def stage_cityscreen():
    d = os.path.join(OUT, "cityscreen")
    os.makedirs(d, exist_ok=True)
    files = sorted(glob.glob(os.path.join(GOG, "Art", "city screen", "*.pcx")))
    for f in files:
        stem = os.path.splitext(os.path.basename(f))[0]
        convert_pcx(f, os.path.join(d, stem + ".png"))
    # The small improvement icons sit in pure green gutters; clear them, or
    # a scaled icon samples the line next to it.
    small = os.path.join(d, "buildings-small.png")
    clear_color(Image.open(small), (0, 255, 0)).save(small)
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
        src = track(os.path.join(GOG, "Sounds", src_rel))
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
    tb = Image.open(os.path.join(GOG, "Conquests", "Art", "Terrain", "TerrainBuildings.PCX"))
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
    tb = Image.open(os.path.join(GOG, "Conquests", "Art", "Terrain", "TerrainBuildings.PCX"))
    tb.load()
    mine = to_rgba(tb, green_clear=True).crop((256, 64, 384, 128))
    mine.save(os.path.join(outdir, "mine.png"))
    manifest["mine"] = {"file": "mine.png", "size": [128, 64], "anchor": [64, 32]}
    # fortress (column 0) and colony (column 1): one cell per era,
    # ancient to modern down the sheet.
    sheet = to_rgba(tb, green_clear=True)
    for era in range(4):
        for name, col in (("fortress", 0), ("colony", 1), ("barricade", 3)):
            cell = sheet.crop((col * 128, era * 64, (col + 1) * 128, (era + 1) * 64))
            cell.save(os.path.join(outdir, f"{name}_{era}.png"))
            manifest[f"{name}_{era}"] = {"file": f"{name}_{era}.png",
                                         "size": [128, 64], "anchor": [64, 32]}
    sheet.crop((256, 0, 384, 64)).save(os.path.join(outdir, "outpost.png"))
    manifest["outpost"] = {"file": "outpost.png", "size": [128, 64], "anchor": [64, 32]}
    with open(os.path.join(outdir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"improvements: 256 roads + {16 * len(IRRIGATION_SHEETS)} irrigation + mine + 4 fortresses + 4 colonies + 4 barricades + outpost")


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

LEADER_COLS = 11  # 121 frames fill an 11 x 11 grid of 200 x 240 cells
# The FLC header speed is 71 ms in Mo_01 and the Japan/Rome base clips but 0
# or 20 in the rest (it is not what the game plays by); 71 ms is the clone's
# rate for all.
LEADER_MS = 71

# The card of the Wonders window shows the art in a 190 x 132 well.
WONDER_THUMB = (190, 132)


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


def search_file(rel):
    """`rel` along the request's search path (else Conquests, then the base
    game), any case."""
    for base in SEARCH or conquests_first():
        hit = find_ci(base, rel)
        if hit:
            return hit
    raise FileNotFoundError(rel)


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


def convert_leader(item):
    """One leaderhead animation of the diplomacy screens (`Art/Flics`).

    Each civ has one clip per era (ancient, middle, industrial, modern),
    200 x 240, 120 or 121 frames. The clip does not loop on its own: it
    drifts away from its first frame the whole way (no cuts), and the `_02`
    twin is the same frames in reverse (checked on the stock clips: all but
    a few pixels of a few frames), so the pair played back to back is a
    ping-pong over one forward clip. Only the forward clip ships, as one grid
    sheet. The ring frame FLC adds at the end (equal to the first) is dropped.
    """
    outdir = os.path.join(OUT, item["key"])
    shutil.rmtree(outdir, ignore_errors=True)
    os.makedirs(outdir, exist_ok=True)
    fwd, rev = item["forward"], item.get("reverse")
    with tempfile.TemporaryDirectory() as tmp:
        frames = decode_flc(fwd, tmp)
        header_ms = flc_ms(fwd)
        ring = frames.pop()
        if ring.tobytes() != frames[0].tobytes():
            print(f"  {item['key']}: ring frame is not frame 0")
        odd = 0
        if rev:
            back = decode_flc(rev, tmp)
            back.pop()
            if len(back) != len(frames):
                print(f"  {item['key']}: the reverse clip is {len(back)} frames")
            else:
                # Not always byte for byte: Mo_B02 differs from the reversed
                # Mo_B01 in 6 frames, by a few pixels near (55, 170).
                odd = sum(a.tobytes() != b.tobytes() for a, b in zip(back, reversed(frames)))
        w, h = frames[0].size
        n = len(frames)
        rows = -(-n // LEADER_COLS)
        sheet = Image.new("RGB", (w * LEADER_COLS, h * rows), (0, 0, 0))
        for i, f in enumerate(frames):
            sheet.paste(f, ((i % LEADER_COLS) * w, (i // LEADER_COLS) * h))
        sheet.save(os.path.join(outdir, "sheet.png"), optimize=False)
        with open(os.path.join(outdir, "clip.json"), "w") as f:
            json.dump({"file": "sheet.png", "frame": [w, h], "cols": LEADER_COLS,
                       "frames": n, "ms": LEADER_MS, "header_ms": header_ms}, f, indent=1)
        print(f"  {item['key']}: {w}x{h} x{n}f (header says {header_ms} ms, "
              f"{odd} frames of the reverse clip differ)")


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
    src = track(search_file("Text/diplomacy.txt"))
    with open(src, "rb") as f:
        text = f.read().decode("cp1252", errors="replace")
    with open(os.path.join(td, "diplomacy.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    print("diplomacy: frames + text/diplomacy.txt")


def stage_wonders_ui():
    """The Wonders of the World window and the splash frame.

    `Art/Wonder Splash/wonderBackground.pcx`: 1024 x 768, a 320 x 320 hole
    at (351, 109) for the wonder's picture (`convert_wonder`) and the
    text below it. `Art/Advisors/wonders_background.pcx` is the F7 window,
    `wondersBOX.pcx` the 370 x 200 card for one wonder, and
    `wondersBOXoverlay.pcx` a copy of the card's picture well (190 x 132 at
    (162, 47), the top right 66 x 47 cut out for the eye button) drawn over
    the picture of a wonder that is not built. `wondersEye.pcx` is the eye
    button, three 66 x 47 states stacked at x = 1.
    """
    d = os.path.join(OUT, "wonders")
    os.makedirs(d, exist_ok=True)
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


def convert_wonder(item):
    """The picture of one wonder (`<key>.png`, 320 x 320) and the thumbnail
    of its card (`<key>.thumb.png`, the middle band scaled to the well)."""
    im = Image.open(item["path"])
    im.load()
    if im.size != (320, 320):
        print(f"  {item['key']}: {im.size}, expected 320 x 320")
    art = to_rgba(im)
    dst = os.path.join(OUT, item["key"])
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    art.save(dst + ".png")
    tw, th = WONDER_THUMB
    band = round(art.size[0] * th / tw)
    top = (art.size[1] - band) // 2
    thumb = art.convert("RGB").crop((0, top, art.size[0], top + band)).resize((tw, th), Image.LANCZOS)
    thumb.save(dst + ".thumb.png")


def convert_tech(item):
    """An advance's icon, as PediaIcons.txt names it."""
    im = Image.open(item["path"])
    im.load()
    dst = os.path.join(OUT, item["key"] + ".png")
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    clear_color(clear_color(to_rgba(im), (218, 0, 218)), (200, 0, 200)).save(dst)


def find_ci(base, rel):
    """`rel` (backslashes or slashes) under `base`, matched without case as
    the game's own file system does; None when it is not there."""
    cur = base
    for part in rel.replace("\\", "/").split("/"):
        if not os.path.isdir(cur):
            return None
        hit = [e for e in os.listdir(cur) if e.lower() == part.lower()]
        if not hit:
            return None
        cur = os.path.join(cur, hit[0])
    return cur if os.path.isfile(cur) else None


def art_file(rel):
    """An art file as Conquests loads it: its own copy first, then the base
    game's."""
    return find_ci(os.path.join(GOG, "Conquests"), rel) or find_ci(GOG, rel)


def clear_color(im, rgb):
    """`im` with every pixel of exactly `rgb` made transparent."""
    im = im.convert("RGBA")
    r, g, b, a = im.split()
    hit = ImageChops.multiply(ImageChops.multiply(
        r.point([255 if v == rgb[0] else 0 for v in range(256)]),
        g.point([255 if v == rgb[1] else 0 for v in range(256)])),
        b.point([255 if v == rgb[2] else 0 for v in range(256)]))
    im.putalpha(Image.composite(Image.new("L", im.size, 0), a, hit))
    return im


def runs(flags, least=20):
    """Index ranges where `flags` holds, at least `least` long."""
    out, start = [], None
    for i, f in enumerate(flags + [False]):
        if f and start is None:
            start = i
        elif not f and start is not None:
            if i - start >= least:
                out.append((start, i))
            start = None
    return out


def stage_advisors():
    """The Domestic and Science Advisors: Civ3's 1024x768 backgrounds, the
    tech boxes, the advisor portraits and tabs, and the advance icons."""
    d = os.path.join(OUT, "advisors")
    os.makedirs(d, exist_ok=True)

    def load(rel):
        im = Image.open(art_file(rel))
        im.load()
        # The sheets' 1-px cell lines are darker magentas, (218, 0, 218)
        # or (200, 0, 200).
        return clear_color(clear_color(to_rgba(im), (218, 0, 218)), (200, 0, 200))

    # Backgrounds. Conquests redrew the arrows of three eras for its tree;
    # its Industrial page is `science_industrial_new` (Ironclads and
    # Fascism). Modern keeps the base game's.
    for era, stem in enumerate(["science_ancient", "science_middle",
                                "science_industrial_new", "science_modern"]):
        load(f"Art/Advisors/{stem}.pcx").save(os.path.join(d, f"science_{era}.png"))
    load("Art/Advisors/domestic.pcx").save(os.path.join(d, "domestic.png"))
    load("Art/Advisors/dialogbox.pcx").save(os.path.join(d, "dialog.png"))
    load("Art/Advisors/non_required.pcx").crop((0, 0, 27, 27)).save(
        os.path.join(d, "non_required.png"))

    # Tech boxes: four eras of four sizes, each in four states (researched,
    # researching, available, unavailable), on a 189-px column stride and
    # split by (218, 0, 218) grid lines. Measured, not hard-coded.
    sheet = Image.open(art_file("Art/Advisors/techboxes.pcx")).convert("RGB")
    px = sheet.load()
    off = {(255, 0, 255), (218, 0, 218)}
    rows = runs([px[45, y] not in off for y in range(sheet.height)])
    assert len(rows) == 16, f"techbox rows: {len(rows)}"
    boxes = clear_color(to_rgba(Image.open(art_file("Art/Advisors/techboxes.pcx"))), (218, 0, 218))
    for i, (y0, y1) in enumerate(rows):
        era, size = divmod(i, 4)
        cols = runs([px[x, (y0 + y1) // 2] not in off for x in range(760)])
        assert len(cols) == 4, f"techbox row {i}: {cols}"
        for state, (x0, x1) in enumerate(cols):
            boxes.crop((x0, y0, x1, y1)).save(
                os.path.join(d, f"techbox_{era}_{size}_{state}.png"))

    # Buttons: the era navigation (129x34 in three states, then the two
    # arrows), the close X (26x30, three states), the government button
    # (146x26, three states) and the rate -/+ (24x24, three states).
    nav = load("Art/Tech Chooser/scienceNAV.pcx")
    for i in range(3):
        nav.crop((0, 34 * i, 129, 34 * i + 34)).save(os.path.join(d, f"nav_{i}.png"))
    nav.crop((0, 102, 45, 112)).save(os.path.join(d, "nav_left.png"))
    nav.crop((45, 102, 90, 112)).save(os.path.join(d, "nav_right.png"))
    ex = load("Art/Advisors/advisor_EXIT.pcx")
    for i in range(3):
        ex.crop((26 * i, 0, 26 * i + 26, 30)).save(os.path.join(d, f"exit_{i}.png"))
    # The boxed close X of the advisors' bottom-right corner (72x48, three
    # states), drawn where a screen's background has no box of its own.
    xb = load("Art/exitBox-backgroundStates.pcx")
    for i in range(3):
        xb.crop((72 * i, 0, 72 * i + 72, 48)).save(os.path.join(d, f"exitbox_{i}.png"))
    # The advisor popups ("our Sages need direction"): the parchment panel
    # of `popupborders.pcx` (187x136, a dark green rule, stretched as nine
    # slices), the round bullets of the answers (the O cells of the X/O
    # sprite: idle, rollover, chosen) and the pull-down arrow.
    pop = load("Art/popupborders.pcx")
    pop = pop.crop((250, 0, 437, 136))
    # The sheet's slice guides run through the parchment: paint each over
    # with its neighbour.
    for x in (62, 124):
        pop.paste(pop.crop((x - 1, 0, x, pop.height)), (x, 0))
    for y in (45, 90):
        pop.paste(pop.crop((0, y - 1, pop.width, y)), (0, y))
    pop.save(os.path.join(d, "popup.png"))
    xo = clear_color(load("Art/X-o_ALLstates-sprite.pcx"), (0, 255, 0))
    for i in range(3):
        xo.crop((36 * i + 1, 1, 36 * i + 20, 21)).save(os.path.join(d, f"bullet_{i}.png"))
    load("Art/pulldownArrows.pcx").crop((1, 22, 22, 43)).save(os.path.join(d, "pulldown.png"))
    gb = load("Art/Advisors/domesticBUTTON.pcx")
    for i in range(3):
        gb.crop((0, 26 * i, 146, 26 * i + 26)).save(os.path.join(d, f"govt_{i}.png"))
    aux = load("Art/Advisors/domestic_icons_aux.pcx")
    for i in range(3):
        aux.crop((50, 24 * i, 74, 24 * i + 24)).save(os.path.join(d, f"minus_{i}.png"))
        aux.crop((74, 24 * i, 98, 24 * i + 24)).save(os.path.join(d, f"plus_{i}.png"))
    # The sliders' small -/+ (12 wide; minus 8 tall, plus 13), three states.
    pm = load("Art/Advisors/domestic_plusminus.pcx")
    for i in range(3):
        pm.crop((12 * i, 0, 12 * i + 12, 8)).save(os.path.join(d, f"less_{i}.png"))
        pm.crop((12 * i, 8, 12 * i + 12, 21)).save(os.path.join(d, f"more_{i}.png"))
    # The slider knobs and column icons: 23x29 cells framed in dark blue.
    icons = load("Art/Advisors/domestic_icons.pcx")
    for name, (x, y) in {"flask": (138, 256), "smiley": (250, 250),
                         "coins": (172, 256)}.items():
        icons.crop((x + 1, y + 1, x + 22, y + 28)).save(os.path.join(d, f"{name}.png"))

    # Portraits: 150-px cells, a row an era, the happy face in column 0.
    # Tabs: 56-px cells, a row an advisor (domestic, trade, military,
    # foreign, culture, science), columns default, rollover, active.
    for who in ["DOMESTIC", "SCIENCE"]:
        sheet = load(f"Art/SmallHeads/popup{who}.pcx")
        for era in range(4):
            sheet.crop((0, 150 * era, 150, 150 * era + 150)).save(
                os.path.join(d, f"portrait_{who.lower()}_{era}.png"))
    tabs = load("Art/SmallHeads/advisor_tab.pcx")
    for row in range(6):
        for state in range(3):
            tabs.crop((56 * state, 56 * row, 56 * state + 56, 56 * row + 56)).save(
                os.path.join(d, f"tab_{row}_{state}.png"))

    # Citizen heads by mood for the city list: popHeads holds four rows an
    # era (happy, content, unhappy, ...); column 4 is the clone's citizen.
    heads = Image.open(art_file("Art/SmallHeads/popHeads.pcx"))
    heads.load()
    for mood, row in [("happy", 0), ("content", 1), ("unhappy", 2)]:
        to_rgba(heads.crop((201, 50 * row + 1, 250, 50 * row + 50))).save(
            os.path.join(d, f"head_{mood}.png"))

    print("advisors: backgrounds, 64 tech boxes, buttons, portraits, tabs")


# In dependency order: `cities` reads what `cityscreen` wrote.
STAGES = {"terrain": stage_terrain,
          "cityscreen": stage_cityscreen, "cities": stage_cities,
          "splash": stage_splash, "audio": stage_audio, "fonts": stage_fonts,
          "features": stage_features, "improvements": stage_improvements,
          "unitbuttons": stage_unitbuttons, "hud": stage_hud,
          "fog": stage_fog, "borders": stage_borders,
          "cursor": stage_cursor, "diplomacy": stage_diplomacy,
          "wonders_ui": stage_wonders_ui, "advisors": stage_advisors}
# Stages whose input depends on the search path (a scenario's own text and
# city sheets).
SEARCH_STAGES = {"diplomacy", "cities"}
ITEMS = ("units", "leaders", "wonders", "techs")


def run_all(stages, items):
    """Convert what is missing or stale: the stages, then the item kinds."""
    os.makedirs(os.path.join(OUT, "ui"), exist_ok=True)
    idx = load_index()
    _GENERATED_SOURCES.clear()
    for entry in idx["entries"].values():
        if entry.get("root") == os.path.realpath(GOG):
            for output in entry["outputs"]:
                _GENERATED_SOURCES[os.path.abspath(os.path.join(OUT, output))] = [s["path"] for s in entry["sources"]]
    save_index(idx)
    done = 0
    for name in [n for n in STAGES if n in stages]:
        done += run_item(idx, "stage:" + name, STAGES[name], search_dependent=name in SEARCH_STAGES)
    if "units" in items:
        for it in UNITS:
            # Keep the colors an earlier run converted: they only grow.
            old = idx["entries"].get(it["key"], {}).get("params")
            colors = sorted(set(TEAM_COLORS) | set(old if isinstance(old, list) else []))
            it = dict(it, colors=colors)
            done += run_item(idx, it["key"], lambda it=it: convert_unit(it), params=colors, scope=it["key"])
    if "leaders" in items:
        for it in LEADERS:
            done += run_item(idx, it["key"], lambda it=it: convert_leader(it), scope=it["key"])
    if "wonders" in items:
        for it in WONDERS:
            done += run_item(idx, it["key"] + ".png", lambda it=it: convert_wonder(it), scope=os.path.dirname(it["key"]))
    if "techs" in items:
        for it in TECHS:
            done += run_item(idx, it["key"] + ".png", lambda it=it: convert_tech(it), scope=os.path.dirname(it["key"]))
    print(f"converted {done} item(s) -> {OUT}")


def lower_rel(path, root):
    """`path` under `root` as a lowercase, slash-separated key."""
    return os.path.relpath(path, root).replace(os.sep, "/").lower()


def scan_install():
    """The stock install in full: every unit folder, leader clip, wonder
    splash and advance icon it has."""
    global UNITS, TEAM_COLORS, LEADERS, WONDERS, TECHS
    roots = [os.path.join(GOG, "Conquests"), os.path.join(GOG, "civ3PTW"), GOG]
    names = []
    for r in roots:
        d = os.path.join(r, "Art", "Units")
        if os.path.isdir(d):
            for e in sorted(os.listdir(d)):
                if os.path.isdir(os.path.join(d, e)) and e.lower() != "palettes" and e not in names:
                    names.append(e)
    UNITS = []
    for n in names:
        dirs = [os.path.join(r, "Art", "Units", n) for r in roots if os.path.isdir(os.path.join(r, "Art", "Units", n))]
        first = next((d for d in dirs if glob.glob(os.path.join(d, "*.[iI][nN][iI]"))), None)
        if first:
            UNITS.append({"art": n, "key": lower_rel(first, GOG), "dirs": dirs})
    pal = os.path.join(GOG, "Art", "Units", "Palettes")
    TEAM_COLORS = sorted({int(m.group(1)) for f in os.listdir(pal) for m in [re.match(r"ntp(\d+)\.pcx$", f, re.I)] if m} | {0})
    # Leader clips: 200 x 240 FLCs with a reverse twin (`..01` / `..02`).
    LEADERS = []
    for r in roots[:1] + roots[2:]:
        d = os.path.join(r, "Art", "Flics")
        if not os.path.isdir(d):
            continue
        for f in sorted(os.listdir(d)):
            m = re.match(r"(.*)01\.flc$", f, re.I)
            if not m:
                continue
            rev = find_in_dirs([d], m.group(1) + "02.flc")
            path = os.path.join(d, f)
            with open(path, "rb") as fh:
                head = fh.read(12)
            if rev and struct.unpack_from("<HH", head, 8) == (200, 240):
                LEADERS.append({"key": lower_rel(path, GOG), "forward": path, "reverse": rev})
    pedia = search_file("Text/PediaIcons.txt")
    lines = open(pedia, "rb").read().decode("cp1252", errors="replace").splitlines()
    WONDERS, TECHS = [], []
    seen = set()
    for i, line in enumerate(lines):
        line = line.strip()
        want = None
        if line.startswith("#WON_SPLASH_"):
            want = WONDERS
        elif line.startswith("#TECH_") and not line.endswith("_LARGE"):
            want = TECHS
        if want is not None and i + 1 < len(lines):
            path = None
            for base in conquests_first():
                path = find_ci(base, lines[i + 1].strip())
                if path:
                    break
            if path and path not in seen:
                seen.add(path)
                want.append({"key": lower_rel(path, GOG), "path": path})


def main():
    global GOG, OUT, SEARCH, UNITS, TEAM_COLORS, LEADERS, WONDERS, TECHS
    args = sys.argv[1:]
    if args[:1] == ["--request"]:
        OUT = os.environ.get("CIV3_CACHE", os.path.dirname(args[1]) or ".")
        with open(args[1]) as f:
            req = json.load(f)
        GOG = req["root"]
        SEARCH = req.get("search", [])
        UNITS = req.get("units", [])
        TEAM_COLORS = req.get("team_colors", [0])
        LEADERS = req.get("leaders", [])
        WONDERS = req.get("wonders", [])
        TECHS = req.get("techs", [])
        run_all(req.get("stages", []), ITEMS)
        return
    want = args or ["all"]
    if want == ["all"]:
        want = list(STAGES) + list(ITEMS)
    scan_install()
    run_all([w for w in want if w in STAGES], [w for w in want if w in ITEMS])
    print("done ->", OUT)


if __name__ == "__main__":
    main()
