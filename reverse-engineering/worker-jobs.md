# Terrain improvement jobs (worker orders)

Clean-room specification of the **terrain improvement jobs** of `Civ3Conquests.exe` (PE32, MSVC 6, image base
`0x400000`): the `TFRM` job table and its in-memory rows, the order tokens that select a job, the legality predicate
`Player::canImprove` `0x55EFA0`, the per-turn work accumulator `Unit::workOnTile` `0x461470`, the worker rate
`Unit::workRate` `0x5B33C0`, the completion effects, and the side effects of setting and clearing the overlay bits.
Not covered here (own documents or open): the **automation** brain that picks jobs for automated workers (section 12),
the movement/ZOC/vision chain (`workers.md`, thin), the colony objects an Airfield/Radar/Outpost job creates
(`colonies.md`), and the tile-ownership side (`borders-culture.md`). Tags: **V** read from the instructions with the body
opened, **H** hypothesis, **O** open. No reference code is given on purpose.

Conventions (`primitives.md`): `__thiscall` (`ecx` = this), `ret N` callee cleanup, the last pushed argument is the first
parameter. Player at `0xA52E98 + 0x20E4 * slot`. Map object `0x9C736C`. A cell is fetched with `0x5D16A0(map; index)`,
`index = ((W >> 1) * y + (x >> 1)) & 0xFFFF`; `x` is the raw coordinate (`x + y` even on valid tiles, `workers.md`).
Unit fields (`unit-turn.md` 2): `+0x24`/`+0x28` x/y, `+0x34` owner slot, `+0x38` nationality (a RACE row), `+0x40` PRTO
row, `+0x50` movement used this turn, `+0x64` order id. This document adds `+0x54` and `+0x58` (section 6).

## 1. Data

### 1.1 `TFRM` rows (the job table) **V**

Memory table `[0x9C7324]`, stride `0x74` (116 bytes: the 4-byte length word, then the 112-byte BIQ body, so memory offset =
body offset + 4). The BIQ layout is in `biq/src/sections/tfrm.rs`; the offsets the game reads:

| memory | field | reader |
|---|---|---|
| `+0x04` | name (32 bytes) | the confirmation dialog of section 3.4 (`0x55F6A9`: `row + 4`) |
| `+0x44` | **turns to complete** | `workOnTile` `0x4615A4` |
| `+0x48` | **required tech** (`-1` none) | `canImprove` `0x55F4AB`, the Irrigation token pre-check `0x5C21B7` (`+0xBC` = row 1 `+0x48`) |
| `+0x4C` | **required resource 1** (`-1` none) | `Player::jobCityGate` `0x55EEB0` (section 4) |
| `+0x50` | required resource 2 | no reader found in `canImprove` or `0x55EEB0`; a whole-image consumer search was **not** done (**O**) |

Job ids are the row indices; the executable hard-codes them (the tables and branches below test the numbers 0..12), so a
scenario cannot reorder them. Shipped `conquests.biq` (13 rows; names and techs from the same file):

| job | name | turns | tech | resources | order token (section 2) |
|---|---|---|---|---|---|
| 0 | Mine | 12 | none | none | `0x20000020` |
| 1 | Irrigation | 8 | none | none | `0x20000040` |
| 2 | Fortress | 16 | 20 Construction | none | `0x20000010` |
| 3 | Road | 6 | none | none | `0x20000004` |
| 4 | Railroad | 12 | 44 Steam Power | 1 Iron, 3 Coal | `0x20000008` |
| 5 | Plant Forest | 18 | 23 Engineering | none | `0x20000200` |
| 6 | Clear Forest | 4 | none | none | `0x20000080` |
| 7 | Clear Wetlands | 16 | none | none | `0x20000100` |
| 8 | Clear Damage | 24 | none | none | `0x20000400` |
| 9 | Airfield | 1 | 58 Flight | none | `0x20002000` |
| 10 | Radar Tower | 1 | 63 Advanced Flight | none | `0x20004000` |
| 11 | Outpost | 1 | 1 Masonry | none | `0x20008000` |
| 12 | Barricade | 16 | 20 Construction | none | `0x20010000` |

