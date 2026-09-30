# Terrain / map graphics

How the generated map becomes pixels: the art-table loader, the sprite
naming scheme (recovered from `0x4C5F9F` + the on-disk inventory), and the
tile-record fields the renderer consumes.

## Art-table loader (verified this session)

`0x4C5F9F..0x4C5FD4` loads a 4x4 table (`esi` steps `0x80` to `0x200`,
`ebp` steps `0x40` to `0x100`), each entry via the generic asset call
`0x598580` with paths built from object fields (`+0x1801C`, `+0x3368`).
`deltaRivers` enters the map view through this table (see `rivers.md`).

## Sprite inventory (`civ3/civ3-gog/app/Art/Terrain/`, 37 files)

| prefix | files | role |
|---|---|---|
| `x` | `xtgc xpgc xdgc xdpc xdgp xggc xhills` | base land tiles (grass/plains/desert/tundra/hills variants) |
| `w` | `wCSO wSSS wOOO` | water (coast/sea/ocean) |
| `l` | none in base — full set in `Conquests/Art/Terrain/`: `lxtgc lxpgc lxdgc lxdpc lxggc lxdgp lwCSO lwOOO lwSSS` | alternate/LM set (expansion terrain) |
| LM extras | `Conquests/Art/Terrain/` only: `LMForests LMHills LMMountains Volcanos(-snow/-forests/-jungles) marsh craters landmark_terrain EditFog x_victory` | landmark/volcano/marsh overlays |
| overlays | `irrigation*.pcx roads.pcx railroads.pcx pollution.pcx goodyhuts.pcx` | improvements |
| forests | `grassland/hill/mountain/plains/tundra forests.pcx`, `hill/mountain jungle.pcx` | cover per base |
| rivers | `deltaRivers mtnRivers waterfalls.pcx` | river segments |
| other | `floodplains FogOfWar polarICEcaps-final StartLoc Territory TerrainBuildings tnt` | misc/markers |

The `x`/`w`/`l` single-letter prefixes match the string table
(`Art\Terrain\xtgc.pcx` at `0x6690B0`, `xpgc`, `xdgc`, `xdpc`, `xdgp`,
`xggc`, `wCSO`, `wSSS`, `wOOO` consecutive in `.rdata`), so the renderer
addresses base tiles as `Art\Terrain\<prefix><base>.pcx`. `mtnRivers` having
no `.text` ref fits a constructed name (`mtn` + `Rivers`), same mechanism.

## Tile fields the renderer reads

From the TILE record reader (`NOTES.md` §13): `byte[+4]` connection mask
(edge continuity, cf. `0x5F1240` test used by the hills pass),
`byte[+5]` (unknown, no vtable accessor), `+8` feature id, `+0x2C`
terrain class word (bits 12..15 = BIQ terrain id via `vfunc 0xC8`).

## Tile-sheet geometry (verified this session by decoding PCX headers + pixels)

All sheets are 8-bit single-plane PCX. Diamonds are 128x64 px.
Header dimensions re-verified 2026-09-29 via PCX header bytes 4..12
(`xmin,ymin,xmax,ymax`): `xtgc`/`wCSO` 1152x576, `deltaRivers` 512x256,
`roads` 2048x1024, `irrigation` 512x256, `goodyhuts` 384x192, `FogOfWar`
1152x576, `pollution` 640x320, `railroads` 2048x1088, `waterfalls` 512x64 —
every entry in the table below reproduces byte-exact from disk.

