#!/usr/bin/env python3
"""Deterministic, original synthetic test media for the clone: no Civ3 bytes.

Everything here is drawn or synthesised from scratch (CC0), never sampled,
traced or palette-copied from an installed copy of the game. The shapes,
palettes, sheet divisions, animation contracts and file names follow the
*formats* `tools/prep_assets.py` converts and the game consumes, not the
original artwork:

  * indexed PCX sheets on original cell grids (diamonds are 128x64, city
    cells 167x95, advisor sheets 1024x768, ...) so every crop, mask and
    assert in the converter lands somewhere sensible;
  * FLC unit clips with one strip of 8 directions per slot (frame order
    south, south-east, east, ... as `units::facing_for_step`), a magenta
    background that `to_rgba` clears and a red shadow it softens;
  * stereo 16-bit WAV: distinct clicks, footfalls, hooves, tool taps and
    combat cues plus a short unobtrusive music loop, keyed to the exact
    names `src/audio.rs` and `prep_assets.UNIT_SLOTS` load;
  * the redistributable Arimo font (OFL) copied to `LSANS.TTF` with its
    licence beside it, instead of a hand-rolled placeholder face.

Palette contract (shared by every sheet):

  0..1     neutral, inside the team-replaced range
  2..31    team ramp; `prep_assets` replaces these with `ntpNN.pcx`
  32..63   neutral greys, also inside the replaced range (stay grey)
  64..253  fixed art colours (skin, metal, wood, foliage, water, ...)
  254      red, the unit shadow `to_rgba` turns translucent
  255      magenta, transparent

Run from the repo root with Pillow installed; ffmpeg is only needed later,
by `prep_assets.py`, not here. `--biq FILE` (repeatable) merges that
scenario's unit, leader, tech, wonder and building names into the reference
manifest with the Rust BIQ reader; without it the committed manifest is
used. `--clean` deletes what the run did not write. The generator never
reads the game install and never uses the network. The one external input
is the vendored `tools/test-font/Arimo.ttf` (see below). `tools/openart/`
holds the drawing code, `tools/check_stub_assets.py` the contract checks.
"""
import argparse
import colorsys
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import wave
from array import array

from PIL import Image, ImageDraw

import prep_assets as prep
from openart import (cityart, features, figures, gfx, ground, icons, overlays, panels, screens, trees, uisheets, unitmodels)

ROOT = Path(__file__).resolve().parent.parent
TAU = math.tau

# ---------------------------------------------------------------- dimensions

# Fallbacks for the committed `sheets` map in stub_asset_refs.json (real PCX
# header sizes, captured once from an install and then frozen; the generator
# itself stays install-independent).
DIMS = {
    "art/terrain/xggc.pcx": (1152, 576),
    "art/terrain/xdgc.pcx": (1152, 576),
    "art/terrain/xdgp.pcx": (1152, 576),
    "art/terrain/xdpc.pcx": (1152, 576),
    "art/terrain/xpgc.pcx": (1152, 576),
    "art/terrain/xtgc.pcx": (1152, 576),
    "art/terrain/wcso.pcx": (1152, 576),
    "art/terrain/wooo.pcx": (1152, 576),
    "art/terrain/wsss.pcx": (1152, 576),
    "art/terrain/polaricecaps-final.pcx": (1024, 256),
    "art/terrain/xhills.pcx": (512, 288),
    "art/terrain/mountains.pcx": (512, 352),
    "art/terrain/mountains-snow.pcx": (512, 352),
    "art/terrain/grassland forests.pcx": (1000, 884),
    "art/terrain/plains forests.pcx": (1000, 884),
    "art/terrain/tundra forests.pcx": (1000, 884),
    "art/terrain/hill forests.pcx": (512, 288),
    "art/terrain/hill jungle.pcx": (512, 288),
    "art/terrain/mountain forests.pcx": (512, 352),
    "art/terrain/mountain jungles.pcx": (512, 352),
    "art/terrain/roads.pcx": (2048, 1024),
    "art/terrain/irrigation.pcx": (512, 256),
    "art/terrain/irrigation plains.pcx": (512, 256),
    "art/terrain/irrigation desett.pcx": (512, 256),
    "art/terrain/irrigation tundra.pcx": (512, 256),
    "art/terrain/deltarivers.pcx": (512, 256),
    "art/terrain/mtnrivers.pcx": (512, 256),
    "art/terrain/goodyhuts.pcx": (384, 192),
    "art/terrain/territory.pcx": (256, 288),
    "art/terrain/fogofwar.pcx": (1152, 576),
    "conquests/art/terrain/terrainbuildings.pcx": (512, 256),
    "art/cities/ramer.pcx": (501, 380),
    "art/cities/reuro.pcx": (501, 380),
    "art/cities/rroman.pcx": (501, 380),
    "art/cities/rmideast.pcx": (501, 380),
    "art/cities/rasian.pcx": (501, 380),
    "art/cities/amerwall.pcx": (167, 380),
    "art/cities/eurowall.pcx": (167, 380),
    "art/cities/romanwall.pcx": (167, 380),
    "art/cities/mideastwall.pcx": (167, 380),
    "art/cities/asianwall.pcx": (167, 380),
    "art/cities/city icons.pcx": (58, 20),
    "art/city screen/buildings-small.pcx": (448, 3000),
    "art/city screen/background.pcx": (1024, 768),
    "art/city screen/xandview.pcx": (282, 280),
    "art/city screen/prodbutton.pcx": (400, 95),
    "art/city screen/hurrybutton.pcx": (200, 28),
    "art/city screen/citymgmtbuttons.pcx": (197, 238),
    "art/city screen/topfadebar.pcx": (1024, 24),
    "art/city screen/topfadebaralpha.pcx": (1024, 24),
    "art/city screen/bottomfadebar.pcx": (1024, 20),
    "art/city screen/bottomfadebaralpha.pcx": (1024, 20),
    "art/city screen/productionqueuebox.pcx": (203, 360),
    "art/city screen/cityicons.pcx": (776, 32),
    "art/smallheads/popheads.pcx": (500, 1000),
    "art/smallheads/popupdomestic.pcx": (800, 800),
    "art/smallheads/popupscience.pcx": (800, 800),
    "art/smallheads/advisor_tab.pcx": (300, 500),
    "art/scroll.pcx": (540, 455),
    "art/resources.pcx": (300, 300),
    "art/leaderheads/to.pcx": (200, 240),
    "art/diplomacy/talk_offer.pcx": (1024, 768),
    "art/diplomacy/consider.pcx": (1024, 768),
    "art/diplomacy/counter.pcx": (1024, 768),
    "art/diplomacy/uparrow.pcx": (26, 24),
    "art/diplomacy/downarrow.pcx": (26, 24),
    "art/advisors/science_ancient.pcx": (1024, 768),
    "art/advisors/science_middle.pcx": (1024, 768),
    "art/advisors/science_industrial_new.pcx": (1024, 768),
    "art/advisors/science_modern.pcx": (1024, 768),
    "art/advisors/domestic.pcx": (1024, 768),
    "art/advisors/dialogbox.pcx": (207, 132),
    "art/advisors/non_required.pcx": (45, 45),
    "art/advisors/advisor_exit.pcx": (100, 50),
    "art/advisors/domesticbutton.pcx": (250, 100),
    "art/advisors/domestic_icons_aux.pcx": (200, 125),
    "art/advisors/domestic_plusminus.pcx": (102, 50),
    "art/advisors/domestic_icons.pcx": (1024, 768),
    "art/advisors/techboxes.pcx": (1000, 1483),
    "art/advisors/wonders_background.pcx": (1024, 768),
    "art/advisors/wondersbox.pcx": (370, 200),
    "art/advisors/wondersboxoverlay.pcx": (370, 200),
    "art/tech chooser/sciencenav.pcx": (198, 200),
    "art/exitbox-backgroundstates.pcx": (216, 48),
    "art/popupborders.pcx": (500, 300),
    "art/x-o_allstates-sprite.pcx": (109, 21),
    "art/pulldownarrows.pcx": (22, 43),
    "art/wonder splash/wonderbackground.pcx": (1024, 768),
    "art/interface/wonderseye.pcx": (128, 145),
    "art/interface/box right color.pcx": (294, 137),
    "art/interface/box right alpha.pcx": (294, 137),
    "art/interface/nextturn states color.pcx": (141, 28),
    "art/interface/nextturn states alpha.pcx": (141, 28),
    "conquests/art/interface/normbuttons.pcx": (257, 320),
    "conquests/art/interface/rolloverbuttons.pcx": (257, 320),
    "conquests/art/interface/highlightedbuttons.pcx": (257, 320),
    "conquests/art/interface/buttonalpha.pcx": (257, 320),
    "art/units/units_32.pcx": (463, 217),
    "conquests/art/units/units_32.pcx": (463, 825),
}
# `info`-style contracts that are not a single PCX per path.
TECH_ICON = (32, 32)
WONDER_SPLASH = (320, 320)
LEADER_FLC = (200, 240)          # every Civ3 leader clip's canvas
LEADER_FRAMES = 7                # our own idle: 6 unique + the FLC ring frame
LEADER_SPEED = 120               # ms per frame (the game plays our value)
CURSOR_FLC = (93, 46)            # selection-ring canvas
CURSOR_FRAMES = 31               # the real clip's count, for the same feel
CURSOR_SPEED = 175
PALETTE_INDICES = list(range(0, 32))   # `ntpNN.pcx`, the team ramps

