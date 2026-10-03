# Goody huts: trigger, outcome roll, the eight outcomes

Standalone specification, recovered from the raw disassembly of `Civ3Conquests.exe` (PE32, MSVC 6, image
base `0x400000`). No Rust module belongs to this document. Companion documents: `primitives.md` (RNG,
pools, `hasAbility`), `research.md` 10.3 (`discover`) and 10.5 (the advance case, restated here in full),
`barbarians.md` (tribe table, unit factory), `unit-turn.md` (`Unit::kill`), `economy.md` (treasury pair),
`NOTES.md` 11.9 (hut placement at map generation).

Notation: `f(this; a, b)` is a `__thiscall` with the stack arguments after the semicolon; `rand(n)` is the
gameplay die `0x60BAB0(0xA526B4; n)` = `(draw * (n & 0xFFFF)) >> 15` and consumes one draw even for
`n = 1` (`primitives.md` 1.6). Evidence tags: **V** read in the disassembly; **H** hypothesis (a guess
from context, not an upgrade of anything); **O** open.

## 1. What a hut is

A hut is overlay bit `0x20` (bit 5) of the tile's overlay word (plane 0). Reads: cell vtable slot `+0x3C(0)`
(`0x5EA7A0`: `(overlay >> 5) & 1`). Clear: `+0xCC(0, 0x20, -1, -1)` (`0x5DA3E0`). Placement happens once, at
map generation (`0x5F21B0`, `NOTES.md` 11.9, `mapgen.md`); nothing in the running game creates a hut (**O**
only in the sense that every writer of `+0xE0(0, 0x20, ..)` was not censused). Camps (bit `0x80`) and huts
are different bits; barbarian camp sites exclude hut tiles (`barbarians.md`).

## 2. Triggers (**V**)

| # | Where | Condition | Call |
|---|---|---|---|
| 1 | `Unit::setPosition` `0x5BD220`, `0x5BDE63..0x5BDE90` | after a unit is placed on a tile: that tile has a hut and the unit's owner (`unit +0x34`) is not `0` | `0x55C6B0(Player[owner]; x, y, unit)` |
| 2 | `0x5D3AB0(map; x, y, newOwner)` at `0x5D3E0A..0x5D3E2B` | the tile's owner byte changed (the routine returns at once when it did not, `0x5D3ADD`) and the tile has a hut | `0x55C6B0(Player[newOwner]; x, y, 0)` |

* Trigger 1 has no domain, type or activity filter between the function entry and `0x5BDE63` that I traced
  (**V**: the head of `0x5BD220` was read in full, `movement.md` 5: nothing between the entry and `0x5BDE63` filters on domain, type or activity). The factory `0x5694D0` and the relocation paths reach
  `0x5BD220` as well (**H**), so a unit created or teleported onto a hut tile pops it.
* Trigger 2 comes from the border code only (`0x5D3AB0` is called from `0x5D4830`; `borders-culture.md` 6 and 5, `barbarians.md` 8). The
  third argument is `0`, so the pop is **unitless** (section 3, `unit == 0`).
* Slot 0 (barbarians) never pops a hut: `0x55C6B0` returns at once when `P.+0x1C == 0` (`0x55C6BA`).

## 3. The pop `0x55C6B0(P; x, y, unit)`, `ret 0xC` (**V**)

```
if P.slot == 0: return                                                     0x55C6B8
clear hut bit: cell(x, y).vt+0xCC(0, 0x20, -1, -1)                         0x55C6FA   huts are single use
UI: 0x4E69F0(0x9F8700; x, y, 0, 0); if it returns true and the local player's bit is set in cell+0x58:
    0x4E6C30(0x9F8700; x, y)                                               (redraw the tile)
done = 0                                                                   local byte
outcome = (unit == 0) ? 2 : rollOutcome(P)                                 0x55C775 / 0x55C77E
0x55B8B0(P; x, y, outcome, &done, unit)                                    section 5
if P.slot == [0x9FD4BC]: 0x535D80()                                        UI hook
```

The hut bit is cleared before anything else, so a hut is consumed even when every outcome later fails and
the dispatcher keeps re-rolling. A unitless pop starts from outcome `2` (a free city) and falls into the
re-roll loop of section 5 whenever that outcome's preconditions fail, so a border pop is in practice a
roll of the same table.

## 4. The outcome roll `rollOutcome(P)` `0x55B7D0`, plain `ret` (**V**)

```
r   = rand(20)                                         one draw, 0x55B7DC
t   = RACE[P.race].hasTrait(2)                         Expansionist, RACE vtable[0] 0x53A080
idx = P.+0x30 + (t ? 0 : 1)                            P.+0x30 = the DIFF row of the player
if idx < 0 or idx > 8: return 3                        nothing, 0x55B8A5
if (flags[0xA5267C] & 0x400) == 0 and r < T1[idx]: return 2          city
if r < T2[idx]: return 6                                              advance
if r < T3[idx]: return 0                                              gold
if r < T4[idx]: return 4                                              settlers
if r < T5[idx]: return 1                                              map
if r < T6[idx]: return 5                                              mercenaries
return T7[idx]
```

Outcome numbers equal the message kind of section 8: `0` gold (`GOODY_MONEY`), `1` map (`GOODY_MAPS`), `2` city
(`GOODY_CITY`), `3` nothing (`GOODY_NOTHING`), `4` settlers (`GOODY_SETTLERS`), `5` mercenaries
(`GOODY_MERCENARIES`), `6` advance (`GOODY_TECH`), `7` barbarians (`GOODY_BARBARIANS`).

