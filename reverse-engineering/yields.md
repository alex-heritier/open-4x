# Tile yields and the city's per-turn totals

Owns: what one worked tile gives (food, shields, commerce), the three running
sums a city keeps, food eaten and surplus, shield waste and the building
multiplier, commerce corruption and the three-way split, the Wealth
improvement, tourist income, specialists, and the city-screen recompute chain.
Reference: `rust/src/yields.rs` (the tile level, 20 tests) and
`rust/src/city.rs` (the city level, 22 tests). Neighbours: `economy.md` (the
corruption routine `0x4B1190`, the food box, the treasury), `government.md`
(the GOVT flags read here), `rust/src/cell.rs` (the map cell), `NOTES.md`
(terrain and resource data paths).

Method: every address below was read from raw disassembly (radare2 and
capstone over `civ3/civ3-gog/app/Conquests/Civ3Conquests.exe`, static VA equal
to runtime VA); the shipped data were checked against the decoded
`conquests.biq` and the Civilopedia. `HYPOTHESIS` marks the few guesses.

## 1. At a glance

* A tile's yield is **three functions of the `Map` object `0x9C736C`**, one per
  stream (`0x5D7180` food, `0x5D75F0` shields, `0x5D7AD0` commerce), each
  `thiscall` with `(x, y, terrain, civ, planning, city)` and `ret 0x18`. The
  only wrapper is `City::tileYield(kind, x, y)` (`0x4B0330`).
* All three share one skeleton: polluted tile gives **0**; terrain number
  (landmark variant when flagged) minus one for a crater; improvement bonus
  plus a **railroad step**; resource bonus (only when the owner knows the
  reveal tech); water, centre and wonder rules; Golden Age and government
  steps; last the **Despotism cap** (anything above 2 loses 1), which
  **applies to the city centre too**; floor 0.
* The **city centre** overrides food (fixed at the RULE food-per-citizen word,
  2), floors shields at 1 and commerce at 1 (4 for the capital), and adds
  size-class bonuses (metropolis +2 shields, +1 more for Industrious;
  commerce +2 / +1, Commercial +5 / +3).
* A city keeps **three running sums** (`[city+0x1C8/0x1CC/0x1D0]`): the centre
  tile plus every tile whose worked-by word `[cell+0x6C]` equals the city id,
  limited to the 21 tiles of the city radius. Everything else is derived from
  them in a fixed chain (`0x4B10F0`): food eaten and surplus (`0x4B0540`),
  shields (`0x4B05D0`), commerce (`0x4B07C0`), then happiness (`0x4BCFF0`).
* **Food eaten** is `(size - resisters) * 2`; a resisting citizen eats
  nothing; a city in disorder eats everything it makes.
* **Shields** are `(4 + sum of ordinary bonuses + best power plant) * (gross -
  waste) / 4`. A power plant (flag `0x2000`) counts only with its Factory and
  only the best one counts.
* **Commerce** is `net = tiles + tourists - corruption`; luxury and science
  are `(net * rate + 5) / 10` (tenths), tax is the rest; then each stream is
  multiplied by `(2 + n) / 2` for its `n` flagged buildings (science also +2
  per Doubles-Research wonder); Wealth gold is added to tax; everything is
  clamped at 0. **`[city+0x25C]` is luxury and `[city+0x260]` is science**
  (the earlier labels in `economy.md` were swapped; corrected there).
* Wealth converts net shields to gold at RULE "Shield Cost Per Gold" (**4**;
  halved to 2 with Economics). Selling an improvement fetches its cost divided
  by the same word.
* Tourist attractions pay 2 to 14 gold per turn by age, with edges one year
  earlier than the Civilopedia's chart says.

## 2. Entry points