| sheet | size | grid | tiles | reading |
|---|---|---|---|---|
| `x*.pcx` base land, `w*.pcx` water, `FogOfWar.pcx` | 1152x576 | 9x9 | 81 | border-blend transition matrix |
| `irrigation*.pcx` | 512x256 | 4x4 | 16 | edge-mask overlay table (2^4); 6 of 16 tile centers are magenta-transparent empties |
| `roads.pcx` | 2048x1024 | 16x16 | 256 | neighbor-mask table (2^8 — hypothesis, exact power match) |
| `railroads.pcx` | 2048x1088 | 16x17 | 272 | 256 mask tiles + 16 extra row (use unknown) |
| `goodyhuts.pcx` | 384x192 | 3x3 | 9 | 8 hut variants; cell (2,2) is 100 % magenta (empty) |
| `pollution.pcx` | 640x320 | 5x5 | 25 | variants |
| `delta/mtnRivers.pcx` | 512x256 | 4x4 | 16 | river segments: 16 = 2^4 edge-mask table, matching the `RiverMask` nibble hypothesis in `rivers.md` |
| `waterfalls.pcx` | 512x64 | 4x1 | 4 | fall variants |
| `Mountains*.pcx` | 512x352 | — | — | taller than 64 px rows: peaks overlap the tile above, not a flat grid |
| `* forests.pcx` | 1000x884 / 512x288 | — | — | variable-size cover sprites, not grid tiles |

Pixel check: `xtgc` tile centers cluster in one green family (dominant
`#9c9c39`, 21/81 tiles), `wCSO` in teals — each sheet blends one terrain
with its neighbours across the 81 cells. The `x` filename letters
(`xdgc xdgp xdpc xggc xpgc xtgc`, alphabet `{c,d,g,p,t}`) name the pair or
triple being blended; exact letter-to-terrain mapping is inference.

## Fog of war sheet (`FogOfWar.pcx`, verified this session)

Same 9x9 geometry and the same per-vertex addressing as the terrain blend
sheets (`col = 3*W + N`, `row = 3*S + E`), but the four vertex digits are
fog states: never seen, remembered, lit. Decoding all 81 cells, the corner
pixels of a digit are 0 / 153 / 255-and-magenta respectively, so the gray
is how much terrain survives at that pixel and a cell fades lit terrain
through remembered into black across one tile. A remembered tile keeps
60 % of its brightness (153/255; a game screenshot measures 0.575-0.6 of
the same terrain art against 0.95-1.0 for lit tiles). `civ3-clone` renders
it as `gen/terrain/fog.png` and picks the cell in `render::fog_cell`.

## The map-view renderer class (verified this session)

Vtable `0x66A508`, 73 slots, installed by three constructors (`0x4C2F5B`,
`0x4C7AFF` with SEH frame, map-side `0x5DC3C4` which then calls `0x4C7A70`).
Object is large (fields out to at least `+0x3EB0`).

| slots | contents |
|---|---|
| `0x00–0x08` | `0x414910/30/40` file hooks |
| `0x0C` | `0x4C7AC0` |
| `0x10`, `0x18–0x4C` | fifteen `0x5Fxxxx` mapgen functions **reused as view methods** (`0x5F9930`; then `0x5F4990`, `0x5F71E0`, `0x5F6370`, `0x5F7150`, `0x5F4BE0`, `0x5F61A0`, `0x5F6B20`, `0x5F6E70`, `0x5F6950`, `0x5F53D0`, `0x5F6780`, `0x5F5110`, `0x5F5F80`, `0x5F46E0`) |
| `0x14` | `0x4C3170` (view method breaking the `0x5F` run) |
| `0x50` | `0x4C31A0` secondary-class wrapper (returns `Cell::0xC4`) |
| `0x54` | `0x4C3210` 500-B tile routine (discards the `0xC4` result; draws via `0x4E3C60`/singleton `0x9F8700`) |
| `0x58–0x110` | view methods incl. `0x4C3880` (slot `0x60`, terrain-id query site `0x4C38B0`), `0x4C5570` (slot `0x8C`) |
| `0x114/0x118` | `0x5F44B0`/`0x5F4520` (more mapgen reuse) |
| `0x11C/0x120` | `0x4C30E0`/`0x4C5A10` (adjacent to art-init `0x4C5BC0`) |

Slot numbers collide with the `Cell` vtable by design (different classes);
a `0xC4`-shaped call on a view object is not `Cell::secondaryClass`. Tile
data always enters through the view object's `+4` cell pointer.

## Cultural border sheet (`Territory.pcx`, verified this session)