Tables (nine `int32` each, indexed by `idx`; read from the image):

| table | address | idx 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|---|
| T1 city | `0x72C844` | 2 | 2 | 2 | 1 | 1 | 1 | 0 | 0 | 0 |
| T2 advance | `0x72C868` | 9 | 8 | 7 | 5 | 4 | 3 | 1 | 0 | 0 |
| T3 gold | `0x72C88C` | 12 | 11 | 9 | 7 | 6 | 4 | 2 | 1 | 0 |
| T4 settlers | `0x72C8B0` | 15 | 14 | 11 | 9 | 8 | 5 | 3 | 1 | 0 |
| T5 map | `0x72C8D4` | 17 | 16 | 13 | 11 | 10 | 6 | 4 | 2 | 1 |
| T6 mercenaries | `0x72C8F8` | 19 | 18 | 15 | 13 | 12 | 7 | 5 | 3 | 2 |
| T7 otherwise | `0x72C91C` | 3 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 |

The tables are cumulative upper bounds on `r` (`0..19`). Reading the grid below, `idx` is the row, `r` the
column; letters are the outcome: `G` gold, `M` map, `C` city, `N` nothing, `S` settlers, `U` mercenaries,
`A` advance, `B` barbarians. The right-hand grid is the same row when the City Elimination flag `0x400` is
set (T1 is skipped, so outcome `2` cannot come out of the roll).

```
idx  r = 0..19 (flag 0x400 clear)   r = 0..19 (flag 0x400 set)
0    CCAAAAAAAGGGSSSMMUUN           AAAAAAAAAGGGSSSMMUUN
1    CCAAAAAAGGGSSSMMUUBB           AAAAAAAAGGGSSSMMUUBB
2    CCAAAAAGGSSMMUUBBBBB           AAAAAAAGGSSMMUUBBBBB
3    CAAAAGGSSMMUUBBBBBBB           AAAAAGGSSMMUUBBBBBBB
4    CAAAGGSSMMUUBBBBBBBB           AAAAGGSSMMUUBBBBBBBB
5    CAAGSMUBBBBBBBBBBBBB           AAAGSMUBBBBBBBBBBBBB
6    AGSMUBBBBBBBBBBBBBBB           AGSMUBBBBBBBBBBBBBBB
7    GMUBBBBBBBBBBBBBBBBB           GMUBBBBBBBBBBBBBBBBB
8    MUBBBBBBBBBBBBBBBBBB           MUBBBBBBBBBBBBBBBBBB
```

Notes:

* `idx` is the player's difficulty row plus `1` for a civilization that is **not** Expansionist, plus `0`
  for one that is: an Expansionist reads the row one step easier. With the shipped nine-row tables a
  Sid-level (`DIFF` row 7) non-Expansionist reads `idx = 8`.
* The left grid is the raw first draw only. Section 5 re-rolls (another `rand(20)` through this same
  function) whenever the chosen outcome's preconditions fail, so the delivered distribution differs from the
  grid: a non-Expansionist can never keep a `C` (outcome 2 needs the Expansionist trait), an Expansionist
  never keeps a `B` (outcome 7 refuses it), and `A`/`S`/`U` fall through when their conditions fail.
* The difficulty row of the **human** is the same field; there is no separate human table (**V**: the
  function reads only `P.+0x30` and the RACE trait).

## 5. The dispatcher `0x55B8B0(P; x, y, outcome, &done, unit)`, `ret 0x14` (**V**)

Stack arguments in call order: `x`, `y`, `outcome`, `done` (byte pointer), `unit` (a Unit object or `0`).

```
cont  = cell(x, y).vt+0xB8()                                  continent word, sign-extended
cell(x, y).vt+0x8C()                                          (result unused)
0x56D040(x, y, P.slot, cont, -1, -1, 0)                       nearest city of P on this continent; result unused,
                                                              the two globals are what matter (6.2)
if (flags & 0x400) and outcome == 2: outcome = rollOutcome(P)          0x55B94A..0x55B951
LOOP:                                                                  0x55B956
    if *done != 0: return
    if outcome > 7: goto LOOP                                          cannot occur (the roll returns 0..7)
    switch outcome:   jump table 0x55C684: 0 -> 0x55B973  1 -> 0x55C2B8  2 -> 0x55B9D1  3 -> 0x55C44F
                                           4 -> 0x55BBE8  5 -> 0x55BCD4  6 -> 0x55BD94  7 -> 0x55BF40
    an outcome whose preconditions fail executes:  outcome = rollOutcome(P);  goto LOOP         0x55B94F
```

So the delivered outcome is the first one, in the order of repeated `rand(20)` rolls, whose preconditions
hold. There is no iteration cap. Outcomes `1` (map) and `3` (nothing) have no precondition, and every row of
the grid in section 4 contains an `M` or an `N`, so the loop terminates with probability 1. A pop whose
roll lands on `B` for a civ that may not receive barbarians (an Expansionist, a civ without a city or a
military unit, or one whose nearest city is adjacent) simply draws again.

Every successful outcome sets `*done = 1` immediately before returning. RNG order inside each outcome is in
section 7.

### 5.1 Outcome 0, gold (`0x55B973`)

