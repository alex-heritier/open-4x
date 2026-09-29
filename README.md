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
`CIV3_SCRIPT='20:key B;60:city;90:btn Change'` drives the game unattended
(keys, turns, unit placement, city-screen buttons and tiles); the action list
is in `src/script.rs`.
See `.agents/skills/game-screenshotting/SKILL.md` for the wrapper script and
the desktop-capture fallback.

## Controls

- Click: select unit, order move, open city. Right-click: move.
- Unit action buttons float over the bottom of the map, as in Civ3; hovering one names the command
  and its key, unavailable ones are darkened.
- Worker: R road, I irrigate, M mine, C clear forest/jungle. Road to road
  costs 1/3 MP; a mine replaces irrigation and vice versa; workers sharing a
  tile and job pool their turns.
- Arrow keys: step. Tab: cycle units that need orders. F: fortify. Space: skip.
  Units with no moves left cannot be selected; when none need orders the
  selection clears and the bottom-right box blinks its next-turn prompt.
- B: found city with the settler. Enter, the next-turn disc, or the
  bottom-right box (when it shows the prompt): end turn.
- City screen: click tiles to assign workers (yields show as Civ3's food
  and shield icons; roads, irrigation and mines are drawn; tiles another
  city works are dimmed), Change build (Build now / Queue), Governor, X or
  ESC closes. Idle citizens show as entertainers.
- P: save a window screenshot as `shot-<unix>.png`.
- F9: reveal-all debug toggle.

## Scope

MVP plus the features slice: terrain, movement, fog, settling, food and
shield boxes, Warrior/Settler/Worker production, Tokugawa splash, UI and
unit sounds, the Asian peace music loop, plus goody huts (poppable for
units, maps, or settlers), capturable barbarian camps, and 22 placed
resources with bonus yields, worker improvements (roads, irrigation, mines,
clearing), and city production of units and buildings with a queue. Out of
scope: AI, tech, diplomacy, trade, combat, save/load, minimap.
