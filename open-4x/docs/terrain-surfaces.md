# Terrain surfaces

The terrain renderer and art generator independently implement the behavior described in
`open-civ3/reverse-engineering/blending.md` and `graphics-terrain.md`. No renderer source,
reference implementation, PCX decoder, executable, or Civ3 bitmap was incorporated.
The existing ground/water/river dual-grid implementation remains the foundation.

## What the reference establishes

128×64 ground diamonds are separate from taller relief sprites. Hills and mountains extend
north of their footprint and overlap terrain behind them. The reference ships separate
hill/mountain forest and jungle sheets, plus snowy mountain art for tundra. A mountain's
own relief, cover and base determine its context: an adjacent forest alone does not turn a
bare mountain into a forested one. That distinction avoids expanding forests as the camera pans.

The reference notes establish inset, owner-colored dashed territory edges. They explicitly
leave the purpose of the curved border-sheet column and the original relief-selection
predicate unresolved. Surface projection below is an original implementation of the requested
contour behavior, not a claim to reproduce an undocumented Civ3 executable algorithm.

## A tile's composition

1. Ground, water and rivers blend at the four centers surrounding each dual-grid cell.
2. Farms and road/rail lines lie on the ground.
3. Relief, cover, mine overlays and border ribbons share a north-to-south painter band.
4. Fog covers that band. Cities, units and combat presentation remain above it.

A sprite is sorted by its tile's ground center, never its image center or peak height.
For logical coordinates `(x,y)`, world position is `(64*(x-y), -32*(x+y))`.
Relief Z is `10 - world_y/50000`; cover fallback, mine and border add respectively
`0.00008`, `0.00016` and `0.00024`. An edge neighbour lies 32 screen pixels lower and
has a Z advance of `0.00064`, so even its plain relief covers every component of the
preceding tile. Tiles in the same screen row do not overlap horizontally except at their
transparent footprint edges. Renderer bands leave space for every supported map dimension.

Border sprites no longer occupy a global layer above every mountain. A ribbon on a rear
tile can be hidden by a foreground mountain; a ribbon on the current tile sits above that
tile's relief. This applies to flat-border fallback artwork too. Remembered terrain and its
borders dim together under the fog. Border eligibility still uses the player's knowledge:
no edge between identical owners, no outline on an unknown tile, and no edge toward an
unknown or off-map neighbour. Rival territories draw an inset line on each owner's side.

## Climate and vegetation contexts

The bundled pack supplies 18 combinations:

| Dimension | Values |
| --- | --- |
| Relief | `mountain`, `hills` |
| Climate suffix | none (grass/plains), `_dry` (desert), `_cold` (tundra) |
| Cover suffix | none (bare), `_forest`, `_jungle` |

For example, tundra forest on a mountain selects `mountain_cold_forest`; jungle on a
hill over grass selects `hills_jungle`. Forest and jungle are embedded in the heightfield,
with roots sampled on the slopes, pointed evergreen crowns or round broadleaf crowns,
steep-cliff exclusions and exposed summits. They are projected and occluded with the rock,
not drawn as small floating trees in front of a bare mountain. Flat forest, tundra pine,
jungle and marsh keep their standalone sprites. Jungle can now coexist with hills and
mountains in scenario/save maps. Marsh remains restricted to flat land. Movement, defense
and yields retain the existing strongest-layer rules. The scenario forge likewise retains
jungle on slopes during future world builds; the shipped historical world is not regenerated
by this change.

A pack lacking a combined context falls back to its bare relief plus standalone trees.
If the bare climate sprite is also absent, it uses the pack's `mountain`. Border lookup uses
the *resolved* relief name so it targets the geometry actually drawn. Packs can therefore
omit new art while retaining the previous separate-layer appearance.

## Original art and contour projection