```
if cell(x, y).vt+0x9C() != -1: reroll                      the tile already has a resource
turn  = [0xA526AC]
base  = (turn >= 50) ? 50 : 25                             0x55B9AD..0x55B9BB: setge / dec / and 0xE7 / add 0x32
sum   = P.+0x44 + P.+0x48 + base
if sum < 0: sum = 0
// split the new treasury again (the same write as 0x4C2350, economy.md "Treasury cell"):
if sum > 0:  a = timeGetTime() mod sum - 0x3039
else:        a = timeGetTime() mod 0xD431 - 0x8235
P.+0x44 = a;  P.+0x48 = sum - a                                       0x55C192..0x55C1C4
if RULE.default_money_resource ([0x9C729C]) != -1:                    0x55C1C7
    cell(x, y).vt+0xEC(good)                                          0x5DA1F0, set the resource id
    n = 0x5F3E70(map; good);  0x5F3E80(map; good, n + 1)             per-good tile tally, map +0x14C
message kind 0 with argument base                                      section 8
```

* Gold is `25` before round 50 and `50` from round 50 on (`[0xA526AC]` is the 0-based round counter). It is
  a fixed amount: no `rand` is involved, and the split uses the wall clock (`timeGetTime`), which does not
  change the sum.
* The hut must sit on a tile without a resource. When the rules name a money resource the tile receives it
  (setting a resource re-evaluates the city working the tile, `0x5DA1F0`). The shipped `conquests.biq` has
  `default_money_resource = -1`, so nothing is placed.

### 5.2 Outcome 1, map (`0x55C2B8`)

```
for n = 1 .. 76:                                                        0x55C2B8..0x55C3AA
    (dx, dy) = spiral(n)                                                0x5E6E50
    tx = wrapX(x + dx);  ty = wrapY(y + dy)                              0x426C00 / 0x426C40
    if tx in [0, W) and ty in [0, H):
        c = cell(tx, ty)
        if c.vt+0xB8() == cont  or  c.vt+0xC8() == 11 (coast):
            if rand(4) != 0:                                             3 of 4
                discover(P; tx, ty)                                      0x55B1A0, research.md 10.3
[0xA281C5] = 1                                                           redraw flag
message kind 1
```

* `n = 1..76` covers the rings `1, 2, 3` fully and the first 28 of the 32 positions of ring 4 (`barbarians.md`
  1.3: ring `r` holds `n` in `(2r-1)^2 .. (2r+1)^2 - 1`; ring 4 is `49..80`).
* The test is "same continent as the hut **or** coast terrain" (terrain nibble `11`); sea and ocean tiles are
  never revealed, a coast tile of another continent is. The draw happens only for tiles that pass the test,
  and for tiles that are in bounds only; off-map tiles take no draw.
* `discover` marks the tile as seen by `P` and counts it (`P.+0xA8`); it takes no draw. It runs for tiles
  already known too (it checks the mask itself).
* Outcome 1 has no precondition, so it never re-rolls.

### 5.3 Outcome 2, a tribe joins: a free city (`0x55B9D1`)

Preconditions, in this order, each failing one re-rolls (**V**):

1. `RACE[P.race].hasTrait(2)` (Expansionist) must be true (`0x55B9ED`).
2. `P.+0x194 <= Cw / (N - 1)` with `Cw = [0xA52690]` (cities in the world), `N = popcount([0xA526C0])`
   (`0x5DF900`), signed integer division (`0x55B9F7..0x55BA1B`): the civ may hold at most the average city
   count per real civ. `N = 1` would divide by zero (**H**: unreachable, slot 0 is normally in the mask).
3. `[0x9C34EC] > 3` (`0x55BA21`): the nearest own city on this continent has a distance key above `3`
   (6.2). With no own city on the continent the key is `0x7FFFFFFF` and the test passes.
4. `P.+0x194 != 0` (`0x55BA2E`): the civ owns at least one city anywhere.
5. `0x442480(0xA52658; x, y, P.slot, 0, 0) != 0` (`0x55BA4F`): the site valuation accepts the tile (6.5).
6. `(flags & 0x400) == 0` (`0x55BA5E`): City Elimination off (redundant with the roll, section 4).
7. `city = 0x5663C0(P; x, y, -1, -1, 0, 1)` returns non-zero (`0x55BA76`, `createCity`, 6.6).

Effects after the city exists:

```
if 0x47B530():                                           network game: tell the other machines   0x55BA7D
    0x470BD0(0x74AF60; city.id, 0)
    if city.owner (byte +0x28) == [0x9FD4BC]:  0x475240(0x74AF60; city.id, local slot);  0x4720B0(0x74AF60; city, &city.name)
if city == 0: reroll                                     0x55BAC5
if [0xA526AC] > 50:                                      round > 50 (strictly)
    0x4B9F60(city; rand(4), -1)                          grow the new city by 0..3 citizens (6.7)
UI: 0x4E69F0 and, when the local player has the tile in its seen set, 0x4E6B50(0x9F8700; x, y, 3, 0)
sound 0x537700(3) for the local player
rand(15) tribe pick, message kind 2
```

* The network block runs **before** the null test on `city` (`0x55BA84` precedes `0x55BAC5`); a failed
  creation would dereference null there. The test of step 5 above makes a failure rare (**H**: a full city
  pool, `0x5663F5`, is the remaining way).
* The extra citizens appear only from round 51 on, while the gold of 5.1 changes at round 50: the two
  thresholds differ (`>= 50` against `> 50`).
