# City graphics

## City-view background loader `0x407C30` (verified this session)

2 933-byte-adjacent SEH-prologue function that builds
`art\city view\Backgrounds\<mode>-<size>-SML.pcx`-family names and patches a
local path buffer with one of two art rows:

* default: bytes at `[0x680DDC]` = `RiverFore.pcx` row,
* mode `edx == 9 || edx == 0xA`: bytes at `[0x680DD0]` = `-FP` floodplain row.

On disk (`Art/City View/Backgrounds/`, 48 files = 4 prefixes x 12):
`{D,G,P,T}-H2O-{CoastBack,CoastRight,Harbor,IslandLeft,NOTtheRiver,
RiverBack,RiverFore[-FP]}.pcx` + `{D,G,P,T}-{LRG,MED,SML,UNCLEAR}.pcx`.
Only the `D-` row carries the `-FP` floodplain variant
(`D-H2O-RiverFore-FP.pcx`), consistent with the `0x407C30` mode patch
(`edx == 9 || 0xA` selects the `-FP` art row).

So the background name is `<L>-<terrain/feature>.pcx` where `<L>` is one of
`D/G/P/T` (the city's base-terrain class) and the feature layer is one of
`H2O/CoastBack/CoastRight/Harbor/IslandLeft/NOTtheRiver/RiverBack/
RiverFore[-FP]`. Size classes `LRG/MED/SML` track city size (`UNCLEAR` for
the unclaimed-fog variant); the same art table holds `BLDG_Courthouse …
BLDG_Palace` plus `Harbor.pcx`, `IslandLeft.pcx`, `CoastBack.pcx`,
`CoastRight.pcx` (`NOTES.md` §14.4).

## City screen chrome (`civ3/civ3-gog/app/Art/city screen/`, 22 files)

`background.pcx TopFadeBar(+Alpha) BottomFadeBar(+Alpha) buildings-large.pcx
buildings-small.pcx CityIcons.pcx CityIcons42x42.pcx cityMgmtButtons.pcx
culture-cityUSE.pcx draftButton.pcx governorBack.pcx HurryButton.pcx
luxuryicons_small.pcx Popheadhilite.pcx ProdButton(+LiteUp).pcx
ProductionQueueBar.pcx ProductionQueueBox.pcx queuebase.pcx XandView.pcx`.

`governorBack.pcx` pairs with the `GOVERNOR_WIN` (`0x732BCE0`) and `Governor`
(`0x732D2B0`) strings: the city governor panel.

## Map-view cities (`civ3/civ3-gog/app/Art/Cities/`)

`city icons.pcx` + per-culture sprawl `rAMER/rASIAN/rEURO/rMIDEAST.PCX` and
walls `AMERWALL/ASIANWALL/EUROWALL/MIDEASTWALL.PCX`: map cities are
culture-group + walled/unwalled + size-class sprites. `Buildings/` holds
`IMP-<name>[-Sh].pcx` improvement art with `_Sh` shadows.

## BLDG id→string resolver `0x407070` (verified: region sweep)

`mov eax,[esp+4]; cmp eax,0x4E; ja default; jmp [eax*4+0x4072A4]` — a
79-entry jump table, one case per building id returning its `BLDG_*` art
key (`0x407084` Palace = idx 0, `0x4070B4` Courthouse, ...). Default
(`0x40729C`) returns `BLDG_Empty`. Quirks: holes at idx 29 and 64–73 map
to default; idx 78 duplicates the idx-0 Palace case. 135 direct callers,
all in `0x4070–0x409E` (city-view). Id-enum vs BIQ `BLDG` section order:
unconfirmed.

## Era roads selector `0x408BD3` (verified: region sweep)

Owner index via the `*2105` idiom into `[edx*4+0xA52F8C]`, clamped to
0..3, then `jmp [eax*4+0x408EC0]` pushing era road art
(`ROADS-ANC`/`-REN`/`-IND`/`-MOD`) into the standard
`push 1; push str; mov ecx,0x9C3508; call 0x598580` load at `0x408C08`.
Wall art: `WallFEA` prefix (`0x681654`) + suffix row (`0x68164C`) strcpy
loop at `0x40E134`; era/size-class branches open.

## Reference implementation

`rust/src/graphics.rs`, `city_background()` (mode+feature filename builder,
exact observed inventory) and `CitySize::{Small,Medium,Large}` mapping;
`bldg_key_valid()` (79-entry shape: holes 29/64–73 invalid, idx 78 Palace
duplicate), tested.
