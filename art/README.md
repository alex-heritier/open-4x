# Art pipeline

Every picture in `assets/packs/base/` is generated here: **procedural SVG scenes -> `rsvg-convert` -> painterly finishing (NumPy) -> PNG.**
The target look is a hand-painted, saturated "Civ-style" strategy-game finish: olive and khaki land, tan beaches, azure shallows over a navy sea,
dark painted forests, brown rocky relief, gritty brick-and-slate industrial cities, a UI of aged parchment and dark slate under gilt frames, with white glyphs for small controls.

## Regenerate

```sh
cd art
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt   # numpy, pillow
brew install librsvg                                                   # provides rsvg-convert
.venv/bin/python build.py              # all groups
.venv/bin/python build.py terrain      # or: terrain | roads | overlays | relief | sprites | cities | units | flavor | ships | icons | parchment | resources
```

`build.py ships` redraws only the five ships and rewrites each one's `muzzles` table (where its guns fire from, per facing) in `pack.json`; the client needs that table to fire a broadside.

Output goes to `assets/packs/base/`; the vector sources of the sprites are written to `art/svg/` (git-ignored, reproducible). Builds are deterministic (fixed seeds).
`preview_*.py` are scratch scripts that render contact sheets into `art/preview/` (git-ignored).

## Terrain look

The terrain follows the look of a hand-painted strategy map: sunlit meadow-green and straw-khaki land with broad light and shadow rolling across it (`terrain.Fields.rolling`, plus a different set of knolls in each tonal variant), dense green woods whose crowns catch a warm yellow light (`nature.py` ramps run to near-black in the crown shade), grassy green hills cut by gullies and spurs, with bushes and stones dotted over them (`relief._scatter`), slender grey mountains (a main peak and three lesser ones on a low foothill skirt) with snow on their upper slopes and green scrub at their feet (`relief.STYLES`, `HILL_STYLES`), and one teal sea that deepens away from shore: pale turquoise shelf (coast), steel-teal sea, slate-blue ocean (`terrain.COAST_STOPS`, `SEA_STOPS`, `OCEAN_STOPS`). The depths fade into each other over a wide, ragged band (`terrain.BAND_SOFT`), with pale wave strokes and a slow swell across all of them.

The band isoline wanders (`terrain.ZONE_JITTER`) with periodic noise, so a border pixel still depends only on the two vertex digits of its edge and sheets keep tiling. Land is still pulled slightly toward its mean colour (`terrain._calm`) and cover uses `build.COVER_GRADE`. When adding terrain art, check it with a city and a unit on top (`cargo run -p fourx-client -- --scenario age-of-imperialism --focus 112,34 --zoom 1`): the sprite should still win against the ground.

## Originality

The generator never opens `ref1-*.png`, `ref2-*.png`, the Civ3 install, or any `open-civ3` asset. The style (palette, lighting, finish) was matched by eye;
every shape, texture and ramp is authored in code in `forge/`.

## Resolution convention

Art is authored at **2x** the size the game draws, so it stays crisp on high-DPI displays and when zoomed in (the client scales sprites with `custom_size` and samples linearly).

| Asset | File px | Drawn px | Anchor |
| --- | --- | --- | --- |
| ground, water, river cell | 192x96 (1.5x) | 128x64 | one dual-grid diamond |
| city (one per flavor at three sizes: `city_<flavor>_small` for size 1-6, `city_<flavor>` 7-12, `city_<flavor>_large` 13+), forest, pine, jungle, marsh, hills, mountain (and the dry and cold variants) | 256x224 | 128x112 | ground-diamond centre at (128, 160) |
| fog sheet cell, flat border sheet cell | 256x128 | 128x64 | one diamond, drawn over the tile |
| infantry, pioneer, worker, cavalry, artillery (and each cell of their animation sheets) | 160x160 | 70x70 | tile centre about (80, 108) |
| farm (plain, rice, corn, wheat), mine overlays | 256x128 | 128x64 | one diamond, drawn over the tile |
| `farm_shore.png`, `farm_rice_shore.png`, `farm_corn_shore.png`, `farm_wheat_shore.png` | 8 x 4 pieces of 256x128 | 128x64 each | the farm cut by the shoreline of the water sheet, one piece per dual cell round a tile (see `forge/improvements.py`); a farm beside water is drawn from four of these instead of the whole diamond; `visuals.farms` picks the crop by the owner's flavor. Keep plants inside the diamond: the cut clips a piece to its cell |
| ironclad, transport, battleship, protected cruiser, torpedo boat (and each cell of their sheets) | 256x192 | 144x108 | tile centre about (128, 123) |