* The city is created by the routine the Settlers action also uses (`0x5B34C0`), with city id `-1`
  (allocate) and `a3 = -1`. Because the creation precedes the `rand(4)`, and `createCity` itself draws
  nothing (6.6), the draw order is unaffected by the city's details.

### 5.4 Outcome 3, nothing (`0x55C44F`)

No precondition and no effect other than the tribe pick `rand(15)` (section 6.3) and message kind 3. It is
also the result of the roll when `idx` is out of range.

### 5.5 Outcome 4, a Settlers unit (`0x55BBE8`)

Preconditions, each failing one re-rolls (**V**):

1. `int16 P.+0x162 + int16 P.+0x122 <= 0`: no live unit and no unit in production carries AI strategy bit
   `13` (SETTLE). `P.+0x108[k]` (`int16[20]`) counts live units per AI-strategy bit; `P.+0x148[k]` counts
   units being built: incremented by `0x55A0E0` (`0x4AFEB8`), decremented by `0x55A080` (`0x4AEDA8`,
   `0x4AFDBE`) (**H** for the "being built" reading, **V** for the arithmetic). `+0x122 = +0x108 + 2*13`,
   `+0x162 = +0x148 + 2*13`.
2. `P.+0x194 <= Cw / (N - 1)`: the same average-city rule as 5.3 step 2 (`0x55BC05..0x55BC26`).
3. `unit = 0x5694D0(P; RULE.start_unit_1, x, y, -1, -1, 0, 0, -1)` returns non-zero (`0x55BC42`).
   `RULE.start_unit_1` is `[0x9C72CC]` (RULE `+0xE8`, `0x55BC30`); the shipped value is PRTO `0`, Settlers.

The new unit keeps the factory's default experience (it is not forced to level 0). Then message kind 4 with
the prototype's name. The unit is created on the hut tile, owned by `P`.

### 5.6 Outcome 5, mercenaries (`0x55BCD4`)

```
type = chooseHutUnit(P; x, y)                                          0x55F7B0, section 6.4
if type == PRTO count ([0x9C3DB0]): reroll
unit = 0x5694D0(P; type, x, y, -1, -1, 0, 0, -1);  if unit == 0: reroll
unit.+0x44 = 0                                                          experience level 0 (EXPR row 0)
message kind 5 with the prototype's name
```

### 5.7 Outcome 6, an advance (`0x55BD94`)

The advance case, restating `research.md` 10.5 with the dispatcher context:

```
if P.+0xF4 > 0: reroll                                      only a civ still in era 0
cur = -1;  best = -1;  bestScore = 0x7FFFFFFF
for t in 0 .. T-1:                                           T = [0x9C3DBC]
    skip if bit P.slot of known[t] ([0xA52B4C] + 4t) is set
    e = TECH[t].+0x48;  skip if e > P.+0xF4 or e == -1       so e == 0
    skip unless 0x561440(P; p) is true for each of the four prerequisites at TECH[t].+0x58 .. (-1 counts as known)
    skip if e > 1                                            (dead after the previous tests)
    0x47B530()                                               called, result unused
    if t == P.+0xFC: cur = t; continue                       the current research is only a fallback
    d = max over the four prerequisites p of 0x561620(P; p, 1)       techDepth
    skip if d > 4
    r = rand(100)                                            first
    v = P.vtable[+0x54](P; t, 0, 0)                          the research valuation, research-ai.md (it draws too)
    score = v + (r & 0xFFFF)
    if score < bestScore: best = t; bestScore = score        strictly lower wins, the first one on ties
if best != -1:  grant(best)
elif cur != -1: grant(cur)
else:           reroll                                       0x55C182
grant(t):
    local player: UI hook 0x537700(3)
    rand(15)                                                 tribe pick, 6.3
    message kind 6 (0x4DCAF0, which also opens the "advance" popup on the controller, UI only)
    if 0x47B530(): 0x475460(0x74AF60; P.slot)                network: send the grant
    else:          0x561860(P; t, 0, 1, 1)                   acquire the advance
```

* A hut gives an advance only to a civ in era `0`, picks among era-0 advances it can already research, and
  prefers the **lowest** valuation plus noise `0..99`, the opposite end of the ranking from the default pick
  (`research.md` 9.2).
* The `rand(100)` of every candidate precedes that candidate's valuation call (the valuation draws as well),
  so the draw order is: for each surviving `t` in increasing index order, `rand(100)`, then the valuation's
  own draws.

### 5.8 Outcome 7, a barbarian band (`0x55BF40`)

Preconditions, in this order (**V**):

1. `RACE[P.race].hasTrait(2)` must be false: an Expansionist never meets barbarians in a hut (`0x55BF5C`).
2. `[0x9C34E8] > 1` (`0x55BF66`): the nearest own city on this continent is at tile distance `(dx' + dy') / 2`
   above `1`. With no own city on the continent the value is `0x7FFFFFFF` (passes).
3. `P.+0x194 != 0` and `P.+0x190 != 0`: the civ has a city and at least one military unit.
4. If `unit != 0`: `unit.hasAbility(4)` (ALL_TERRAIN_AS_ROADS, the Explorer) must be false (`0x5BC8B0`,
   `0x55BF8F..0x55BF9E`): an Explorer never triggers barbarians.

