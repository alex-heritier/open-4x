# Adding and removing a building: `0x4ACF40` (this document calls it `City::setBuilding`)

Clean-room specification of the single routine through which every improvement, wonder, small wonder,
palace and spaceship part enters or leaves a city, and of everything that follows from it: the
per-city and per-player bookkeeping, the wonder registry, the "replaced by this flag" sale, the
victory-point and free-advance effects, and the recomputations that make the building's yields,
upkeep and corruption effects take hold. Every claim cites the address it was read at (raw
disassembly; `r2`). Tags: **V** read from the instructions, **H** hypothesis (data flow read, meaning
inferred), **O** open (not decoded). No reference implementation is attached: the ordered steps and
the golden vectors in section 9 are the contract.

Neighbours (not repeated here): `primitives.md` (3.1 `hasBuilding` `0x4ACB50`, 3.2 the reach set and
`rebuildReach` `0x55A560`, 3.3 `countWonders`, 3.7 `buildingUpkeep` `0x4ACDF0`, 2.1 `knowsTech`),
`city-turn.md` (the completion that calls this with `add = 1`, the production costs), `economy.md`
(the sale price `0x4B32F0`, the treasury writer `0x4C2350`, culture and borders), `victory.md` (the
victory-point rule and `[0xA529AC]`), `buildable.md` (what may be built), `research.md` (canResearch
`0x561580`, acquiring a tech `0x561860`), `capture.md` (a captured city's buildings), `government.md`
(`0x55CF10` upkeep), `yields.md` (`0x4B0330`, `0x4B0470`, `0x4B05D0`, `0x4B07C0`, `0x4B10F0`).

## 1. Signature and data

`0x4ACF40(this = city; id, add, quiet)` (`ret 0xC`, C++ exception frame, 0x564 bytes of locals):

* `id` = BLDG row; `add` = 1 to add, 0 to remove; `quiet` non-zero suppresses the spaceship dialog
  (add step 1) and the wonder notice (add step 9) and is forwarded unchanged as the `quiet` argument of
  the free-advance acquire (add step 11). Nothing else reads it. There is **no presence guard**: the
  routine never tests whether the city already has (or lacks) `id`; callers must (section 3).
* All table offsets below are **in-memory** (the file body shifted by 4); `BLDG` = `[0x9C40AC]`,
  stride `0x110`. Flag names are those of `biq/src/sections/bldg.rs`.

| field | meaning | evidence |
|---|---|---|
| city `+0x274` | **the building bitmap**: bit `id` set when the city has `id` (the `hasBuilding` fallback) | `0x4AD10F`, `0x4AD785` (`0x5DF890(map; id, bit)`) |
| city `+0x2B8` | a second building bitmap: set on add, **never cleared** by this routine; serialized with the city (`0x4AC590`, `0x4AC840`); readers **O** (H: "has ever had") | `0x4AD11D` |
| city `+0xC4` | array of 12-byte records, one per BLDG row: `{ stamp, builder slot, 0 }` | `0x4AD14E..0x4AD177` |
| city `+0x2C` | summed building upkeep (gold per turn) | `0x4AD1BC..0x4AD1C9` |
| city `+0x48` | summed pollution of the buildings (`BLDG +0xCC`) | `0x4AD1D2..0x4AD1DE` |
| city `+0x13C` | cached building culture (section 6) | `0x4ADC8F..0x4ADD4A` |
| Player `+0x15DC` | pointer to an `int16` array: buildings of each type this player owns | `0x4AD1B1`, `0x4AD7B6` |
| Player `+0x15E8` | pointer to a dword array: for a small wonder, the id of the city that holds it (`-1` none) | `0x4AD2C3`, `0x4AD853` |
| Player `+0x15FC` | pointer to an `int16` array: spaceship parts built per part type | `0x4ACFED..0x4AD01A` |
| Player `+0x2C` | the capital city id | `0x4AD09E` |
| Player `+0xFC` | the **current research** (an advance, `T` = future technology, `-1` = none; `research.md` 2): the start index of the free-advance scan | `0x4AD657` |
| `0xA52658` | the wonder registry: `0x539010(reg; id, cityId)` set the holder, `0x538FF0(reg; id, flag)` set the "built" flag, `0x539030(reg; id)` read the holder | `0x4AD204..0x4AD211`, `0x4AD81B` |

The 12-byte record `{ stamp, builder, 0 }`: `stamp` is the value in the object that `0x4C1F20(turn counter
[0xA526AC])` fills (the dword at object `+0x68`), or **1** if that value is 0 (`0x4AD133..0x4AD15A`; **H**: a
calendar year, see section 6); `builder` is the owner slot at that moment (`0x4AD16D`). `0x4F8CE0` reads
`builder` (section 6). The remove path leaves the record in place.

## 2. Predicates used (all **V**)

| address | meaning |
|---|---|
| `0x4ACB50(city; b, withReach)` | `hasBuilding` (`primitives.md` 3.1) |
| `0x4ACCC0(city; b)` (`ret 4`) | **is obsolete**: `t = BLDG[b].+0xE0`; false if `t < 0`; else `Player.knowsTech(t)` (`0x561440`). **Not** a "switched off" predicate (`0x4ACCC0..0x4ACD0F`) |
| `0x569BB0(P; b)` (`ret 4`) | ordinary improvement: not Palace (`+0xEC` bit 0), not great or small wonder (`+0xF0` bits 2, 3), not Capitalization (`+0xEC` bit 19), no spaceship part (`+0xD8 == -1`) |
| `0x4B32F0(city; b)` | sale price `0x569FE0(P; b, 0) / [0x9C7268]` (`economy.md`) |
| `0x4ACD70(city)` | the cost of the current production item: kind (`+0x50`) 0 returns 0; kind 1 `0x569FE0(P; id, 0)`; kind 2 `0x56A210(P; id, 0)`; any other kind returns the saved `ecx` (garbage, unreachable) (`0x4ACD70..0x4ACDEB`) |
| `0x4B5050(city; _)` (`ret 4`, argument ignored) | true if the current kind (`+0x50`) is not 1, else `0x569BB0(P; +0x4C)`: "the current production is not a wonder, palace, spaceship part or Capitalization" (`0x4B5050..0x4B5091`) |
| `0x55CF10(P)` | recompute every upkeep: for each city of `P`: `+0x2C = 0`, then for each BLDG row `b` with `0x4ACB50(city; b, 0)` and **not** `0x4ACCC0(city; b)`: `+0x2C += 0x4ACDF0(city; b)` (`0x55CF10..0x55CFA4`). So the authoritative sum **excludes obsolete buildings** and ignores the reach set; the incremental `+=` of the add path does not exclude them until the next full recompute |
| `0x561290(P)`, `0x5612F0(P)` | for each city of `P`: `0x4B0E80(city)`, respectively `0x4B10F0(city)` (`0x561290..0x5612E9`, `0x5612F0..0x561349`) |
| `0x4B3340(city; b)` (`ret 4`) | shields refunded for selling `b`: 0 unless `knowsTechWithFlags(0x40)` (the Recycling flag, `primitives.md` 2.2) holds; 0 again if the current kind is 1 and not `0x569BB0(P; currentId)`; else `cost(b) / 4` computed as `(c + ((c >> 31) & 3)) >> 2` with `c = 0x569FE0(P; b, 0)` (`0x4B3340..0x4B33F0`) |

## 3. Callers (**V**: 21 direct call sites found by a byte scan of `E8` displacements; arguments are the
pushes in front of each call, written `(id, add, quiet)`)