All light comes from the upper left, in every module.

## Terrain sheets (`terrain/ground.png`, `water.png`, `rivers.png`)

A square is drawn as layers, like a Civ3 tile: **ground**, the **water** over it, **rivers** along its edges, then sprites for relief and cover. The ground and water sheets are
16 columns of 2:1 diamonds (192x96 each, drawn 128x64); the client takes the cell size from `image_width / 16`, so any resolution works.

* **Ground**: digits grassland 0, plains 1, desert 2, tundra 3 at the N/E/S/W vertices; `index = (4*S + E)*16 + 4*W + N`, so rows 0-15 hold the 256 combinations. A water vertex
  carries the ground type of the land beside it.
* **Water**: digits land 0, coast 1, sea 2, ocean 3, same indexing. It paints the beach, foam, surf and the depth tint with alpha over the ground, so a coast cell is two sprites.
  Water next to land is always coast.
* Rows 16-17 of both: **tonal variants** of the pure cells, `index = 256 + 4*(k-1) + digit` for `k = 1..6` (the water sheet has none for land). The client picks one by a position hash.
  Variants differ only in the interior; their border pixels equal the base cell, so every combination still tiles without seams.
* **Rivers**: 16 columns, one per mask of half-edge branches that meet at a tile corner (bit 0 north-east, 1 south-east, 2 south-west, 3 north-west), and 3 rows of different
  meanders. Every branch leaves the cell at the middle of its edge at the same width, so rivers join across cells.
* Seamlessness: all texture fields are periodic over the diamond's lattice, so two cells that share an edge evaluate identical pixels there.
  A border pixel depends only on the two vertex terrains of that edge. The ground and water sheets are saved as palette PNGs without dithering (`util.indexed`), which keeps
  equal pixels equal and makes the sheets about eight times smaller.
* Coasts: the shoreline is the 0.5 isoline of the blended vertex weights, pushed by noise that fades to zero away from the boundary.
  Beach sand, turquoise shallows and a foam line are layered around it.

**Roads** (`terrain/roads.png`, `terrain/railroads.png`, `forge/roads.py`, `python build.py roads`, about 40 s): 16 columns of 192x96 cells, one per mask of the eight neighbours a tile is joined to (bit order NE, E, SE, S, SW, W, NW, N), then variants of the masks with at most two links. Exits sit at the same eight points of every cell, at the same width, so neighbours join; wander and swell fade to nothing toward them. Roads are bezier routes with a worn edge, junctions branch tangentially; railroads add a ballast bed, sleepers at even spacing, and turnouts. The contract is in `docs/roads.md`.

Map overlays (`terrain/fog.png`, `terrain/borders.png`, built by `forge/overlay.py` with `python build.py terrain`):

* **Fog** (2304x1152, 9 x 9 cells): black with the fog's opacity in the alpha channel. A tile's four vertices (north, east, south, west) are each 0 never seen, 1 remembered, or 2 in sight,
  and the cell for `(N, E, S, W)` is at row `3*W + N`, column `3*S + E`. Opacity runs bilinearly between the vertices (255 unseen, 172 remembered, 0 in sight), so the edge of what you know or
  see is a soft gradient. Neighbouring tiles share their common vertices and therefore agree along the shared edge. The diamond masks are half-open, so the cells partition the plane exactly;
  the client must sample the sheet with nearest-neighbour filtering or seams appear.
* **Borders** (1024x128, 4 cells): a dashed line along one edge just inside it, in the order up-right, down-right, down-left, up-left. The dashes are white so the client tints them with
  the owner's colour. They are slightly thick and carry no dark rim or outline.

Overlays (`sprites/`): `forest`, `pine`, `jungle`, `marsh`, `hills`, `hills_dry`, `hills_cold`, `mountain`, `mountain_dry`, `mountain_cold`; `python build.py overlays` rebuilds them.

**Margin** (`margin/`, `forge/margin.py`, `python build.py margin`): what lies past the top and bottom of an upright world, drawn like the border of a Victorian atlas plate. `band.png` is the neatline frame (a gilt cable, a meander frieze, a chequered degree scale), a 256x240 tile drawn 128x120 along the edge with its last 8 units a shadow over the map; `edge.png` is the torn end of the sheet and `desk.png` the walnut it lies on. Strips are authored for the north edge, map side down, and flipped for the south. The engravings (`compass`, `cartouche`, `ribbon`, `ship`, `steamer`, `serpent`, and `instruments` for the desk) are drawn at half their file size; the client letters the cartouche with the campaign's name and the ribbons with mottoes.