| address | what | notes |
|---|---|---|
| `0x5D7180` | `Map::tileFood(x, y, terrain, civ, planning, city)` | `ret 0x18` |
| `0x5D75F0` | `Map::tileShields(...)` | same signature |
| `0x5D7AD0` | `Map::tileCommerce(...)` | same signature |
| `0x4B0330` | `City::tileYield(kind, x, y)` | `ret 0xC`; kind 0 food, 1 shields, 2 commerce, anything else returns 0 |
| `0x5D16A0` | checked cell accessor | out-of-range index returns the dummy cell `0xCAA330` |
| `0x4B0470` | `City::addTile(index, add)` | adds or removes one tile of the radius |
| `0x4B0E80` | `City::recomputeAll()` | zero the sums, centre, indices 1 to 20, then the chain |
| `0x4B10F0` | `City::recomputeTotals()` | eaten, shields, commerce, then tail-jumps to `0x4BCFF0` |
| `0x4B0540` | food eaten and surplus | |
| `0x4B05D0` | shields: waste and multiplier | |
| `0x4B07C0` | commerce split | |
| `0x4B0710` | tourist gold of one building | |
| `0x4B0AC0` | Wealth gold | |
| `0x4B1190` | corruption / waste (`economy.md`) | `ret 8`, `this` = city |
| `0x560160` | per-turn income: research cell and treasury | `this` = player; sums the streams of its cities |
| `0x4BAF80` | luxury-resource counter (section 8) | `ret 4` |
| `0x4ACA50` / `0x4ACAE0` / `0x4AC9E0` | stream accessors `(i, bonus)` | stream plus specialists / stream alone / specialists alone; science x1.25 when `bonus` is set and the timed bonus runs |
| `0x4C2680` | worked-tile predicate | |
| `0x4BB6F0` | assign a citizen to a tile | writes the city id into `[cell+0x6C]` |
| `0x4C2740` | toggle a tile (city-screen click) | writes the city id or -1 |

**The wrapper** `0x4B0330` builds the cell index `(W / 2) * y + (x >> 1)`
(`W = [0x9C74D4]`, masked to 16 bits), fetches the cell with `0x5D16A0`, calls
the cell's vtable `+0xC8` (the terrain id, no arguments: the three words it
pushed before, `city`, `0` and the owner byte `[city+0x28]`, stay on the stack
and become arguments 6, 5 and 4 of the main call), and calls the tile function
with `(x, y, terrain)` on top. So **`planning` is always 0 here**. The only
caller that passes a non-zero `planning` is the AI tile scorer `0x435BA0`: for
food and shields the flag is its own argument, for commerce it is the constant
1; the weights it applies (`[0x6847F8] = 4.0`, `[0x666AB8] = 1.4`,
`[0x6847FC] = 4.0`, `[0x666AB4] = 1.1`, `[0x684800] = 1.0`) are unread.

`planning` means "as if the tile were improved": it substitutes for the
irrigated, mined and road predicates and for the railroad predicate (the step
still needs the owner to know the railroad tech).

## 3. The cell fields and the vtable slots the yields read

Cell layout is in `rust/src/cell.rs`; the yield code reads these:

| field | meaning | consumer |
|---|---|---|
| vtable `+0x44(0)` | irrigated (overlay bit 3) | food |
| vtable `+0x48` | mined (overlay bit 2) | shields |
| vtable `+0x50(0)` | polluted (overlay bit 6): yield 0 | all |
| vtable `+0x54(0)` | crater (overlay bit 8): -1 on a positive base | all |
| vtable `+0x5C` | railroad predicate `0x5DA0D0`: overlay bit 1 and the owner knows the Railroad job tech | food, shields |
| vtable `+0x60` | river: `byte [cell+4] != 0` | commerce |
| vtable `+0x64` | road predicate `0x5D9FF0`: overlay bit 0 and the owner knows the Road job tech | commerce |
| vtable `+0x6C` | feature bit 16: the bonus-grassland shield | shields |
| vtable `+0x78` | landmark flag, `[cell+0x30]` bit 29: use the landmark column of TERR | all |
| vtable `+0x8C` | water: terrain 11, 12 or 13 (`0x5EAA30`) | all |
| vtable `+0xB4` | has a city: `word [cell+0x1A] != -1` | all |
| vtable `+0xB8` | water body id (`0x4AE280`); table `[0x9C7580] + 40 * body + 0x24` is its size | all |
| vtable `+0xC8` | terrain id: high nibble of `[cell+0x2C]` (`0x5EAB30`) | all |
| vtable `+0x114` | trait mask of the race that owns the city | centre |
| Map vtable `+0x30` | `cell(x, y)` | |
| Map vtable `+0x60` | `freshWater(x, y)` (`0x5F39E0`): river or lake in the 3x3 | food centre |
| `[cell+8]` | resource id, -1 none | all |
| `word [cell+0x6C]` | **id of the city whose citizen works the tile**, `0xFFFF` none | `0x4B0E80`, `0x4C2680` |