One xref: the art-init routine pushes `str.ArtTerrainTerritory.pcx`
(`0x728640`) at `0x4C6A41`. The sheet is 256x288 = **2 columns x 4 rows of
128x72 cells** — the slice loop at `0x4C6A4B` steps x by `0x80` and y by
`0x48` into sprite records of stride `0x2c` at object `+0x1859c`, so the
cell height is 72, not the 64 a plain tile diamond needs. Each cell holds
one tile diamond inset 4 px top and bottom (apex y=4, left and right
vertices y=36, bottom vertex y=68), which puts the diamond's center on the
cell's center: a border sprite needs no anchor offset.

Transparency: right after loading, `0x4C6A8F`..`0x4C6AAA` call the sheet
method `0x5FFED0` with (0, 0xff) and (1, 0xff), i.e. the art makes palette
indices 0 and 1 transparent alongside the purple index 255. Index 1 is the
gray (177,177,177) diamond that fills every cell, so the gray is an
artist's tile template and only the ribbon survives; index 0 is unused.

What survives is a dashed ribbon of beads on a thread, running along one
edge of the cell's diamond. Each cell holds exactly 64 pixels of each of
four colors: yellow `(236,255,0)` index 64 is the bead core, red
`(255,0,0)` 65 the bead's rim, and `(255,163,255)` 249 with
`(255,218,255)` 252 the thread and the bead's soft edge. All four paint in
the **owner's color**, not fixed art: a game screenshot of an orange
(Scandinavian) civ shows orange beads, a darker orange rim of the same
hue, and a nearly neutral dark thread, so 64/65 are the color and its dark
shade while 249/252 are a translucent black.

The dash pattern along an edge repeats every 9 px of arc length (4 pixel
rows): 3 px of thread, then 6 px of bead (rim, core, rim). One tile edge
is 71.5 px, so a border run shows beads roughly every 9 px.

| sheet row | edge of the diamond | map neighbor across it |
|---|---|---|
| 0 | upper-left (left vertex -> apex) | `(x-1, y)` |
| 1 | upper-right (apex -> right vertex) | `(x, y-1)` |
| 2 | lower-left (left vertex -> bottom vertex) | `(x, y+1)` |
| 3 | lower-right (right vertex -> bottom vertex) | `(x+1, y)` |

The mapping follows the clone's `tile_to_world`, which is Bevy world space
with y up: `(x+1, y)` draws at `(+64, -32)`, down and to the right on
screen, `(x, y+1)` down and to the left, and so on, so the neighbor sits
across the edge named in the table. (An earlier revision read the offsets
as y-down screen space and mirrored every row vertically, which left the
outline as scattered dashes.) The ribbon always lies
just **inside** the cell's diamond, which is why the game draws it only on
the owned tile of a border and why a territory outline hugs its tiles from
within.

Column 1 is a second variant of the same four edges whose ribbon leaves
the edge near the upper vertex and bulges into the tile. It is a
hand-drawn arc, not a straight line: the row-0 pixels fit a circle of
radius ~72 centered near the tile's bottom vertex. No screenshot of the
game shows it, and a territory assembled from the straight column
reproduces the game's zigzag at every corner, so what column 1 is for
stays open. `tools/prep_assets.py borders` ships the four straight cells
as `gen/borders/border_{yp,xp,xm,ym}.png` and asserts each ribbon lands in
the quadrant its name claims.

## Blending (owned by [`blending.md`](blending.md))

Display and seamless blending — sprite addressing `(sheet*81+cell)*44`,
the neighbor-mask builder, painter's order, context variants — live in
[`blending.md`](blending.md) and `rust/src/blend.rs`, not here.

## Reference implementation

`rust/src/graphics.rs`: `terrain_sprite()` filename from `(base, variant)`
following the `x`/`w` table above, overlay stacking order (base < forest <
road < pollution < FogOfWar), river variant selector, sheet-grid constants.
The filename table is observed inventory; the stacking order is inferred
from file roles, not disassembly. The renderer slot map above has no Rust
counterpart (dispatch tables, not algorithms).

`civ3-clone`'s counterpart is `src/borders.rs` (art, the per-edge rule,
the sprite sync) with the plot model in `cities::{culture_level,
culture_radius, territory}` and the prep stage `borders`. The straight
column is what ships.
