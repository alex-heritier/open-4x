# Unit and city art, and the national colour

How a pack names unit art, how a culture may field a design of its own, and how a nation's
colour rides on all of it. See [`combat-animation.md`](combat-animation.md) for the clip sheets
themselves.

## Unit art in the pack (`visuals.units`)

Each unit design (`visuals.units.<design>`, keyed by the `rules.units` id) has:

| Field | Notes |
| --- | --- |
| `sprite` | Pack-relative PNG, drawn when the design has no clips. Its first idle frame, facing south-west, for the base designs. |
| `clips` | Optional. The five frame-by-frame clips (`idle`, `run`, `attack`, `victory`, `death`), each a sheet of eight facing rows. A design with clips is animated. |
| `flavor_sprites` | Optional. `{ "<flavor>": "<path>" }`: a culture's own still art for this design. |
| `flavor_animations` | Optional. `{ "<flavor>": { "frame": [w,h], "clips": {…} } }`: a culture's own clip sheets. |

The base design is the fallback. When a unit is drawn, the owner's `flavor` picks its sprite
from `flavor_sprites` and its clips from `flavor_animations`, each falling back to the design's
own `sprite`/`clips`. So every nation shares one `infantry` design while a Zulu, an Ottoman and a
Qing banner-man are drawn from their own kits. The base art is the Western kit; the base pack
gives each of the other ten flavors its own infantry and cavalry.

Validation rejects an override that names an unknown flavor or a design the rules do not know,
so a typo fails when the pack loads rather than drawing nothing.

## City art and the flag (`visuals.cities`, `visuals.city_banners`)

`visuals.cities` holds exactly one town sprite per flavor, as before. `visuals.city_banners` maps
a flavor to a second, transparent sprite: the flag pole and cloth the town raises. It is drawn
over the town and multiplied by the owner's colour, so the flag reads as that nation's while the
town keeps its own paint. A flavor with no banner simply has no flag.

## The national colour

Every nation's `color` (see [`scenario-format.md`](scenario-format.md)) is multiplied over its
**units**, its city **flag**, and its culture **borders**, so all three read as one nation. The
multiplier is the colour washed most of the way to white, which keeps the art's shading and its
paint (a terracotta roof stays terracotta, a navy coat keeps its folds) but turns its hue to the
nation's. Two nations that share a unit design are told apart by that accent, not by a redraw.

This is the same idea as Civ3's civilization colours. Civ3 keeps a fixed 16-entry accent ramp
(`Art/Units/Palettes/ntpNN.pcx`, light to dark) per colour index and swaps which ramp a unit's
accent index resolves to, leaving the rest of the palette alone; the `.biq` `RACE` row names a
default and a unique colour index per civ (`../open-civ3/reverse-engineering/biq.md`). `open-4x`
does not bundle Civ3's ramps or indexed art — it re-derives the multiplier from the scenario
colour and applies it in true colour, reaching the same "one accent, every civ" result.

Because the colour is a multiplier, a nation's colour should be a single distinctive mid-tone
hue; near-black, near-white and muddy greys multiply poorly over painted sprites and terrain.
`tools/scenario-forge/data/nations.json` is curated with that constraint in mind.
