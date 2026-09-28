# Unit graphics and animation

## Per-unit directory (`civ3-gog/app/Art/Units/<Name>/`, 80 units)

Each unit is a directory: one `.ini`, one or more `.flc` animations, combat
sounds. `Settler/` (complete example):

```ini
[Animations]
DEFAULT=settDefault.flc   RUN=settRun.flc      DEATH=settDeath.flc
FIDGET=settFidget.flc     BUILD=settBuild.flc  CAPTURE=SettlerCaptured.flc
; ATTACK1/2/3, DEFEND, FORTIFY, ... all empty (non-combat unit)
[Sound Effects]
RUN=SettlerRun.amb  FIDGET=SettlerFidget.wav  BUILD=SettlerBuild.wav
[Speed] Normal Speed=225  Fast Speed=225
```

Slot table (union over units): `BLANK DEFAULT WALK RUN ATTACK1..3 DEFEND
DEATH DEAD FORTIFY FORTIFYHOLD FIDGET VICTORY TURNLEFT TURNRIGHT BUILD ROAD
MINE IRRIGATE FORTRESS CAPTURE STOP_AT_LAST_FRAME Pause* JUNGLE FOREST`.
Empty slot = no file = unit cannot do that action. Combat units fill
`ATTACK1..3/DEFEND`; workers fill `ROAD/MINE/IRRIGATE/FORTRESS/FOREST`.

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

## Reference implementation

`rust/src/graphics.rs`, `UnitAnimSet`: parse the `.ini` slot table
(exact key set above), `has_action()` from slot emptiness, icon path
builder. Tested against the Settler inventory (6 filled / rest empty).