The worked-by word's writers (word-sized stores): `0x4BB6F0` (assign: the city
id), `0x4C2740` (UI toggle: the id, or -1 to free), and the -1 clearers
`0x4BA230` (a citizen removed, for instance by starvation), `0x4BA850`,
`0x4BACE0`, `0x4BBC80`, `0x4AECC0` (city destroyed); `0x5663C0` writes an id
for a whole city (HYPOTHESIS: the capture transfer). `rust/src/cell.rs`
does not document this word yet.

A **water body** is "large" when its size word is above 20 (`cmp [body+0x24],
0x14` at `0x5D7470`, `0x5D790F`, `0x5D7D79`, `0x5D7EF4`): ocean-like. A smaller
one is a lake.

## 4. The three tile functions

`T` is the TERR row (table `[0x9C7328]`, stride `0xF0`): normal variant
`+0x64 / +0x68 / +0x6C` base food, shields, commerce and `+0x4C / +0x50 /
+0x54` irrigation, mining, road; landmark variant `+0x88 / +0x8C / +0x90` and
`+0x94 / +0x98 / +0x9C` (getters `0x5E8C70..0x5E8CC0`). `R` is the RESOURCE
row (`[0x9C71D4]`, stride 92): `+0x4C` reveal tech, `+0x50 / +0x54 / +0x58`
bonus food, shields, commerce. A resource counts only when `Player::hasTech(
R.+0x4C)` (`0x561440`; -1 is always true).

### 4.1 Food `0x5D7180`

1. Polluted (`0x5D7192..0x5D71AC`): return 0.
2. `f = T.food`; crater and `f > 0`: `f -= 1` (`0x5D7289..0x5D72A5`).
3. Irrigated or `planning` (`0x5D72C3`): `f += T.irrigation`; then, when
   `T.irrigation > 0`, the railroad predicate or `planning`, and the owner
   knows the railroad tech (`TFRM[4]`, the dword at `[[0x9C7324] + 0x218]`,
   shipped 44 Steam Power; `0x5D7341..0x5D7370`): `f += 1`. An Agricultural
   civ gets `+1` on an irrigated Desert (terrain 0).
4. Resource bonus `R.food` if revealed.
5. Water: large body `f += Harbors` (buildings with BLDG `+0xEC & 0x01000000`
   counted by `City::countFlag` `0x4B1F90` in the working city, `0x5D7484`);
   a lake `f += 1` (`0x5D7470`).
6. City centre (`0x5D74AE..0x5D7520`): `f = [0x9C72B4]` (RULE body `+0xEC`,
   2: the same word that multiplies citizens in `0x4B05A4`), `+1` if the
   city owner's race is Agricultural. Terrain, irrigation, resource and water
   all drop out.
