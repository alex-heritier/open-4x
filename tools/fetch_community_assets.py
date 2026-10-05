#!/usr/bin/env python3
"""Build a second asset root from community-made Civ3 art, for test fixtures.

`test-assets/` is generated art (tools/make_stub_assets.py), so it has no
hand-drawn terrain, cities or unit animations. This script layers a curated
slice of "The Great Library Reborn" preservation archive (the King Arthur Civ3
Mods Collection on archive.org, 5.9 GiB) over a copy of it, so the renderer
and prep pipeline also run against art that real modders made: other palettes,
other FLC sizes and frame counts, other INI timing and sound references.

    python3 tools/fetch_community_assets.py            # test-assets-community/
    cargo run --release -- "civ3_utils/biq/tests/data/TEST.SAV" --assets test-assets-community

The output is git-ignored and never committed: the archive is fan content that
may carry Firaxis-derived pixels, so only the *script* lives in the repo.
Every overlaid file is listed with its archive path in `PROVENANCE.md` inside
the output. Overlay rules: a community file replaces a generated file of the
same name only when the pixel size matches, so a pack for another game
version cannot break a sheet contract.

Needs 7zz (`brew install sevenzip`) and, for the download, aria2c or curl.
"""
import argparse
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
URL = "https://archive.org/download/king-arthur-civ3-mods-collection/KingArthur_Civ3_Collection.7z"
SIZE = 6_304_237_640
NAME = "KingArthur_Civ3_Collection.7z"
CACHE = Path(os.environ.get("OPEN4X_COMMUNITY_CACHE", Path.home() / ".cache/open-4x-community"))

TERRAIN = "Civ3 Graphics/Terrain/ARES Terrain/Terrain"
CITIES = "Civ3 Graphics/Terrain/Cities"
TECHS = "Civ3 Graphics/Techs/CIV4_4_CIV3_techsV1/CIV4_4_CIV3_techsV1"
ADVISORS = "Civ3 Graphics/Advisors"

# Scenario unit name -> community unit folder (each holds the unit's INI and
# its FLCs). Chosen to span foot, mounted, siege, ship and vehicle art with
# different frame sizes (64x78 .. 200x200) and clip sets.
UNITS = {
    "Warrior": "01 Ancient Foot/European/Tribal European Warrior",
    "Spearman": "01 Ancient Foot/African/Tribal African Spearman",
    "Legionary": "01 Ancient Foot/Romans_updated/ImperialLegio",
    "Swordsman": "04 Middle Foot/High Medieval English/English Swordsman",
    "Archer": "04 Middle Foot/6. High Medieval - Germans/German Crossbow",
    "Horseman": "02 Ancient Mounted/African Horseman",
    "Musketeer": "07 Industrial Foot/GENERIC CIV/Generic 1750 Grenadier",
    "Galley": "03 Ancient Sea/RTrireme",
    "Frigate": "06 Middle Sea/HeavyFrigate",
    "Artillery": "10 Industrial Vehicles/6inchHowitzer",
    "Mech Infantry": "10 Industrial Vehicles/M3A1",
}
UNIT_KEEP = {".flc", ".ini", ".wav", ".amb"}

# Archive city members that are named for a stock sheet.
CITY_ZIPS = {"Medieval_Castle_by_RedAlert.zip": ("EUROWALL.pcx", "rEURO.pcx"),
             "RomanWALLS.zip": ("RomanWALLS.pcx",)}
CITY_ALIAS = {"rASIAN (2).PCX": "rASIAN.PCX", "RomanWALLS.pcx": "ROMANWALL.PCX",
              "sahara_wall.pcx": "MIDEASTWALL.PCX"}

# Include patterns for `7zz x` (the archive is solid, so one pass).
SUBSET = [f"{TERRAIN}/*", f"{CITIES}/*", f"{TECHS}/*", f"{ADVISORS}/*.pcx"] + \
         [f"Civ3 Units/{folder}/*" for folder in UNITS.values()]