| call site | routine | arguments | meaning |
|---|---|---|---|
| `0x4AD0DB` | this routine | `(id, 0, 0)` | remove the Palace from the old capital (add step 2) |
| `0x4AD41B` | this routine | `(b, 0, 0)` | the sale of a replaced building (4.1) |
| `0x4B96B0` | production completion `0x4B9270` tail | `(id, 1, 0)` | a completed improvement (`city-turn.md`) |
| `0x4AE592` | `City::init` `0x4AE2A0` | `(row, 1, 0)` | the first city of an owner receives the building whose `+0xEC` bit 0 (Center of Empire) is set (`city-founding.md` 3.7) |
| `0x4AEEE1` | city removal `0x4AECC0` | `(b, 0, 0)` for each `b` in `0..N-1` with `0x4ACB50(city; b, 0)`, after `0x4482B0(owner; city)` (`0x4AEEBC..0x4AEEEE`) | every building leaves a destroyed city, in row order |
| `0x4B36E6` | sell command `0x4B3400` | `(b, 0, 0)`, after `+0x44` is rewritten (`0x4B36DC`) | the player sells a building (`economy.md`) |
| `0x4BE216` | disorder turnover `0x4BDFF0` | `(b, 0, 0)` | the building destroyed by a riot (`happiness.md` section 7 specifies the roll and the choice) |
| `0x4C1444`, `0x4C1562` | `0x4C1320`, `0x4C1470` | `(b, 0, 0)` | bombardment destroys the building that holds the defense (`combat.md`) |
| `0x4C171D` | `0x4C1590` (random destruction) | `(b, ebx, ebx)` (**O**: `ebx` not traced) | a random non-wonder building is destroyed (`combat.md`) |
| `0x4C187F` | `0x4C1590` | `(b, 0, 0)` | the same, second path |
| `0x564954`, `0x5649B5`, `0x564A29`, `0x564A74` | city transfer `0x564800` | `(id, 0, 0)` | buildings lost on capture (`capture.md` section 8 passes B, C, D): the Center-of-Empire building (`+0xEC & 1`), one counted case, a random one (`rand(..) == 0`, gameplay RNG), the small wonders (`+0xF0 & 8`) |
| `0x564F53` | `0x564800` | `(id, 1, 0)` | when this is the new owner's only city: the Center-of-Empire building is added (`capture.md` step 11) |
| `0x4484A1` | `0x4482B0` (called by the capital-lost step of `capture.md`) | `(id, 1, 0)` | adds a building (**H**: the Palace replacement, since the caller just lost the capital) |
| `0x41FD06` | the city screen `0x41F270` | `(id, 1, 1)` with `id` from a picker dialog (`0x611530`) | a manual add (debug or editor; **H**) |
| `0x5B417B` | the nuclear detonation `0x5B4070` | `(edi, 0, 0)` after `rand(2) == 0` | a building destroyed by the blast (`world-events.md`) |
| `0x5D29CC` | loader/setup `0x5D25F0` | `(b, 1, 1)` if `P.+0x15E8[b] == -1` | a small wonder with no holder is placed (**H**) |
| `0x5D2C74` | loader/setup `0x5D2B20` | `(id, 1, 1)` | setup add (**O**) |