## Modules (`forge/`)

| Module | Contents |
| --- | --- |
| `palette.py`, `noise.py`, `util.py` | colour ramps, periodic and flat fBm noise, rasterising and image helpers |
| `svgkit.py`, `paint.py` | SVG scene builder and isometric helpers; Kuwahara oil-paint filter, grain, rim light, grade |
| `terrain.py` | dual-grid ground and water sheets and their variants |
| `rivers.py` | river sheet: meandering banded water along the half-edges of a corner |
| `nature.py`, `relief.py` | paint-dab forest, pine, jungle and marsh; heightfield hills and snow mountains in green, dry and cold styles (ridged noise, Lambert lighting, front-to-back projection) |
| `buildings.py` | isometric kit for the city: gable/hip/flat roofs, windows, brick chimneys, pagoda, flag |
| `kit.py` | round and organic primitives in the same light: drums, cones, domes, onion domes, minarets, palms, palisades, stilts, tents, canoes |
| `cities.py` | the eleven city skins, one per flavor: `western`, `latin`, `orthodox`, `arab`, `east_asian`, `south_asian`, `southeast_asian`, `african`, `steppe`, `native`, `oceanic` (`sprites/city_<flavor>.png`). They share one camera, ground disc and light. Each raises a white banner, also written alone as `sprites/city_<flavor>_banner.png`, which the client draws over the town tinted with the nation's colour |
| `roads.py` | the road and railroad sheets: routes paired through each cell, junction branches, sleepers and rails, seamless exits |
| `overlay.py` | the fog-of-war and culture-border sheets |
| `rig.py` | a small 3D puppet renderer that writes SVG: the game's 2:1 camera, painter-sorted polygons, tubes, tapered limbs and balls, Lambert light from the upper left, two-bone IK, the eight facings |
| `troops.py` | the land units as puppets: riflemen, pioneers and workers, horse and dragoon, field gun and gunner, and their five clips. It also holds the per-culture **kits** (`FLAVOR_KITS`: coat, trousers, accent and headgear) and **steeds** (`FLAVOR_STEEDS`), from which `BUILDERS` gains an `infantry_<flavor>` and a `cavalry_<flavor>` for each culture that fields its own |
| `fleet.py` | the five ships on one 3D hull (stations, bow and stern, strakes, planked deck) with casemates, oval funnels and their trailing plumes, turrets that train round to the starboard beam, sponsons, tripod masts, ensign, bow and stern foam, and their five clips; `muzzle_table(design)` reports where the guns are when a broadside fires, for `pack.json`'s `muzzles` |
| `chrome.py`, `emblems.py`, `scenes.py` | window chrome: parchment/slate/felt surfaces, gilt frames with boss and scroll corners, cartouches, compass; yield emblems; terrain vignettes (`build.py chrome`) |
| `icons.py` | flat white glyph icons (`icon_*`, `hud_*`) |
| `resources.py` | the 43 natural resources of the Age of Imperialism map as small free-standing paintings (heaps of rounded, softly lit stones or packed coal lumps, crops, animals on tapered legs, a derrick over an oil slick, a hamlet of cottages for bonus terrain, flags for the colonial markers; no frame or outline, only a contact shadow and a few grass blades at the foot), `sprites/resources/<id>.png`, 128 px shown at 60; `python build.py resources` |

## Unit animation (`units/`)

`python build.py units` renders every design's five clips (about a quarter of an hour on four cores) and writes them as
`units/<design>/<clip>.png` and its matching `<clip>_accent.png`, plus the design's single `sprites/<design>.png` (its first idle frame, facing south-west).
The base sheet keeps the culture's uniform, skin, leather, wood and metal colors. The accent sheet is grayscale with transparency, drawn from designated 3D surfaces and occluded by the same depth-sorted geometry. Only collars, cuffs, headbands, saddle blankets, flags and cargo bindings are recolored. The client draws this layer above the base with the nation's color, matching its exact atlas frame, facing, brightness and opacity during idle, movement, combat and capture. Packs opt in with `accent` beside each clip's `sheet`; older packs keep their original colors.

`crate` now uses the same eight-facing, five-clip pipeline as the other land designs. `build.py cargo` rebuilds just that design. Unit builds process one design at a time to bound memory use.

A clip sheet has a column per frame and a row per facing, in Civ3's strip order: south-west, south, south-east, east,
north-east, north, north-west, west. Facing `k` looks along the ground at `225 + 45k` degrees (east 0, north 90).