```
tribe = first index in the tribe table (6.3) that is unused, else 75      rand(15)
budget = 4;  spawned = 0
for n = 1 .. 8:                                                           0x55C026..0x55C170
    k = ((turn + n) mod 8) + 1                                            signed mod, turn = [0xA526AC]
    (dx, dy) = spiral(k)                                                  ring 1 only: k in 1..8
    tx = x + dx wrapped by hand (flag [0x9C755C] bit 0: tx += W if tx < 0, tx -= W if tx >= W)
    ty = y + dy wrapped by hand (bit 1 likewise with H = [0x9C74C0])
    if tx in [0, W) and ty in [0, H) and not cell(tx, ty).vt+0x8C() (land)
       and tileOccupant(tx, ty, -1, 1) == -1:                             6.1: no city, no unit, no camp, no colony owner
        if rand(max(budget, 1)) != 0:
            u = 0x5694D0(Player[0]; RULE.basic_barbarian_unit, tx, ty, tribe, -1, 0, 0, -1)
            if u != 0:
                u.+0x44 = 0                                               level 0
                0x4E6C30(0x9F8700; tx, ty)                                 UI
                spawned += 1;  budget -= 1
if spawned == 0: reroll
else: sound (local player), message kind 7 with the tribe name and the prototype's name
```

* `RULE.basic_barbarian_unit` is `[0x9C7254]` (RULE `+0x70`, `0x55C120`); the shipped value is PRTO `6`, Warrior.
* A tile spawns with probability `(b - 1) / b` where `b = max(budget, 1)`, so successive spawns have chances
  `3/4`, `2/3`, `1/2` and then `0` (`rand(1)` is always `0`): **at most three units**. The draw is taken for
  every eligible tile, including the final ones at `b = 1`.
* The eight ring-1 tiles are visited in a rotation that depends on the round number (golden vector G3).
* The hut tile itself is not a candidate (ring 1 starts at `n = 1`).
* The spawned units belong to slot 0 and are subject to the barbarian rules of `barbarians.md`; the tribe id
  is stored in `unit +0x3C`. The tribe table entry is **not** marked as used by this outcome (the scan only
  reads it), unlike camps.

## 6. Helper routines

### 6.1 `tileOccupant(x, y, viewer, vis)` `0x56D7D0`, plain `ret` (**V**)

Cdecl, four stack arguments. Returns the slot of the civilization that occupies the tile, `0` for a
barbarian presence, `-1` for an empty tile. Callers: this document (`(x, y, -1, 1)`), `unit-turn.md` 4.1
(`healEligible`), `victory.md` 12.3, `0x56D630` is the same walk over all units (`barbarians.md` 3).

```
if (x, y) is off the map (0x426BD0): return -1
cell = getCell(x, y)
if cell has a city (0x5EA6C0):                       r = cell.vt+0x114()          owner of that city
elif cell hosts a colony (vt+0x68: colony id != 0xFFFF):
        c = cell.vt+0x118()                          owner of the colony (airfield, radar, outpost, colony)
        r = first value among the units on the tile in list order (cell.vt+0xA0 head, iterated with 0x426C80,
            unit = 0x437870(unitPool 0xA52E80; id)), taking only units u with
                (not (vis and viewer != -1)  or  0x5BB650(u; viewer, 1))          hostile filter, off for viewer = -1
            and  a = 0x52CA00(u; viewer) != 0                                     apparent owner, non-zero
        if none: r = c
else:
        r = -1
        for each unit u on the tile with the same filter:  r = 0x52CA00(u; viewer);  stop at the first r != 0
        (so r ends 0 when the last unit examined was barbarian-looking, -1 when no unit passed the filter)
if r == -1 and cell.vt+0x1C(viewer == -1 ? 0 : viewer):  return 0       a barbarian camp on the tile
return r
```

* `0x52CA00(u; viewer)` returns `0` when `u` has ability 17 (Hidden Nationality) and `viewer` is not `-1`, `0`
  or the unit's owner; otherwise `u.+0x34`.
* The hut spawn uses it as "nothing at all stands on the tile": a camp, a colony, a city, or a unit of any
  civ (including a barbarian, which yields `0`) makes the tile ineligible.
* This is **not** the border owner of the tile: that is the owner byte `cell.vt+0x98()`.

### 6.2 The nearest-city globals

`0x56D040` (`barbarians.md` 1.5) writes `[0x9C34EC] = 0x7FFFFFFF` and `[0x9C34E8] = 0x7FFFFFFF` at entry
(`0x56D04E`, `0x56D053`) and overwrites them with the winner's key and tile distance. The dispatcher calls it
once, before the roll loop, with `(x, y, P.slot, cont, -1, -1, 0)`: the nearest city **owned by P on the hut's
continent** (`barbarians.md` 1.5 describes the continent filter for water bodies). Re-rolls do not recompute
the globals. This settles the open point of `barbarians.md` 1.5: with no city the globals stay
`0x7FFFFFFF`.

### 6.3 The tribe pick (message flavour; tribe id for outcome 7)

Used by every outcome (eight scans: `0x55BBC3`, `0x55BCB8`, `0x55BD78`, `0x55BFE8`, `0x55C27A`, `0x55C40F`,
`0x55C493`, `0x55C584`):

```
r = rand(15)                                        one draw
g = RACE[P.race].+0x90C                              culture group, 0..4
for j in 0 .. 14:
    i = 15*g + (r + j) mod 15
    if used[i] == 0:  return i                      used[] = the 75-byte table at 0xA526C8
return 75                                           all 15 in use
```

