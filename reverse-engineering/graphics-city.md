# City graphics

## City-view background loader `0x407C30` (verified this session)

2 933-byte-adjacent SEH-prologue function that builds
`art\city view\Backgrounds\<mode>-<size>-SML.pcx`-family names and patches a
local path buffer with one of two art rows:

* default: bytes at `[0x680DDC]` = `RiverFore.pcx` row,
* mode `edx == 9 || edx == 0xA`: bytes at `[0x680DD0]` = `-FP` floodplain row.

On disk (`Art/City View/Backgrounds/`): `D-H2O-CoastBack.pcx`,
`D-H2O-CoastRight.pcx`, `D-H2O-Harbor.pcx`, `D-H2O-IslandLeft.pcx`,
`D-H2O-NOTtheRiver.pcx`, `D-H2O-RiverBack.pcx`, `D-H2O-RiverFore-FP.pcx`,
`D-H2O-RiverFore.pcx`, `D-LRG.pcx`, `D-MED.pcx`, `D-SML.pcx`, `D-UNCLEAR.pcx`.

So the background name is `<L>-<terrain/feature>.pcx` where `<L>` is the
graphics-mode prefix (era/culture) and the feature layer is one of
`H2O/CoastBack/CoastRight/Harbor/IslandLeft/NOTtheRiver/RiverBack/
RiverFore[-FP]`. Size classes `LRG/MED/SML` track city size; the same art
table holds `BLDG_Courthouse … BLDG_Palace` plus `Harbor.pcx`,
`IslandLeft.pcx`, `CoastBack.pcx`, `CoastRight.pcx` (`NOTES.md` §14.4).

## City screen chrome (`civ3-gog/app/Art/city screen/`, 22 files)

`background.pcx TopFadeBar(+Alpha) BottomFadeBar(+Alpha) buildings-large.pcx
buildings-small.pcx CityIcons.pcx CityIcons42x42.pcx cityMgmtButtons.pcx
culture-cityUSE.pcx draftButton.pcx governorBack.pcx HurryButton.pcx
luxuryicons_small.pcx Popheadhilite.pcx ProdButton(+LiteUp).pcx
ProductionQueueBar.pcx ProductionQueueBox.pcx queuebase.pcx XandView.pcx`.

`governorBack.pcx` pairs with the `GOVERNOR_WIN` (`0x732BCE0`) and `Governor`
(`0x732D2B0`) strings: the city governor panel.

## Map-view cities (`civ3-gog/app/Art/Cities/`)

`city icons.pcx` + per-culture sprawl `rAMER/rASIAN/rEURO/rMIDEAST.PCX` and
walls `AMERWALL/ASIANWALL/EUROWALL/MIDEASTWALL.PCX`: map cities are
culture-group + walled/unwalled + size-class sprites. `Buildings/` holds
`IMP-<name>[-Sh].pcx` improvement art with `_Sh` shadows.

## Reference implementation

`rust/src/graphics.rs`, `city_background()` (mode+feature filename builder,
exact observed inventory) and `CitySize::{Small,Medium,Large}` mapping.
