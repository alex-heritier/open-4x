# Terrain display and blending

How one map tile becomes pixels: the sprite tables, the exact runtime
addressing formula, the neighbor-mask builder, the painter's order that
makes borders seamless, and the context-variant art (mountains, forests,
ice). Descriptive; every verified claim cites its address or its
on-disk measurement. Reference implementation: `rust/src/blend.rs`.

Sibling docs: [`graphics-terrain.md`](graphics-terrain.md) owns the art-table
loader, the full sprite inventory, and the renderer class; this file owns
blending only.

## Result

A tile draws exactly one 128x64 diamond cell, selected as
`table + (sheet*81 + cell)*44`. The cell art already contains its
north-edge blends; south edges are never blended in any cell because the
southern neighbor overdraws them (painter's algorithm, north to south).
Seamlessness is therefore a property of the art (each of the 81 cells is a
hand-blended transition), not of runtime compositing.

## Sprite tables (verified: `0x4C5A55`, `0x4C5B31` loader loops)

At startup the engine dices each sheet via `0x5F7F90` into 44-byte sprite
records (`edi` advances `0x2C` per cell):

| set | sheets (load order) | dice | sheet stride | table base |
|---|---|---|---|---|
| base | `xtgc xpgc xdgc xdpc xdgp xggc wCSO wSSS wOOO` | 9x9 of 128x64 | `0xDEC` (81x44) | view `+0x191A4` |
| LM (Conquests) | `lxtgc ... lwOOO` (same order) | 9x9 of 128x64 | `0xDEC` | view `+0x102D0` |
| hills | `xhills.pcx` (512x288) | 4x4 of 128x72 | — | view `+0xBCC` |
| hill forest | `hill forests.pcx` (512x288) | 4x4 of 128x72 | — | view `+0xFA90` |
| polar ice | `polarICEcaps-final.pcx` (1024x256) | 8x4 of 128x64 | — | view `+0x283C`/`+0x4FE4` |

The `.rdata` path entries sit at a `0x104` (260-byte) stride from `0x6690B0`
to `0x6699D4` (9 base sheets); the LM loop repeats the shape from `0x669BE0`
to `0x66A504`. Hills cells are 72 px tall — 8 px taller than a diamond —
so ridges overlap the tile above; `Mountains.pcx` (512x352) is taller still
for the same reason.

## Runtime addressing (verified: `0x4C3880`)

```asm
; eax = sheetIdx, edx = cellIdx (two caller args, §open below)
lea eax, [eax+eax*8]      ; x9
lea ecx, [edx+eax*8]      ; cellIdx + sheetIdx*72
add eax, ecx              ; sheetIdx*81 + cellIdx
lea edx, [eax+eax*4]      ; x5
lea eax, [eax+edx*2]      ; x11
lea ecx, [edi+eax*4+BASE] ; + (sheetIdx*81+cellIdx)*44
```

`BASE` is `0x191A4` normally, `0x102D0` when `byte[0x9C7340]` is set (LM
tileset switch). `0x5F8130` then draws the 44-byte record. Formula
(`rust/src/blend.rs::sprite_offset`):

```
offset = (sheet*81 + cell) * 44
```

## Blend-mask builder (verified: `0x4C34CA` in the `0x4C3210` tile routine)

Per tile, per direction `edi` in 0..3, the engine fetches the neighbor at
(`x + edi/2 - (edi&1)`, `y + (edi+1)/2 - 1`) — i.e. N, W, E, S — with
per-axis wrap from the `+0x1F0` flags, then:

```
idx  = (nx>>1) + (W>>1)*ny & 0xFFFF      ; halved-coordinate fetch
cell = getCell(idx)                       ; 0x5D16A0
bit  = dwordTable[param*2105]             ; 0xA52EB4, 2105-dword rows
dir  = (cell[0x58] & (1 << bit)) != 0
mask |= dir << edi                        ; [esp+0x54] nibble
```

Out-of-bounds neighbors and a non-positive mode (`ebp <= 0`) force the bit
to 1. The same idiom recurs at `0x4C37C0` (single-neighbor gate for overlay
drawing) and the same 2105-dword row stride appears at `0xA53BC8`
(`rivers.md` overlay gate). `+0x58` is therefore the renderer's per-cell
flag word; its bit assignments are open.

## Painter's order (verified in pixels, `xtgc.pcx`)

SE/SW edge midpoints are interior texture in all 81 cells: no base-sheet
cell ever blends its south half. A tile draws only its north-edge
transitions; the tiles to its south overdraw the seam. Consequences for
clones: draw rows north to south, and a tile needs only its N/W/E
neighbors' classes — never its southern ones — to pick its cell.

Verified edge facts (`xtgc.pcx`, 3x3-median sampling):

* NE-edge art takes exactly 3 states down the columns
  (`e0ddbb` sand / `b5ad42` olive / `84d6ad` water), repeating byte-identical
  every 3 columns: the NE blend depends on `col mod 3`.
* Water appears on north edges only, in 39 of 81 cells
  (NE+NW: 15, NE: 12, NW: 12); the other 42 cells are land-land transitions.
* All 81 cells are unique images (mean inter-cell distance 13–38 levels);
  no stamping or mirroring.

## Context variants: mountains, forests, jungle, snow (verified on disk)

The engine ships one overlay sheet per (relief, cover) pair and selects by
the tile's relief + feature, from a `.data` art-name row (`0x3285C0` ff):

| tile | sheet |
|---|---|
| forest on grassland / plains / tundra | `grassland/plains/tundra forests.pcx` |
| forest on hills | `hill forests.pcx` (512x288, 128x72 cells) |
| forest on mountains | `mountain forests.pcx` |
| jungle on hills / mountains | `hill jungle.pcx` / `mountain jungles.pcx` |
| mountains, plain | `Mountains.pcx` (512x352, peaks overlap above) |
| mountains on tundra (snow) | `Mountains-snow.pcx` |
| volcano variants | `Volcanos(-snow/-forests/-jungles).pcx` |
| LM relief | `LMForests/LMHills/LMMountains.pcx` |
| marsh, craters | `marsh.pcx`, `craters.pcx` |
| irrigation per base | `irrigation(.pcx\| TUNDRA\| PLAINS\| DESETT).pcx` |

So "mountains near forest look different" is not a blend: a mountain tile
carrying the forest feature draws from `mountain forests.pcx` instead of
`Mountains.pcx`, and a mountain on tundra draws `Mountains-snow.pcx`.
The exact selection predicate (which tile fields gate each row) is open;
the rows and their dice geometry are verified.

Unit animation detail (for scale): `settDefault.flc` is 30x55 px —
overlays are tile-sized, units are not.

## Open (do not treat as verified)

1. Which tile fields produce `sheetIdx` (0..8): the `x` filename letters
   (`tgc pgc dgc dpc dgp ggc`) do not name single terrains — five of six
   land sheets share one grass-green center family (`#9c9c39`), `xdpc`
   alone is sand-centered — so the letters encode pairs/contexts, mapping
   unknown.
2. Exact 81-cell row/col semantics: NE art is a function of `col mod 3`
   (3 states); the NW/second-axis encoding is unrecovered. The runtime
   caller computing `(sheetIdx, cellIdx)` was not located (no direct
   callers of `0x4C3880`; no slot-`0x60` dispatch inside the view region).
3. Mountain/forest variant selection predicate.
4. `+0x58` flag-word bit assignments behind table `0xA52EB4`.

## Update: cell selection resolved by pixel measurement (clone session)

Classifying vertex and edge regions of every cell in all nine base sheets
(`xtgc xpgc xdgc xdpc xdgp xggc wCSO wSSS wOOO`) shows the 81 cells are
3^4, not "north edges only": **cell = row*9 + col with `col = 3*W + N`,
`row = 3*S + E`**, where N/E/S/W are the terrain types at the diamond's
four *vertices* (screen up/right/down/left) and each digit indexes the
sheet's triple in filename-letter order (`xtgc` = tundra, grass, coast;
`xpgc` = plains, grass, coast; `xdgc`; `xdpc`; `xdgp` = desert, grass,
plains; `xggc` = grass, grass, coast; `wCSO` = coast, sea, ocean).
Checks: E depends only on `row%3`, S on `row/3`, W on `col/3`, N on
`col%3` (9x9 grids identical across the other axes); the all-same cells
are (0,0), (4,4), (8,8) and match each triple's center colors.

This supersedes the "NE = col mod 3, south never blends" reading in
"Painter's order" above (the south vertices do vary per cell; the
earlier sampling hit magenta padding). The vertex touches four tiles, so
consistent seams need a shared vertex type; the clone (`src/blend.rs`)
uses a priority over the four tiles: water beats land and tundra > desert
> plains > grass on land; on water tiles land counts as coast and
shallower water wins. Sheet choice: a triple containing the tile's own
type that covers the most vertex types (coast weighted 3); missing types
use nearest substitutes. Still open: the engine's real vertex-priority
and sheet-choice rules, `wSSS`/`wOOO` roles (pure sea/ocean variants),
and hills/forest/mountain overlays (not blended here).