def find_ci(root, rel):
    """`rel` under `root`, matched case-insensitively; None when absent."""
    cur = Path(root)
    for part in Path(rel).parts:
        if not cur.is_dir():
            return None
        cur = next((e for e in cur.iterdir() if e.name.lower() == part.lower()), None)
        if cur is None:
            return None
    return cur


def size_of(path):
    with Image.open(path) as im:
        return im.size


def norm(stem):
    return "".join(c for c in stem.lower() if c.isalnum())


def fetch(archive):
    if archive.exists() and archive.stat().st_size == SIZE:
        return
    archive.parent.mkdir(parents=True, exist_ok=True)
    print(f"downloading {URL} ({SIZE / 2**30:.1f} GiB) to {archive}")
    if shutil.which("aria2c"):
        cmd = ["aria2c", "-x8", "-s8", "-c", "-d", str(archive.parent), "-o", archive.name, URL]
    else:
        cmd = ["curl", "-L", "-C", "-", "-o", str(archive), URL]
    subprocess.run(cmd, check=True)
    if archive.stat().st_size != SIZE:
        sys.exit(f"{archive}: {archive.stat().st_size} bytes, expected {SIZE}")


def extract(archive, into):
    if (into / "Civ3 Units").is_dir() and (into / TERRAIN).is_dir():
        return
    if not shutil.which("7zz"):
        sys.exit("7zz not found (brew install sevenzip)")
    into.mkdir(parents=True, exist_ok=True)
    subprocess.run(["7zz", "x", str(archive), f"-o{into}", "-y", "-bso0", "-bsp0"]
                   + [f"-i!{p}" for p in SUBSET], check=True)


class Overlay:
    def __init__(self, out, source):
        self.out, self.source = out, source
        self.done, self.skipped = [], []

    def image(self, src, rels, note):
        """Replace the first existing `rels` entry that `src` fits in size."""
        want = size_of(src)
        for rel in rels:
            dst = find_ci(self.out, rel)
            if dst is None:
                continue
            have = size_of(dst)
            if have != want:
                self.skipped.append((str(src.relative_to(self.source)), f"{want} is not {have}"))
                return False
            shutil.copyfile(src, dst)
            self.done.append((dst.relative_to(self.out).as_posix(), str(src.relative_to(self.source)), note))
            return True
        self.skipped.append((str(src.relative_to(self.source)), "the engine has no such sheet"))
        return False

    def terrain(self):
        for f in sorted((self.source / TERRAIN).glob("*.[pP][cC][xX]")):
            self.image(f, (f"Art/Terrain/{f.name}", f"Conquests/Art/Terrain/{f.name}"), "terrain")

    def cities(self):
        cities = self.source / CITIES
        members = list(cities.glob("*.[pP][cC][xX]"))
        for zname, wanted in CITY_ZIPS.items():
            with zipfile.ZipFile(cities / zname) as z:
                for member in wanted:
                    members.append(Path(z.extract(member, cities / (zname + "!"))))
        for f in sorted(members):
            self.image(f, (f"Art/Cities/{CITY_ALIAS.get(f.name, f.name)}",), "city")

    def advisors(self):
        for f in sorted((self.source / ADVISORS).glob("*.pcx")):
            self.image(f, (f"Art/SmallHeads/{f.name}",), "advisor popup")

    def techs(self):
        icons = find_ci(self.out, "Art/Stub/Techs")
        slots = {}
        for p in (icons.glob("*.[pP][cC][xX]") if icons else ()):
            slots.setdefault(norm(p.stem.removeprefix("TECH_")), p)
        for f in sorted((self.source / TECHS).glob("*.[pP][cC][xX]")):
            stem, _, size = f.stem.rpartition("-")
            if size.lower() not in ("s", "small"):
                continue
            slot = slots.get(norm(stem))
            if slot is None:
                continue
            self.image(f, (slot.relative_to(self.out).as_posix(),), "tech icon")

    def units(self):
        for name, folder in UNITS.items():
            src = self.source / "Civ3 Units" / folder
            dst = find_ci(self.out, f"Art/Units/{name}")
            if dst is None:
                self.skipped.append((folder, f"the scenario has no unit {name}"))
                continue
            shutil.rmtree(dst)
            dst.mkdir()
            kept = [f for f in sorted(src.iterdir()) if f.suffix.lower() in UNIT_KEEP]
            for f in kept:
                target = dst / (f"{name}.ini" if f.suffix.lower() == ".ini" else f.name)
                shutil.copyfile(f, target)
                self.done.append((target.relative_to(self.out).as_posix(),
                                  str(f.relative_to(self.source)), "unit"))
            if not any(f.suffix.lower() == ".ini" for f in kept):
                # Some packs ship clips only; name the slots from the clips.
                (dst / f"{name}.ini").write_text(ini_for(f.name for f in kept if f.suffix.lower() == ".flc"))
                self.done.append((f"{dst.relative_to(self.out).as_posix()}/{name}.ini",
                                  "(written from the clip names)", "unit"))


