# Civ3 Clone

A solo Japan sandbox built with Rust Bevy, using art and audio converted
from a local Civilization 3 GOG install. No AI, no tech, no diplomacy:
found Kyoto, explore, pop goody huts, disperse barbarian camps, work
resources, grow cities, end turns. Map-feature placement follows the
reverse-engineered mapgen stages (see `reverse-engineering/NOTES.md`).

## Setup

Prereqs: Rust, Python 3 with PIL, ffmpeg.

1. Point prep at the GOG install (default `../civ3-gog/app`):
   `export CIV3_GOG=/path/to/civ3-gog/app`
2. Convert assets once (writes gitignored `assets/gen/`):
   `python3 tools/prep_assets.py`
3. Run: `cargo run`

`MAP_SEED` overrides the fixed default map seed; `MAP_CENTER=x,y` and
`MAP_ZOOM=<0.35..2.5>` frame the starting camera, `CIV3_REVEAL=1` starts with
fog off and `CIV3_NO_SPLASH=1` skips the greeting.

For testing and debugging, `CIV3_SHOT=out.png CIV3_SHOT_FRAME=120 cargo run`
writes the window to `out.png` and exits; `CIV3_SHOT_FRAME=30,120` (or `{}` in
the path) grabs several frames, and `CIV3_SHOT_KEEP=1` leaves the game open.
See `.agents/skills/game-screenshotting/SKILL.md` for the wrapper script and
the desktop-capture fallback.

## Controls

- Click: select unit, order move, open city. Right-click: move.
- Bottom bar: Civ3's unit action buttons; hovering one names the command
  and its key, unavailable ones are darkened.
- Worker: R road, I irrigate, M mine, C clear forest/jungle.
- Arrow keys: step. Tab: cycle units. F: fortify. Space: skip.
- B: found city with the settler. Enter or End Turn button: end turn.
- City screen: click tiles to assign workers, Change build, X or ESC closes.
- P: save a window screenshot as `shot-<unix>.png`.
- F9: reveal-all debug toggle.

## Scope

MVP plus the features slice: terrain, movement, fog, settling, food and
shield boxes, Warrior/Settler/Worker production, Tokugawa splash, UI and
unit sounds, the Asian peace music loop, plus goody huts (poppable for
units, maps, or settlers), capturable barbarian camps, and 22 placed
resources with bonus yields. Out of scope: AI, tech, diplomacy, trade,
worker improvements, combat, save/load, minimap.
