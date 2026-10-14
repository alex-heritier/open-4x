# Road and railroad sheets

Roads and railroads are drawn from two atlases, `visuals.roads` and `visuals.railroads` in `pack.json`
(base pack: `terrain/roads.png`, `terrain/railroads.png`). One sprite per tile per kind, chosen by which neighbours the tile
is joined to. Both fields are optional: a pack without them draws no roads. Client: `crates/client/src/roads.rs`.
Art: `art/forge/roads.py` (`python build.py roads`).

The layout follows Civ3's `roads.pcx` and `railroads.pcx`: one cell for every combination of the eight neighbours.

## Cells

* 16 columns, row-major. Cell width is `image_width / 16`; cell height is half of that (a 2:1 diamond). The base pack uses
  192x96 cells (drawn 128x64). At least 16 rows.
* Cells 0-255: the **neighbour mask**. Bit `i` is set when the tile is joined to neighbour `i`, in the order of
  `Coord::NEIGHBORS`: `NE=1, E=2, SE=4, S=8, SW=16, W=32, NW=64, N=128`. These are the same eight directions and the same bit
  order as Civ3's sheet (checked on all 256 cells of its road sheet).
* Cells 256 and up: **variants** of the masks with at most two bits set (37 masks: none, 8 stubs, 28 pairs). Variant `k >= 1`
  of the mask ranked `r` among those 37 (ascending) is cell `256 + (k-1)*37 + r`. The client derives the number of variants
  from the row count: `1 + (rows*16 - 256) / 37`, and picks one per tile by a position hash. Masks with three or more bits have
  one drawing; their junctions are too constrained for variants to matter. The base pack has 4 road and 3 rail variants.
* Mask 0 draws a short lone stretch (a tile that is road or rail but joined to nothing).

## Exits

Every road leaves a cell at one of eight fixed points, all at the same width and heading straight out:

| Bit | Exit | Point (fractions of the cell) |
| --- | --- | --- |
| NE, SE, SW, NW | midpoint of the diamond's side | (3/4, 1/4), (3/4, 3/4), (1/4, 3/4), (1/4, 1/4) |
| E, S, W, N | vertex | (1, 1/2), (1/2, 1), (0, 1/2), (1/2, 0) |

Cells are cut along the diamond's sides, so a road crossing a side is finished by the neighbour. To avoid hairline seams the solid
part of the road overlaps the line by 1.5 px and the translucent halo is cut exactly on it. Wander and swell (the variations
of the road's course and width) fade to zero toward every exit, so any two cells that meet agree at their shared point.

## Drawing

* A road is a set of cubic Bezier routes. The exits of a mask are paired, widest angle first, so two exits at least a right angle
  apart run through the cell as one smooth curve; leftover exits branch off the nearest route tangentially and merge into
  it (Y junctions, not crossings of equal strokes).
* Roads are packed earth with a worn edge and light from the upper left; railroads are a ballast bed, sleepers at an even
  spacing along the curve, and two rails. Junctions in rails are real turnouts (a branch peels off a main line).
* All noise is periodic over the diamond lattice, as in the terrain sheets.

## Client

`roads::draw` runs with the rest of the visible map. For every tile the player knows that has the `ROAD` or `RAIL` bit, or is a
city, it spawns `WorldVisual` sprites at the tile centre, cut from the sheets by the neighbour masks:

* Rail joins two tiles that both have rail (a city counts as rail, as it does for movement). Any other pair of tiles with
  track is joined by road, so a railway that meets a road leaves a road stub toward it. A railroad tile draws no road under
  its rails, only those stubs.
* Only a spanning forest of those links is drawn, so a patch of track never becomes a mesh of crossings and rings: every
  square stays joined to all it can reach, by the fewest links. Rail links are taken first (the railways are the trunk
  lines), then links on thin lines before links inside a thick patch (counted by the squares with track that touch both
  ends), then a scatter of the position. The forest is built over the whole map, so a square's picture does not change as
  the camera moves.
* Squares the player does not know are never linked, so fog does not reveal roads.
* A city draws only the stubs that reach it, not a lone piece of track on its own ground.
* Z: roads 3.0 (over farms, under relief), rails 3.05.

A pack may use any resolution and any number of variants as long as the contract above holds. `roads::Sheet::cut` rejects sheets
with fewer than 16 rows. Tests in `roads.rs` cover the bit order against the screen directions, the links and cells, the variant
indexing, and the header sizes of the base pack's sheets; `art/tests/test_roads.py` covers the cell layout and that neighbouring
cells join without a gap, a colour jump, or a change of width.
