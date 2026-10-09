#!/usr/bin/env python3
"""Render every picture in the base pack: SVG scenes -> rsvg-convert -> painterly finishing -> PNG.

    python build.py                 # everything
    python build.py terrain ui      # only some groups (terrain | overlays | relief | sprites | units | ui | icons)

Writes PNGs into ../assets/packs/base and the vector sources into ./svg.
Needs: python3 + numpy + pillow (see requirements.txt) and `rsvg-convert` (librsvg) on PATH.
"""

import json
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor

from PIL import Image

from forge import cities, fleet, icons, improvements, nature, overlay, relief, rivers, terrain, troops, ui
from forge.paint import paint, render_sprite
from forge.util import PACK, downsample, indexed, save_png, write_svg

# Authoring is at 2x the sizes the game draws: sprites 256x224 (shown 128x112), land units 160x160
# (shown 70x70), ships 256x192 (shown 112x84).  Terrain cells are 192x96 (shown 128x64).


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


def build_sprites():
    sprite("forest", nature.build_forest_svg(), (256, 224), "sprites/forest.png", ss=4, seed=4, radius=1)
    for flavor, build in cities.BUILDERS.items():
        sprite(f"city_{flavor}", build(), (256, 224), f"sprites/city_{flavor}.png", ss=4, seed=3, radius=1)
    # Tile overlays: the same 2:1 diamond as a terrain cell, 256x128 shown at 128x64.
    sprite("farm", improvements.build_farm(), (256, 128), "sprites/farm.png", ss=4, seed=6, radius=1)
    sprite("mine", improvements.build_mine(), (256, 128), "sprites/mine.png", ss=4, seed=7, radius=1, rim=0.5)


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
    jobs = []
    for name in names:
        _, clips = unit_spec(name)
        for clip, (frames, _, _) in clips.items():
            jobs += [(name, clip, k, i) for k in range(8) for i in range(frames)]
    with ProcessPoolExecutor(os.cpu_count() or 4) as pool:
        images = dict(zip(jobs, pool.map(_unit_frame, jobs, chunksize=4)))
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


def flat(svg, w, h, **kw):
    """UI chrome: no oil-paint smoothing or colour grade, just a whisper of grain."""
    kw = {"radius": 0, "grain": 0.04, "strokes": 0.0, "grade_kw": None, "ss": 4, **kw}
    return render_sprite(svg.render(), w, h, **kw)


def build_ui():
    save_png(ui.parchment_texture(512), "ui/parchment.png")
    save_png(ui.textured_panel("navy"), "ui/panel_navy.png")
    save_png(ui.textured_panel("parchment"), "ui/panel_parchment.png")
    save_png(flat(ui.nameplate_svg(), 288, 56), "ui/plate_name.png")
    save_png(flat(ui.button_svg(), 96, 96), "ui/button_brass.png")
    save_png(flat(ui.button_svg(pressed=True), 96, 96), "ui/button_brass_pressed.png")
    save_png(flat(ui.dark_button_svg(), 192, 72), "ui/button_dark.png")
    save_png(flat(ui.bar_frame_svg(), 256, 32), "ui/bar_frame.png")
    save_png(flat(ui.bar_fill_svg("green"), 64, 32), "ui/bar_fill_green.png")
    save_png(flat(ui.select_svg(), 256, 128), "ui/select.png")
    save_png(flat(ui.flag_svg("dawn"), 96, 64), "ui/flag_dawn.png")
    save_png(flat(ui.flag_svg("league"), 96, 64), "ui/flag_league.png")
    save_png(flat(ui.battle_svg(), 160, 160, radius=1, grain=0.10), "ui/battle.png")
    (PACK / "ui" / "nine_slice.json").write_text(json.dumps(ui.NINE_SLICE, indent=2) + "\n")


def build_icons():
    for name, im in icons.render_all().items():
        save_png(im, f"ui/icons/{name}.png")


GROUPS = {
    "terrain": build_terrain,
    "overlays": build_overlays,
    "relief": build_relief,
    "sprites": build_sprites,
    "units": build_units,
    "ui": build_ui,
    "icons": build_icons,
}


def main(argv):
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