| Clip | Land frames x ms | Ship frames x ms | What it shows |
| --- | --- | --- | --- |
| `idle` | 8 x 160 (loops) | 8 x 170 (loops) | breathing, a glance around, a horse's tail; a ship riding the swell, smoke drifting aft |
| `run` | 8 x 60 (loops) | 8 x 90 (loops) | one stride, gallop or wheel turn in place; a ship's bow wave and wake |
| `attack` | 10 x 80 | 8 x 120 | the shot 30% of the way in (a charge connects at 50%), then back to rest |
| `victory` | 8 x 90 | 8 x 110 | rifles, sabre or cap raised, a horse rearing; a ship's whistle |
| `death` | 10 x 90 | 8 x 220 | struck, crumpling and falling; a ship's magazine going up in two columns of fire and smoke, then listing and going down in a ring of foam |

These numbers are also in `pack.json` (`visuals.units`), and a content test checks every sheet against them. Each
figure is posed in its own frame and drawn through the camera, so all eight facings come from one model: a puppet is
built from tapered limbs (its knees and elbows by two-bone IK toward its feet and hands), elliptical tubes for coats
and barrels, boxes for packs and deckhouses. Sheets are palette PNGs, one palette per sheet, which at the drawn size
cannot be told from full colour and is about five times smaller. `preview_anim.py <design> <clip>` draws a quick
contact sheet of one clip into `preview/`.

### Culture variants (`python build.py flavor`)

The base designs are cut as the Western kit. The ten other cultures each field an infantry and a
cavalry of their own, which `python build.py flavor` renders as `units/<design>_<flavor>/<clip>.png` and
`sprites/<design>_<flavor>.png` — the same frame, clips and layout as the base design, only painted
as that culture's people. `pack.json`'s `visuals.units.<design>.flavor_sprites` and
`flavor_animations` name them, so the client draws a Zulu warrior, an Ottoman asker or a Qing
banner-man from its own art while every other design stays shared. `python build.py units
cavalry_steppe infantry` re-renders just the named designs.

Each kit keeps its own uniform palette. National color appears only on designated cloth accents. The
headgear carries the strongest read at the drawn size: a white tunic under a dark kepi for Latin
America, a shako for the Orthodox world, a turban for the Islamic one, a dark blue coat under a red-banded
kepi for East Asia (the Meiji army's uniform), a steep conical straw hat for South-East Asia, a cloth cap for the subcontinent, a red headband over an ochre tunic for Africa,
long hair under a band for the steppe, buckskin and a feather for the indigenous Americas, and a
bare head over a red-brown tunic and pale wrap for the Pacific. The horse's coat follows the culture
too, so a steppe pony, an Arab and a European charger read apart.

## UI glyphs (`ui/icons/`)

Flat white glyphs at 64 px, drawn by `forge/icons.py`; detail is cut out as transparent holes. The client tints them with `ImageNode` colours. Buttons, plates and text are drawn in code in `crates/client/src/theme.rs`; panels are dressed with the pictures of `ui/chrome/` (see below) by `crates/client/src/chrome.rs`.

## Window chrome (`ui/chrome/`)

`forge/chrome.py` draws the seamless `parchment`, `slate` and `felt` tiles, the 9-slice gilt `frame` (160 px, drawn at half size) and `frame_small` (100 px) with boss corners at top right and bottom left and scroll corners at the others, the `stud`, the side `cartouche_h`/`cartouche_v`, and the `compass`. `forge/emblems.py` draws the `ico_*` yield emblems (96 px) and `forge/scenes.py` the `scene_*` terrain vignettes (340 x 150). Build with `python art/build.py chrome`.

## Relief surfaces and borders

`python build.py relief` builds 18 relief/vegetation contexts (mountain or hills × temperate,
dry, cold × bare, forest, jungle), with embedded tree crowns and a matched four-edge border
sheet for each. `terrain/<relief-name>_borders.png` contains four 256×224 cells, with exactly
the relief sprite's ground anchor at (128,160). The runtime tints these grayscale ribbons
with the owner's color and depth-sorts them with the terrain. Flat borders share their eight
beads and thread pattern. Regenerate relief and border pairs together; do not restyle the
heightfield without rebuilding its ribbons. The `overlays` group includes `relief`.

Run `python -m unittest discover -s tests` to check geometry, overlap and analytical
occlusion. See [terrain surfaces](../docs/terrain-surfaces.md) for the behavioral evidence,
pack fields, fallback handling, and deterministic game fixture.
