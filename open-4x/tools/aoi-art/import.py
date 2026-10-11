#!/usr/bin/env python3
"""Import the nation art of the Civ3 scenario *Age of Imperialism* into the base pack.

In that scenario a nation's "leader head" is its flag: a 200 x 240 picture of the waving cloth
(`Art/Leaderheads/<name>_large.pcx`, and the same cloth as a flic in `Art/Flics/`). This script
finds which picture each of the scenario's nations uses and writes

  assets/packs/base/nations/<nation id>.png     the flag / leader art, 200 x 240
  assets/packs/base/audio/diplomacy.mp3         the music under the diplomacy window

then points `visuals.nations` and `visuals.diplomacy_music` of `pack.json` at them.

Which picture belongs to which nation is read from the scenario itself: `biq-import --art` prints
each player's Civilopedia key (`RACE_...`) and flics, and the scenario's `text/PediaIcons.txt`
maps that key to the Leaderheads picture. A nation whose key names a picture that is not
installed falls back to the first frame of its flic.

Needs Pillow, ffmpeg (for the flic fallback), and cargo (to run biq-import).

    python3 tools/aoi-art/import.py [--aoi DIR] [--music FILE]

`--aoi` is the scenario's folder (default: the Conquests install under `civ3/` at the workspace
root). Nothing here is needed to build or run the game once the files are in the pack.
"""
import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
OPEN_4X = HERE.parent.parent
WORKSPACE = OPEN_4X.parent
PACK = OPEN_4X / "assets" / "packs" / "base"
DEFAULT_AOI = WORKSPACE / "civ3/civ3-gog/app/Conquests/Conquests/AoI"
MUSIC = "Sounds/Diplomusic/DipNAEarlyPeace.mp3"


def find(root: Path, relative: str) -> Path | None:
    """`relative` under `root`, ignoring case and the scenario's backslashes."""
    here = root
    for part in relative.replace("\\", "/").split("/"):
        if not part:
            continue
        match = next((c for c in here.iterdir() if c.name.lower() == part.lower()), None) if here.is_dir() else None
        if match is None:
            return None
        here = match
    return here


def pedia_flags(aoi: Path) -> dict[str, str]:
    """`RACE_x` (lower case) -> the Leaderheads picture `PediaIcons.txt` gives it."""
    flags, key = {}, None
    for line in (aoi / "text" / "PediaIcons.txt").read_text(encoding="latin-1").splitlines():
        line = line.strip()
        if line.startswith("#RACE_"):
            key = line[1:].lower()
        elif key and line.lower().startswith("art\\leaderheads"):
            flags[key] = line
            key = None
        elif line.startswith("#"):
            key = None
    return flags


def first_frame(flic: Path, out: Path) -> None:
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(flic), "-frames:v", "1", str(out)], check=True)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--aoi", type=Path, default=DEFAULT_AOI)
    ap.add_argument("--music", default=MUSIC, help="the music file, relative to --aoi")
    args = ap.parse_args()
    aoi = args.aoi
    biq = aoi.parent / "AoI5.0.biq"
    if not biq.exists():
        sys.exit(f"{biq} not found")

    art = subprocess.run(
        ["cargo", "run", "-q", "--release", "--manifest-path", str(OPEN_4X / "tools/biq-import/Cargo.toml"), "--", str(biq), "--art"],
        check=True, capture_output=True, text=True,
    ).stdout
    nations = json.loads(art)
    flags = pedia_flags(aoi)

    out = PACK / "nations"
    out.mkdir(parents=True, exist_ok=True)
    written = {}
    with tempfile.TemporaryDirectory() as tmp:
        for nation, info in sorted(nations.items()):
            source = find(aoi, flags.get(info["race"].lower(), ""))
            if source is None:
                flic = next((find(aoi, f) for f in info["flics"] if f and find(aoi, f)), None)
                if flic is None:
                    print(f"  {nation}: no art", file=sys.stderr)
                    continue
                source = Path(tmp) / f"{nation}.png"
                first_frame(flic, source)
                print(f"  {nation}: from the first frame of {flic.name}")
            image = Image.open(source).convert("RGBA")
            image.save(out / f"{nation}.png", optimize=True)
            written[nation] = f"nations/{nation}.png"

    music = find(aoi, args.music)
    if music is None:
        sys.exit(f"{args.music} not found under {aoi}")
    shutil.copyfile(music, PACK / "audio" / "diplomacy.mp3")

    # Point the pack at them: replace the block if it is there, else put it after `turn_sound`.
    path = PACK / "pack.json"
    text = path.read_text()
    block = json.dumps(written, indent=6)[:-1] + "    }"
    block = '    "nations": ' + block + ",\n"
    music_line = '    "diplomacy_music": "audio/diplomacy.mp3",\n'
    text = re.sub(r'    "nations": \{.*?\n    \},\n', "", text, flags=re.S)
    text = re.sub(r'    "diplomacy_music": .*\n', "", text)
    anchor = re.search(r'    "turn_sound": .*\n', text)
    text = text[: anchor.end()] + music_line + block + text[anchor.end():]
    json.loads(text)
    path.write_text(text)
    print(f"{len(written)} nations, {music.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