The routine itself passes `quiet = 0` in both recursions.

## 4. The add path (`add != 0`, `0x4ACFA7..0x4AD8A3`)

Steps in the exact order they run. Let `B = BLDG[id]`, `P` the owner's player object.

1. **Spaceship part** (`B.+0xD8 != -1`, `0x4ACFBA`): `P.+0x15FC[B.+0xD8] += 1`; if `quiet == 0`, the owner is
   the local slot and the game is not multiplayer: dialog `0x5A3910(0xC90E80; 0)` (the spaceship
   screen, H). **Return** (jump to the exit `0x4ADD64`): a spaceship part never enters the city's
   bitmaps, the record, the upkeep or any other effect below.
2. **Center of Empire** (`B.+0xEC` bit 0, the Palace, `0x4AD069`): the previous capital `c` = the city with
   id `P.+0x2C` (looked up in the city pool, null if none or out of range). If `c` exists and
   `0x4ACB50(c; id, 0)`: `0x4ACF40(c; id, 0, 0)` (remove it there; its whole tail runs). Then, whether or
   not there was a previous capital, `0x55CB20(P; this)` (**V**, `0x55CB20..0x55CBA8`): `P.+0x2C = this.+0x20`
   (the city id; `-1` if the city argument is null), `0x57E450(0xB72888; 0)` (trade network rebuild), and
   for every other player object `o` in slots `1..` (stride `0x20E4`, skipping `P`'s own slot `P.+0x1C`)
   whose slot bit is in the in-play mask `[0xA526C0]` and for which `P.+0xD50[o]` or `P.+0xD70[o]` is
   non-zero (contact flags, `diplomacy.md`): `0x55B1A0(o; city.x, city.y)` = `discover(o; x, y)` (marks the
   capital's tile as seen by `o` and counts it in `o.+0xA8`; `research.md` 10.3), i.e. a civ in contact learns where the
   new capital is.
3. **Mark**: set bit `id` in `+0x274` (`0x5DF890(map; id, 1)`) and in `+0x2B8` (`0x4AD10F`, `0x4AD11D`).
4. **Record**: write `{ stamp, owner, 0 }` at `+0xC4 + 12*id` (section 1).
5. **Counters** (`0x4AD188..0x4AD1DE`): `P.+0x15DC[id] += 1`; `+0x2C += 0x4ACDF0(this; id)`;
   `+0x48 += B.+0xCC`.
6. **Great wonder** (`B.+0xF0` bit 2, `0x4AD1E7`): `0x539010(0xA52658; id, this.id)`,
   `0x538FF0(0xA52658; id, 1)`; announce `0x58B5D0(0xC88588; 5, owner, x, y, B.name (+0x44), 0)` (event
   kind 5, **not** gated by `quiet`); if `B.+0xF8` bit 7 (`PAYS_TRADE_MAINTENANCE`): `0x55CF10(P)` (every
   city of `P` recomputes `+0x2C`, section 2). On the add path this test is nested inside the
   great-wonder block; the remove path (section 7) tests it for every building.
7. **Small wonder** (`B.+0xF0` bit 3, `0x4AD293`): `P.+0x15E8[id] = this.id`.
8. **Replace-by-flag** (the new building has `B.+0xEC` bit 13, `REPLACES_ALL_WITH_THIS_FLAG`, `0x4AD2D3`):
   for every BLDG row `b` in `0..N-1` in ascending order with `b != id`, `BLDG[b].+0xEC` bit 13 set,
   `0x4ACB50(this; b, 0)`, `BLDG[b].+0xF0` bits 2 and 3 clear (neither wonder kind), `BLDG[b].+0xEC` bit 0
   clear and `(BLDG[b].+0xEC & 0x1800) == 0` (neither `ALLOWS_CITY_SIZE_LEVEL_2` nor `_3`), the city
   **sells** `b` (4.1).
9. **Wonder notice** (`B.+0xF0 & 12 != 0` (great or small wonder), `quiet == 0`, owner == the local slot
   `[0x9FD4BC]`, `0x4AD44B..0x4AD508`): in multiplayer (`0x47B530` true) the dialog `MP_WONDER_COMPLETION`
   (`0x47A430` on the host `0x49FC50`, text argument 0 = `B.name` (`+0x44`), argument 1 = the city name
   (`+0x1E0`); flag `0x4000`; shown with `0x611530(host; 0, 0)`); in single player the wonder screen
   `0x5CF640(0xCA6160; id, this)` (**O**: the movie/screen body).
10. **Victory points** (great wonder only, `0x4AD508..0x4AD612`): if `[0xA5267C] & 0x26000` (the victory-point
    modes, `victory.md`): `P.+0x11CC += B.cost * [0xA529AC]` and `P.+0x15B8 += B.cost * [0xA529AC]`
    (`B.cost` = `+0x94`, stock multiplier 10; the "great wonder completed" row of `victory.md` is this
    block, `0x4AD51A..0x4AD5DA`). Then, **whether or not** the mask test passed, `0x55C9A0(P)` runs
    (`0x4AD5E1..0x4AD607`): the **Golden Age starter** (`research.md` section 11: it does nothing unless
    `P.+0x3C == -1`, and starts the age when, for every trait `k` in `0..7` that the civ's RACE row has, the civ
    owns a great wonder whose category bit for `k` is set in `BLDG.+0xF0`; `P.+0x3C` is the age's end turn). It
    is therefore evaluated after every great-wonder add, with the new wonder already registered in the
    registry (step 6).
11. **Free advances** (`B.+0xF8` bit 10, Two Free Advances, `0x4AD612..0x4AD77D`): run **two rounds**
    (section 5).
12. Fall into the common tail (section 6).

### 4.1 The sale of a replaced building (`0x4AD36F..0x4AD41B`)

```
price = 0x4B32F0(this; b)                            # cost(b) / [0x9C7268] (economy.md)
0x4C2350(P; treasury + price)                        # credit; treasury = P.+0x44 + P.+0x48, rewritten with the treasury writer
if 0x4B5050(this; 0):                                # the current production can receive shields
    bonus = 0x4B3340(this; b)                        # 25 % of the cost when the Recycling tech flag is known
    if bonus > 0:  +0x44 = min(+0x44 + bonus, 0x4ACD70())          # added to the shield box, clamped at the item cost
0x4ACF40(this; b, 0, 0)                              # remove b
```

No message is shown. The loop continues with the next row (rows after `b` see the already-updated
bitmap).

## 5. Free advances (`wonder_flags` bit 10, `TWO_FREE_ADVANCES`)

The same loop is specified from the research side in `research.md` 10.4; this is the exact code reading
(`0x4AD612..0x4AD77D`). `techs = [0x9C3DBC]` (the tech count `T`), `last = -1` (kept across both rounds).
Two rounds; each:

```
cursor = P.+0xFC                                                           # the current research, RE-READ at the start of each round
if cursor < 0 or cursor >= techs:  skip the round                           # 0x4AD65E..0x4AD668
for k in 0 .. techs-1:
    t = (cursor + k) mod techs                                              # signed idiv; cursor + k is never negative here
    if not 0x561580(P; t):  continue                                         # canResearch
    if last != -1 and last == t:  continue                                  # never the same tech twice in one build
    last = t
    P.+0xAC = 0;  P.+0xAD = 0;  P.+0xCD = 0;  P.+0xF0 = -1                  # clear the "acquired from" notice: nobody gave it
    if 0x47B530():  0x475460(0x74AF60; P.slot, t, 0, quiet, 1)               # multiplayer: the network acquire
    else:           0x561860(P; t, 0, quiet, 1)                             # acquire(tech, 0, quiet, 1)
    break                                                                   # next round
```

The routine does not write `P.+0xFC`. In single player the first `acquire` of the tech that **is** the current
research completes it and chooses a new target (`research.md` 6.2 step 11), so round 2 starts from the new
target; in multiplayer the acquire is deferred to the network message, `+0xFC` is unchanged when round 2
starts, and `last` is what keeps round 2 off the same tech. With `+0xFC == -1` (no research chosen) or `T`
nothing happens.

## 6. The common tail (both paths, `0x4AD8A3..0x4ADD64`)

Executed after step 12 of the add path and after the remove path (section 7). `B` and `id` as above.

1. **Trade network** (`B.+0xEC & 0x300000` (`ALLOWS_WATER_TRADE`, `ALLOWS_AIR_TRADE`) or `B.+0xF8 & 1`
   (`SAFE_SEA_TRAVEL`)): `0x57DE90(0xB72888; 0)` (**O**: a trade-network recompute entry; `0x4AD8A3..0x4AD8C6`).
2. **Gain in every city** (`B.+0x88 != -1` or `B.+0x8C != -1`, `0x4AD8CC..0x4AD968`): in this order
   `0x55A560(P)` (rebuild the reach set, `primitives.md` 3.2), `0x55CF10(P)` (all upkeep, section 2),
   `0x561290(P)` (every city of `P`: `0x4B0E80(city)`).
3. **Corruption** (`B.+0xEC` bit 0 (`CENTER_OF_EMPIRE`) or `B.+0xF4` bit 5 (small-wonder `REDUCES_CORRUPTION`),
   `0x4AD96E..0x4AD9B3`): `0x5612F0(P)` (every city of `P`: `0x4B10F0(city)`, the yield/corruption chain of
   `yields.md`).
4. **Water-tile yields** (`B.+0xEC & 0x3800000` (`INCREASES_SHIELDS/FOOD/TRADE_IN_WATER`) or `B.+0xF8 & 0x20`
   (`PLUS_ONE_TRADE`), `0x4AD9B3..0x4ADBD7`): rebuild the gross yields from the tiles.

   ```
   +0x1C8 = +0x1CC = +0x1D0 = 0                                   # gross food, shields, trade
   for n in 0 .. 20:                                                # n = 0 is the city centre
       (dx, dy) = 0x5E6E50(n)                                       # the spiral of the 21-tile radius (yields.md 6)
       x' = 0x426C00(map 0x9C736C; city.x(+0x24) + dx);  y' = 0x426C40(city.y(+0x26) + dy)    # wrap / clamp
       if not 0x426BD0(map; x', y'):  continue                      # off the map
       if n >= 1:                                                   # the centre is never tested
           cell = 0x5D16A0( ((W >> 1) * y' + (x' >> 1)) & 0xFFFF )  # W = [0x9C74D4], the doubled-grid width
           if int16(cell.+0x6C) != this.+0x20:  continue            # the tile must be worked by / assigned to this city
       for i in 0..2:  +0x1C8[i] += 0x4B0330(this; i, x', y')       # tile yield i (yields.md)
   ```

   The tests are done in this order (the validity test, then the owner word). The meaning of cell `+0x6C`
   as "city using this tile" is **H** (the value compared is the city id `+0x20`).
5. **Food eaten, surplus, shields, commerce, happiness** (always, `0x4ADBDB..0x4ADC76`; when step 4 ran, the
   same sequence was already run at its end `0x4ADB75..0x4ADBD2` and runs again here; the two variants differ
   only in how they count the citizens that do not eat):

   ```
   if +0x30 & 1 (disorder):  eaten = +0x1C8
   else:                      eaten = (+0x138 - n) * [0x9C72B4]          # [0x9C72B4] = food per citizen
   +0x244 = eaten;  +0x250 = +0x1C8 - eaten
   0x4B05D0(this);  0x4B07C0(this);  0x4BCFF0(this)
   ```

   `n` is the number of resisting citizens: `0x4BB2A0(this; -1)` in the step-4 copy and, in the final copy,
   an inline count of the citizen-pool nodes whose object has `+0x20 != 0` (the pool's `+0xE0` array, indices
   `0..+0xEC`, null nodes skipped). That the two counts are always equal is **H**.
6. **Culture** (`B.+0x98 > 0`, i.e. the changed building has culture, `0x4ADC7B..0x4ADD64`): rebuild
   `+0x13C`:

   ```
   +0x13C = 0
   for b in 0 .. N-1:
       if 0x4ACB50(this; b, 1) and ( BLDG[b].+0xE0 < 0  or  not 0x561440(P; BLDG[b].+0xE0)  or  great wonder(b) ):   # present and (active or a great wonder)
           +0x13C = max(0, +0x13C + 0x4F8CE0(&P.+0x181C; b, this))
   ```

   `0x4F8CE0(this = S; b, city)` (`ret 8`, **V**, `0x4F8CE0..0x4F8DB0`), where `S` is the sub-object embedded at
   `Player+0x181C` of the city's owner (its `+0x28`, i.e. `Player+0x1844`, holds the player's slot: the player
   initialiser writes `Player+0x1C` there, `0x567D0F`):

   ```
   has      = 0x4ACB50(city; b, 0)                      # the city's own bitmap (no reach set)
   obsolete = 0x4ACCC0(city; b)
   if obsolete and not great wonder(b):  return 0
   if has and city.+0xC4[12*b + 4] != S.+0x28:  return 0     # the recorded builder is not this owner: a captured building yields nothing
   c = BLDG[b].+0x98
   if has and 0x4C2420(Player+0x18C4; b) >= 1000:  c = 2*c   # age test, below
   if Player[city.owner].+0xA4 == 1:  c = (c + 1) / 2        # signed halving, rounds up
   return c
   ```

   The reach-set case (`has == 0` but present through `0x4ACB50(.., 1)`) therefore skips the builder test and
   the age doubling, and gets the full `BLDG.+0x98` (halved when `Player.+0xA4 == 1`). The age function
   `0x4C2420(T; b)` (`ret 4`, **V** `0x4C2420..0x4C24B8`) is `stampNow - dword[ [T+0x1C] + 12*b ]`, where
   `stampNow` is the stamp built from the turn counter `[0xA526AC]` (an object filled by `0x4FCB20`,
   `0x5DF250`, `0x5DF100`; the result field is 1 if it is 0, the same guard as the record stamp of section 1)
   and `[T+0x1C]` is a per-player array of 12-byte records written elsewhere (**O**). The stamp is most
   likely a calendar year (**H**: the `0 -> 1` guard fits "no year 0", and the threshold 1000 reads as
   "older than 1000 years").

Then the exception frame is unwound and the routine returns.

## 7. The remove path (`add == 0`, `0x4AD782..0x4AD89D`)

1. Clear bit `id` of `+0x274` (`0x5DF890(map; id, 0)`); **`+0x2B8` and the 12-byte record stay**.
2. `P.+0x15DC[id] -= 1` only when it is positive (`0x4AD7C4..0x4AD7CA`).
3. `+0x2C -= 0x4ACDF0(this; id)`, `+0x48 -= B.+0xCC` (unconditional; both go negative when the city did not
   have the building, since there is no presence guard).
4. Great wonder: `0x539010(0xA52658; id, -1)` (the registry holder becomes none; the "built" flag
   `0x538FF0` is **not** reset, so the wonder cannot be built again).
5. Small wonder: `P.+0x15E8[id] = -1`.
6. `B.+0xF8` bit 7 (`PAYS_TRADE_MAINTENANCE`), for **any** building: `0x55CF10(P)`.
7. The common tail (section 6).

The remove path performs none of: the Palace handling, the sale, the notices, the victory points, the
free advances, the spaceship counters. Removing a Palace does **not** clear `P.+0x2C` here (the caller
that moves the capital registers the new one first: add step 2; a city that is destroyed or captured
resets it elsewhere, `capture.md`).

## 8. Where each recompute sits (summary)

| trigger on the building | recompute |
|---|---|
| any add/remove | section 6 step 5 (`0x4B05D0`, `0x4B07C0`, `0x4BCFF0` and food eaten) |
| `improvement_flags` bits 20, 21 (water / air trade), `wonder_flags` bit 0 (safe sea travel) | trade network `0x57DE90(0xB72888; 0)` |
| `gain_in_every_city`, `..._on_continent` | `0x55A560`, `0x55CF10`, `0x561290` |
| `improvement_flags` bit 0 (Center of Empire), `small_wonder_flags` bit 5 (reduces corruption) | `0x5612F0` |
| `improvement_flags` bits 23-25 (water yields), `wonder_flags` bit 5 (plus one trade) | gross yields rebuilt (step 4) |
| culture `> 0` | `+0x13C` (step 6) |
| `wonder_flags` bit 7 (pays trade maintenance) | `0x55CF10`: on add only for great wonders (add step 6), on remove for any building (remove step 6) |
| great wonder add | registry, announce, victory points, Golden Age check `0x55C9A0` |

## 9. Golden vectors

All with a city of owner `P`, `quiet = 0`. They are consequences of the steps above (hand-derived, not
captured from a run), so they check an implementation against this text, not against the executable.

* **B1 add an ordinary building (no trade, gain, corruption, water or culture flag):** `+0x274` and
  `+0x2B8` gain bit `id`; `+0xC4 + 12*id` = `{ stamp, P.slot, 0 }`; `P.+0x15DC[id]` +1; `+0x2C` +=
  `0x4ACDF0(city; id)` (`BLDG.+0xAC`, or 0 when the reach set supplies the building); `+0x48` += `BLDG.+0xCC`.
  The tail runs step 5 only.
* **B2 add the Palace in city B while city A holds it:** `0x4ACF40(A; id, 0, 0)` runs first (its own tail
  included); then `P.+0x2C = B.+0x20`, the trade network is rebuilt (`0x57E450`), the contacted civs get the
  reveal call; then B receives the building; B's tail runs `0x5612F0(P)` (the building has `+0xEC` bit 0).
  Two tails in total, A's before B's counters change.
* **B3 add a great wonder, `[0xA5267C] & 0x26000 != 0`, `BLDG.+0x94 = 400`, `[0xA529AC] = 10`:** registry
  holder = `B.+0x20`, built flag 1, event 5 logged, `P.+0x11CC` += 4000 and `P.+0x15B8` += 4000, then the
  Golden Age check `0x55C9A0(P)`. With the mask clear, the two additions are skipped and the check still runs.
* **B4 replace-by-flag, `[0x9C7268] = 1`:** the new building has `+0xEC` bit 13; the city has two older buildings
  with bit 13, neither a wonder nor a Palace nor an Aqueduct/Sewer-class, of cost 60 and 90, in rows `u < v`.
  Result, per row in the order `u`, `v`: credit the price (the treasury sum rises by 60, then by 90, two
  `0x4C2350` writes); if `0x4B5050` is true and the owner knows a tech with flag `0x40`, the shield box gains
  `(60 + 0) >> 2 = 15` (then `(90 + 0) >> 2 = 22`), each time clamped at `0x4ACD70`; then remove the row with
  `(b, 0, 0)` (its own tail runs) before the next row is examined.
* **B5 free advances, 8 techs, current research `P.+0xFC = 6`, researchable `{1, 3, 6}`:** round 1 takes tech 6.
  Multiplayer (`+0xFC` still 6): round 2 scans 6 (equal to `last`, skipped), 7, 0, then takes 1; result `{6, 1}`.
  Single player, where completing tech 6 re-chose the target 3: round 2 starts at 3 and takes 3; result `{6, 3}`.
  With `+0xFC` equal to 8 or -1 both rounds do nothing.
* **B6 spaceship part (`BLDG.+0xD8 = 2`):** `P.+0x15FC[2]` +1; the dialog `0x5A3910(0xC90E80; 0)` only if not quiet,
  the owner is the local slot and the game is single player; nothing else changes (bitmaps, record, `+0x2C`,
  `+0x48`, tail).
* **B7 remove a small wonder:** bit cleared in `+0x274` (not in `+0x2B8`), `P.+0x15DC[id]` -1 if it was positive,
  `+0x2C` and `+0x48` reduced, `P.+0x15E8[id] = -1`; the tail runs.
* **B8 culture (`0x4F8CE0`):** Temple (`+0x98 = 1`, builder = owner, younger than 1000): 1. Mobilized owner
  (`P.+0xA4 == 1`): `(1 + 1) / 2 = 1`; culture 3 mobilized: 2; culture 5 mobilized: 3. Same building older than
  1000: `2`, mobilized `(2 + 1) / 2 = 1`. A captured building (record builder differs from `S.+0x28`): 0. An obsolete
  ordinary building: 0; an obsolete great wonder keeps its value. A building present only through the reach set
  (`has = 0`): full `+0x98` without the builder test or the age doubling.
* **B9 obsolescence:** `BLDG.+0xE0 = -1`: never obsolete. `+0xE0 = t >= 0` and the owner does not know `t`: not
  obsolete. Known: obsolete (`0x4ACCC0`).

## 10. Open items

1. `+0x2B8` bitmap's readers; who writes the per-player stamp array `[Player+0x18E0]` read by `0x4C2420`; the unit
   of the stamp (H: calendar years).
2. `0x5CF640`, `0x5A3910` (the wonder and spaceship screens), `0x4B0E80`.
3. The trade-network functions `0x57DE90`, `0x57E450`, `0x57F360`, `0x580540` (`trade-network.md` is pending).
4. Whether a Palace removal elsewhere resets `P.+0x2C` (only the capture/destroy callers were not re-read).

## 11. Corrections to earlier documents

* `world-events.md` section 3.6: `0x4ACCC0` is the obsolescence test (done in place).
* `economy.md` "Per-building `0x4F8CE0` (child-reported)": the age test is `>= 1000` and the halving
  condition is `Player +0xA4 == 1` (mobilization); the builder-record test (captured buildings give 0)
  and the great-wonder exemption from obsolescence are new here.
* `city-turn.md` section 11 item 1 now points here.