7. Despotism cap (GOVT `+0x1C`, Anarchy and Despotism): `f > 2` gives `f -= 1`,
   except for an Agricultural civ's city centre with fresh water
   (`0x5D759F..0x5D75AE`; the exemption follows the **civ's** trait).
8. `max(f, 0)`.

### 4.2 Shields `0x5D75F0`

1. Polluted: 0.
2. `s = T.shields`; crater and `s > 0`: `-1`.
3. Mined or `planning`: `s += T.mining`; railroad step as for food (needs
   `T.mining > 0`; `0x5D7833..0x5D786E`).
4. Resource bonus `R.shields` if revealed.
5. Grassland (terrain 2) with the feature bit 16: `+1` (bonus grassland).
6. Large water body: `s += Offshore Platforms` (flag `0x00800000`, `0x5D791D`).
7. City centre (`0x5D79B2..0x5D7A35`): class from `0x427540` (population above
   `[0x9C72E8]` = 12 is 2, above `[0x9C72E4]` = 6 is 1, else 0): class 2 adds 2
   (and 1 more for an Industrious civ); class 1 adds 1 (an Industrious test is
   made here too but its result is dropped); then `s = max(s, 1)`.
8. Golden Age (`turn < Player+0x3C`) and `s > 0`: `+1` (`0x5D7A52`).
9. Mobilization (`City::mobilizationBonus` `0x4BFEE0`: the owner is mobilized
   and the city builds a military unit) and `s > 0`: `+1` (`0x5D7A5F..0x5D7A95`).
10. Despotism cap: `s > 2` gives `-1`.
11. `max(s, 0)`.

### 4.3 Commerce `0x5D7AD0`

1. Polluted: 0.
2. `c = T.commerce`; crater and `c > 0`: `-1`.
3. Road (predicate) or `planning`: `c += T.road`.
4. Resource bonus `R.commerce` if revealed (`0x5D7B4C..0x5D7B99`).
5. River: `+1`.
6. Large water body: `c += Commercial Docks` (flag `0x02000000`, `0x5D7D87`).
7. City centre (`0x5D7E1F..0x5D7F56`): class 2 adds 2 (Commercial civ: 5),
   class 1 adds 1 (Commercial: 3); then `c = max(c, 1)`, or `max(c, 4)` for the
   capital (`city+0x20 == Player+0x2C`); a Seafaring owner with a large water
   body adjacent to the city adds 1.
8. Golden Age and `c > 0`: `+1` (`0x5D7F75`).
9. Colossus (`Player::countWonderFlag(0x20, city)`, `0x55A8D0`, BLDG `+0xF8 &
   0x20`, "+1 trade in each trade-producing tile") and `c > 0`: `c += count`
   (`0x5D7F97`).
10. Trade-bonus government (GOVT `+0x20`: Republic, Democracy) and `c > 0`: `+1`
    (`0x5D7FC3`).
11. Despotism cap: `c > 2` gives `-1` (`0x5D7FCB`).
12. `max(c, 0)`.

Order matters in two places: the Golden Age, Colossus and trade-bonus steps
test `c > 0` **before** they add, so a tile with no commerce never gets them,
and the Despotism cap follows them (Despotism on a Colossus coast tile: 2 + 1
= 3, then 2).

### 4.4 Shipped data

TERR rows of `conquests.biq` (normal variant; base food / shields / commerce,
then irrigation / mining / road bonus). The landmark variant differs only
where listed.

| id | terrain | base F/S/C | irrig / mine / road | landmark variant |
|---|---|---|---|---|
| 0 | Desert | 0 / 1 / 0 | 1 / 1 / 1 | |
| 1 | Plains | 1 / 1 / 0 | 1 / 1 / 1 | |
| 2 | Grassland | 2 / 0 / 0 | 1 / 1 / 1 | mining 0 |
| 3 | Tundra | 1 / 0 / 0 | 0 / 1 / 1 | mining 0 |
| 4 | Flood Plain | 3 / 0 / 0 | 1 / 0 / 1 | |
| 5 | Hills | 1 / 1 / 0 | 0 / 2 / 1 | mining 1 |
| 6 | Mountains | 0 / 1 / 0 | 0 / 2 / 1 | mining 1 |
| 7 | Forest | 1 / 2 / 0 | 0 / 0 / 1 | mining 2 |
| 8 | Jungle | 1 / 0 / 0 | 0 / 0 / 1 | |
| 9 | Marsh | 1 / 0 / 0 | 0 / 0 / 1 | irrigation 1, road 0 |
| 10 | Volcano | 0 / 3 / 0 | 0 / 0 / 0 | shields 1, mining 1, road 1 |
| 11 | Coast | 1 / 0 / 2 | 0 / 0 / 0 | |
| 12 | Sea | 1 / 0 / 1 | 0 / 0 / 0 | |
| 13 | Ocean | 0 / 0 / 0 | 0 / 0 / 0 | |

Resource bonuses (food / shields / commerce) and reveal tech: Horses 0/0/1
(The Wheel, 4), Iron 0/1/0 (Iron Working, 7), Saltpeter 0/0/1 (Gunpowder, 30),
Coal 0/2/1 (Steam Power, 44), Oil 0/1/2 (Refining, 53), Rubber 0/0/2
(Replaceable Parts, 57), Aluminum 0/2/0 (Rocketry, 64), Uranium 0/2/3
(Fission, 65); always visible: Wines 1/0/1, Furs 0/1/1, Dyes 0/0/1, Incense
0/0/2, Spices 0/0/2, Ivory 0/0/2, Silks 0/0/3, Gems 0/0/4, Whales 1/1/2, Game
2/0/0, Fish 2/0/1, Cattle 2/1/0, Wheat 2/0/0, Gold 0/0/4, Sugar 1/0/1,
Tropical Fruit 1/0/1, Oasis 2/0/0, Tobacco 0/0/1. The full tables with names
are `TERRAIN` and `RESOURCES` in `rust/src/yields.rs`.

## 5. The city's sums and the chain

### 5.1 City object fields (this section's slice)

| offset | meaning |
|---|---|
| `+0x20` | city id; equals `word [cell+0x6C]` for a worked tile |
| `+0x24 / +0x26` | x, y (signed words) |
| `+0x28` | owner civ (byte) |
| `+0x30` | mood flags: bit 0 civil disorder (`0x4BDFF0`); bit 1 celebration, "We Love the King Day" (`0x4BE440`); bit 2 cleared at the start of every city turn (`0x4BE996`, setter not located); bit 5 the city is building a Capitalization improvement (set and cleared by `0x560160` each turn). `happiness.md` |
| `+0x40` | stored food |
| `+0x70` | draft timer, decremented each turn (`happiness.md`) |
| `+0xCC..+0xD4` | nine reason bytes: what made citizens unhappy, as percentages after a recompute (`happiness.md` section 6) |
| `+0x4C / +0x50` | current build item and kind (kind 1 = improvement) |
| `+0x9C` | bit mask of the resources the city can use (read by `0x4ADE30`; HYPOTHESIS: filled by the trade-network pass) |
| `+0xA8` | the building-age record passed to `0x4C2420` |
| `+0xDC / +0xE0 / +0xEC` | the citizen list: pool pointer `+0xE0` (8-byte slots, the citizen record is at `slot[+4] - 0x1C`), last index `+0xEC` |
| `+0x138` | population |
| `+0x1C8 / +0x1CC / +0x1D0` | gross food, shields, commerce: the running sums |
| `+0x1C0` / `+0x1C4` | propaganda unhappiness (faces) and the hurry-sacrifice timer, both decremented each turn (`happiness.md`) |
| `+0x244` | food eaten |
| `+0x248` | shields lost (waste) |
| `+0x24C` | commerce lost (corruption) |
| `+0x250` | food surplus |
| `+0x254` | net shields |
| `+0x258` | net commerce, then (after the split) the sum of the three streams |
| `+0x25C / +0x260 / +0x264` | luxury, science, tax |
| `+0x268 / +0x26C / +0x270` | the specialists' luxury, research, tax |

A citizen record: byte `+0x20` is non-zero while the citizen is **resisting**
(set by `0x4AC000`, ended with the RESISTANCEENDS message; counted by
`0x4BB2A0`), dword `+0x13C` is the CTZN index (`[0x9C40B0]`, stride `0x80`), dword `+0x128` is
the **mood** (0 happy, 1 content, 2 unhappy, 3 resisting, 4 specialist; `happiness.md`) and dword
`+0x140` the race. Unknown: `+0x12C`, `+0x138`.

### 5.2 Adding and recomputing

* `0x4B0470(index, add)` converts the radius index to an offset with
  `0x5E6E50` (the spiral, `rust/src/spiral.rs`), adds it to the city position,
  wraps by the flags `[0x9C755C]` (bit 0 x, bit 1 y; width `[0x9C74D4]`, height
  `[0x9C74C0]`), skips off-map tiles and adds (or subtracts) the three
  `City::tileYield` values into the running sums **as they are now**. A later
  change of the tile (a finished mine, a government change) is not seen until
  a full recompute.
* `0x4B0E80` zeroes the three sums and adds index 0 (the centre, always)
  and indices 1 to 20 whose `word [cell+0x6C]` equals the city id. The loop
  bound is `cmp ebx, 0x15`: **21 tiles in all**. Then it falls into the same
  chain as `0x4B10F0`.
* The chain (`0x4B10F0`): food eaten (inline copy of `0x4B0540`) and surplus,
  `0x4B05D0` shields, `0x4B07C0` commerce, tail jump `0x4BCFF0` (the happiness
  routine). Happiness therefore runs **last**; a disorder flag it sets is
  first seen by the next recompute.

### 5.3 Food eaten and surplus `0x4B0540`

* `[city+0x30] & 1` (disorder): `eaten = [city+0x1C8]` (all the gross food).
* Else `eaten = ([city+0x138] - resisters) * [0x9C72B4]`, `resisters` being the
  citizens with byte `+0x20` set.
* `[city+0x250] = [city+0x1C8] - eaten`.

`[0x9C72B4]` is RULE body `+0xEC` ("Food Consumption per Citizen", 2). The
earlier report that called this expression "waste" was wrong.

### 5.4 Shields `0x4B05D0`

```text
lost = 0x4B1190(gross, 1)           [city+0x248]
net  = gross - lost                 [city+0x254]
a = 4, b = 0
for each building present (0x4ACB50) and not obsolete (BLDG +0xE0 is -1 or the owner lacks that tech):
    if BLDG +0xEC & 0x2000 (replaces all):
        if the city has BLDG +0x90 (the required improvement): b = max(b, BLDG +0xD0)
    else: a += BLDG +0xD0
net = (a + b) * net / 4              truncating toward zero (cdq; and edx, 3; add; sar 2)
```

Shipped production values (BLDG `+0xD0`): Factory 2, Manufacturing Plant 2
(an ordinary building whose required improvement is the Factory, BLDG row 13),
Coal, Hydro and Solar Plants 2 and Nuclear Plant 4 (the four with flag
`0x2000`, each requiring the Factory), Iron Works 4. So a Factory is +50%,
Factory plus Manufacturing Plant doubles, and a power plant adds +50% (+100%
Nuclear) only while the city has the Factory; of several plants only the
largest counts. The waste routine returns the **whole gross** for a city in
disorder (kind 1 only) and for a government of corruption class 4 (Anarchy);
see `economy.md`.

### 5.5 Commerce `0x4B07C0`, in order

1. `gross = [city+0x1D0] + Σ tourist(b)` over every building `b` the city has
   (present only; `0x4B0710`).
2. `lost = 0x4B1190(gross, 0)` into `[city+0x24C]`; `net = gross - lost` into
   `[city+0x258]`.
3. `lux = (net * Player+0x1A4 + 5) / 10`, `sci = (net * Player+0x1A8 + 5) / 10`
   (signed `0x66666667` division, truncating toward zero). If `lux + sci >
   net`, `lux = net - sci` (`0x4B0887`). `tax = net - lux - sci`.
4. Multipliers: start 2, 2, 2; for each present, non-obsolete building,
   `BLDG +0xEC & 8` adds 1 to the luxury factor, `& 4` to the research factor,
   `& 0x10` to the tax factor. Research also gets `2 * countWonderFlag(0x10,
   city)` (`0x55A8D0`, BLDG `+0xF8 & 0x10`: Copernicus, Newton's, SETI).
   `lux = lux * Lf / 2`, `sci = sci * Rf / 2`, `tax = tax * Tf / 2`, all
   truncating toward zero.
5. `tax += 0x4B0AC0()` (Wealth).
6. Each of the three clamped to at least 0 (`0x4B09ED` loop).
7. `[city+0x258] = lux + sci + tax`.
8. Specialists: `[city+0x268..0x270]` zeroed; for each citizen with byte `+0x20`
   clear, add the CTZN row's outputs (`+0x6C`, `+0x70`, `+0x74` of the row:
   luxury, research, taxes). Shipped: Entertainer 1 luxury, Scientist 3
   research, Tax Collector 2 taxes; Laborer, Policeman, Civil Engineer 0.

The specialists are **not** in `[city+0x258]` or the three streams. Three
accessors, each `ret 8` with `(i, bonus)`, give the views: `0x4ACAE0` the stream
alone (`[city+0x25C + 4i]`), `0x4AC9E0` the specialists alone (`[city+0x268 +
4i]`), `0x4ACA50` their sum. For the science stream only (`i == 1`) and only
when the caller's `bonus` argument is non-zero, the value is multiplied by 1.25
(`fild`, `fmul [0x66905C]`, `_ftol`: `x * 5 / 4`) while the owner's timed
research bonus runs (`0x55C890`: `Player+0x15D0` bit 0 set and `turn <=
Player+0x15D4`). The player-level research total `0x55D810` adds `0x4AC9E0(city,
1, 0)` and `[city+0x260]` over the owner's cities and applies the 1.25 **once**
at the end; `0x562200`, `0x566140` and `0x569E80` are the same family. The
per-turn income routine `0x560160` is the proof of the stream order: for each
city of the player it adds `0x4ACA50(city, 1, 0)` to `Player+0xF8` (the
research cell) and `0x4ACA50(city, 2, 0)` to the treasury (re-splitting the
two treasury cells with the tamper guard each time); it also sets or clears
bit 5 (`0x20`) of the city's mood word `[city+0x30]` according to whether the
city builds a Capitalization improvement (`[city+0x50] == 1`, BLDG `+0xEC &
0x80000`). Luxury (stream 0) is consumed by the happiness code.

**Flags that matter for the shipped rules.** No shipped BLDG row has flag
`0x8` ("+50% Luxury Output"), so the luxury factor stays 2 and the luxury
stream is never multiplied. The Marketplace has `0x10` (tax) and `0x400`
("Increases Luxury Trade"); `0x400` is consumed by the luxury-resource counter
`0x4BAF80` (section 8), not by this split. Library, University and Research
Lab carry `0x4`; Marketplace, Bank and Stock Exchange carry `0x10`.

### 5.6 Wealth `0x4B0AC0`

Only when `[city+0x50] == 1` (an improvement is being built), BLDG `+0xEC &
0x80000` (Capitalization: Wealth) holds for `[city+0x4C]` and `[city+0x254] >
0`. `d = [0x9C7268]` (RULE body `+0xA4`, "Shield Cost Per Gold", **4** in
`conquests.biq`); if the owner knows a tech with flag `0x1000` (TECH `+0x68`
bit 12, "Doubles Effect of Wealth": Economics, `0x561480`), `d = max(1, d / 2)`.
Result: 1 if `d > net`, else `net / d`. The gold goes to the tax stream
(`0x4B09D1`). What the shields do in the production step is not read here.

### 5.7 Tourists `0x4B0710`

Zero unless BLDG `+0xF8 & 0x20000` (Tourist Attraction: the ancient wonders
plus Copernicus, Shakespeare, Leonardo, Bach, Newton, Hoover, United
Nations). `age = 0x4C2420(city+0xA8, building)` (HYPOTHESIS: years since the
wonder was built; the record is not decoded). Steps: `age <= 1000` 0; `< 1500`
2; `< 1750` 4; `< 1875` 6; `< 2000` 8; `< 2250` 10; `< 2500` 12; else 14.
The Civilopedia chart lists the same seven amounts with the upper edges inclusive
(1000-1500, 1501-1750, ...); the code switches at the edge itself, so at age
1500 it already pays 4.

## 6. Corrections to earlier notes

* `economy.md` "Waste": the `(population - tilecount) * [0x9C72B4]` expression is
  **food eaten** (5.3), not corruption; `tilecount` is the resister count.
* `economy.md` commerce split: `[city+0x25C]` is luxury, `[city+0x260]` is
  science; `Player+0x1A4` is the luxury rate and `+0x1A8` the science rate
  (tenths). The multiplier bits were child-reported and are now verified.
* `economy.md` "`0x4B1190` returns lost commerce (disorder and anarchy return
  gross)": it returns **lost** commerce for kind 0 and lost shields for kind
  1; disorder returns gross only for kind 1; class 4 (Anarchy) returns gross
  for both; no capital returns 0.
