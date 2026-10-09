# Scenario format (version 3)

A **scenario** is one JSON document that holds everything needed to start a campaign: a calendar
date, a map, the nations, and their starting cities, units, and wars. It is separate from a
content pack. The pack supplies rules (unit designs, costs, research), art, sounds, and the
turn script; the scenario supplies the world. A pack can ship several scenarios and names one as
the default.

The bundled default is [`world-1876`](../assets/packs/base/scenarios/world-1876.json): the whole
world on an upright Mercator map of 175,104 tiles (84°N to 58°S, no Antarctica) on **Saturday, 1 January 1876**, with 119 nations. The compact
`dawn-straits` scenario is the small fixture the tests use.

## Conventions

- Strict. Unknown fields are rejected (`deny_unknown_fields`) and every cross-reference is
  checked when the pack loads, so a bad scenario fails with a precise message instead of
  reaching the simulation.
- References are by **stable string ID**: 1–48 characters of `a-z`, `0-9`, and interior `-`.
  Nations, regions, and cities never refer to each other by position, so documents can be
  reordered and diffed.
- Colours are lowercase `#rrggbb`. Dates are ISO `YYYY-MM-DD` in the proleptic Gregorian
  calendar. Coordinates are `{ "x": column, "y": row }` of the map's square grid, with the origin at its first cell
  (see [Map](#map): on an upright map these are not screen columns and rows).
- Output is deterministic. The same scenario, rules, and seed always produce the same game.
- `format` is the schema version. A reader must reject a version it does not know.

## Top level

| Field | Type | Notes |
| --- | --- | --- |
| `format` | integer | Must be `3`. Older documents are refused: version 1 had `armies`, and version 2 maps were plain rectangles that showed turned 45° on screen. |
| `id` | string | Scenario ID, e.g. `world-1876`. |
| `name` | string | Shown in the title bar. 1–96 characters. |
| `description` | string | Up to 4000 characters. Not used by the simulation. |
| `start_date` | date | Calendar date of turn 1. **One turn is one day.** |
| `commander` | nation ID | The nation played unless the host picks another (`--nation`). |
| `intro` | string | First dispatch and the idle-panel briefing. Up to 600 characters. |
| `charted` | boolean | Optional, default `false`. When `true`, every nation starts with the whole map known: all terrain, borders, and cities. Enemy units are still only seen within sight of a friendly unit or city. |
| `rules` | object | Optional overrides of pack rule values for this scenario only. |
| `map` | object | Terrain and regions. See below. |
| `nations` | array | 1–250 nations. |
| `regions` | array | 0–1024 named areas of the map. |
| `cities` | array | Up to 4096 starting cities. |
| `units` | array | Up to 16384 starting units, each one soldier, worker, or ship. |
| `wars` | array | Wars already under way on the start date. |

`rules` may set `victory_industry` (1–1 000 000, the total shields a nation must produce to win
by industry), `research_cost` (1–100 000), `starting_gold`
(0–1 000 000). Anything omitted keeps the pack's value. The world
scenario raises `victory_industry` and `research_cost`, because the default values are tuned for
the two-empire starter.

## Map

```json
"map": {
  "width": 598,
  "height": 598,
  "lattice": { "columns": 256, "rows": 684 },
  "terrain": ["~~~~~sccgg", "..."],
  "relief": ["..........", "..hh^^...."],
  "cover": ["..........", "..ff..jj.."],
  "rivers": [0, 10958, 2, 1, 0, 44],
  "regions": [0, 10958, 44, 1, 0, 1],
  "improvements": [0, 5000, 3, 12, 0, 1]
}
```

A square is built from layers, the way a Civ3 tile is: a base terrain, a relief, a cover, and
rivers on its edges.

- `width` and `height` are the size of the square grid that holds the map. `lattice` is optional and
  says that the map is an **upright rectangle** embedded in that grid; see [Upright maps](#upright-maps).
- `terrain` has one string per row, all of equal length, one ASCII glyph per tile, at most 1024
  per side and 2^20 tiles in all. The glyph is the **base terrain**:

  | Glyph | Terrain | Glyph | Terrain |
  | --- | --- | --- | --- |
  | `~` | ocean (deep water) | `s` | sea |
  | `c` | coast (shelf and shallows) | `g` | grassland |
  | `p` | plains | `d` | desert |
  | `t` | tundra | | |

  Water beside land is drawn as coast whatever it says. The old combined glyphs still read, so
  hand-written maps keep working: `.` grassland, `,` desert, and `f`, `m`, `n` (forest,
  mountains, forested mountains on grassland) or `F`, `M`, `N` (on desert). A map is always
  written back in layers.
- `relief` is optional (omit it for a flat map): the same shape as `terrain`, with `.` flat, `h`
  hills, `^` mountains. Water has none.
- `cover` is optional: `.` bare, `f` forest, `j` jungle, `m` marsh. Forest grows on any land
  relief; jungle also grows on hills and mountains. Marsh needs flat ground. Water has none.
- `rivers` is an optional run-length layer of the same shape as the others below: a flat list of
  `[edges, count, …]` pairs, one value per tile in row-major order. Bit 0 (`1`) is a river on
  the tile's east edge (between it and the tile at `x + 1`), bit 1 (`2`) on its south edge (the
  tile at `y + 1`). A tile's north and west edges belong to its neighbours. Both tiles on either
  side of a river must be land inside the map. Rivers water farms and add gold; they do not
  block movement.
- `regions` is a run-length layer in row-major order: a flat list of `[region, count, region,
  count, …]` pairs covering exactly `width × height` tiles. Region `0` means unclaimed; region
  *n* refers to the *n*th entry of `regions` in the document. An empty list means everything is
  unclaimed. The total is checked before any allocation.
- `improvements` is an optional run-length layer of the same shape holding worker improvements
  as a bit set: `1` road, `2` railroad, `4` mine, `8` farm. Only land can hold them, a railroad
  needs a road, and a tile has a mine or a farm but not both. Omit it for an unimproved map.
- There is **no owner layer in the document**, and no layer for border claims. Territory is
  derived from the cities when the game is built: each city claims the tiles within its border
  level's reach (see [Cities](#regions-cities-units-wars)), and a tile's owner is the nation
  holding the city that claims it. A scenario that sets `owners` or `claims` is rejected.
  Regions only name stretches of map for display; they no longer decide who owns a square.

Movement is eight-connected: a unit may step diagonally.

### Upright maps

Civ3 draws its map as a rectangle that stands upright on screen: north up, east right, and every
other row shifted half a tile. The simulation works on a square grid, and a plain `width × height`
rectangle of that grid is shown turned 45°, as a diamond. `lattice` fixes this. It says the world has
`rows` rows of `columns` tiles each, and the document stores them in a square grid of side
`columns + rows / 2` (`rows` must be even), which `width` and `height` must equal. In the world
scenario that is `256` columns and `684` rows in a `598 × 598` grid, and `dawn-straits` uses `12` and
`36` in `30 × 30`.

With a tile's native Civ3 position written `(cx, cy)` (`cx` counts half tiles across, `cy` counts
rows down, and a tile exists where `cx + cy` is even), the grid cell `(x, y)` holds

```text
cx = x - y + columns        x = (cx + cy) / 2
cy = x + y - columns        y = (cy - cx) / 2 + columns
```

The grid cells outside the rectangle are **void**: the simulation treats them as off the map, so no
unit, city, claim, or sight reaches them, and the document must write them as plain open ocean with
no river, region, or improvement. All eight neighbours of a tile, the four that share an edge and the
four that touch at a corner, remain one step away on the grid, so movement, borders, and distances
are unchanged. A map without `lattice` is a plain grid, as before.

A tile's `east` and `south` rivers and the `y`-down row order of the layers follow the grid, not the
screen: on an upright map the grid's `x + 1` neighbour is the tile to the lower right.

## Nations

```json
{
  "id": "canada", "name": "Dominion of Canada", "adjective": "Canadian",
  "color": "#f0a8bb", "status": "dependent", "suzerain": "united-kingdom",
  "government": "Self-governing dominion",
  "leader": "Prime Minister Alexander Mackenzie",
  "gold": 300, "technology": 2, "notes": "…"
}
```

| Field | Notes |
| --- | --- |
| `id`, `name` | Required. |
| `adjective` | Names raised units and founded cities (`Canadian`). Defaults to the name. |
| `color` | Required. Used for borders, the minimap, and flags. |
| `flavor` | Cultural look of the nation's cities: `western` (default), `latin`, `orthodox`, `arab`, `east_asian`, `south_asian`, `southeast_asian`, `african`, `steppe`, `native`, or `oceanic`. The pack's `visuals.cities` must hold a sprite for each. Cosmetic: the simulation never reads it, but it travels in snapshots so every client draws the same skin. |
| `status` | `sovereign` (default), `dependent`, or `unrecognized`. |
| `suzerain` | Required exactly when `status` is `dependent`, forbidden otherwise. Chains must terminate: a nation cannot be its own overlord. |
| `government`, `leader` | Display text, up to 96 characters. |
| `gold` | Starting treasury. Defaults to the rules' `starting_gold`. |
| `technology` | Starting technology level, 0–100. |
| `notes` | Free-form historical context, up to 1000 characters. Not used by the simulation. |

`dependent` covers vassals, tributaries, protectorates, and self-governing dominions: they hold
their own land and field their own forces but answer to a suzerain. `unrecognized` covers
rebels and pretenders that hold land without being a state.

Every nation needs at least one starting city and at most one `capital`. Nation IDs are numbered
from 1 in document order when the game is built; `0` always means "nobody".

## Regions, cities, units, wars

```json
{ "id": "canada", "name": "Dominion of Canada", "nation": "canada" }

{ "nation": "united-kingdom", "position": {"x":255,"y":170}, "name": "London",
  "population": 11, "industry": 7, "capital": true, "border": 6 }

{ "nation": "united-kingdom", "position": {"x":255,"y":170}, "kind": "infantry",
  "level": 2, "fortified": true }

{ "a": "spain", "b": "carlist-spain", "name": "Third Carlist War" }
```

- **Regions** are named stretches of map held by one nation on the start date. Several regions
  can belong to the same nation. Selecting a tile shows its region and owner.
- **Cities** must be on land, one per tile, with `population` and `industry` of 1–100.
  `population` is the number of citizens, who each eat 2 food a day; `industry` is the city's own
  workshops, which add shields to what the land of its region yields. Food, shields, and gold
  come from the squares of the border region and are not part of the document: they follow from
  the map's terrain, rivers, and improvements (see the README's Economy rules). A poor region
  starts a large city starving, so size cities to their land. Each owns a fixed **border region**. `border` is the cultural border level, 1–6, and never changes
  during play. Level *L* claims every tile within squared distance 2, 5, 10, 18, 26, or 41 of the
  city (9, 21, 37, 61, 89, or 137 tiles on open land), and open water only within squared
  distance 5. Where regions overlap, the nearest city wins, then the higher level, then the
  larger and the older city. If `border` is omitted it follows size: levels 2, 3, and 4 for
  populations up to 3, 4–6, and 7 or more, plus one for a capital. Whoever holds a city holds its
  region; capturing it flips the whole region to the captor.
