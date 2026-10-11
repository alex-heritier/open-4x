#!/usr/bin/env python3
"""Render every picture in the base pack: SVG scenes -> rsvg-convert -> painterly finishing -> PNG.

    python build.py                 # everything
    python build.py terrain roads   # only some groups (terrain | roads | overlays | relief | sprites | units | ships | icons | resources)

Writes PNGs into ../assets/packs/base and the vector sources into ./svg.
Needs: python3 + numpy + pillow (see requirements.txt) and `rsvg-convert` (librsvg) on PATH.
"""

import json
import os
import re
import sys
import time
from concurrent.futures import ProcessPoolExecutor

from PIL import Image

from forge import cities, fleet, icons, improvements, nature, overlay, relief, resources, rivers, roads, terrain, troops
from forge.paint import paint, render_sprite
from forge.util import PACK, downsample, indexed, save_png, write_svg

# Authoring is at 2x the sizes the game draws: sprites 256x224 (shown 128x112), land units 160x160
# (shown 70x70), ships 256x192 (shown 144x108).  Terrain cells are 192x96 (shown 128x64).


def render_jobs(jobs, fn, chunksize, workers=None):
    """Map `fn` over `jobs` on every core, falling back to this process when a pool cannot start
    (a sandbox that forbids the pool's pipes or sockets renders serially instead of crashing)."""
    try:
        with ProcessPoolExecutor(workers or os.cpu_count() or 4) as pool:
            return list(pool.map(fn, jobs, chunksize=chunksize))
    except (OSError, PermissionError) as exc:
        print(f"  (no process pool: {exc}; rendering {len(jobs)} frames serially)", flush=True)
        return [fn(job) for job in jobs]


def sprite(name, svg, size, rel, **kw):
    write_svg(name, svg.render())
    im = render_sprite(svg.render(), *size, **kw)
    save_png(im, rel)
    return im


def build_terrain():
    fields = terrain.Fields()
    save_png(indexed(terrain.build_ground(fields)), "terrain/ground.png")
    save_png(indexed(terrain.build_water(fields)), "terrain/water.png")
    save_png(rivers.build_river_sheet(), "terrain/rivers.png")
    save_png(overlay.fog_sheet(), "terrain/fog.png")
    save_png(overlay.border_sheet(), "terrain/borders.png")


def build_roads():
    """Road and railroad sheets: one cell per combination of joined neighbours, over the terrain."""
    save_png(roads.build_road_sheet(), "terrain/roads.png")
    save_png(roads.build_rail_sheet(), "terrain/railroads.png")


def relief_sprite(layer, rel):
    img = paint(layer, radius=1, grain=0.10, strokes=0.05, grade_kw={"sat": 1.1, "contrast": 1.08, "key": 0.10})
    save_png(downsample(img, 256, 224), rel)


def build_overlays():
    """Cover and relief that sit on top of the ground: pine, jungle, marsh, hills and mountains."""
    sprite("pine", nature.build_pine_svg(), (256, 224), "sprites/pine.png", ss=4, seed=4, radius=1)
    sprite("jungle", nature.build_jungle_svg(), (256, 224), "sprites/jungle.png", ss=4, seed=5, radius=1)
    sprite("marsh", nature.build_marsh_svg(), (256, 224), "sprites/marsh.png", ss=4, seed=6, radius=1)
    build_relief()


def build_relief():
    """Relief and matching surface borders in all climate/vegetation contexts."""
    for kind, base_seed in (("mountain", 3), ("hills", 21)):
        for style, suffix, offset in (("temperate", "", 0), ("arid", "_dry", 1), ("arctic", "_cold", 2)):
            for cover in ("bare", "forest", "jungle"):
                name = f"{kind}{suffix}" + (f"_{cover}" if cover != "bare" else "")
                layer, borders = relief.build_surface(kind, base_seed + offset, style, cover)
                relief_sprite(layer, f"sprites/{name}.png")
                save_png(borders, f"terrain/{name}_borders.png")
                print(f"  {name}", flush=True)


def city_scene(flavor, build):
    """A city skin with the nation's banner raised over it: white cloth, tinted to the owner."""
    svg = build()
    cities.banner(svg, cities.Iso(cities.CX, cities.CY - 64, cities.K), flavor)
    return svg


def build_sprites():
    sprite("forest", nature.build_forest_svg(), (256, 224), "sprites/forest.png", ss=4, seed=4, radius=1)
    build_cities()
    # Tile overlays: the same 2:1 diamond as a terrain cell, 256x128 shown at 128x64.
    farm = sprite("farm", improvements.build_farm(), (256, 128), "sprites/farm.png", ss=4, seed=6, radius=1)
    water = Image.open(PACK / "terrain/water.png").convert("RGBA")  # build `terrain` first
    save_png(improvements.farm_shore_sheet(farm, water), "sprites/farm_shore.png")
    sprite("mine", improvements.build_mine(), (256, 128), "sprites/mine.png", ss=4, seed=7, radius=1, rim=0.5)


def build_cities():
    """The eleven city skins, plus the matching transparent banner each raises over its town."""
    for flavor, build in cities.BUILDERS.items():
        sprite(f"city_{flavor}", city_scene(flavor, build), (256, 224), f"sprites/city_{flavor}.png", ss=4, seed=3, radius=1)
        banner = cities.Svg(256, 224)
        cities.banner(banner, cities.Iso(cities.CX, cities.CY - 64, cities.K), flavor)
        sprite(f"city_{flavor}_banner", banner, (256, 224), f"sprites/city_{flavor}_banner.png", ss=4, seed=3, radius=1)