* `economy.md` "`[0x9C7268]` is open": it is 4, RULE body `+0xA4`; a sold
  improvement fetches `cost / 4` (cost in the game's units: BIQ cost times X).
* `economy.md`, `rust/src/economy.rs`: "the Despotism cap site is not located,
  the city square is exempt": the cap is the last step of each tile function
  and it applies to the city centre (`0x5D759F` is the one exemption).
* `TOWN_MAX` and `CITY_MAX` (`[0x9C72E4]`, `[0x9C72E8]`) were "matched by
  value": the RULE reader `0x5E78E0` stores them directly (`combat.md`
  section 9 gives the address-to-body map); the decoded RULE has 6 and 12.
* `economy.md` lead on `0x4BE440`: the global it compares the population with,
  `[0x9C72D4]`, is not "a turn-like counter" but RULE body `+0x10C` (6). `0x4BE440` is only
  the celebration test (`happiness.md` section 8).

## 7. Verified, hypothesis, never located

Verified (raw disassembly, plus data checks): sections 2 to 5 as written.
Cross-checked with the Civilopedia: Factory and power plant percentages,
Wealth ratios, tourist amounts (but not its edge years).

HYPOTHESIS: `[city+0x9C]` as the usable-resource mask; the unit of the
building-age record; the role of `0x5663C0` as the transfer writer of
`[cell+0x6C]`.