### 1.2 Job rows (the effect table) **V**

Five dwords per job at `0x9C73B0 + 20 * job` = map object `+0x44 + 20 * job` (13 rows, `0x44..0x147`). Built by
`0x5E89D0(row; job)` from the map constructor `0x5F3A80` (a jump table at `0x5E8AB0`; default row = all zero, `+0x10 = -1`).
Fields: `+0x00` **A** (overlay bits the job sets), `+0x04` **B** (bits it clears), `+0x08` **C** (bits that must already be
present), `+0x0C` the job id, `+0x10` **D** (terrain the job produces, `-1` none). Overlay bit numbers (cell plane 0):
0 road, 1 rail, 2 mine, 3 irrigation, 4 fortress, 5 hut, 6 pollution, 7 camp, 8 crater, 28 barricade, 29 airfield, 30 radar,
31 outpost.

| job | A set | B clear | C required | D |
|---|---|---|---|---|
| 0 Mine | `0x00000004` | `0x00000008` | 0 | -1 |
| 1 Irrigation | `0x00000008` | `0x00000004` | 0 | -1 |
| 2 Fortress | `0x00000010` | `0xE0000000` | 0 | -1 |
| 3 Road | `0x00000001` | 0 | 0 | -1 |
| 4 Railroad | `0x00000002` | 0 | `0x00000001` | -1 |
| 5 Plant Forest | 0 | `0x0000000C` | 0 | 7 (Forest) |
| 6 Clear Forest | 0 | 0 | 0 | 14 |
| 7 Clear Wetlands | 0 | 0 | 0 | 14 |
| 8 Clear Damage | 0 | `0x00000040` | 0 | -1 |
| 9 Airfield | `0x20000000` | `0xC000001C` | 0 | -1 |
| 10 Radar Tower | `0x40000000` | `0xA0000010` | 0 | -1 |
| 11 Outpost | `0x80000000` | `0x60000010` | 0 | -1 |
| 12 Barricade | `0x10000000` | `0xE0000000` | `0x00000010` | -1 |

`D = 14` is a sentinel meaning "the base terrain under this tile" (section 7, step 2): the cell's base-terrain slot
(cell vtable `+0xC4`) replaces the current terrain. Jobs 5, 6, 7 are the only rows with `D != -1`.

### 1.3 Terrain table `[0x9C7328]`, stride `0xF0` **V**

| offset | meaning | used by |
|---|---|---|
| `+0x4C` / `+0x50` / `+0x54` | irrigation / mining / road bonus | the job gates (nonzero = allowed), section 3 |
| `+0x5C` | movement cost | the work requirement (section 6) |
| `+0x70` | **worker job** (`-1` none) | jobs 5/6/7 only act on a terrain whose `+0x70` equals the job |
| `+0x78` | Allow Cities | |
| `+0x7A`, `+0x7B` | Impassable, Impassable-by-wheeled | |
| `+0x7C` / `+0x7D` / `+0x7E` / `+0x7F` | Allow Airfields / Forts / Outposts / Radar Towers | jobs 9 / 2+12 / 11 / 10 |
| `+0x94` / `+0x98` / `+0x9C` / `+0xA0` | the landmark variants of irrigation bonus / mining bonus / road bonus / movement cost | used instead of `+0x4C/+0x50/+0x54/+0x5C` when the cell has a landmark |

The four accessors are `Cell::irrigationBonus` `0x5DBE70`, `Cell::miningBonus` `0x5DBEC0`, `Cell::roadBonus` `0x5DBF10`,
`Cell::moveCost` `0x5DBF60`: each tests the cell's landmark flag (feature-plane bit 29, cell vtable `+0x78`) and returns the
landmark field (`0x5E8CA0/B0/C0/D0`) or the plain field. **V**