# ---- unit animation sheets ---------------------------------------------------------------------
# Every unit design has five clips (idle, run, attack, victory, death).  A clip is one sheet: a row per facing in
# the Civ3 strip order (SW, S, SE, E, NE, N, NW, W) and a column per frame.  Land units are 160x160 cells, ships
# 256x192.  The design's static sprite is the first idle frame facing south-west.
UNIT_BUILDERS = {**troops.BUILDERS, **fleet.BUILDERS}
UNIT_SS = 3  # supersampling for the frames; 4 doubles the build time for no visible gain at the drawn size


def unit_spec(name):
    if name in fleet.BUILDERS:
        return (256, 192), fleet.CLIPS
    return (160, 160), troops.CLIPS


# ---- culture variants ---------------------------------------------------------------------------
# Some cultures field a design of their own (a Zulu warrior, a Qing banner-man, a Cossack) whose
# kit and horse differ from the Western base.  Each is written under `<design>_<flavor>` (as
# pack.json's `flavor_sprites`/`flavor_animations` name them) and shares its base design's frame,
# clips and layout, so the client draws it exactly like the base and only the paint differs.
FLAVOR_UNITS = {
    name: split for name in troops.BUILDERS if (split := troops.split_flavor(name)) is not None
}


def _unit_frame(job):
    name, clip, k, i = job
    (w, h), clips = unit_spec(name)
    frames, _, loops = clips[clip]
    # loops sample a whole cycle without repeating its first frame; one-shots run from the first pose to the last
    t = i / frames if loops else i / max(1, frames - 1)
    svg = UNIT_BUILDERS[name](k, clip, t).render()
    return render_sprite(svg, w, h, ss=UNIT_SS, seed=2, radius=1, rim=0.6)


def build_units(names=None):
    names = names or list(UNIT_BUILDERS)
    unknown = [name for name in names if name not in UNIT_BUILDERS]
    if unknown:
        sys.exit(f"unknown unit design {unknown[0]!r}; choose from {', '.join(UNIT_BUILDERS)}")
    jobs = []
    for name in names:
        _, clips = unit_spec(name)
        for clip, (frames, _, _) in clips.items():
            jobs += [(name, clip, k, i) for k in range(8) for i in range(frames)]
    images = dict(zip(jobs, render_jobs(jobs, _unit_frame, 4)))
    for name in names:
        (w, h), clips = unit_spec(name)
        for clip, (frames, _, _) in clips.items():
            sheet = Image.new("RGBA", (w * frames, h * 8), (0, 0, 0, 0))
            for k in range(8):
                for i in range(frames):
                    sheet.paste(images[(name, clip, k, i)], (i * w, k * h))
            # a palette per sheet: indistinguishable at the drawn size and about five times smaller
            save_png(indexed(sheet), f"units/{name}/{clip}.png")
        save_png(images[(name, "idle", 0, 0)], f"sprites/{name}.png")
        print(f"  {name}", flush=True)


def build_flavor_units():
    """Every culture's own infantry and cavalry, written as `<design>_<flavor>` like any design."""
    build_units(list(FLAVOR_UNITS))


def build_icons():
    for name, im in icons.render_all().items():
        save_png(im, f"ui/icons/{name}.png")


def build_resources():
    """One round badge per natural resource, 96x96 shown at 44x44, named by the resource's id."""
    for name in resources.RESOURCES:
        sprite(f"resource_{name}", resources.build(name), (resources.SIZE, resources.SIZE), f"sprites/resources/{name}.png", ss=4, seed=2, radius=1, rim=0.5)


def build_ships():
    """Only the ships: their sheets, and the muzzle tables the client fires a broadside from."""
    build_units(list(fleet.BUILDERS))
    write_muzzles()


def write_muzzles():
    """Put each ship's per-facing muzzle table into pack.json, replacing the one there, line by line so the
    hand-formatted file stays readable."""
    path = PACK / "pack.json"
    text = path.read_text()
    for name in fleet.BUILDERS:
        rows = fleet.muzzle_table(name)
        body = ",\n".join("            " + json.dumps(row, separators=(",", ":")) for row in rows)
        block = f'          "muzzles": [\n{body}\n          ],\n'
        head = re.compile(rf'(      "{re.escape(name)}": {{\n        "sprite": "[^"\n]+",\n        "clips": {{\n          "frame": \[\d+, \d+\],\n)(          "muzzles": \[\n(?:            .*\n)+?          \],\n)?')
        text, n = head.subn(lambda m: m.group(1) + block, text, count=1)
        if n != 1:
            sys.exit(f"pack.json has no visuals.units.{name} block to put muzzles in")
    path.write_text(text)
    print("  muzzles")


GROUPS = {
    "terrain": build_terrain,
    "roads": build_roads,
    "overlays": build_overlays,
    "relief": build_relief,
    "sprites": build_sprites,
    "cities": build_cities,
    "units": build_units,
    "flavor": build_flavor_units,
    "ships": build_ships,
    "icons": build_icons,
    "resources": build_resources,
}


def main(argv):
    # `units NAME...` builds only the named designs (`infantry`, `cavalry_steppe`, ...).
    if argv[:1] == ["units"] and len(argv) > 1:
        build_units(argv[1:])
        return
    wanted = argv or list(GROUPS)
    for name in wanted:
        if name not in GROUPS:
            sys.exit(f"unknown group {name!r}; choose from {', '.join(GROUPS)}")
    for name in wanted:
        t = time.time()
        GROUPS[name]()
        print(f"{name:8s} {time.time() - t:5.1f}s")


if __name__ == "__main__":
    main(sys.argv[1:])