SLOT_OF_CLIP = (("attacka", "ATTACK1"), ("attackb", "ATTACK2"), ("attackc", "ATTACK3"),
                ("attack", "ATTACK1"), ("default", "DEFAULT"), ("run", "RUN"),
                ("death", "DEATH"), ("fortify", "FORTIFY"), ("fort", "FORTIFY"),
                ("victory", "VICTORY"), ("fidget", "FIDGET"))


def ini_for(clips):
    """An INI whose [Animations] names each clip by the end of its file name."""
    slots = {}
    for clip in sorted(clips):
        stem = Path(clip).stem.lower()
        for suffix, slot in SLOT_OF_CLIP:
            if stem.endswith(suffix):
                slots.setdefault(slot, clip)
                break
    body = "\n".join(f"{slot}={clip}" for slot, clip in slots.items())
    return f"[Animations]\n{body}\n"


def provenance(out, ov):
    kinds = {}
    for _, _, kind in ov.done:
        kinds[kind] = kinds.get(kind, 0) + 1
    lines = [
        "# Community fixture assets", "",
        "Built by `tools/fetch_community_assets.py` over a copy of `test-assets/`.",
        f"Source: {URL}", "",
        "Third-party fan content (the archive's uploader marks it public domain, but",
        "parts derive from Firaxis art). It is a local test fixture: do not commit or",
        "redistribute this directory.", "",
        "| kind | files |", "|---|---|",
        *[f"| {k} | {n} |" for k, n in sorted(kinds.items())], "",
        "## Overlaid", "", "| file here | archive path |", "|---|---|",
        *[f"| `{dst}` | `{src}` |" for dst, src, _ in ov.done], "",
        "## Skipped", "", "| archive path | why |", "|---|---|",
        *[f"| `{src}` | {why} |" for src, why in ov.skipped], "",
    ]
    (out / "PROVENANCE.md").write_text("\n".join(lines))
    return kinds


def main():
    args = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    args.add_argument("--base", type=Path, default=ROOT / "test-assets", help="generated asset root to start from")
    args.add_argument("--out", type=Path, default=ROOT / "test-assets-community")
    args.add_argument("--archive", type=Path, default=CACHE / NAME, help="local copy of the 7z (downloaded if absent)")
    args.add_argument("--extract-dir", type=Path, default=CACHE / "extract")
    opt = args.parse_args()

    if not opt.base.is_dir():
        sys.exit(f"{opt.base}: run tools/make_stub_assets.py first")
    if opt.out.exists() and not (opt.out / "PROVENANCE.md").exists():
        sys.exit(f"{opt.out} exists and was not made by this script; not replacing it")
    fetch(opt.archive)
    extract(opt.archive, opt.extract_dir)

    shutil.rmtree(opt.out, ignore_errors=True)
    shutil.copytree(opt.base, opt.out, symlinks=True)
    ov = Overlay(opt.out, opt.extract_dir)
    ov.terrain()
    ov.cities()
    ov.advisors()
    ov.techs()
    ov.units()
    kinds = provenance(opt.out, ov)
    print(f"{opt.out}: {len(ov.done)} community files ({', '.join(f'{n} {k}' for k, n in sorted(kinds.items()))}), "
          f"{len(ov.skipped)} skipped; see PROVENANCE.md")


if __name__ == "__main__":
    main()
