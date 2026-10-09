#!/usr/bin/env python3
"""Render every picture in the base pack: SVG scenes -> rsvg-convert -> painterly finishing -> PNG.

    python build.py                 # everything
    python build.py terrain ui      # only some groups (terrain | overlays | sprites | ui | icons)

Writes PNGs into ../assets/packs/base and the vector sources into ./svg.
Needs: python3 + numpy + pillow (see requirements.txt) and `rsvg-convert` (librsvg) on PATH.
"""

import json
import sys
import time

from PIL import Image

from forge import cities, icons, improvements, nature, overlay, relief, rivers, ships, terrain, ui, units
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
    # the same relief in three climates: temperate green, arid ochre, cold grey with snow
    for style, suffix, seed in (("temperate", "", 0), ("arid", "_dry", 1), ("arctic", "_cold", 2)):
        relief_sprite(relief.build_mountain_layer(3 + seed, style), f"sprites/mountain{suffix}.png")
        relief_sprite(relief.build_hills_layer(21 + seed, style), f"sprites/hills{suffix}.png")


def build_sprites():
    sprite("forest", nature.build_forest_svg(), (256, 224), "sprites/forest.png", ss=4, seed=4, radius=1)
    for flavor, build in cities.BUILDERS.items():
        sprite(f"city_{flavor}", build(), (256, 224), f"sprites/city_{flavor}.png", ss=4, seed=3, radius=1)
    for name, build in (
        ("infantry", units.build_infantry),
        ("pioneer", units.build_pioneer),
        ("worker", units.build_worker),
        ("cavalry", units.build_cavalry),
        ("artillery", units.build_artillery),
    ):
        sprite(name, build(), (160, 160), f"sprites/{name}.png", ss=4, seed=2, radius=1, rim=0.6)
    sprite("ironclad", units.ironclad(), (256, 192), "sprites/ironclad.png", ss=4, seed=2, radius=1, rim=0.6)
    for name, build in (
        ("transport", ships.transport),
        ("battleship", ships.battleship),
        ("protected-cruiser", ships.protected_cruiser),
        ("torpedo-boat", ships.torpedo_boat),
    ):
        sprite(name, build(), (256, 192), f"sprites/{name}.png", ss=4, seed=2, radius=1, rim=0.6)
    # Tile overlays: the same 2:1 diamond as a terrain cell, 256x128 shown at 128x64.
    sprite("farm", improvements.build_farm(), (256, 128), "sprites/farm.png", ss=4, seed=6, radius=1)
    sprite("mine", improvements.build_mine(), (256, 128), "sprites/mine.png", ss=4, seed=7, radius=1, rim=0.5)


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


GROUPS = {"terrain": build_terrain, "overlays": build_overlays, "sprites": build_sprites, "ui": build_ui, "icons": build_icons}


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