- **Units** name a design from the pack's `units` table in `kind` and stand on one tile. `level`
  is the experience level 0–3 (Conscript, Regular, Veteran, Elite) and defaults to Regular;
  `fortified` starts a land unit dug in and is only allowed on land units that can defend. Land
  units must start on land and ships on water, or in a coastal city (one with water next to it) of
  their own nation, where they begin in port. Passengers are not part of the format: units start
  ashore and board in play. One tile holds the units of a single nation, at
  most 64, and a city's tile holds only its owner's units.
- **Wars** are unordered pairs, each listed once, between two different nations. The computer
  nations attack their enemies in range and otherwise stay home, so a scenario with no wars is
  peaceful until the player starts one.

## Calendar

The date is part of the game state, not a display setting. Turn 1 is `start_date`; turn *n* is
`start_date + (n − 1)` days, with real leap years. `Game::date()` reports it, snapshots carry it,
the HUD prints it, and the Lua `on_turn` context receives the date of the turn about to be played
as `date` (ISO text), `year`, `month`, and `day`. Turns, saves, and replays stay in lockstep with
the calendar, because it is derived from the turn counter.

## Using a scenario

A pack lists its scenarios and the default in `pack.json`:

```json
"scenarios": [
  {"id": "world-1876", "path": "scenarios/world-1876.json"},
  {"id": "dawn-straits", "path": "scenarios/dawn-straits.json"}
],
"default_scenario": "world-1876"
```

