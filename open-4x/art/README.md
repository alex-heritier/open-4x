# Art pipeline

Every picture in `assets/packs/base/` is generated here: **procedural SVG scenes -> `rsvg-convert` -> painterly finishing (NumPy) -> PNG.**
The target look is a hand-painted, saturated "Civ-style" strategy-game finish: olive land, tan beaches, deep teal sea with turquoise shallows,
dark painted forests, snow-capped relief, gritty brick-and-slate industrial cities, near-black navy UI panels in brass frames, cream parchment.

## Regenerate

```sh
cd art
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt   # numpy, pillow
brew install librsvg                                                   # provides rsvg-convert
.venv/bin/python build.py              # everything (~20 s)
.venv/bin/python build.py terrain      # or: terrain | overlays | sprites | ui | icons
```

Output goes to `assets/packs/base/`; the vector sources of the sprites are written to `art/svg/` (git-ignored, reproducible). Builds are deterministic (fixed seeds).
`preview_*.py` are scratch scripts that render contact sheets into `art/preview/` (git-ignored).

## Originality

The generator never opens `ref1-*.png`, `ref2-*.png`, the Civ3 install, or any `open-civ3` asset. The style (palette, lighting, finish) was matched by eye;
every shape, texture and ramp is authored in code in `forge/`.

## Resolution convention

Art is authored at **2x** the size the game draws, so it stays crisp on high-DPI displays and when zoomed in (the client scales sprites with `custom_size` and samples linearly).

| Asset | File px | Drawn px | Anchor |
| --- | --- | --- | --- |
| ground, water, river cell | 192x96 (1.5x) | 128x64 | one dual-grid diamond |
| city (one per flavor), forest, pine, jungle, marsh, hills, mountain (and the dry and cold variants) | 256x224 | 128x112 | ground-diamond centre at (128, 160) |
| fog sheet cell, border sheet cell | 256x128 | 128x64 | one diamond, drawn over the tile |
| infantry, pioneer, worker, cavalry, artillery | 160x160 | 70x70 | tile centre about (80, 108) |
| farm, mine overlays | 256x128 | 128x64 | one diamond, drawn over the tile |
| ironclad, transport, battleship, protected cruiser, torpedo boat | 256x192 | 112x84 | tile centre about (128, 123) |

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

Map overlays (`terrain/fog.png`, `terrain/borders.png`, built by `forge/overlay.py` with `python build.py terrain`):

* **Fog** (2304x1152, 9 x 9 cells): black with the fog's opacity in the alpha channel. A tile's four vertices (north, east, south, west) are each 0 never seen, 1 remembered, or 2 in sight,
  and the cell for `(N, E, S, W)` is at row `3*W + N`, column `3*S + E`. Opacity runs bilinearly between the vertices (255 unseen, 172 remembered, 0 in sight), so the edge of what you know or
  see is a soft gradient. Neighbouring tiles share their common vertices and therefore agree along the shared edge. The diamond masks are half-open, so the cells partition the plane exactly;
  the client must sample the sheet with nearest-neighbour filtering or seams appear.
* **Borders** (1024x128, 4 cells): a dashed line along one edge just inside it, in the order up-right, down-right, down-left, up-left. The dashes are white so the client tints them with
  the owner's colour; a darker rim keeps them legible on any ground.

Overlays (`sprites/`): `forest`, `pine`, `jungle`, `marsh`, `hills`, `hills_dry`, `hills_cold`, `mountain`, `mountain_dry`, `mountain_cold`; `python build.py overlays` rebuilds them.

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
| `cities.py` | the eleven city skins, one per flavor: `western`, `latin`, `orthodox`, `arab`, `east_asian`, `south_asian`, `southeast_asian`, `african`, `steppe`, `native`, `oceanic` (`sprites/city_<flavor>.png`). They share one camera, ground disc and light, and carry no flag: the client tints the nation's plate over them |
| `overlay.py` | the fog-of-war and culture-border sheets |
| `units.py` | soldiers, workers, horse, field gun, ironclad |
| `ships.py` | shared isometric `Hull` frame; troop transport, battleship, protected cruiser, torpedo boat. Wakes are fitted inside the sprite and fade out |
| `ui.py`, `icons.py` | brass-framed panels, plates, buttons, bars, flags, parchment; engraved and HUD icons |

## UI kit (`ui/`)

Nine-slice sources are drawn at 2x; `ui/nine_slice.json` lists the border inset (source px) that keeps corner ornaments undistorted.
`parchment.png` is a seamless 512 px tile. Icons live in `ui/icons/` at 64 px (shown at 32).
The current client UI is still built from flat colours; these are ready to be wired with Bevy `ImageNode` + `TextureSlicer`.