Never located or unread:

* (Resolved, see `happiness.md`: the happiness routine `0x4BCFF0` and its family, and `city +0xCE`,
  which is reason byte 2.)
* Citizen fields `+0x12C`, `+0x138` (`+0x128` is the mood); the third argument of `0x4ACB50`
  (always 1 in the chain above); the unit and storage of the age in `0x4C2420`.
* The roles of the callers `0x42B650`, `0x435430`, `0x435520`, `0x4B9F60`,
  `0x4BA6D0` of the tile or total routines.
* The Civil Engineer's construction bonus (the CTZN construction value is
  applied elsewhere).
* The weights of the AI tile scorer `0x435BA0`.
* `[0x9C7260]` (RULE body `+0x9C`, 20).
* The timed research bonus (`Player+0x15D0` bit 0, `+0x15D4` last turn), the Science Age: **it is
  set**, by `0x55C830(P, 1)` (flag set, last turn = turn + 20), whose only caller is the unit action
  `0x5C03B0` (a leader with the scientific-leader marker consumed inside a city); `0x558C20`
  clears it at construction, `0x560050` expires it, `0x558FC0` saves and restores it. See
  `research.md` section 11. (An earlier revision of this file claimed nothing sets it; wrong.)
* What Wealth shields do in the production step, and `0x4B0AB0` (returns
  `[city+0x3B8]` while an improvement is being built).
* Whether a city can ever work more than the 21 tiles of the radius: `0x4B0E80`
  stops at index 20.

## 8. The luxury-resource counter `0x4BAF80` (side result)

`0x4BAF80(city, flag)` (`ret 4`), found while checking flag `0x400`: counts the
RESOURCE rows (`[0x9C3DA4]`, stride 0x5C) with category 1 (luxury, `0x5E3720`:
`[row+0x3C] == 1`) that the city can use (`0x4ADE30`), saturating at 11; counts
the present, non-obsolete buildings with BLDG `+0xEC & 0x400`; returns
`table[n]` when there is such a building or `flag != 0`, else `n`. The table at
`0x665868` is 0, 1, 2, 4, 6, 9, 12, 16, 20, 24, 28, 32 for n = 0 to 11. The
only caller is the AI's improvement scorer `0x42C8A0` (`0x42F7FF`), which adds
`4 * (table[n] - n)` to the score of a Marketplace-class building. The table is
**happy faces**: the recompute `0x4BCFF0` runs the same count (`0x4BD0E3..0x4BD14C`) and adds
`table[n]` happy faces with such a building, `n` without (`happiness.md` section 3, step 6).
