#!/usr/bin/env python3
"""Rewrite the `visuals.units` block of the base pack to format 5, with flavor overrides.

Every design gains a `sprite` next to its `clips`.  Infantry and cavalry additionally list a
`flavor_sprites`/`flavor_animations` design per culture whose people would otherwise march in
another's kit, so a Zulu warrior and a Qing banner-man are drawn from their own art while the
base design stays the Western one.

    .venv/bin/python tools/pack_units.py            # rewrite pack.json in place
    .venv/bin/python tools/pack_units.py --check     # report drift, change nothing
"""

import argparse
import json
import sys
from pathlib import Path

PACK = Path(__file__).resolve().parents[2] / "assets" / "packs" / "base" / "pack.json"

# Which cultures field a design of their own.  The two flavours absent here draw the base design
# (which is cut as a Western kit): a Latin-American army in European kit reads fine beside a
# Latin city, and it keeps the pack from carrying two near-identical sheets.
FLAVOR_VARIANTS = [
    "african",
    "arab",
    "east_asian",
    "native",
    "oceanic",
    "orthodox",
    "south_asian",
    "southeast_asian",
    "steppe",
]
OVERRIDDEN = ("infantry", "cavalry")
FLAVORS = [
    "western",
    "latin",
    "orthodox",
    "arab",
    "east_asian",
    "south_asian",
    "southeast_asian",
    "african",
    "steppe",
    "native",
    "oceanic",
]

# Cell size per design, in the order the art is authored: land units 160x160 (drawn 70x70),
# ships 256x192 (drawn 112x84).  Kept in step with art/build.py's `unit_spec`.
FRAMES = {
    "infantry": [160, 160],
    "cavalry": [160, 160],
    "artillery": [160, 160],
    "worker": [160, 160],
    "pioneer": [160, 160],
    "ironclad": [256, 192],
    "transport": [256, 192],
    "battleship": [256, 192],
    "protected-cruiser": [256, 192],
    "torpedo-boat": [256, 192],
}


def banners():
    """A transparent banner sprite per flavor (see `art/forge/cities.py`)."""
    return {f: f"sprites/city_{f}_banner.png" for f in FLAVORS}


def migrate(units):
    out = {}
    for name, block in units.items():
        # Already moved to the format-5 shape (a sprite plus its clips and flavor overrides)?
        if "sprite" in block:
            visual = {key: value for key, value in block.items() if value}
            clips = visual.get("clips")
            if clips and "frame" not in clips:  # an older layer: clips without a frame
                visual["clips"] = {"frame": FRAMES[name], "clips": clips}
            out[name] = visual
            continue
        visual = {"sprite": f"sprites/{name}.png"}
        # A design's own clips are `{"frame": [w, h], "clips": {...}}`, the same shape as a
        # flavor's, so the client lays a variant out exactly like its base.
        if block.get("clips"):
            visual["clips"] = {"frame": block["frame"], "clips": block["clips"]}
        # Re-running on an already-migrated pack must be a no-op: keep any overrides it already
        # names, and treat a design that keeps them but has lost its `frame` as done.
        if block.get("flavor_sprites"):
            visual["flavor_sprites"] = block["flavor_sprites"]
        if block.get("flavor_animations"):
            visual["flavor_animations"] = block["flavor_animations"]
        if name in OVERRIDDEN and "flavor_animations" not in visual:
            visual["flavor_sprites"] = {f: f"sprites/{name}_{f}.png" for f in FLAVOR_VARIANTS}
            visual["flavor_animations"] = {
                f: {
                    "frame": block["frame"],
                    "clips": {
                        clip: {**sheet, "sheet": f"units/{name}_{f}/{clip}.png"}
                        for clip, sheet in block["clips"].items()
                    },
                }
                for f in FLAVOR_VARIANTS
            }
        out[name] = visual
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit non-zero when pack.json is stale")
    args = ap.parse_args()
    text = PACK.read_text()
    pack = json.loads(text)
    if pack.get("format") != 5:
        sys.exit(f"pack.json is format {pack.get('format')}, expected 5")
    migrated = migrate(pack["visuals"]["units"])
    if args.check:
        if migrated != pack["visuals"]["units"] or pack["visuals"].get("city_banners") != banners():
            sys.exit("visuals.units is out of date; run art/tools/pack_units.py")
        print("visuals.units is up to date")
        return
    pack["visuals"]["units"] = migrated
    pack["visuals"]["city_banners"] = banners()
    PACK.write_text(json.dumps(pack, indent=2) + "\n")
    print(f"wrote {PACK}")


if __name__ == "__main__":
    main()
