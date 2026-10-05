#!/usr/bin/env python3
"""Check a generated asset root against the engine's contracts.

    python3 tools/check_stub_assets.py [ROOT]          # default test-assets
    python3 tools/check_stub_assets.py --civ3 civ3/civ3-gog/app test-assets

Checks, all read-only:
  * every sheet in tools/stub_asset_refs.json exists with its pixel size;
  * every PCX decodes and is 8-bit;
  * every unit folder has an INI whose [Animations] files exist, and each
    clip is a valid FLC with a multiple of 8 frames (8 directions);
  * every referenced leader clip is a 200 x 240 FLC;
  * with --civ3 (a Civ3 install): no file here is byte-identical to one
    there, and no PCX shares pixel data with an install PCX, so nothing in
    a generated root is derived from the shipped art.

Exit status 1 when anything fails. Case-insensitive like the engine.
"""
import argparse
import configparser
import hashlib
import json
import struct
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent


def index(root):
    """lower-case relative path -> Path, for the files and links under root."""
    return {p.relative_to(root).as_posix().lower(): p for p in root.rglob("*") if p.is_file()}


def flc_header(path):
    head = path.read_bytes()[:16]
    size, magic, frames, w, h = struct.unpack("<IHHHH", head[:12])
    return magic, frames, w, h


def pixels_of(path):
    im = Image.open(path)
    return hashlib.sha1(im.tobytes()).hexdigest()


def main():
    args = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    args.add_argument("root", nargs="?", type=Path, default=ROOT / "test-assets")
    args.add_argument("--civ3", type=Path, help="a Civ3 install to compare against")
    opt = args.parse_args()
    root = opt.root
    refs = json.loads((ROOT / "tools/stub_asset_refs.json").read_text())
    files = index(root)
    bad, kinds = [], {}

    def fail(msg):
        bad.append(msg)
        kinds.setdefault("copied from Civ3" if ("identical" in msg or "pixels of" in msg) else "contract", []).append(msg)

    for rel, want in refs["sheets"]["pcx"].items():
        path = files.get(rel)
        if path is None:
            fail(f"missing sheet {rel}")
        elif Image.open(path).size != tuple(want):
            fail(f"{rel}: {Image.open(path).size}, the contract is {tuple(want)}")

    pcx = [p for rel, p in files.items() if rel.endswith(".pcx")]
    for p in pcx:
        try:
            im = Image.open(p)
            im.load()
            if im.mode != "P":
                fail(f"{p.relative_to(root)}: mode {im.mode}, not 8-bit")
        except Exception as e:
            fail(f"{p.relative_to(root)}: {e}")

    units = {}
    for rel, path in files.items():
        parts = rel.split("/")
        if len(parts) == 4 and parts[-4:-2] == ["art", "units"] and rel.endswith(".ini"):
            units[parts[2]] = path
    for _, name in refs["unit"]:
        if name.lower() not in units:
            fail(f"unit {name}: no folder with an INI")
    for folder, ini in sorted(units.items()):
        cfg = configparser.ConfigParser(strict=False)
        cfg.optionxform = str
        cfg.read(ini, encoding="latin-1")
        anims = dict(cfg["Animations"]) if "Animations" in cfg else {}
        clips = [v.strip() for v in anims.values() if v.strip()]
        if not anims.get("DEFAULT", "").strip():
            fail(f"{folder}: no DEFAULT clip")
        for clip in clips:
            path = next((p for r, p in files.items() if r == f"art/units/{folder}/{clip.lower()}"), None)
            if path is None:
                fail(f"{folder}: {clip} is missing")
                continue
            magic, frames, w, h = flc_header(path)
            if magic != 0xAF12 or frames % 8:
                fail(f"{folder}/{clip}: magic {magic:#x}, {frames} frames")

    for rel in refs["leader"]:
        rel = rel.replace("\\", "/")
        path = files.get(rel.lower())
        if path is None:
            fail(f"leader clip {rel} is missing")
        elif flc_header(path)[2:] != (200, 240):
            fail(f"{rel}: {flc_header(path)[2:]}, not 200 x 240")

    if opt.civ3:
        shipped, shipped_px = {}, {}
        for p in opt.civ3.rglob("*"):
            if p.is_file() and not p.is_symlink():
                shipped.setdefault(p.stat().st_size, []).append(p)
        for rel, p in files.items():
            if p.stat().st_size in shipped:
                digest = hashlib.sha1(p.read_bytes()).hexdigest()
                for q in shipped[p.stat().st_size]:
                    if hashlib.sha1(q.read_bytes()).hexdigest() == digest:
                        fail(f"{rel} is byte-identical to {q}")
        for q in (q for v in shipped.values() for q in v if q.suffix.lower() == ".pcx"):
            try:
                shipped_px.setdefault(pixels_of(q), q)
            except Exception:
                pass
        for p in pcx:
            twin = shipped_px.get(pixels_of(p))
            if twin:
                fail(f"{p.relative_to(root)} has the pixels of {twin}")

    print(f"{root}: {len(files)} files, {len(pcx)} PCX, {len(units)} unit folders, "
          f"{'FAILED' if bad else 'ok'}")
    for kind, msgs in sorted(kinds.items()):       # a few of each kind
        for msg in msgs[:8]:
            print("  " + msg)
        if len(msgs) > 8:
            print(f"  ... and {len(msgs) - 8} more {kind}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