The name shown is `RACE[0].cityNames[i]` (the pointer at `[0x9C71D0] + 0x14`, 24-byte entries; the barbarian
civilization's city list holds the tribe names). The scan never writes `used[]`. In outcomes other than 7 the
index is only a string argument of the message; in outcome 7 it also becomes the units' tribe id. Index `75`
(the sentinel) names a string one past a 75-entry list (**O**: the shipped list's length decides whether
that is a valid string).

### 6.4 `chooseHutUnit(P; x, y)` `0x55F7B0`, `ret 8` (**V**)

Returns a PRTO row, or `n = [0x9C3DB0]` (the PRTO count) for "none".

```
water = (cell(x, y).vt+0x8C() != 0)
n     = [0x9C3DB0]
step  = (n mod 3 != 0) ? 3 : 1                       coprime with n, so the walk visits every row once
cur   = rand(n) & 0xFFFF                              one draw, before the n > 0 test
if n <= 0: return n
repeat n times:
    row = PRTO[cur]                                   [0x9C71E0] + 0x138 * cur
    ok  = not hasAbility(row, 0)                      0x5E4EF0: Wheeled
          and (row.avail & 1)                         available_to_civs (row +0x90) bit 0: RACE row 0, the barbarians
          and (row.avail & (1 << P.race))
          and row.class == water                      row +0x9C: 0 land, 1 sea; air (2) never matches
          and techEra(row) == P.+0xF4                 techEra = TECH[row.+0x74].+0x48, 0 when the row needs no advance
    if ok:
        players = 0;  have = 0
        for q in 1 .. 31 with bit q of [0xA526C0]:
            players += 1
            c = int16 Player[q].+0x15F0[cur]           live units of that type
            if c > 0:                      have += c
            elif Player[q].canBuildUnit(cur, 1, 0):    have += 1        0x56A7C0, buildable.md 3
        if players > 0 and have >= players: return cur
    cur = (cur + step) mod n
return n
```

* The pool of mercenaries is therefore: units a barbarian could have **and** the civ may build, of the
  civ's current era, and "common enough" (live count plus the number of civs that could build it, summed over
  every civ in play, at least the number of civs).
* In the shipped `conquests.biq` only two rows pass the static filter (non-wheeled, land, available to race
  0): Warrior (PRTO `6`) and Horseman (PRTO `11`), both era 0. `n = 141`, `141 mod 3 = 0`, so the step is
  `1` and the walk is sequential with wrap: a start `r` in `0..6` meets the Warrior first, `7..11` the
  Horseman first, `12..140` wraps round to the Warrior first. A Warrior needs no advance, so it always passes
  the popularity test (every civ either has some or can build one).
* After the era moves on (`P.+0xF4 > 0`) no row matches and the outcome re-rolls.

### 6.5 The site valuation `0x442480(game; x, y, civ, a4, a5)`, `ret 0x14`

Only the gates relevant here were read (**V**); the scoring body (`0x44265x..0x443140`) is **O** and belongs
to the AI settler documentation. The result is `max(score, 1)` for an accepted site, and `0` for a rejected one:

1. `0x5F3160(map; x, y, civ, 1) != 0` (`0x44249D`): the tile is not a legal city site (**H**: legality and
   minimum-distance rule) returns `0`.
2. `civ != -1` and the tile's owner byte `cell.vt+0x98() > 0` and `!= civ` (`0x4424DA`): the tile belongs to
   another civ returns `0`.
3. Over the 49 spiral tiles `n = 0..48`: `>= 4` tiles beyond the 3x3 block (`n >= 9`) owned by another civ
   return `0` (`0x44262D`); `>= 10` tiles with plane-2 bit `0x20000` set (a tile inside some city's work area,
   set by `0x4AE2A0` for `n < 21`) return `0` (`0x442640`).
4. Further returns of `0` at `0x442C8C` (only when `a4` is non-zero and `0x5DBE70(cell)` is zero) and at
   `0x443153` (**O**: the condition reads a stack slot that I could not attribute).

A hut city is therefore refused next to foreign borders or inside an existing city's catchment. The call
reads no gameplay RNG (**V** by call-site census: no `call 0x60BAB0` in the function body; callees not followed).

### 6.6 `createCity(P; x, y, a3, cityId, a5, a6)` `0x5663C0`, `ret 0x18` (**V**, gates only)

Returns the new City or `0`. The hut passes `(x, y, -1, -1, 0, 1)`; the Settlers action `0x5B34C0` passes
`(x, y, unit.+0x38, .., .., 1)`. Argument `a4` (stack position `+0x38`) is the requested city id.