Shipped terrain rows (id: worker job; allow flags city/colony/airfield/fort/outpost/radar; bonuses irrigation/mine/road;
move cost): 0 Desert: -1; all 1; 1/1/1; 1. 1 Plains: **5**; all 1; 1/1/1; 1. 2 Grassland: **5**; all 1; 1/1/1; 1.
3 Tundra: **5**; all 1; **0**/1/1; 1. 4 Flood Plain: -1; all 1; 1/**0**/1; 1. 5 Hills: -1; all 1; 0/2/1; 2. 6 Mountains: -1;
city 0, others 1; 0/2/1; 3. 7 Forest: **6**; all 1; 0/0/1; 2. 8 Jungle: **7**; all 1; 0/0/1; 3. 9 Marsh: **7**; city 0,
colony 0, airfield 0, fort 0, **outpost 1**, radar 0; 0/0/1; 2. 10 Volcano: -1; all five flags 0; 0/0/0; 3.
11 Coast, 12 Sea, 13 Ocean: -1; all 0; 0/0/0; 1. The bonus columns are the raw BIQ values (the gates test them for nonzero only).

### 1.4 Rules and globals

| datum | where | meaning |
|---|---|---|
| `[0x9C727C]` | `RULE` row `+0x98` (`0x9C71E4 + 0x98`), `forest_value_in_shields` | shields granted by clearing a forest (section 9) |
| `[0x9C71D8]` GOVT, stride `0x1E8` | `+0x1C0` | **worker rate** (shipped: Anarchy 1, Despotism 2, Monarchy 2, Communism 2, Republic 2, Democracy 3, Fascism 4, Feudalism 2) |
| `[0x9C71E0]` PRTO, stride `0x138` | `+0x12C` (float) | **worker strength** (shipped: Worker 1.0, Crusader 1.0, the two TOW Infantry rows 1.0, all others 0.0 or absent) |
| `[0x9C71D0]` RACE, stride `0x974` | `vtable[0]` `0x53A080(trait)` | trait test; **5 = Industrious** (`biq/src/sections/race.rs` `trait_id`) |
| `Player +0xA0` | government index | |
| `Player +0x20` | race (civilization) index | |
| `hasTechFlag(mask)` `0x561480` | TECH flag word | mask `0x40000` = *Doubles Work Rate of Workers* (stock Replaceable Parts, `biq/src/sections/tech.rs`) |
| `[0xA526BC]`, `[0xA526C0]` | per-slot bit masks | `0xA526BC`: the civs for which `workOnTile` skips the order re-validation (section 6, step 2; **H**: the AI civs); `0xA526C0`: civs whose UI is refreshed (**H**) |
| `[0x9FD4BC]` | local human slot | |
| `[0xA281C4]` | redraw-dirty byte | |

## 2. Order tokens **V**

The unit-action gate `Unit::canDoAction(token)` `0x5C1AD0` (`ret 4`, `ecx` = unit) splits the token as
`(word << 28) | mask`. The **word 2** branch (`0x5C2054`) handles `0x2xxxxxxx`: a 128-entry byte index at `0x5C2C10` and a
nine-entry jump table at `0x5C2BEC` for the single-bit tokens `0x20000001..0x20000080`, then a compare chain for the rest.

| token | meaning | gate |
|---|---|---|
| `0x20000001` | found a colony | `0x5D7080(x, y, owner) == 0` (`colonies.md`) |
| `0x20000002` | found a city | `0x5F3160(map; x, y, owner, 1) == 0` (`city-founding.md` 2A.1) |
| `0x20000004` | Road | `canImprove(3, x, y, 0)` |
| `0x20000008` | Railroad | `canImprove(4, x, y, 0)` |
| `0x20000010` | Fortress | `canImprove(2, x, y, 0)` |
| `0x20000020` | Mine | `canImprove(0, x, y, 0)` |
| `0x20000040` | Irrigation | first `knowsTech(TFRM[1].requiredTech)` (`0x561440`), then `canImprove(1, x, y, 0)` |
| `0x20000080` | Clear Forest | `canImprove(6, x, y, 0)` |
| `0x20000100` | Clear Wetlands | `canImprove(7, x, y, 0)` |
| `0x20000200` | Plant Forest | `canImprove(5, x, y, 0)` |
| `0x20000400` | Clear Damage | `canImprove(8, x, y, 0)` |
| `0x20000800` | Automate | `Player +0x194 > 0` (**H**: the player has at least one city) |
| `0x20001000` | **O** (city-tile action) | a city at the unit's tile, `City::0x4B1DC0() == -1` and `City +0x250 >= 0` |
| `0x20002000` | Airfield | `canImprove(9, x, y, 0)` |
| `0x20004000` | Radar Tower | `canImprove(10, x, y, 0)` |
| `0x20008000` | Outpost | `canImprove(11, x, y, 0)` |
| `0x20010000` | Barricade | `canImprove(12, x, y, 0)` |