```sh
cargo run -p fourx-server -- --list                          # scenarios and their nations
cargo run -p fourx-client                                    # the default: world-1876, as Japan
cargo run -p fourx-client -- --nation united-kingdom         # same world, another nation
cargo run -p fourx-client -- --scenario dawn-straits         # the compact starter
cargo run -p fourx-server -- --scenario world-1876 --nation russia --simulate 30
```

Paths are relative to the pack directory and may not leave it. A save embeds the pack, rules,
script, and the full game state, so it resumes independently of later edits to the files. Saves
and packs from earlier versions (pack format 1, save format 1) are refused with a clear message.

## Building the world scenario

`world-1876.json` is generated, not hand-edited. `tools/scenario-forge` combines curated
history tables in `tools/scenario-forge/data/` with public geographic data:

| Source | Used for | Licence |
| --- | --- | --- |
| [Natural Earth](https://www.naturalearthdata.com/about/terms-of-use/) 1:50m countries, lakes, and rivers | coastlines, country shapes, lakes, and the rivers | public domain |
| [ETOPO5](https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO5/) (NOAA, 5′) | land elevation (hills, mountains, the tree line) and sea depth (coast, sea, ocean) | public domain |
| Köppen–Geiger climate classes (Kottek et al. 2006, 0.5°) | desert, steppe, grassland, tundra, forest, and jungle | free with attribution |

```sh
cargo run --release -p scenario-forge -- fetch   # download and verify the source data
cargo run --release -p scenario-forge -- build   # regenerate the scenario
```

The source files are pinned by SHA-256 and cached in `tools/scenario-forge/cache/` (ignored by
git). The build projects the globe into a Web Mercator map (about 78 km per tile at the
equator; a tile is square everywhere, so high latitudes are stretched as on any Mercator map)
and keeps rows from 84°N down to 58°S: a 512 × 342 picture, which clears Greenland and Cape Horn and
leaves Antarctica off. The tiles are laid on that picture the way Civ3 lays them, as diamonds twice
as wide as tall with every other row shifted half a tile (one tile covers one unit of area, so the
world keeps 175,104 tiles) and the result is stored as an upright map of 256 columns and 684 rows. It supersamples 4 × 4 to decide land and water, paints regions with
ordered operations (whole countries, polygons, rectangles, single tiles), forces a few straits
open (Bosporus, Dardanelles, Bab-el-Mandeb, Suez), and then paints the layers:

1. **Base terrain** from the Köppen class under each tile: rainforest and monsoon climates are
   grassland under jungle, savanna and steppe are plains, deserts are desert, temperate and
   continental climates are grassland or plains, polar climates are tundra, and the boreal belt
   is plains under forest.
2. **Relief** from ETOPO5: the roughest 6.5% of land (relief plus altitude, with a minimum peak)
   becomes mountains and the next 16.5% hills.
3. **Cover** from the climate's vegetation share, laid down in coherent clumps by noise ranked
   so that each climate gets exactly its share: jungle in the tropics, forest in the temperate
   and boreal zones, nothing above the tree line. Jungle on a slope becomes forest.
4. **Rivers**: Natural Earth polylines are snapped onto tile edges, join the sea when they stop
   within two edges of it, and big ones leave marsh on the low ground at their mouths.
5. **Water depth**: coast on the shelf and beside land, sea on the slope and in enclosed basins
   (the Black Sea, the Caspian), ocean in the deeps.
6. **Hand corrections** (`patches` in `data/terrain.json`) for what the data cannot see:
   wetlands such as the Pripet Marshes and the Sudd, and alluvial plains such as Mesopotamia.
   Every city stands on flat, bare ground.

It validates the result exactly as the game does before writing it. Later region files override
earlier ones, so colonial claims are layered over the modern country outlines.

| File | Contents |
| --- | --- |
| `data/meta.json` | ID, name, start date, commander, intro, rule overrides, wars |
| `data/nations.json` | Nations: colours, leaders, status and suzerain, a development tier (0–10: industry, technology, treasury), a military tier (0–4: one to three formations of infantry, cavalry, and artillery, starting at the capital and moving on to later cities, with experience rising from Conscript to Elite), and fleets by home port. Each capital also gets one to three workers by development tier, and its infantry start fortified |
| `data/regions/*.json` | Ordered region paint operations by continent |
| `data/cities/*.json` | Starting cities by continent |
| `data/terrain.json` | `patches` (a bbox or polygon, a terrain, relief, or cover, and a density) that correct the derived terrain; forced water and land points |

Borders are approximate to a tile. Colonial claims show who held the coast and the river routes,
not paper claims. Where 1876 history is contested, the tables record what the forge's author
judged most defensible; correcting an entry means editing a table and rebuilding.