```
if ([0xA52E78] - [0xA52E74] + 1) >= 512: return 0            city pool full: last index - free count + 1 (0x5663F0)
if cell(x, y) has a city (0x5EA6C0): return 0
cont = cell(x, y).vt+0xB8()
if cityId >= 0 and pool[cityId] is occupied: return 0         explicit id (loading, network)
allocate the City object (0x544 bytes, ctor 0x4AC590, vtable 0x66DC78) in pool slot cityId, or from the free
list head [0xA52E70] / count [0xA52E74], or appended ([0xA52E78] += 1)
for every unit in the unit pool whose owner != P.slot and whose (x, y) is this tile:
    network:  0x474140(0x74AF60; unit.id, P.slot, 0, 0, 0, 1)     queue the kill
    else:     Unit::kill(unit; P.slot, 0, 0, 0, 1, 0, 0)          a5 = 1, unit-turn.md 5
terrain: if cell.vt+0x78() is false and cell.vt+0x11C() != -1 (the TERR row's worker job): 0x5D59E0(map; cell.vt+0xC4(), x, y)   (H: a city turns forest, jungle or marsh into its base terrain)
clear overlay mask 0x5C (mine, irrigation, fortress, pollution) on the tile: vt+0xCC(0, 0x5C, x, y)
[0xA52690] += 1                                             cities in the world
[0xA52B48][cont] += 1                                       cities per continent (global array)
P.+0x194 += 1;  P.+0x1610[cont] += 1                        cities of P; cities of P per continent
if a city currently works this tile (word cell+0x6C valid): 0x4BBC80(thatCity, 0x5F3F50(...)) release the tile    (H)
cell.vt+0xE4(cityId)  (city id word cell+0x1A = cityId, cell+0x24 = 0);  word cell+0x6C = cityId
if a city W was working this tile: W.vt+0x38(0)                                (W is refreshed, not the new city)
0x4AE2A0(city; cityId, x, y, P.slot, a3, a5, a6)            City::init, 7 arguments, section 6.8
network games: clear bits 0xC of [0xA52680]; redraw (0x4E69F0) and [0xA281C4] = 1 when the local human sees the tile
return city
```

`0x5663C0` and `0x4AE2A0` do not call the gameplay RNG (`city-founding.md`); the full routine is
`city-founding.md` section 2.

### 6.7 `0x4B9F60(city; n, race)`, `ret 8` (**V** structure, **H** labels)

Adds `n` citizens one at a time (`n <= 0` does nothing). Per citizen, while `city.+0x138 < 255`
(`0x4B9F7D`):

```
rec = 0x4C2040(city+0xDC; &out);  if none: next iteration                     allocate a citizen record
if race == -1: race = Player[city.+0x28].+0x20                                  the owner's race, 0x4B9FAE..0x4B9FC2
0x4ABD90(rec; &out-value, city.+0x20 (city id), race)                          fill the record
city.+0x138 += 1 (saturating at 0x7FFFFFFF)                                     citizens ever added
city.+0x1C8 .. +0x1D0 = 0                                                       three accumulators
for k in 0 .. 20 (spiral: the centre, then the 20 ring tiles):
    if the tile is valid and word cell+0x6C == city.id:                         the city works this tile
        for c in 0 .. 2:  city.+0x1C8+4c += 0x4B0330(city; c, tx, ty)           (H: food, shields, trade of the tile)
city.+0x244 = (city.+0x30 & 1) ? city.+0x1C8 : (city.+0x138 - 0x4BB2A0(city; -1)) * RULE.food_per_citizen ([0x9C72B4])
city.+0x250 = city.+0x1C8 - city.+0x244                                         (H: food surplus)
0x4B05D0();  0x4B07C0();  0x4BCFF0();  city.vtable[+0x38](0)                    recompute and refresh
```

The hut passes `n = rand(4)` and `race = -1`. The record layout and the three recompute routines are **O**
(`happiness.md` and `economy.md` cover `0x4BCFF0` and the yields).

### 6.8 `City::init` `0x4AE2A0`

Specified in full in `city-founding.md` section 3. The hut passes `nationality = -1` (the first citizen takes the
owner's race), `name = 0` (automatic name, `city-founding.md` 4) and `borderFlag = 1` (the border update
`0x5D4830` runs, after the Palace step and before the first citizen). A hut city is the owner's only city when
the owner had none, in which case it is also the capital (`city-founding.md` 3.7), and an AI owner receives the
start bonus of `city-founding.md` 5.

### 6.9 The unit factory (`0x5694D0`, `barbarians.md` 3)

`0x5694D0(P; type, x, y, tribe, unitId, flag6, flag7, extra)` returns `0` when the pool holds `>= 0x2000`
units, when `type == -1`, or when a unit of another civ stands on the tile (`0x56D630(x, y, -1, 1)`, bypassed by
`flag6`). A new unit gets experience level 1 unless the caller overrides it, as outcomes 5 and 7 do.

## 7. RNG draw order (**V**)

All draws are `rand(n)` on the gameplay generator. `roll` = `rand(20)` of section 4. The order below is the
order of the draws within one pop.

| path | sequence of draws |
|---|---|
| unitless pop | none for the first outcome (`2`); every re-roll draws one `rand(20)` |
| unit pop | `roll`; every re-roll draws one more `roll` |
| outcome 0 | none for the amount; then `rand(15)` (tribe pick) |
| outcome 1 | for each of up to 76 spiral tiles in order, `rand(4)` per qualifying tile; then `rand(15)` |
| outcome 2 | the city is created first (no draw); `rand(4)` only when turn > 50; then `rand(15)` |
| outcome 3 | `rand(15)` |
| outcome 4 | the unit is created (no draw); then `rand(15)` |
| outcome 5 | `rand(n)` (`chooseHutUnit`); the unit is created; then `rand(15)` |
| outcome 6 | per surviving tech: `rand(100)` then the valuation's draws; then `rand(15)` after the choice |
| outcome 7 | `rand(15)` first (tribe pick); then one `rand(max(budget,1))` per eligible ring tile in rotation order |