Any other word-2 token fails (`0x5C1C60`). The player is `Player[unit.+0x34]`, `(x, y)` = `unit.+0x24/+0x28`. The standing
**order id** stored in `unit +0x64` while a unit works on job `j` is `j + 2` (the completion loop of section 7 compares
`+0x64` with `job + 2`, `0x46176A`). **V**

## 3. `Player::canImprove(job, x, y, confirm)` `0x55EFA0` (`ret 0x10`) **V**

`this` = the acting player (slot `P.+0x1C`). The last argument enables the destruction confirmation of section 3.4. The
routine first calls `0x49FC50()` (the UI/dialog object, kept for the confirmation). Let `cell` = the cell at `(x, y)`,
`W = cell.vt+0xA8(0)` (plane-0 overlay word), `T` = the cell's terrain id (`vt+0xC8`).

### 3.1 Per-job head (any failure returns 0)

| job | test |
|---|---|
| 9 Airfield | owner byte `o = cell.vt+0x98()`: if `o != 0` it must equal `P.+0x1C`; then `TERR[T].+0x7C != 0` |
| 11 Outpost | same ownership rule; then `TERR[T].+0x7E != 0` |
| 10 Radar Tower | `o` must equal `P.+0x1C` (an unowned tile **fails**); then `TERR[T].+0x7F != 0` |
| 0 Mine | `cell.miningBonus() != 0` (`0x5DBEC0`) |
| 1 Irrigation | `cell.irrigationBonus() != 0` (`0x5DBE70`), **and** (`0x5D8400(map; x, y, 1) != 0` **or** the player knows any tech whose TECH flag word has bit 1 set: scan of all techs, `[0x9C7320] + 0x68 + 0x74*t`, known-by mask `[0xA52B4C][t]` bit `P.+0x1C`) |
| 3 Road, 4 Railroad | `cell.roadBonus() != 0` (`0x5DBF10`) |
| 8 Clear Damage | `cell.vt+0x58(0)` is true (H: the tile is polluted or cratered) |
| 2 Fortress | `TERR[T].+0x7D != 0` |
| 12 Barricade | `TERR[T].+0x7D != 0`, and `cell.vt+0x34(0)` (a fortress is present), and not `cell.vt+0x38(0)` (no barricade yet) |
| 5, 6, 7 | no head test |

### 3.2 The water-source test `0x5D8400(map; x, y, chain)` (`ret 0xC`) **V**

1. If `map.vt+0x60(x, y)` (`0x5F39E0`) is true return 1.
2. For spiral index `k = 1..8` (the eight neighbours, `primitives.md` 1.8): wrap and bound the tile; if the neighbour has
   irrigation (cell vtable `+0x44(0)`) return 1; else if `chain != 0` and the neighbour has a city (`0x5EA6C0`) and
   `0x5D8400(map; nx, ny, 0)` is true return 1.
3. Return 0.

`0x5F39E0(x, y)`: true if `map.vt+0x54(x, y, 9)` (`0x5F38C0`) or the cell's river predicate (`vt+0x60` → `vt+0x94() != 0`).
`0x5F38C0(x, y, n)`: true if, among the spiral tiles `k < n` (n = 9: the 3x3 block including the centre), some in-bounds tile
is water (`vt+0x8C`, terrain id in [11, 14)) whose continent record (`map.vt+0x84(continentId)`) has `+0x24 <= 20`. **H**
for the meaning of that field (a tile count: small water bodies, i.e. lakes, count; the open ocean does not).

### 3.3 Common tail (reached by every job that passed its head) **V**

