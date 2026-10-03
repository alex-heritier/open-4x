# Random map generation: the oracle and what each stage does

This file is the working record of the map-generator reimplementation
(`rust/src/landmass.rs`, `pipeline.rs`, ...). It supersedes `NOTES.md` where
they disagree: `NOTES.md` was written from static reading, this file from
**running the exe's own generator** and diffing what each stage changed.

## The oracle

`tools/mapgen/oracle.py` boots `Civ3Conquests.exe` under Unicorn (the harness
in `tools/emu`), loads a saved game with the game's own loader (`0x590030`,
which builds the rules tables, the players and the `Map` singleton), then calls
`Map::generate(seed, ret)` (`0x5D16F0`) exactly as the New Game screen does
(`0x48E332`). One generation takes 2 to 8 seconds depending on map size; the
cells are read back from `Map+0x148`.

What was checked before trusting it:

* Re-running `Map::generate` on the loaded `EGYPT.SAV` (a generator-made 100x100
  map, played for some turns) reproduces the save's river masks (74/74 cells),
  continent ids, water depth, terrain image/file bytes and resource ids (189
  resources; one of them one cell away) exactly. The 38 terrain cells that
  differ are forest/jungle/marsh tiles the player cleared, each with an
  overlay bit set. Owner, overlay and feature-plane bits differ for gameplay
  reasons.
* A second `generate` in the same process gives a byte-identical map.
* FPU precision control `0x27F` (the CRT's value) must be set; the harness
  does not run the CRT start-up.
* Post-load `0x5D2150` ("Building trade network...") is stubbed: it takes
  minutes and does not touch the map.

`Map::generate` reads: the seed argument (becomes `Map+0x1EC`), the six option
pairs `Map+0x04..0x30`, size `Map+0x34`, `Map+0x154/0x158/0x168` (height,
start-site radius, width), `Map+0x15C` (civilisation count), wrap flags
`Map+0x1F0` and the rules tables (`WSIZ`, `GOOD`, `TERR`). It writes every byte
of `Cell+0x04..0x38` except the padding at `+0x06`, `+0x07` and `+0x21`; the
rest of the cell is not part of the generator's output.

`Map::setCellCount(n)` (vtable slot 0, `0x5F3CC0`) rebuilds the cell array when
the count changes and otherwise resets every cell to terrain 13; the oracle
uses it to change map size.

## What each stage writes

Measured with `oracle.py` snapshots (80x80, landmass 1, seed 4242). The byte
columns are `Cell` offsets.

| stage (entry) | cells changed | bytes written | what it is |
|---|---|---|---|
| `0x5F1F50` rollRandomOptions | 0 | - | options only |
| `0x5ECEB0` generateLandmass | all | `+0x1E` continent, `+0x2D` | land/sea and continent numbering |
| `0x5ED440` landmassFix | 0 here | - | style 1 only |
| `0x5EEB00` | ~30% | `+0x2D` | water-depth classification (not "deconflictStarts") |
| `0x5EDB70` | ~6% | `+0x2D` | terrain class pass (not only start deserts) |
| `0x5EDDB0` paintContinents | 0 | - | fills an internal region map |
| `0x5F1480` assignBiomes | ~18% | `+0x2D`, `+0x32` | land terrain and feature-plane decoration |
| `0x5ED5D0` | 0 | - | internal (per-continent start data) |
| `0x5F07D0` | 2.5% | `+0x04` river mask, `+0x2D` | **rivers** (and the mountain/hill sources) |
| `0x5EBE80` postProcess | ~99% | `+0x10`, `+0x11` image/file, `+0x2D` | terrain sprite variants |
| `0x5F22A0` placeResources | 4% | `+0x08..0x0B` | resource ids |
| `0x5F21B0` | 0.4% | `+0x28` | goody huts (overlay bit `0x20`) |
| `0x5F2090` | 5% | `+0x32` | feature-plane bit (qty `0x10000` = bonus grassland), not barbarian camps |
| `0x5EEEE0` finalPass | one per civ | `+0x32` | start locations (feature bit `0x80000`) |
| `0x5D3100` contour | a few | `+0x32` | feature-plane bits |

After `Map::generate` every cell has: owner 0, `+0x0C` = -1, packed dword 0,
barbarian/city/colony/victory ids = -1, water depth byte 6, ruin 0, flags
`+0x34` = 0; only river mask, resource, image/file, continent, overlay, terrain
word and feature plane vary.

## Corrections to NOTES.md found while building the oracle

* `word[Map+0x40]` is the **cell count**, not a player count. Every loop in
  `generateMap` bounded by it (`deconflict`, the tidy-up loop at `0x5EB6C3`) is
  a per-cell pass.
* The `Map` singleton is at `0x9C736C` (the save writer calls
  `getCell` with `ecx = 0x9C736C`, `0x597C6D`). `Map+0x148` is a `Cell**`
  array; `Cell` is `0xDC` bytes (`0x5D8AF5`).
* Option fields: `Map+0x04/0x08` climate, `+0x0C/0x10` barbarians, `+0x14/0x18`
  landmass, `+0x1C/0x20` ocean coverage, `+0x24/0x28` temperature, `+0x2C/0x30`
  age (raw / derived), `+0x34` world size. The New Game code copies the UI
  globals `0x99039C..0x9903B8` into them (`0x48E243..0x48E2FA`) and sets width
  and height from the `WSIZ` row (`+0x50`, `+0x44`) and the start-site radius
  from its `+0x48` minus one.
* The `.biq` `TILE` row, the `.sav` per-cell chunks and the generator's output
  are the same `Cell` object: the row is `Cell+0x04..0x38` packed field by
  field, the save dumps `+0x04..0x28`, `+0x28..0x34`, `+0x34..0x38` and
  `+0x58..0xD8` as four chunks.