Two details that affect replay: (a) the tribe pick in outcome 7 happens **before** the spawn loop while in all
other outcomes it happens last; (b) a failed outcome consumes only what it drew before the failing test,
which is nothing for outcomes 0, 2, 4 (all their tests precede any draw) and, for outcome 6 and outcome 5,
the draws listed above when they reach the end of their scan without a result (outcome 6 re-rolls after it
has drawn `rand(100)` for every surviving tech; outcome 5 after the single `rand(n)`).

## 8. UI and network side effects (no effect on the simulation state except as listed)

* `0x4DCAF0(0x9F8700; slot, x, y, tribeIndex, kind[, extra])` (cdecl, six or seven arguments) shows the
  popup only when `slot` equals the local player (`[ui+0x4DBC]`). Kinds `0..7` select the keys
  `GOODY_MONEY`, `GOODY_MAPS`, `GOODY_CITY`, `GOODY_NOTHING`, `GOODY_SETTLERS`, `GOODY_MERCENARIES`,
  `GOODY_TECH`, `GOODY_BARBARIANS` through `0x4ED220(0x9F8700; x, y, key, 1)`. The text variables are the tribe
  name (slot 0 of `0x61C5A0`), the amount (kind 0), the prototype name (kinds 4, 5, 7) or the advance name
  (kind 6). Kind 6 additionally prepares the research popup on the controller `0x49FC50()`
  (`0x4DCD5A..0x4DCE01`, `0x611530`).
* Sound hooks `0x537700(k)` with `k = 6` (gold) or `3` (map, city, settlers, mercenaries, advance, barbarians)
  run only for the local player and take no draw.
* In network games (`0x47B530` true) outcomes 2 and 6 send synchronisation records through the object
  `0x74AF60` (`0x470BD0`, `0x475240`, `0x4720B0`, `0x475460`); the host-side simulation then proceeds as in a
  local game. The treasury split of outcome 0 uses `timeGetTime`, so the pair differs between machines while
  the sum agrees.

## 9. Golden vectors

**G1. Roll grid.** The two grids of section 4 are the complete output of `rollOutcome` for all
`(idx, r)`, with and without the City Elimination flag.

**G2. Gold.** Round 49, treasury pair sum `100`: `base = 25`, new sum `125`. Round 50: `base = 50`, `150`.
Pair sum `-60` (not reachable normally) and round 10: `sum = 0`, the clock branch of the empty split.

**G3. Outcome 7 visiting order.** `k_n = ((turn + n) mod 8) + 1`, `n = 1..8`, and ring offsets
`k: 1 (1,-1)  2 (1,0)  3 (1,1)  4 (0,1)  5 (-1,1)  6 (-1,0)  7 (-1,-1)  8 (0,-1)`.
Round `0`: `k = 2,3,4,5,6,7,8,1`; round `7`: `k = 1,2,3,4,5,6,7,8`; round `6`: `k = 8,1,2,3,4,5,6,7`.

**G4. Number of barbarians when `e` of the eight ring tiles are eligible** (every eligible tile takes its
draw, spawn chance `(b-1)/b`, `b = max(budget,1)`, start budget 4), exact fractions:

| eligible `e` | P(0) | P(1) | P(2) | P(3) |
|---|---|---|---|---|
| 1 | 1/4 | 3/4 | - | - |
| 2 | 1/16 | 7/16 | 1/2 | - |
| 3 | 1/64 | 37/192 | 13/24 | 1/4 |
| 4 | 1/256 | 175/2304 | 115/288 | 25/48 |
| 8 | 1/65536 | 58975/47775744 | 249355/5971968 | 952525/995328 |

The outcome re-rolls when no unit was placed (probability `P(0)` of the row), and four units are never placed.

**G5. Mercenary pick, shipped rules.** `n = 141`, step `1`. With `rand(141) = 100` the walk is
`100, 101, .., 140, 0, 1, .., 6`: the first row that passes is PRTO `6` (Warrior) for a civ in era 0 whose
race bit is set in its `available_to_civs` (`-33554433` clears only bit 25). With `rand(141) = 9` the first row
is PRTO `11` (Horseman) if Horseback Riding is known or buildable by enough civs, otherwise the walk wraps and
ends at PRTO `6`.

## 10. Corrections and open items

Corrections this document makes to other files (applied in the same change): `barbarians.md` 1.5 (the globals
keep `0x7FFFFFFF`), `barbarians.md` 9 (the hut scans are tribe picks, not message variants, and outcome 7 uses
the pick as the tribe id), `research.md` 10.5 (the outcome roll is now specified), `unit-turn.md` 4.1 and
`victory.md` 12.3 (`0x56D7D0` is the tile **occupant** resolver, not a border-ownership routine).

Open:

1. `0x4AE2A0` is decoded (`city-founding.md`); its undecoded callees (`0x4B0470`, `0x4B10F0`) are listed in
   `city-founding.md` 7 (`0x55CB20`, `0x4ACF40` are in `city-buildings.md`; `0x5D4830` is in `borders-culture.md`).
2. The scoring body of `0x442480` and its `0x443153` return.
3. (Settled: the head of `0x5BD220` has no trigger-1 filter, `movement.md` 5.)
4. The citizen record layout and the recompute routines called by `0x4B9F60` (6.7).
5. Whether anything creates huts after map generation, and the shipped length of the barbarian civ's city name
   list (the index-75 sentinel).
6. `0x5F3160` (city legality), `0x5DBE70`, `0x56D630` (all units), `0x4E69F0`.