1. The tile must not hold a city (`0x5EA6C0`).
2. `req = TFRM[job].+0x48`: `-1` passes; `req == techCount` (`[0x9C3DBC]`, one past the last tech) **fails** (the "never
   available" value, exactly `knowsTech` `0x561440` semantics); otherwise the player must know `req` (`[0xA52B4C][req]` has
   bit `P.+0x1C`).
3. `A = row[job].A`: if `W & A != 0` the job is **already done**: fail.
4. `C = row[job].C`: `W & C` must equal `C` (prerequisite bits present): else fail.
5. If `row[job].D != -1` (jobs 5, 6, 7): the owner byte must be `0` or `P.+0x1C`; and `TERR[T].+0x70` must equal `job`.
6. `Player::jobCityGate(job; x, y)` (`0x55EEB0`, section 4) must pass.
7. Confirmation (section 3.4); then return 1.

### 3.4 Destruction confirmation **V** (UI only)

Reached when not multiplayer (`0x47B530() == 0`), the `confirm` argument is non-zero, and `P.+0x1C == [0x9FD4BC]`. Let
`X = W & row[job].B` (existing improvements the job would remove). If `X != 0`, for each job row `k = 0..12` whose `A` has
a bit in `X`, post the dialog `MIMIMI` (`0x72C9B4`) naming `TFRM[k].name`; if any is declined (`0x611530` returns 0) the
routine returns 0. The token gates above always pass `confirm = 0`; the callers that pass it are the UI order handlers.

## 4. `Player::jobCityGate(job; x, y)` `0x55EEB0` (`ret 0xC`) **V**

Job 3 (Road) is exempt (jump to `0x55EF88`; the body there is not part of the resource test; **O**). Otherwise
`res = TFRM[job].+0x4C`; if `res == -1` the gate passes. If not, scan the player's cities in pool order
(`[0xA52E6C]`, indices `0..[0xA52E78]`, owner byte `+0x28` equal to the player): the gate passes at the first city `C` for
which both `0x57F130(0xB72888; C, x, y)` is true (the resource-network cell query, `primitives.md` 4) and
`City::resourceUsable(C; res)` `0x4ADE30` is true. If no city qualifies the gate fails. In the shipped file only Railroad
(Iron) has a resource.

## 5. The action record flow