`art/forge/relief.py` builds a seeded heightfield over `(u,v)` in `[0,1]²`, with rock,
foothill and snow shading and optionally rooted vegetation. The 2× image projects samples as
`pixel_x = 128 + 128*(u-v)`, `pixel_y = 96 + 64*(u+v) - height`. Its ground center is
`(128,160)` in a 256×224 image. Drawing at 128×112 with the sprite center lifted 24 world
pixels places the footprint exactly on the tile center. The northern diamond apex is image
y=96; pixels above it overlap the tile behind.

The same seeded surface produces the four border masks. Edge parameterizations are
`(u,v)`, `(v,1-u)`, `(1-u,1-v)`, `(1-v,u)` for up-right, down-right, down-left, up-left.
A ribbon stays inside the footprint; its inset bows from 0.055 at each end to 0.175 midway
through a slope, then its pixels rise by the local height, including the vegetation surface.
Eight bead periods occupy an edge, with a dark rim and narrow thread between white cores.
Phase is measured on the footprint, so lifting a slope cannot stretch or renumber its beads.
Flat borders use the same paint with a constant inset.

Projection traverses surface diagonals front to back with a per-pixel-column occlusion
limit. Even an unpainted foreground sample updates that limit: rear ribbon paint disappears
behind a summit rather than shining through it. Surface ribbons are projected once at art
build time; runtime uses ordinary atlas sprites, with no CPU mesh warping or GPU shader.
Relief and ribbons must be regenerated together after a heightfield, seed or tree change.

## Pack contract

`visuals.relief_borders` is an optional map from relief sprite name to a PNG path. A sheet
contains **four equal-width cells side by side**, in the same edge order as `visuals.borders`.
The bundled sheet is 1024×224 (four 256×224 cells); the client derives the atlas dimensions
from each loaded image, so custom sheets may use a different uniform resolution. Its aspect
and ground anchor must match the associated relief sprite. Each cell draws at 128×112 with
a 24-pixel lift; the line is grayscale and tinted by the current owner.

Every path participates in the content loader's traversal and symlink checks. Missing
`relief_borders` deserializes as an empty map, so pack format 4 remains compatible. While a
sheet is loading, the client uses the flat ribbon; its atlas arrival requests a redraw. A
pack without a sheet for the selected relief keeps that flat fallback permanently.

## Reproduce and verify

From `open-4x/`:

```sh
cargo run -p fourx-client -- --scenario terrain-study --focus 10,10 --zoom 1.6
cargo run -p fourx-client -- --scenario terrain-study --focus 10,10 --zoom 1.6 \
  --smoke --screenshot /tmp/terrain-study.png
cargo test --workspace --exclude fourx-client --profile quick
cargo test -p fourx-client --lib --profile quick
```

The `terrain-study` scenario is a charted 20×20 map with two rival cities, bands of relief,
all three climates and vegetation contexts, and borders crossing those bands. Charting
makes geography visible while retaining normal remembered/in-sight fog behavior. Inspect
both ownership colors, rear versus front edges, the transitions to flat land, snowy trees,
and the jungle band. The ordinary world and starter remain the default/playable scenarios.

From `art/` with the requirements installed:

```sh
python build.py relief                      # regenerate all 18 sprite/surface-border pairs
python -m unittest discover -s tests         # shipped asset geometry and analytical occlusion
```

The `terrain` art group regenerates flat borders; `overlays` includes the `relief` group.
Rust checks cover all context selections, old-pack fallbacks, optional fields, asset existence,
path confinement and painter-order gaps. Python checks test an analytical foreground wall
occluding rear paint, taller-than-diamond mountain extents, and vegetation altering the
surface geometry rather than merely recoloring it. PNG generation is seeded and repeatable.

## Limits

This implements original art, surface contouring and sprite overlap, not pixel-for-pixel Civ3
artwork. Relief has one seeded shape per climate rather than neighbour-mask ridge atlases;
adjacent peaks overlap but do not generate a new continuous mesh. Surface ribbons follow
the authored surface including trees; roads, rivers, farms and units are not height-warped.
New relief styles in a custom pack need their own matching sheets to gain contour borders.
There is no runtime inference of height from arbitrary PNG artwork.
