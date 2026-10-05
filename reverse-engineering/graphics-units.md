# Unit graphics and animation

## Per-unit directory (`civ3/civ3-gog/app/Art/Units/<Name>/`, 77 unit dirs)

77 subdirectories + 3 loose files at `Units/` root (`units_32.pcx`,
`s_Planes.pcx`, `Planes.pcx`). 76 dirs hold one `.ini` (mixed `.ini`/`.INI`
case); `Palettes/` holds only palette files. Each unit dir: one `.ini`, one
or more `.flc` animations, combat sounds. `Settler/settler.ini` (complete,
verified 2026-09-29):

```ini
[Speed]
Normal Speed=225
Fast Speed=225
[Animations]
BLANK=                        ; 30 keys total (full universe below)
DEFAULT=settDefault.flc
WALK=
RUN=settRun.flc
ATTACK1=                      ; empty = Settler cannot attack
ATTACK2=
ATTACK3=
DEFEND=
DEATH=settDeath.flc
DEAD=
FORTIFY=
FORTIFYHOLD=
FIDGET=settFidget.flc
VICTORY=
TURNLEFT=
TURNRIGHT=
BUILD=settBuild.flc
ROAD=
MINE=
IRRIGATE=
FORTRESS=
CAPTURE=SettlerCaptured.flc
STOP_AT_LAST_FRAME=
PauseROAD=                    ; pause-variant keys, always empty in data
PauseMINE=
PauseIRRIGATE=
JUNGLE=
FOREST=
PauseFOREST=
[Timing]                      ; every animation key repeated, all `0.500000`
[Sound Effects]               ; RUN=SettlerRun.amb FIDGET=SettlerFidget.wav
                              ; BUILD=SettlerBuild.wav, rest empty
[Version]
VERSION=1
[Palette]
PALETTE=                      ; empty = default unit palette
```

Slot universe over all 76 inis (case-insensitive scan, verified 2026-09-29):
`ATTACK1 ATTACK2 ATTACK3 BLANK BUILD CAPTURE DEAD DEATH DEFAULT DEFEND
FIDGET FOREST FORTIFY FORTIFYHOLD FORTRESS IRRIGATE JUNGLE MINE PLANT
PauseFOREST PauseIRRIGATE PauseMINE PauseROAD ROAD RUN STOP_AT_LAST_FRAME
TURNLEFT TURNRIGHT VICTORY WALK` (30 keys). Slots filled in at least one
shipped ini: `ATTACK1 ATTACK2 BUILD CAPTURE DEAD DEATH DEFAULT FIDGET FOREST
FORTIFY FORTRESS IRRIGATE JUNGLE MINE PLANT ROAD RUN VICTORY`. Never filled
anywhere: `ATTACK3 DEFEND WALK FORTIFYHOLD BLANK STOP_AT_LAST_FRAME
TURNLEFT TURNRIGHT Pause*` — engine-supported slots the shipped data never
uses. Empty slot = no file = unit cannot do that action. Combat units fill
`ATTACK1/ATTACK2`; workers fill `ROAD/MINE/IRRIGATE/FORTRESS/FOREST/JUNGLE/
PLANT`.

## Engine side (verified this session)

* Action-name strings `FIDGET` (`0x6807FC`), `FORTIFY`, `DEATH`, `ATTACK1..3`
  (`0x680824`...) live in `.data`; `ATTACK1` has 15 `.text` refs
  (`0x49A887`, `0x4B0B06`, `0x4D8F63`...). At `0x4D8F63` the user tests flag
  `0x20010000` via `0x5C1AD0` then indexes a unit record
  (`[esi+0x28]/[esi+0x34]/[esi+0x24]`) — action availability is a bitmask on
  the unit record, consistent with empty `.ini` slots meaning "action absent".
* `art\units\units_32.PCX` + `art\units\s_planes.pcx`, `planes.pcx`
  (`0x732BFBC` region): the 32-px build-queue/map icons, separate from FLCs.
* `Art\Units\Palettes\ntp` (`0x6805A0`): unit palette path prefix.
* `Art\Units\` (`0x732E1D0`) has no full-path `.text` ref: paths are built
  as `Art\Units\<Name>\<file from .ini>`, i.e. the `.ini` is the lookup
  table. `RankView\warrior_{death,victory,scratch,smash}.flc`
  (`0x732D49C`...) are the combat-preview copies.

## FLC animation format (observed sample)

`Settler/settDefault.flc` header: magic `0xAF12`, 120 frames, 30x55 px, 8
bpp, speed 125. Small unit-faced sprites; 476 `.flc` files ship under
`Units/`. Combat example `warrior/`: two attacks (`warriorAttackA.flc` +
`warriorAttackB.flc`, each with `.amb` + Foot/Grunt/Slash/Whoosh wavs),
`warriorDeath/Default/Fidget/Fortify/Run/Victory.flc` — matching the filled
combat slots above.

## FLC direction blocks (verified from the warrior sheet)

Every unit FLC holds 8 direction blocks of `total/8` frames each, in the
order the map draws them: `d0 = S`, `d1 = SE`, `d2 = E`, `d3 = NE`,
`d4 = N`, `d5 = NW`, `d6 = W`, `d7 = SW` — the compass cycle clockwise
from south. Read off `warrior/DEFAULT.flc`: `d1` is the chest-on view
(map-southeast walks straight at the camera), `d5` the back with the
mane over the shoulders, and `d3`/`d7` the right/left profiles
(map-northeast/southwest walk straight across the screen). The four map
cardinals are the 3/4 views, so on screen the block order does *not* run
in even 45-degree steps: picking a block by rounding the isometric
screen angle hands east and north a diagonal neighbour.
`civ3-clone` picks it in `units::facing_for_step` from the compass angle.


## Selection ring (`Art/Animations/Cursor/Cursor.flc`)

The dashed ellipse the game draws under the *selected* unit is not in the
interface art and not in a unit folder: it is `Art/Animations/Cursor/`, a
31 frame FLC on a 93x46 canvas (art inset 1 px) whose dashes crawl around
the ellipse — every frame differs from every other across the whole
ellipse — and `Cursor.ini` gives the timing (175 ms per frame,
`DEFAULT`). Its colors follow the usual art rules: magenta around it and
the red under the dashes is Civ3's shadow, so the ring lands as white
dashes on a translucent dark outline.

Two near-misses worth naming: `Art/CURSOR.PCX` is the mouse cursor sheet,
and the `x_` FLCs beside a unit's clips are full-canvas variants of its
own animations (the Cannon's `x_CannonDefault.flc` is 120x60 against
`CannonDefault.flc`'s 58x46 with the crew drawn in), not selection art.
`civ3-clone` converts the ring in the prep stage `cursor` and loops it in
`units::SelectionRing` / `ring_follow`.

## Reference implementation

`rust/src/graphics.rs`: `UNIT_ANIM_SLOTS` (exact 30-key universe),
`parse_anim_slots()` over `[Animations]`, `has_action()` from slot
emptiness, icon path builder. Tested against the full Settler ini (6 filled
slots) and the never-filled slot set.