A worker order carries the order id `job + 2` in `unit +0x64` (section 2). **H**: the UI handlers and the automation write
it, and the per-turn executors call `workOnTile(job)` each turn while the order stands. Callers found
(`call 0x461470`): the routines at `0x461F90`, `0x4620D0`, `0x4622D0`, `0x462670`. **V** for `0x461F90` (`ret` plain, `ecx` = unit): it is a **go-to-and-build** executor for the orders `0x11` and `0x12`, not the job chooser. Order `0x11`: if `Player.canImprove(job 3, U.x, U.y, 0)` (`0x55EFA0`) or `U.+0x58 == 3`, call `workOnTile(3)` (`0x461470`) and return. Order `0x12`: the same test for job 3, then for job 4. Otherwise (nothing to build on the unit's own tile): if `0x5B3290(U)` returns 0, ask the path finder `0x580540(0xB72888; U.x, U.y, U.+0xB0, U.+0xB4, U, owner, 0x143, 0)` for a step direction; if it is positive, take it through the unit vtable `+0x4C(dir, 0)`; the order is cancelled (`setOrder(0)`) when there is no step, the step fails, or the unit has no movement left to continue (`0x5BE5B0(U) - U.+0x4C` test). The other three routines were seen to have the same shape (**H**); bodies **O**.
`canImprove` is also called from `0x42B820`, `0x433CD0`, `0x435F80`, `0x454DA0`, `0x45D814`, `0x45EC1B` (AI planners and UI
handlers; bodies **O**).

## 6. `Unit::workOnTile(job)` `0x461470` (`ret 4`, `ecx` = unit) **V**

Unit fields used: **`+0x54` accumulated work** (dword), **`+0x58` current job** (dword, `-1` none), `+0x50` movement used,
`+0x64` order id, `+0x38` nationality. `(x, y)` = the unit's tile, `cell` its cell.

1. If the cell has a city (`0x5EA6C0`): `Unit::cancelOrders(0)` (`0x5B3040`) and return.
2. Order re-validation. Let `m = 1 << P.+0x1C`. If `[0xA526BC] & m == 0`, or `unit.+0x68 != 0` (the byte cleared by
   cancel-orders, `unit-turn.md` 2): evaluate `f = (0x5BC8B0(unit; 0x11) == 0)` (`ret 4`, one argument) and then
   `ok = 0x449810(Player; x, y, 1, f, 0, 1, 0, 0x31, 0)` (`ret 0x24`, nine arguments); if `ok` is false: `cancelOrders(0)`
   and return. (**O** for both helpers: `0x449810` lies in the planner range, **H** "is this tile still worth working".)
   Otherwise (`[0xA526BC]` has `m` and `+0x68 == 0`) skip the check.
3. `need = TFRM[row[job].jobId].turns * cell.moveCost()` (`0x5DBF60`; the landmark variant when present). `row[job].jobId`
   (`+0x0C`) equals `job`.
4. `rate = Unit::workRate(job)` (section 6.1); `unit.+0x54 += rate`; `unit.+0x58 = job`; `unit.+0x50 = Unit::moveBudget()`
   (`0x5BE470`: the unit's full movement allowance, so it cannot move this turn). If `[0xA526C0]` has the **local** slot
   bit, the UI hook `0x4068E0(0x73DA40; unit, job)` runs.
5. `total` = the sum of `+0x54` over **every unit on the tile** whose `+0x58 == job` (the tile's unit list through the list
   head `0xA52DD4`, cell vtable `+0xA0`; includes the acting unit, whose field was just updated). Own units and foreign units
   are both counted (no owner test). **V**
6. If `total < need` return (the work continues next turn). Otherwise the job completes (section 7).

### 6.1 `Unit::workRate(job)` `0x5B33C0` (`ret 4`) **V**

Computed in x87 single precision with a final `_ftol` (truncation toward zero):

```
R  = float(GOVT[P.+0xA0].+0x1C0)                       # 1, 2, 3 or 4 in the shipped file
if RACE[P.+0x20].hasTrait(5):          R = R * 1.5     # Industrious; constant 0x666AE4 = 0x3FC00000
if P.hasTechFlag(0x40000):             R = R + R       # Doubles Work Rate
if unit.+0x38 != P.+0x20:              R = R * 0.5     # a foreign-nationality unit; constant 0x6653B4 = 0x3F000000
R  = R * PRTO[unit.+0x40].+0x12C                       # worker strength (float)
return max(1, trunc(R))
```

The order of the multiplications is as shown (the Industrious product is rounded to a 32-bit float before the next step).
Golden values: Despotism worker (strength 1.0): 2; Industrious: 3; foreign nationality: 1; Fascism Industrious with
Replaceable Parts: 12; a unit with strength 0 (e.g. a Warrior) returns the floor 1. Required work examples: Road on
Grassland 6 x 1 = 6; Mine on Hills 12 x 2 = 24; Railroad on Mountains 12 x 3 = 36; Airfield on Hills 1 x 2 = 2.

## 7. Completion effects (in this order) **V**

Let `A`, `B` = `row[job].A`, `.B`; `wasForest` = (terrain id == 7) before anything changes.

1. **Release the workers.** For every unit on the tile: if `+0x58 == job` set `+0x54 = 0` and `+0x58 = -1`; if
   `+0x64 == job + 2` call `cancelOrders(0)`. (`0x461724..0x4617A8`)
2. **Terrain change.** If `TERR[terrain].+0x70 == job`: when `job == 5` and the cell's base terrain (`vt+0xC4`) is 3
   (Tundra), set feature-plane bit `0x200000` (`vt+0xE0(2, 0x200000, -1, -1)`; **H**: a "tundra forest" art variant); then
   `Map::setTerrain(map; t, x, y)` `0x5D59E0` with `t = cell.vt+0x11C()` = `0x5E9A00`: `j = TERR[terrain].+0x70` (`-1` returns
   `-1`), `D = row[j].D`; if `D == 14` the result is the base terrain (`vt+0xC4`), else `D`. (So Plant Forest produces
   terrain 7; Clear Forest and Clear Wetlands restore the base terrain.) `0x5D59E0` writes the terrain (`cell.vt+0x128`),
   and, if the base-terrain class of the tile changed, re-tests the four orthogonal neighbours (`0x5EBDC0`) and may rewrite
   surrounding tiles (the coast/lake recomputation; **O** in detail). **V** for the call; **O** for the neighbour rule.
3. **Fortress or Barricade (jobs 2 and 12) destroy colonies.** If the cell has a colony id (`vt+0x68`, `0x5EA910`): the
   airfield (bit 29) is destroyed through `0x5DAEC0(airfield; 1)` (the airfield pool `[0xA52E3C]`, node `- 0x1C`); a radar
   (bit 30) through `0x5D6360(map; x, y)`; an outpost (bit 31) through `0x5D6430(map; x, y)` (`colonies.md`). This mirrors
   `B = 0xE0000000` of those rows.
4. **Landmark.** For jobs 5, 6, 7: if the cell has a landmark (feature-plane bit 29) it is cleared (`vt+0x7C(0)`).
5. **Cell reset.** For jobs 0, 1, 2, 12, 3, 4, 5, 9, 10, 11: `cell.vt+0xF0(0)` writes `0` into the cell dword `+0x24`
   (the same write city founding performs, `goody-huts.md` 3.5; the reader of the field is **O**).
6. **Clear Damage (job 8).** If the cell is *not* polluted (`vt+0x50(0)`) but is cratered (`vt+0x54(0)`), `B |= 0x100`
   (the crater bit is cleared too). If the cell is polluted only the pollution bit goes (`B = 0x40`); a crater under pollution
   is removed by a second job.
7. **Clear then set.** `Cell::clearBits(0; B, x, y)` `0x5DA3E0`, then `Cell::applyOverlay(cell; 0, A, x, y, unit)` `0x5DA240`
   (section 8).
8. **Forest chop bonus** (section 9), if `wasForest`.
9. Cleanup: `0x4E69D0(0x9F8700; unit, 0, 0)` (UI); if it returns true and the local civ's slot bit is set in the cell's
   discovered mask (`cell.+0x58`), the dirty byte `[0xA281C4]` is set; finally `0x4069C0(0x73DA40; unit)` (UI). After an
   Airfield/Radar/Outpost job the unit is already dead and the hooks still receive its pointer. The order of the calls is **V**.

### 7.1 Effect of a worker finishing a colony job

`applyOverlay(layer 0)` runs before the raw set: if `A & 0x20000000` it calls `Unit::buildAirfield` `0x5B3790`; if `A < 0`
(bit 31) `Unit::buildOutpost` `0x5B38F0`; if `A & 0x40000000` `Unit::buildRadar` `0x5B3840`. Each of those creates the colony
object (`colonies.md`), redraws, and **kills the acting worker** (`Unit::kill` `0x5BBBC0` with the nationality-loss argument
`a2 = 1`). Only the unit that issued the completing turn is consumed; other workers on the tile were released in step 1.
Then the raw set `vt+0xE0(0, A, x, y)` follows.

## 8. Setting and clearing overlay bits **V**

Plane `p` of a cell is the dword at `cell + 0x28 + 4p`: plane 0 overlay, plane 2 feature word (`+0x30`).

`Cell::clearBits(layer; mask, x, y)` `0x5DA3E0` (`ret 0x10`): the raw clear `0x5EAB40` (`word &= ~mask`; clearing the road bit
of plane 0 also clears the rail bit); then if the tile **had** a road and `mask & 3` and the tile is in bounds,
`0x57DEF0(0xB72888; x, y)` (incremental road/trade network update); if `mask & 0xF` (road, rail, mine, irrigation) the city
that works the cell (`cell.+0x6C`, pool index) runs `City::0x4B0E80()` then `vtable[0x38](0)`; if `mask & 0x40`
(pollution), the same two calls run for the city on each of the 20 neighbour tiles (spiral `k = 1..20`, wrapped and bounded,
`0x56D2C0` city lookup). `Cell::setBits` (`0x5DA2A0`, raw set `0x5EABF0`) is symmetric (network update, city refresh for
`mask & 0xF`, neighbour refresh for pollution). Clearing the colony bits (29..31) calls **no** destroyer: only the explicit
steps 3 (fort/barricade) and the colony destroy paths in `colonies.md` remove the objects.

## 9. Forest chop bonus **V**

Condition: `wasForest`, and the terrain after step 2 is not 7, and feature-plane bit `0x10000000` (`vt+0xAC`) is **clear**.
Then, for spiral index `k = 1..20`: take the tile (wrapped, bounded); if it has a city whose owner byte equals the acting
player and `City::0x4B5050(0)` is true (the current production can receive shields, `city-buildings.md`): set city flag
`+0x30 |= 0x10`; `city.+0x44 = min(city.+0x44 + forestValue, City::0x4ACD70())` (shield box plus `[0x9C727C]`, capped at the
item cost); if the owner is the local civ post `HARVEST_FOREST` (`0x684AC4`) with the new amounts; **stop at the first such
city**. Whether or not a city received shields, feature-plane bit `0x10000000` is then set on the tile
(`vt+0xE0(2, 0x10000000, -1, -1)`), so the same tile never pays again (replanting and re-clearing gives nothing). If the bit
was already set, or the tile was not a forest, nothing happens. In the shipped file `forest_value_in_shields` is the
`RULE` value.

## 10. Golden vectors

* Job rows: section 1.2 (13 rows, A/B/C/D) are the complete table; the table at `0x5E8AB0` indexes it by job id.
* Tokens: section 2.
* `workRate`: the formula of 6.1 with the shipped governments gives, for strength 1.0 and no modifier: 1, 2, 2, 2, 2, 3, 4, 2
  (Anarchy .. Feudalism, in file order).
* Work needed: `turns x moveCost`, e.g. Plant Forest on Plains (move 1) 18; Clear Forest on Forest (move 2) 8.

## 11. Quirks of the original **V** unless noted

* Road and Railroad require a **nonzero road bonus** on the terrain (not an "Allow Roads" flag): Volcano, Coast, Sea and Ocean
  have 0 and refuse both.
* Radar Towers refuse unowned tiles; Airfields and Outposts accept them.
* Jobs 5/6/7 refuse a tile owned by another civ; no other job tests foreign ownership in `canImprove`.
* The clear step does not destroy colony objects for jobs 9/10/11 (`B` removes the other colony bits from the overlay only):
  building a Radar Tower on an Airfield tile clears bit 29 while the airfield object and the cell colony id stay until the new
  creator overwrites the id (**H**: derived from the code; not executed).
* Clearing a forest pays shields to **one** city only, the first found in the spiral order within 20 tiles whose current
  build can take shields; the pay-once flag is set even if no city paid.
* The `Plant Forest` tundra variant bit and the `0x10000000` flag live in the same feature word as the landmark flag
  (bit 29); jobs 5/6/7 clear the landmark.
* `need` multiplies by the **terrain** movement cost of the tile (landmark variant if present), not by roads or rivers.
* All workers on the tile share one pool: a second civ's worker on the same tile (same job id) adds to `total` (no owner test).

## 12. Open items

1. **Automation**: the job selection for `0x20000800` workers is **not** in the go-to executors `0x461F90`, `0x4620D0`, `0x4622D0`, `0x462670`
   (section 5). It is the **Terraform** strategy handler `0x45C750` (11.7 KB, bit 12 of `PRTO.ai_strategies`; it contains the code regions earlier listed as `0x45D814` and `0x45EC1B`), reached through the strategy dispatcher `0x4611F0` (`unit-ai.md` 3 and 5); its body is not decoded.
2. `0x449810` and `0x5BC8B0` (the re-validation of step 6.2), `0x55EF88` (Road exemption), `0x5B3040` (cancel orders).
3. The reader(s) of `TFRM +0x50` and of cell `+0x24`.
4. The coast/lake recomputation in `0x5D59E0` after a terrain change (`0x5EBDC0`).
5. Token `0x20001000`.