# ---------------------------------------------------------------- palette

# Index bases. `TEAM` and `GRAY` are the 0..63 window the ntp ramps replace,
# everything from `SKIN` up is fixed art colour, exactly like the original
# engine's split between team palette and unit palette.
I_BG = 255
I_SHADOW = 254
I_TEAM = 2
I_TEAM_N = 30
I_GRAY = 32
I_GRAY_N = 32          # 32..63, the rest of the replaced window
I_SKIN = 64
I_SKIN_N = 16
I_METAL = 80
I_METAL_N = 16
I_WOOD = 96
I_WOOD_N = 16
I_GREEN = 112
I_GREEN_N = 16
I_WATER = 128
I_WATER_N = 16
I_SOIL = 144
I_SOIL_N = 16
I_SNOW = 160
I_SNOW_N = 16
I_FIRE = 176
I_FIRE_N = 8
I_DYE = 184
I_DYE_N = 8
I_BLACK = 192
I_WHITE = 193
I_ICON = 194
I_ICON_N = 60


def _ramp(start, end, n):
    """`n` RGB triples from `start` to `end` (inclusive), left to right."""
    return [tuple(round(a + (b - a) * i / max(1, n - 1)) for a, b in zip(start, end))
            for i in range(n)]


def _build_palette():
    pal = [(0, 0, 0)] * 256
    pal[0] = pal[1] = (128, 128, 128)
    pal[I_BG] = (255, 0, 255)
    pal[I_SHADOW] = (255, 0, 0)
    for i, c in enumerate(_ramp((168, 186, 220), (34, 46, 84), I_TEAM_N)):
        pal[I_TEAM + i] = c
    for i, c in enumerate(_ramp((18, 18, 20), (235, 235, 238), 8)):
        pal[I_GRAY + i] = c
    for i, c in enumerate(_ramp((40, 40, 44), (250, 250, 252), 8)):
        pal[I_GRAY + 8 + i] = c
    for i, c in enumerate(_ramp((48, 48, 52), (255, 255, 255), 8)):
        pal[I_GRAY + 16 + i] = c
    for i, c in enumerate(_ramp((38, 38, 42), (246, 246, 248), 8)):
        pal[I_GRAY + 24 + i] = c
    for i, c in enumerate(_ramp((252, 220, 186), (122, 78, 52), I_SKIN_N)):
        pal[I_SKIN + i] = c
    for i, c in enumerate(_ramp((236, 240, 250), (66, 72, 88), I_METAL_N)):
        pal[I_METAL + i] = c
    for i, c in enumerate(_ramp((226, 188, 132), (66, 40, 22), I_WOOD_N)):
        pal[I_WOOD + i] = c
    for i, c in enumerate(_ramp((178, 210, 128), (20, 54, 26), I_GREEN_N)):
        pal[I_GREEN + i] = c
    for i, c in enumerate(_ramp((176, 232, 236), (14, 46, 104), I_WATER_N)):
        pal[I_WATER + i] = c
    for i, c in enumerate(_ramp((232, 216, 160), (118, 88, 52), I_SOIL_N)):
        pal[I_SOIL + i] = c
    for i, c in enumerate(_ramp((250, 252, 255), (150, 176, 204), I_SNOW_N)):
        pal[I_SNOW + i] = c
    for i, c in enumerate(_ramp((255, 236, 150), (150, 32, 8), I_FIRE_N)):
        pal[I_FIRE + i] = c
    for i, c in enumerate(_ramp((226, 180, 240), (72, 24, 96), I_DYE_N)):
        pal[I_DYE + i] = c
    pal[I_BLACK] = (0, 0, 0)
    pal[I_WHITE] = (255, 255, 255)
    for i in range(I_ICON_N):
        h = (i * 0.61803398875) % 1.0
        s = 0.55 + 0.35 * ((i // 7) % 3) / 2.0
        v = 0.95 - 0.35 * ((i % 7) / 6.0)
        r, g, b = colorsys.hsv_to_rgb(h, s, v)
        pal[I_ICON + i] = (int(r * 255), int(g * 255), int(b * 255))
    return bytes(v for c in pal for v in c)


PAL = _build_palette()
QUANT = figures.Quantizer(PAL)


def seed_of(*parts):
    """A stable 16-bit seed: `hash()` is salted per process, this is not."""
    blob = "|".join(str(p) for p in parts).encode()
    return int.from_bytes(hashlib.sha256(blob).digest()[:2], "big")


def grey_index(level, n=I_GRAY_N):
    return I_GRAY + min(n - 1, max(0, int(level)))


# ---------------------------------------------------------------- drawing

class Tree:
    """The output root, with path guards and the shared writers."""

    def __init__(self, out):
        self.out = out
        self.files = []

    def path(self, rel):
        p = self.out / rel
        if not p.resolve().is_relative_to(self.out.resolve()):
            raise ValueError(f"asset reference escapes output tree: {rel}")
        return p

    def write(self, rel, data):
        p = self.path(rel)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        self.files.append(p)

    def link(self, rel, target):
        if rel.lower() == target.lower():       # the same file on a case-blind disk
            return
        p = self.path(rel)
        p.parent.mkdir(parents=True, exist_ok=True)
        if p.exists() or p.is_symlink():
            p.unlink()
        p.symlink_to(os.path.relpath(self.out / target, p.parent))
        self.files.append(p)

    def pcx(self, rel, size, draw=None, palette=None, bg=I_BG):
        im = Image.new("P", size, bg)
        im.putpalette(palette or PAL)
        if draw is not None:
            draw(ImageDraw.Draw(im), im)
        want = DIMS.get(rel.lower())
        if want and tuple(im.size) != tuple(want):
            raise SystemExit(f"{rel}: drew {im.size}, the contract is {want}")
        p = self.path(rel)
        p.parent.mkdir(parents=True, exist_ok=True)
        im.save(p, format="PCX")
        self.files.append(p)

    def indexed(self, rel, im):
        """Write a prepared 'P' image (see `openart.gfx.to_indexed`) as PCX."""
        want = DIMS.get(rel.lower())
        if want and tuple(im.size) != tuple(want):
            raise SystemExit(f"{rel}: drew {im.size}, the contract is {want}")
        p = self.path(rel)
        p.parent.mkdir(parents=True, exist_ok=True)
        im.save(p, format="PCX")
        self.files.append(p)

    def dim(self, rel, fallback):
        return DIMS.get(rel.lower(), fallback)


# ---------------------------------------------------------------- FLC writer

def _brun(rows, w):
    """One FLI_BRUN chunk body: a packet count per line, then RLE packets.

    ffmpeg's flic decoder takes the first byte of each line as the number of
    packets that follow (a `count<0x80, value` pair repeating `count` pixels);
    verified by round-tripping a frame through `ffmpeg -i x.flc x_%02d.png`."""
    out = bytearray()
    for row in rows:
        packets = []
        i = 0
        while i < w:
            run = 1
            while i + run < w and row[i + run] == row[i] and run < 127:
                run += 1
            packets.append((run, row[i]))
            i += run
        out += bytes([len(packets)])
        for count, value in packets:
            out += bytes([count, value])
    return bytes(out)


def _lc(prev, cur, w, h):
    """One FLI_LC chunk body: the lines of `cur` that differ from `prev`, as
    (skip, copy | repeat) packets; None when the frame is too busy to gain.

    ffmpeg's decoder reads the first changed line and a line count, then per
    line a packet count and packets of `skip, size` (size > 0 copies that
    many bytes, size < 0 repeats the next byte `-size` times)."""
    if w > 255:
        return None
    rows = [(prev[y * w:(y + 1) * w], cur[y * w:(y + 1) * w]) for y in range(h)]
    changed = [y for y, (a, b) in enumerate(rows) if a != b]
    if not changed:
        return struct.pack("<HH", 0, 0)
    first, count = changed[0], changed[-1] - changed[0] + 1
    out = bytearray(struct.pack("<HH", first, count))
    for a, b in rows[first:first + count]:
        spans = []
        x = 0
        while x < w:
            if a[x] == b[x]:
                x += 1
                continue
            end = x
            while end < w and (a[end] != b[end] or (end + 1 < w and a[end + 1] != b[end + 1])
                               or (end + 2 < w and a[end + 2] != b[end + 2])):
                end += 1
            spans.append((x, end))
            x = end
        packets = bytearray()
        n = 0
        at = 0
        for x0, x1 in spans:
            i = x0
            while i < x1:
                run = 1
                while i + run < x1 and b[i + run] == b[i] and run < 127:
                    run += 1
                if run >= 4:
                    packets += bytes([i - at, 256 - run, b[i]])
                    i += run
                else:
                    j = i + run
                    while j < x1 and j - i < 127 and not (j + 3 < x1 and b[j] == b[j + 1] == b[j + 2] == b[j + 3]):
                        j += 1
                    packets += bytes([i - at, j - i]) + b[i:j]
                    i = j
                at = i
                n += 1
        if n > 255:
            return None
        out += bytes([n]) + packets
    return bytes(out)


def flc(frame_bytes, w, h, speed, pal=None):
    """A one-palette FLC whose frames are raw `w*h` index buffers.

    Frame 0 carries the FLI_COLOR256 chunk; every frame carries one chunk,
    FLI_BRUN (whole picture) or, when smaller, FLI_LC (only what changed from
    the frame before). The header's speed dword at offset 16 is what
    `prep_assets.flc_ms` and the game's clip speed read."""
    chunks = []
    prev = None
    for n, buf in enumerate(frame_bytes):
        rows = [buf[y * w:(y + 1) * w] for y in range(h)]
        body, kind = _brun(rows, w), 15
        if prev is not None:
            lc = _lc(prev, buf, w, h)
            if lc is not None and len(lc) < len(body):
                body, kind = lc, 12
        if len(body) % 2:
            body += b"\0"
        block = struct.pack("<IH", 6 + len(body), kind) + body
        count = 1
        if n == 0:
            block += struct.pack("<IH", 6 + 4 + 768, 4) + struct.pack("<HBB", 1, 0, 0) + (pal or PAL)
            count = 2
        chunks.append(struct.pack("<IHH8x", 16 + len(block), 0xF1FA, count) + block)
        prev = buf
    head = bytearray(128)
    struct.pack_into("<IHHHHHHI", head, 0, 128 + sum(map(len, chunks)), 0xAF12,
                     len(frame_bytes), w, h, 8, 0, speed)
    return bytes(head) + b"".join(chunks)


# ---------------------------------------------------------------- unit art

UNIT_REFS = []          # (entry, name) of every unit the scenarios reference
BUILDINGS = []          # BLDG civilopedia entries in row order

# Animation slots of one unit folder. `SLOT_FRAMES` counts frames per
# direction (default 1); each FLC holds all 8 directions of that many.
WORK_SLOTS = ["BUILD", "ROAD", "MINE", "IRRIGATE", "FORTRESS", "JUNGLE", "FOREST", "PLANT"]
BASE_SLOTS = ["DEFAULT", "RUN", "FORTIFY", "FIDGET", "ATTACK1", "ATTACK2", "ATTACK3",
              "DEFEND", "DEATH", "VICTORY", "CAPTURE"]
SLOT_FRAMES = {"RUN": 2, "ATTACK1": 2, "DEATH": 2, "FORTIFY": 2}
SLOT_MS = {"DEFAULT": 200, "RUN": 100, "FORTIFY": 160, "FIDGET": 200, "ATTACK1": 80,
           "ATTACK2": 90, "ATTACK3": 100, "DEFEND": 150, "DEATH": 130, "VICTORY": 180,
           "CAPTURE": 160}
WORK_MS = {"BUILD": 150, "ROAD": 150, "MINE": 150, "IRRIGATE": 150, "FORTRESS": 180,
           "JUNGLE": 160, "FOREST": 160, "PLANT": 150}

# Ordered keyword rules; the first hit wins.
UNIT_RULES = [
    ("missile", ("cruise missile", "icbm", "tactical nuke", "missile")),
    ("air", ("fighter", "bomber", "jet", "stealth", "helicopter", "zeppelin")),
    ("ship", ("galley", "caravel", "galleon", "frigate", "privateer", "ironclad",
              "transport", "destroyer", "battleship", "cruiser", "submarine", "carrier",
              "dromon", "man-o-war", "canoe", "raft")),
    ("siege", ("catapult", "trebuchet", "cannon", "artillery", "howitzer", "ballista")),
    ("mech", ("mech infantry", "mech", "walker")),
    ("vehicle", ("tank", "armor", "panzer", "vehicle", "wagon")),
    ("mounted", ("horseman", "knight", "cavalry", "chariot", "camel", "conquistador",
                 "rider", "hussar", "elephant", "war elephant", "lancer", "dragoon")),
    ("leader", ("leader", "army")),
    ("civilian", ("settler", "worker", "engineer", "civil")),
]
WEAPON_RULES = [
    ("bow", ("archer", "longbow", "bowman", "slinger")),
    ("gun", ("musket", "rifle", "infantry", "marine", "paratrooper", "guerrilla",
             "partisan", "police", "officer", "explorer", "scout", "sniper", "conquistador",
             "dragoon")),
    ("spear", ("spear", "hoplite", "pike", "phalanx", "halberd", "javelin", "horseman",
               "chariot", "elephant", "lancer")),
    ("sword", ("sword", "legion", "immortal", "crusader", "samurai", "blade", "knight",
               "cavalry", "hussar", "rider", "camel")),
]


def classify(name):
    low = name.lower()
    kind = "foot"
    for k, keys in UNIT_RULES:
        if any(w in low for w in keys):
            kind = k
            break
    weapon = "club"
    for w, keys in WEAPON_RULES:
        if any(k in low for k in keys):
            weapon = w
            break
    if kind in ("ship",):
        weapon = "cannon"
    return kind, weapon


def unit_frame(name, kind, weapon, w, h, d, slot, i, fpd):
    return unitmodels.frame(QUANT, name, kind, weapon, w, h, d, slot, i, fpd)


def unit_clips(tree, name, kind, weapon):
    """Every FLC clip of one unit folder, plus its INI and its sounds."""
    w, h = unitmodels.SIZES[kind]
    slots = list(BASE_SLOTS) + (list(WORK_SLOTS) if kind == "civilian" else [])
    folder = f"Art/Units/{name}"
    stem = re.sub(r"[^A-Za-z0-9_-]", "", name)      # INI files are read as Latin-1
    for slot in slots:
        fpd = SLOT_FRAMES.get(slot, 1)
        frames = []
        for d in range(8):
            for i in range(fpd):
                frames.append(unit_frame(name, kind, weapon, w, h, d, slot, i, fpd))
        speed = WORK_MS[slot] if slot in WORK_MS else SLOT_MS[slot]
        tree.write(f"{folder}/{stem}{slot}.flc", flc(
            [f.tobytes() for f in frames], w, h, speed))
    anims = "\n".join(f"{s}={stem}{s}.flc" for s in slots)
    sounds = _unit_sound_names(kind, weapon)
    effects = "\n".join(f"{s}={sounds[s]}" for s in sorted(effects_slots(sounds)))
    ini = f"[Animations]\n{anims}\n\n[Sound Effects]\n{effects}\n"
    tree.write(f"{folder}/{name}.ini", ini.encode())
    for wav in sorted(set(sounds.values())):
        tree.link(f"{folder}/{wav}", f"Sounds/unit/{wav}")
    for link_name, lib in HARD_SOUNDS.get(name, []):
        tree.link(f"{folder}/{link_name}", f"Sounds/unit/{lib}")
    return slots


def _unit_sound_names(kind, weapon):
    attack = {"bow": "AttackShot.wav", "gun": "AttackShot.wav", "cannon": "AttackBoom.wav"}.get(
        weapon, "AttackSlash.wav")
    out = {"DEFEND": "Defend.wav", "DEATH": "Death.wav", "VICTORY": "Victory.wav",
           "CAPTURE": "Capture.wav", "ATTACK1": attack, "ATTACK2": attack, "ATTACK3": attack}
    if kind == "civilian":
        out.update({"BUILD": "BuildHammer.wav", "ROAD": "ShovelScrape.wav",
                    "MINE": "PickStrike.wav", "IRRIGATE": "ShovelScrape.wav",
                    "FORTRESS": "ShovelScrape.wav", "JUNGLE": "AxeChop.wav",
                    "FOREST": "AxeChop.wav", "PLANT": "ShovelScrape.wav"})
    return out


def effects_slots(sounds):
    return {k: v for k, v in sounds.items() if k in prep.SOUND_SLOTS}


HARD_SOUNDS = {
    "Settler": [("SetRunFoot1.wav", "FootStep.wav"), ("SettlerBuild.wav", "BuildHammer.wav")],
    "Worker": [("WorkRunFoot1.wav", "FootStep.wav"), ("WorkRoadShovelIn.wav", "ShovelScrape.wav"),
               ("WorkMinePickAxe.wav", "PickStrike.wav"), ("WorkForestAxe.wav", "AxeChop.wav"),
               ("WorkIrrigateHoe1.wav", "ShovelScrape.wav")],
    "Warrior": [("WarriorRunFoot1.wav", "FootStep.wav"), ("WarriorFortify.wav", "Defend.wav")],
    "Scout": [("ScoutRunFoot1.wav", "FootStep.wav")],
    "Archer": [("ArchRunFoot1.wav", "FootStep.wav")],
    "Spearman": [("SpearmanRunFoot1.wav", "FootStep.wav")],
    "Horseman": [("HorsemanRunHooves.wav", "HoofBeat.wav")],
}

# ---------------------------------------------------------------- terrain


def terrain(tree):
    # Ground: every cell is a blend of lattice-periodic textures, so the
    # cells join without seams (openart/ground.py). Civ3's own spelling of
    # the file names; lookups elsewhere are case-blind.
    for stem, triple in ground.LAND_SHEETS.items():
        rgb, alpha = ground.blend_sheet(triple, rough=0.4, feather=1.1)
        tree.indexed(f"Art/Terrain/{stem}.pcx", gfx.to_indexed(rgb, alpha))
    # Only wCSO (coast, sea, ocean) is read by the engine; wOOO / wSSS are
    # kept because Civ3 ships them, as plain ocean / sea.
    rgb, alpha = ground.blend_sheet(ground.WATER_SHEET[1], rough=0.28, feather=2.0)
    tree.indexed("Art/Terrain/wCSO.pcx", gfx.to_indexed(rgb, alpha))
    for stem, name in (("wOOO", "ocean"), ("wSSS", "sea")):
        rgb, alpha = ground.uniform_sheet(name, 9, 9)
        tree.indexed(f"Art/Terrain/{stem}.pcx", gfx.to_indexed(rgb, alpha))
    rgb, alpha = ground.uniform_sheet("ice", 8, 4)
    tree.indexed("Art/Terrain/polarICEcaps-final.pcx", gfx.to_indexed(rgb, alpha))

    # Overlays are drawn, not placed: 3D-rendered hills and mountains, lit tree
    # canopies on the engine's cover grid, river spokes, 256 road masks,
    # irrigation, huts and field buildings (openart/overlays, trees, features).
    def put(rel, parts, check=True):
        rgb, alpha, shadow = parts
        if check and tree.dim(rel, rgb.size) != rgb.size:
            raise ValueError(f"{rel}: drew {rgb.size}, the engine expects {tree.dim(rel, rgb.size)}")
        tree.indexed(rel, gfx.to_indexed(rgb, alpha, shadow=shadow))

    put("Art/Terrain/xhills.pcx", overlays.hills())
    put("Art/Terrain/Mountains.pcx", overlays.mountains(False))
    put("Art/Terrain/Mountains-snow.pcx", overlays.mountains(True))
    for stem, pal in (("grassland forests", "grass"), ("plains forests", "plains"), ("tundra forests", "tundra")):
        put(f"Art/Terrain/{stem}.pcx", trees.cover_sheet(pal))
    # Hills and mountains carrying trees: the same terrain with a clump on top.
    put("Art/Terrain/hill forests.pcx", trees.on_sheet(overlays.hills(), 4, 4, 128, 72, "forest", "grass", 9))
    put("Art/Terrain/hill jungle.pcx", trees.on_sheet(overlays.hills(), 4, 4, 128, 72, "jungle", "jungle", 9))
    put("Art/Terrain/mountain forests.pcx", trees.on_sheet(overlays.mountains(False), 4, 4, 128, 88, "forest", "grass", -10))
    put("Art/Terrain/mountain jungles.pcx", trees.on_sheet(overlays.mountains(False), 4, 4, 128, 88, "jungle", "jungle", -10))
    put("Art/Terrain/deltaRivers.pcx", features.rivers("delta"))
    put("Art/Terrain/mtnRivers.pcx", features.rivers("mtn"))
    put("Art/Terrain/roads.pcx", features.roads())
    for stem, soil in (("irrigation", "grass"), ("irrigation PLAINS", "plains"),
                       ("irrigation DESETT", "desert"), ("irrigation TUNDRA", "tundra")):
        put(f"Art/Terrain/{stem}.pcx", features.irrigation(soil))
    put("Art/Terrain/goodyhuts.pcx", features.huts())
    put("Conquests/Art/Terrain/TerrainBuildings.PCX", features.buildings())

    tree.indexed("Art/Terrain/Territory.pcx", features.territory(_territory_palette()))

    def fog_draw(dr, im):
        pal = im.getpalette()
        pal[3 * I_BG:3 * I_BG + 3] = [255, 0, 255]
        px = im.load()
        grey = {0: 0, 1: 153, 2: 255}
        verts = [(64, 0), (128, 32), (64, 64), (0, 32)]   # N, E, S, W
        for col in range(9):
            for row in range(9):
                v0, v3 = col % 3, col // 3
                v1, v2 = row % 3, row // 3
                g = [grey[v0], grey[v1], grey[v2], grey[v3]]
                x0, y0 = col * 128, row * 64
                for y in range(64):
                    for x in range(128):
                        if abs(x - 64) / 64 + abs(y - 32) / 32 > 1:
                            continue
                        num = den = 0.0
                        for (vx, vy), gv in zip(verts, g):
                            d2 = (x - vx) ** 2 + (y - vy) ** 2 + 1.0
                            wgt = 1.0 / d2
                            num += wgt * gv
                            den += wgt
                        v = int(num / den)
                        px[x0 + x, y0 + y] = grey_index(round(v / 255 * (I_GRAY_N - 1)), I_GRAY_N)
        # a grey ramp palette so prep's R-channel read is the blend itself
        pal[3 * I_GRAY:3 * I_GRAY + 3 * I_GRAY_N] = [v for i in range(I_GRAY_N)
                                                     for v in (round(i * 255 / (I_GRAY_N - 1)),) * 3]
        im.putpalette(pal)
    tree.pcx("Art/Terrain/FogOfWar.pcx", tree.dim("Art/Terrain/FogOfWar.pcx", (1152, 576)), fog_draw)


def _territory_palette():
    """Territory's own palette: the loader reads indices 1/64/65/249/252/255."""
    pal = bytearray(PAL)
    for idx, rgb in ((1, (128, 128, 128)), (64, (255, 255, 255)), (65, (200, 200, 200)),
                     (249, (249, 249, 249)), (252, (252, 252, 252)), (255, (255, 0, 255))):
        pal[3 * idx:3 * idx + 3] = bytes(rgb)
    return bytes(pal)


# ---------------------------------------------------------------- cities

def cities(tree):
    """Town/city/metropolis sprites per era and culture, and the walled cities."""
    for group, stem in enumerate(["rAMER", "rEURO", "rROMAN", "rMIDEAST", "rASIAN"]):
        rgb, alpha = cityart.sheet(group)
        tree.indexed(f"Art/Cities/{stem}.PCX", gfx.to_indexed(rgb, alpha))
        rgb, alpha = cityart.wall_sheet(group)
        tree.indexed(f"Art/Cities/{stem[1:]}WALL.PCX", gfx.to_indexed(rgb, alpha))

    def icons(dr, im):
        for i in range(5):
            cx = 6 + i * 11
            dr.rectangle((cx, 6, cx + 6, 16), fill=I_SOIL + 5)
            dr.polygon([(cx - 1, 6), (cx + 3, 1), (cx + 7, 6)], fill=I_WOOD + 5)
    tree.pcx("Art/Cities/city icons.PCX", tree.dim("Art/Cities/city icons.PCX", (58, 20)), icons)


# ---------------------------------------------------------------- city screen

def city_screen(tree):
    for rel, img in (
            ("Art/city screen/XandView.pcx", uisheets.city_xandview()),
            ("Art/city screen/background.pcx", uisheets.city_background()),
            ("Art/city screen/buildings-small.pcx", uisheets.city_buildings(BUILDINGS)),
            ("Art/city screen/CityIcons.pcx", uisheets.city_icons()),
            ("Art/city screen/ProdButton.pcx", uisheets.prod_button()),
            ("Art/city screen/HurryButton.pcx", uisheets.hurry_button()),
            ("Art/city screen/cityMgmtButtons.pcx", uisheets.mgmt_buttons()),
            ("Art/city screen/ProductionQueueBox.pcx", uisheets.queue_box())):
        tree.indexed(rel, screens.indexed(img))
    for stem, h, top in (("TopFadeBar", 24, True), ("BottomFadeBar", 20, False)):
        tree.indexed(f"Art/city screen/{stem}.pcx", screens.indexed(uisheets.fade_bar(h, top, False)))
        tree.indexed(f"Art/city screen/{stem}Alpha.pcx", screens.indexed(uisheets.fade_bar(h, top, True)))


# ---------------------------------------------------------------- UI sheets

def ui(tree):
    def sheet(rel, img):
        tree.indexed(rel, screens.indexed(img))

    sheet("Art/SmallHeads/popHeads.pcx", uisheets.pop_heads())
    for who in ("DOMESTIC", "SCIENCE"):
        sheet(f"Art/SmallHeads/popup{who}.pcx", uisheets.advisor_portraits(who))
    sheet("Art/SmallHeads/advisor_tab.pcx", uisheets.advisor_tabs())

    for era, stem in enumerate(("science_ancient", "science_middle", "science_industrial_new", "science_modern")):
        sheet(f"Art/Advisors/{stem}.pcx", uisheets.advisor_panel(era, 71 + era))
    sheet("Art/Advisors/domestic.pcx", uisheets.advisor_panel(0, 79))
    sheet("Art/Advisors/dialogbox.pcx", uisheets.dialog_box())
    sheet("Art/Advisors/non_required.pcx", uisheets.non_required())
    sheet("Art/Advisors/advisor_EXIT.pcx", uisheets.exit_button())
    sheet("Art/Advisors/domesticBUTTON.pcx", uisheets.govt_button())
    sheet("Art/Advisors/domestic_plusminus.pcx", uisheets.plusminus_small())
    sheet("Art/Advisors/domestic_icons_aux.pcx", uisheets.plusminus_aux())
    sheet("Art/Advisors/domestic_icons.pcx", uisheets.domestic_icons())
    sheet("Art/Advisors/techboxes.pcx", uisheets.techboxes())
    sheet("Art/Advisors/wonders_background.pcx", uisheets.wonders_window())
    sheet("Art/Advisors/wondersBOX.pcx", uisheets.wonders_card(False))
    sheet("Art/Advisors/wondersBOXoverlay.pcx", uisheets.wonders_card(True))
    sheet("Art/Tech Chooser/scienceNAV.pcx", uisheets.science_nav())
    sheet("Art/exitBox-backgroundStates.pcx", uisheets.exitbox())
    sheet("Art/popupborders.pcx", uisheets.popup_borders())
    sheet("Art/X-o_ALLstates-sprite.pcx", uisheets.bullets())
    sheet("Art/pulldownArrows.pcx", uisheets.pulldown())
    sheet("Art/scroll.pcx", uisheets.scroll_parts())
    sheet("Art/interface/wondersEye.pcx", uisheets.wonders_eye())
    for seed, name in enumerate(("talk_offer", "consider", "counter")):
        sheet(f"Art/Diplomacy/{name}.pcx", uisheets.diplomacy_screen(81 + seed * 7))
    sheet("Art/Diplomacy/uparrow.pcx", uisheets.diplomacy_arrow(True))
    sheet("Art/Diplomacy/downarrow.pcx", uisheets.diplomacy_arrow(False))
    sheet("Art/Wonder Splash/wonderBackground.pcx", uisheets.wonder_frame())

    rgb, alpha, shadow = icons.resources(prep.RESOURCES)
    tree.indexed("Art/resources.pcx", gfx.to_indexed(rgb, alpha, shadow=shadow))

    def unit_icons(dr, im, rows):
        """Cell n of the 14-wide sheet shows the n-th referenced unit."""
        units = UNIT_REFS or [("", "Warrior")]
        for r in range(min(rows, im.size[1] // 33)):
            for c in range(14):
                n = r * 14 + c
                _, name = units[n % len(units)]
                kind, weapon = classify(name)
                im.paste(unitmodels.icon(QUANT, name, kind, weapon), (c * 33 + 1, r * 33 + 1))
    tree.pcx("Art/Units/units_32.pcx", tree.dim("Art/Units/units_32.pcx", (463, 217)),
             lambda dr, im: unit_icons(dr, im, 6))
    tree.pcx("Conquests/Art/Units/units_32.pcx",
             tree.dim("Conquests/Art/Units/units_32.pcx", (463, 825)),
             lambda dr, im: unit_icons(dr, im, 25))

    colour, alpha = panels.plaque()
    for stem, img in (("box right color", colour), ("box right alpha", alpha)):
        tree.indexed(f"Art/interface/{stem}.pcx", gfx.to_indexed(img))
    colour, alpha = panels.nextturn()
    for stem, img in (("nextturn states color", colour), ("nextturn states alpha", alpha)):
        tree.indexed(f"Art/interface/{stem}.pcx", gfx.to_indexed(img))

    # Unit action buttons: 8x10 discs with a pictogram per #UNIT_ACTIONS entry,
    # plus the shared disc alpha mask.
    for src, state in (("NormButtons.PCX", "norm"), ("rolloverbuttons.PCX", "over"),
                       ("highlightedbuttons.PCX", "down")):
        tree.indexed(f"Conquests/Art/interface/{src}", gfx.to_indexed(panels.buttons(state)))
    tree.indexed("Conquests/Art/interface/ButtonAlpha.pcx", gfx.to_indexed(panels.button_alpha()))


README = """# Generated source assets

Written by `python3 tools/make_stub_assets.py`. Every pixel, frame and sample
is drawn or synthesised by `tools/openart/`; nothing is read from, traced from
or copied out of a Civ3 install (an install is consulted only for file formats
and sheet geometry). Dedicated to the public domain under CC0-1.0. The font
is the vendored Arimo (OFL, licence beside it).

What is here: ground sheets with blended shorelines, hill, mountain, forest,
river, road and irrigation overlays, resources, one folder of INI and
8-direction FLC clips per unit, cities with walls, the city, advisor,
diplomacy and wonder screens, a picture per advance and wonder, leader clips
(16 archetypes by era; civs share them through links), sound effects and music.

Run a scenario or save with no install:

    CIV3_DIR=/nonexistent cargo run --release -- FILE.biq --assets test-assets

Regenerate (add `--biq FILE.biq`, repeatable, to merge a scenario's names into
`tools/stub_asset_refs.json`; `--clean` removes files the run did not write):

    python3 tools/make_stub_assets.py --clean

Verify the result against the engine's contracts, and against a Civ3 install
to confirm nothing here is a copy; then boot both fixtures on it:

    python3 tools/check_stub_assets.py test-assets --civ3 ../civ3/civ3-gog/app
    tools/smoke_assets.sh test-assets

For hand-made art from real modders, `python3 tools/fetch_community_assets.py`
builds the git-ignored `test-assets-community/` from this tree plus a slice of
a community archive.

Conversion needs Pillow and ffmpeg. Each scenario and source root has its own converted asset cache under
`.cache/<scenario namespace>/`.
"""

# ---------------------------------------------------------------- per-item art

LEADER_STD = re.compile(r"([A-Za-z]+)_([A-Da-d]?)(\d+)\.flc")             # Bs_B01.flc, Bs_B02.flc
LEADER_X = re.compile(r"(?i)(x2?_.+?)[ _]+(?:diplo[ _]+)?(ancient|mid|indust|mod)[ _]+(fwrd|bwrd)\.flc")
LEADER_RANK = {}        # civ file prefix -> archetype slot (set from the leader refs)
LEADER_FILES = {}       # (archetype, era) -> the path written for it


def leader_parts(name):
    """(civ, era 0..3, is the reverse twin) of a leader clip's file name, or
    None. Stock clips are `<civ>_<A-D>01/02`, Conquests civs `x_<name> diplo
    <era> fwrd/bwrd`."""
    m = LEADER_STD.fullmatch(name)
    if m:
        return m.group(1).lower(), uisheets.LEADER_ERA[m.group(2).upper()], m.group(3) != "01"
    m = LEADER_X.fullmatch(name)
    if m:
        return m.group(1).lower(), ("ancient", "mid", "indust", "mod").index(m.group(2).lower()), m.group(3).lower() == "bwrd"
    return None


def leader_clip(tree, rel):
    """One leaderhead: a 200x240 idle FLC with its own palette. Civs share
    16 archetypes by era (the same bytes, so the other paths link to the
    first one written); a reverse twin is a link to its forward clip, which
    `prep_assets.convert_leader` plays as the ping-pong it builds anyway."""
    civ, era, _ = leader_parts(os.path.basename(rel))
    key = (LEADER_RANK[civ] % uisheets.ARCHETYPES, era)
    if key in LEADER_FILES:
        tree.link(rel, LEADER_FILES[key])
        return
    w, h = LEADER_FLC
    frames = uisheets.leader_frames(*key, LEADER_FRAMES - 1)
    pal = uisheets.palette_for(frames)
    data = [f.quantize(palette=pal, dither=Image.Dither.NONE).tobytes() for f in frames]
    data.append(data[0])                # the ring frame prep drops
    tree.write(rel, flc(data, w, h, LEADER_SPEED, bytes(pal.getpalette()[:768])))
    LEADER_FILES[key] = rel
    if key == (0, 0):
        tree.indexed("Art/leaderheads/TO.pcx", frames[0].quantize(colors=256, dither=Image.Dither.NONE))


def cursor_clip(tree):
    """The selection ring: a double ellipse with a bright comet circling it."""
    w, h = CURSOR_FLC

    def ring(phase):
        im = Image.new("P", (w, h), I_BG)
        im.putpalette(PAL)
        dr = ImageDraw.Draw(im)
        dr.ellipse((3, 3, w - 4, h - 4), outline=I_SHADOW, width=1)
        dr.ellipse((5, 5, w - 6, h - 6), outline=I_GRAY + 22, width=1)
        for k in range(14):
            a = phase * TAU / CURSOR_FRAMES - k * 0.07
            px = w / 2 + (w / 2 - 4.5) * math.cos(a)
            py = h / 2 + (h / 2 - 4.5) * math.sin(a)
            level = I_WHITE if k < 3 else I_GRAY + max(8, 30 - k * 2)
            dr.ellipse((px - 1.4, py - 1.4, px + 1.4, py + 1.4), fill=level)
        return im
    frames = [ring(i) for i in range(CURSOR_FRAMES)]
    tree.write("Art/Animations/Cursor/Cursor.flc",
               flc([f.tobytes() for f in frames], w, h, CURSOR_SPEED))


def tech_icon(tree, key):
    """A 32x32 advance icon: one pictogram per advance."""
    tree.indexed(f"Art/Stub/Techs/{key}.pcx", screens.indexed(uisheets.tech_icon(key)))


def wonder_art(tree, key):
    """A 320x320 scene for one wonder (or building) picture."""
    rgb = uisheets.wonder_splash(key, WONDER_SPLASH[0])
    tree.indexed(f"Art/Stub/Wonders/{key}.pcx", rgb.quantize(colors=250, dither=Image.Dither.FLOYDSTEINBERG))


def palettes(tree):
    """Team ramps occupy 0..63; transparency and shadow stay outside them.

    ruleset::team_rgb samples this window for badges and territory, so it
    must contain only team colours and neutral greys, never key colours.
    """
    for n in PALETTE_INDICES:
        pal = bytearray(PAL)
        hue = (n * 0.61803398875 + 0.08) % 1.0
        if n == 0:
            team = [(128, 128, 128)] * 30
        else:
            team = [tuple(int(c * 255) for c in colorsys.hsv_to_rgb(
                hue, 0.65 - 0.35 * (i / 29), 0.95 - 0.45 * (i / 29))) for i in range(30)]
        for i, c in enumerate(team):
            pal[3 * (I_TEAM + i):3 * (I_TEAM + i) + 3] = bytes(c)
        for i in range(I_GRAY_N):
            v = 20 + int(235 * i / (I_GRAY_N - 1))
            pal[3 * (I_GRAY + i):3 * (I_GRAY + i) + 3] = bytes((v, v, v))
        tree.pcx(f"Art/Units/Palettes/ntp{n:02d}.pcx", (1, 1), palette=bytes(pal))


# ---------------------------------------------------------------- audio

RATE = 22050
MUSIC_RATE = 16000


class Noise:
    """A tiny deterministic PRNG for noise bursts (no `random` state)."""

    def __init__(self, seed):
        self.s = seed & 0x7FFFFFFF or 1

    def next(self):
        self.s = (1103515245 * self.s + 12345) & 0x7FFFFFFF
        return self.s / 0x3FFFFFFF - 1.0


def _decay(t, k):
    return math.exp(-k * t)


# name -> (duration, generator). Every generator is a small original DSP
# formula: clicks, thumps, clops, scrapes, pings, sweeps and short chords.
def sfx(name, seed):
    rnd = Noise(seed)
    dur = SFX_DUR.get(name, 0.2)
    n = int(RATE * dur)
    out = []
    for i in range(n):
        t = i / RATE
        v, pan = _sfx_sample(name, t, dur, rnd)
        lg = v * (1.0 - 0.25 * pan)
        rg = v * (1.0 + 0.25 * pan)
        out.append((lg, rg))
    return out


SFX_DUR = {
    "click": 0.06, "ok": 0.14, "cancel": 0.14, "check": 0.18, "enter": 0.22,
    "hut": 0.16, "whattobuild": 0.34, "popup": 0.10, "cityview": 0.55,
    "grid": 0.05, "paper": 0.28, "barbarian": 0.5, "wonder": 0.9,
    "foot": 0.10, "hoof": 0.20, "shovel": 0.32, "pick": 0.22, "axe": 0.26,
    "hammer": 0.24, "slash": 0.22, "shot": 0.20, "boom": 0.34, "defend": 0.26,
    "death": 0.5, "victory": 0.7, "capture": 0.4, "silence": 0.05,
}


def _sfx_sample(name, t, dur, rnd):
    def sine(f, ph=0.0):
        return math.sin(TAU * f * t + ph)
    if name == "silence":
        return 0.0, 0.0
    if name == "click":
        return (0.6 * sine(1400) + 0.4 * rnd.next()) * _decay(t, 70), 0.0
    if name in ("ok", "cancel"):
        f0, f1 = (660, 1180) if name == "ok" else (880, 420)
        f = f0 + (f1 - f0) * min(1.0, t / dur)
        return 0.5 * sine(f) * _decay(t, 9) * min(1.0, t / 0.01), 0.1
    if name == "check":
        return 0.5 * sine(1320) * _decay(t, 14) + 0.25 * sine(1980) * _decay(t, 22), -0.1
    if name == "enter":
        return 0.6 * sine(110) * _decay(t, 8) + 0.25 * rnd.next() * _decay(t, 26), 0.0
    if name == "hut":
        return 0.5 * sine(220 + 180 * min(1.0, t / dur)) * _decay(t, 12) + 0.3 * rnd.next() * _decay(t, 40), 0.2
    if name == "whattobuild":
        note = 523 if t < dur / 2 else 784
        return 0.45 * sine(note) * _decay(t % (dur / 2), 3.5), 0.0
    if name == "popup":
        return 0.4 * sine(880) * _decay(t, 30), -0.2
    if name == "cityview":
        return 0.22 * (sine(392) + sine(494) + sine(587)) * min(1.0, t / 0.06) * _decay(t, 3.2), 0.15
    if name == "grid":
        return 0.35 * sine(2200) * _decay(t, 90), 0.0
    if name == "paper":
        return (0.35 * rnd.next() * _decay(t, 9) * (0.4 + 0.6 * sine(6))), 0.25
    if name == "barbarian":
        base = 80 + 30 * sine(5.5)
        return 0.5 * math.sin(TAU * base * t) * _decay(t, 3) + 0.2 * rnd.next() * _decay(t, 6), -0.3
    if name == "wonder":
        step = int(t / 0.2)
        f = [523, 659, 784, 1047][min(3, step)]
        return 0.4 * (sine(f) + 0.4 * sine(2 * f)) * _decay(t % 0.2, 3.0), 0.2
    if name == "foot":
        return 0.5 * rnd.next() * _decay(t, 45) + 0.35 * sine(90) * _decay(t, 30), 0.0
    if name == "hoof":
        v = 0.0
        for off in (0.0, 0.09):
            if t >= off:
                d = t - off
                v += 0.5 * rnd.next() * _decay(d, 65) + 0.4 * sine(260) * _decay(d, 40)
        return v, 0.0
    if name == "shovel":
        return (0.4 * rnd.next() * _decay(t, 10) * (0.5 + 0.5 * sine(9 + 6 * t)), 0.0)
    if name == "pick":
        return 0.45 * sine(1100 + 500 * t) * _decay(t, 26) + 0.3 * rnd.next() * _decay(t, 55), 0.1
    if name == "axe":
        return (0.55 * rnd.next() * _decay(t, 34) + 0.35 * sine(150) * _decay(t, 20)
                + 0.2 * sine(900) * _decay(t, 60)), -0.1
    if name == "hammer":
        v = 0.0
        for off in (0.0, 0.12):
            if t >= off:
                d = t - off
                v += 0.45 * rnd.next() * _decay(d, 50) + 0.3 * sine(320) * _decay(d, 30)
        return v, 0.15
    if name == "slash":
        return 0.5 * rnd.next() * _decay(t, 16) * (0.6 + 0.4 * sine(14)), -0.2
    if name == "shot":
        return 0.6 * rnd.next() * _decay(t, 30) + 0.3 * sine(180) * _decay(t, 24), 0.05
    if name == "boom":
        return 0.55 * rnd.next() * _decay(t, 12) + 0.4 * sine(70) * _decay(t, 7), 0.0
    if name == "defend":
        return 0.45 * sine(1320) * _decay(t, 16) + 0.35 * sine(880) * _decay(t, 12), -0.15
    if name == "death":
        f = 420 - 240 * min(1.0, t / dur)
        return 0.5 * sine(f) * _decay(t, 4.5) + 0.2 * rnd.next() * _decay(t, 12), 0.0
    if name == "victory":
        step = int(t / (dur / 3))
        f = [523, 659, 784][min(2, step)]
        return 0.42 * (sine(f) + 0.35 * sine(2 * f)) * _decay(t % (dur / 3), 2.5), 0.1
    if name == "capture":
        f = 440 + 880 * min(1.0, t / dur)
        return 0.4 * sine(f) * _decay(t, 5), 0.2
    return 0.0, 0.0


def music(tag, dur=5.0):
    """A short, quiet chord loop: four bars, stereo, deterministic.

    Each of the three music paths is transposed by its tag, so menu, early
    and late peace are audibly distinct."""
    chords = [(220.00, [220.00, 277.18, 329.63]),
              (174.61, [174.61, 220.00, 261.63]),
              (261.63, [261.63, 329.63, 392.00]),
              (196.00, [196.00, 246.94, 293.66])]
    shift = 2 ** ((seed_of(tag) % 11 - 5) / 12.0)
    chords = [(root * shift, [f * shift for f in notes]) for root, notes in chords]
    bar = dur / len(chords)
    n = int(MUSIC_RATE * dur)
    out = []
    for i in range(n):
        t = i / MUSIC_RATE
        ci = min(len(chords) - 1, int(t / bar))
        root, notes = chords[ci]
        tl = t - ci * bar
        env = min(1.0, tl / 0.12) * (0.55 + 0.45 * math.cos(math.pi * tl / bar))
        v = sum(0.14 * math.sin(TAU * f * t + 0.4 * k) for k, f in enumerate(notes))
        v += 0.20 * math.sin(TAU * root / 2 * t)
        v *= env * 0.7
        trem = 1.0 + 0.05 * math.sin(TAU * 3.0 * t)
        out.append((v * trem, v * 0.96 * (1.0 + 0.05 * math.sin(TAU * 3.0 * t + 1.0))))
    return out


def wav(samples, rate):
    a = array("h")
    for l, r in samples:
        a.append(int(max(-1.0, min(1.0, l)) * 30000))
        a.append(int(max(-1.0, min(1.0, r)) * 30000))
    if sys.byteorder == "big":
        a.byteswap()
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(a.tobytes())
    return buf.getvalue()


def audio(tree):
    tree.write("silence.wav", wav(sfx("silence", 1), RATE))
    for name in prep.UI_SOUNDS:
        kind = {
            "Select.wav": "click", "Button OK.wav": "ok", "Button Cancel .wav": "cancel",
            "Check.wav": "check", "EnterTurn.wav": "enter", "Hut.wav": "hut",
            "WhatToBuild.wav": "whattobuild", "PopupInfo.wav": "popup",
            "City View.wav": "cityview", "Grid.wav": "grid", "PaperTurn.wav": "paper",
            "Barbarian Raid.wav": "barbarian", "Wonder.wav": "wonder",
        }[name]
        tree.write(f"Sounds/{name}", wav(sfx(kind, seed_of(kind)), RATE))
    lib = {"FootStep.wav": "foot", "HoofBeat.wav": "hoof", "ShovelScrape.wav": "shovel",
           "PickStrike.wav": "pick", "AxeChop.wav": "axe", "BuildHammer.wav": "hammer",
           "AttackSlash.wav": "slash", "AttackShot.wav": "shot", "AttackBoom.wav": "boom",
           "Defend.wav": "defend", "Death.wav": "death", "Victory.wav": "victory",
           "Capture.wav": "capture"}
    for file, kind in lib.items():
        tree.write(f"Sounds/unit/{file}", wav(sfx(kind, seed_of(kind)), RATE))
    # The MUSIC paths keep their .mp3 names; ffmpeg probes the contents, so
    # real WAV streams convert without a network codec and without ffmpeg here.
    for rel, tag in (("Diplomusic/DipASEarlyPeace.mp3", "peace_early"),
                     ("Diplomusic/DipASLatePeace-2.mp3", "peace_late"),
                     ("Menu/Menu1.mp3", "menu")):
        tree.write(f"Sounds/{rel}", wav(music(tag), MUSIC_RATE))


# ---------------------------------------------------------------- manifest

def load_refs(manifest, biqs):
    """The art references; each BIQ given adds its units, leaders, advances and
    buildings to the committed manifest (the first one also fixes `buildings`,
    the BLDG row order the improvement icon sheet follows)."""
    old = json.loads(manifest.read_text()) if manifest.exists() else {}
    for n, biq in enumerate(biqs):
        result = subprocess.check_output(
            ["cargo", "run", "--release", "--manifest-path",
             str(ROOT / "civ3_utils/biq/Cargo.toml"), "--example", "asset_refs", "--",
             str(biq.resolve())], text=True)
        refs = {k: [] for k in ("unit", "leader", "tech", "wonder")}
        for line in result.splitlines():
            kind, *values = line.split("\t")
            refs[kind].append(values if kind == "unit" else values[0])
        if n == 0:
            old["buildings"] = refs["wonder"]
        for kind, found in refs.items():
            have = old.get(kind, [])
            old[kind] = have + [x for x in found if x not in have]
    if biqs:
        old["sheets"] = old.get("sheets", sheets_map())
        manifest.write_text(json.dumps(old, indent=2) + "\n")
    return json.loads(manifest.read_text())


def sheets_map():
    """The committed dimension/contract map (metadata only, no pixels)."""
    return {"pcx": {k: list(v) for k, v in DIMS.items()},
            "info": {"tech_icon": list(TECH_ICON), "wonder_splash": list(WONDER_SPLASH),
                     "leader_flc": [*LEADER_FLC, LEADER_FRAMES, LEADER_SPEED],
                     "cursor_flc": [*CURSOR_FLC, CURSOR_FRAMES, CURSOR_SPEED],
                     "team_palettes": PALETTE_INDICES,
                     "units_32_grid": [14, 33], "city_cell": [167, 95],
                     "city_icon_grid": [30, 31], "building_icon_grid": [32, 33],
                     "techbox_grid": [4, 16, 189, 93], "tile_diamond": [128, 64]}}


def pedia(refs):
    pedia_lines = []
    for entry, name in refs["unit"]:
        pedia_lines += ["#ANIMNAME_" + entry, name]
    for entry in refs["tech"]:
        pedia_lines += ["#" + entry, f"Art/Stub/Techs/{entry}.pcx"]
    for entry in refs["wonder"]:
        pedia_lines += ["#WON_SPLASH_" + entry, f"Art/Stub/Wonders/{entry}.pcx"]
    return "\n".join(pedia_lines) + "\n"


def font(tree):
    """The vendored Arimo face (OFL, metric-compatible with Arial) stands in
    for the proprietary LSANS; its licence ships beside it. No network."""
    src, licence = font_sources()
    tree.write("LSANS.TTF", src.read_bytes())
    tree.write("FONT-LICENSE.txt", licence.read_bytes())


def font_sources():
    """The vendored Arimo face and its OFL text, checked before anything is
    written (both are required: the licence ships with the font)."""
    src = ROOT / "tools/test-font/Arimo.ttf"
    licence = ROOT / "tools/test-font/OFL.txt"
    for need in (src, licence):
        if not need.exists():
            raise SystemExit(f"missing vendored font file {need} "
                             "(Arimo.ttf + OFL.txt under tools/test-font)")
    return src, licence


# ---------------------------------------------------------------- main

# Files an earlier revision of this generator wrote. test-assets is a
# generated tree, so a regeneration drops them instead of leaving stale
# clips and placeholder sheets behind.
LEGACY = ("placeholder-unit.flc", "placeholder-leader.flc",
          "Art/placeholder-tech.pcx", "Art/placeholder-wonder.pcx",
          "Art/Units/*/placeholder.flc", "Art/Units/*/silence.wav")


def prune(tree):
    for pat in LEGACY:
        for p in tree.out.glob(pat):
            if p.is_file() or p.is_symlink():
                p.unlink()

def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--biq", type=Path, action="append", default=[],
                      help="a scenario whose art references are added to the manifest (repeatable)")
    args.add_argument("--out", type=Path, default=ROOT / "test-assets")
    args.add_argument("--clean", action="store_true",
                      help="delete files under --out that this run did not write")
    opt = args.parse_args()
    font_sources()          # fail before writing anything if the font is absent
    manifest = ROOT / "tools/stub_asset_refs.json"
    refs = load_refs(manifest, opt.biq)
    UNIT_REFS[:] = refs["unit"]
    BUILDINGS[:] = refs.get("buildings", refs["wonder"])
    for key, value in (refs.get("sheets", {}).get("pcx", {}) or {}).items():
        DIMS[key.lower()] = tuple(value)
    tree = Tree(opt.out)
    prune(tree)

    tree.write("README.md", README.encode())
    tree.write("civ3PTW/README.md",
               b"Synthetic search-path directory; source media fall back to the base root.\n")
    terrain(tree)
    cities(tree)
    city_screen(tree)
    ui(tree)
    cursor_clip(tree)
    named = {}              # one spelling per path: a scenario may use two cases
    for r in sorted(refs["leader"]):
        r = r.replace("\\", "/")
        if leader_parts(os.path.basename(r)):
            named.setdefault(r.lower(), r)
    leaders = sorted(named.values(), key=lambda r: (leader_parts(os.path.basename(r))[2], r))
    LEADER_RANK.update({c: i for i, c in enumerate(sorted({leader_parts(os.path.basename(r))[0] for r in leaders}))})
    for rel in leaders:
        leader_clip(tree, rel)
    for entry in refs["tech"]:
        tech_icon(tree, entry)
    for entry in refs["wonder"]:
        wonder_art(tree, entry)
    palettes(tree)
    audio(tree)
    for entry, name in refs["unit"]:
        unit_clips(tree, name, *classify(name))
    tree.write("Text/PediaIcons.txt", pedia(refs).encode())
    tree.write("Text/diplomacy.txt", b"#HELLO\n#random 1\nPlaceholder greeting.\n")
    font(tree)
    if opt.clean:
        keep = {p.parent.resolve() / p.name for p in tree.files}     # a link by its own name
        for p in sorted(opt.out.rglob("*"), reverse=True):
            if p.is_dir() and not p.is_symlink():
                if not any(p.iterdir()):
                    p.rmdir()
            elif p.parent.resolve() / p.name not in keep:
                p.unlink()
    files = [p for p in opt.out.rglob("*") if p.is_file() or p.is_symlink()]
    print(f"{opt.out}: {len(files)} files, "
          f"{sum(p.lstat().st_size for p in files):,} bytes (symlinks counted once)")


if __name__ == "__main__":
    main()
