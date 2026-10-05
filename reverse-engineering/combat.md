# Combat resolution: odds, rounds, retreat, bombard

Owns: **how one fight is resolved** in `Civ3Conquests.exe`: the per-round odds
and every percentage term feeding them, hit points, the round loop with its
retreat rolls, the choice of the defender, ranged attacks (bombard, city
strikes, the cruise missile, defensive bombard), the bookkeeping after a
victory, the barbarian raid on a city, the diplomatic gates in front of an
attack, and the random source behind all of it. Air defense (SAM, flak,
patrolling interceptors) lives in `air.md`. Reference implementation:
`rust/src/combat.rs` (dice: `rust/src/rng.rs`; air layer: `rust/src/air.rs`).

Status words used below: **verified** = read from disassembly, and where noted
cross-checked against text the game ships; **HYPOTHESIS** = inferred, not proven;
**open** = not located or not read. Raw disassembly overrules any other source;
static VAs equal runtime VAs (image base `0x400000`).

## At a glance

```text
odds   = 1024 * P(defender wins a round)          clamp(1024*X / (X+Y), 1, 1023)
X      = (defStr + defArmyBonus) * (100 + D)      D = terrain, river, tile, fortify, barbarian
Y      = (atkStr + atkArmyBonus) * (100 + P)      P = radar, barbarian, amphibious
round  : roll = next(1024)  on the global gameplay Random (0xA526B4)
         roll >= odds -> attacker wins the round, defender takes 1 damage
         roll <  odds -> defender wins the round, attacker takes 1 damage
dead   : maxHP - damage <= 0          maxHP = EXPR[level].baseHP + PRTO bonus   (floor 1)
retreat: one roll per side, only when that side is left at exactly 1 HP,
         the opponent has > 1 HP, the side is not barbarian, and its unit is fast
```

Odds are computed **once per duel**; every round reuses them (the only
recompute is a participant change inside a stack or army, section 6).

## 1. The random source

The earlier docs said combat uses MSVC `rand()`. That was wrong. There are three
random sources:

| source | where | users |
|---|---|---|
| **Gameplay `Random` instance** | global object `0xA526B4`; methods `0x60BA80` (float), `0x60BAB0` (`next(n)`) | 171 call sites pass `ecx = 0xA526B4`: combat rounds (`0x4A5B3C`), retreat (`0x4A650A`, `0x4A7013`), bombard (`0x4A3621..0x4A3661`), tile strike, riot, enslave and others |
| Map generator instances | private `Random` objects seeded from `water_level + stage constant` | map generation only (`NOTES.md`) |
| MSVC `rand()` | `0x64A20E`, `srand` `0x64A201`, state in per-thread data `ptd+0x14` | 70 direct call sites; **no combat die** |

The `Random` class is the generator already in `rust/src/rng.rs`:

```text
s            = s * 0x41C64E6D + 0x3039                 (u32, wrapping)
k            = (s >> 16) & 0x7FFF                      15-bit output
next(n)      = floor(k * (n & 0xFFFF) / 32768)         (float multiply by 2^-15 at 0x6716C8, _ftol truncation)
```

Because `k * n` has at most 31 significant bits the float path is exact, so
`next(n) == (k * (n & 0xFFFF)) >> 15`. For the round die `n = 1024` that is
`k >> 5`: each of the 1024 values has exactly 32 preimages, so a round is
**exactly uniform** and `P(attacker wins a round) = (1024 - odds) / 1024`. The
retreat dice (`n = retreat% + 50`) are *not* exact (32768 is not a multiple of
`n`), so a clone must reuse `Rng::below` rather than a modulus.

### Seeding (verified; every site read this session)

| site | effect |
|---|---|
| `0x56C1F0..0x56C1FA` start-up | `[0xA526B4] = timeGetTime()`; a second `timeGetTime()` goes to `srand` |
| `0x591B66..0x591B76` after loading a game | `[0xA526B4] = timeGetTime()` **unless** `[0xA5267C] & 0x100` (bit 8). The flag's meaning is **HYPOTHESIS** ("preserve seed"). So by default a loaded save does not reproduce its dice |
| `0x4947EF..0x4947F7` new game | `[0xA526B4] = [0x9903B8]` (shared seed) |
| `0x478FA3..0x478FAC` multiplayer resync | `[0xA526B4] = [0x9903B8]` |

The writer of `[0x9903B8]` is **open**. The one stream is shared by 171 sites,
so any other gameplay roll between two fights shifts the combat dice; a clone
that wants the original's statistics needs only the distributions, not the
stream.

## 2. Call map

**The duel and its terms**

| address | role | state |
|---|---|---|
| `0x4A0ED0` | `odds(A, B, bombard, flag4)`, `ret 0x10` | verified |
| `0x4A53A0` | the duel loop (2 235 lines); `this` = step object: `[+0]` A, `[+4]` B, `[+8]` defender-retreat flag, `[+9]` attacker-retreat flag, `[+0xC]` retreat direction, `[+0x1C]/[+0x20]` target tile | verified |
| `0x4A47C0` | builds the step: chooses the defender (6.1) and sets the retreat flags (7) | verified |
| `0x56CD80` | terrain + river term | verified |
| `0x56CEB0` | tile term: city / fortress / barricade, plus radar | verified |
| `0x4C10B0` | city building bonus (land) | verified |
| `0x4BB2A0` | resisting citizens of a city | verified |
| `0x5BCAE0` | army bonus | verified |
| `0x5BE6E0` / `0x5BE820` | attack / defense strength | verified |
| `0x5BE5B0` | maximum hit points | verified |
| `0x5BE470` | maximum movement | verified (used as `maxMove - [unit+0x50]`, clamped to 0..9999 by `0x426710`) |
| `0x5E6DA0` | direction index of a coordinate delta | verified |
| `0x4A8320` | attacker radar test | verified |
| `0x55AEF0` / `0x55AF40` | set / clear a civ's radar bit on a tile | verified |
| `0x5BFB60` | move a retreating defender | called; internals open |
| `0x5BCA90` | container-of (the unit that carries this one; 0 if none) | verified |
| `0x42B5B0` | fortify percentage of the outermost container (3.2, 4.4) | verified |
| `0x5BC8B0` | `Unit::hasAbility(n)`; `0x5E4EF0` is the PRTO form | verified |
| `0x5BB650(u, owner, 1)` | is `u` hostile to `owner` | called |

**Choosing the defender (section 6.1)**

| address | role | state |
|---|---|---|
| `0x4A1590` | legality filter for one candidate | verified |
| `0x4A1740` / `0x4A1910` | strongest / weakest defender loops | verified |
| `0x4A12D0` / `0x4A1430` | the two comparators (King clause, tie-breaks) | verified |

**Ranged attacks (section 8)**

| address | role | state |
|---|---|---|
| `0x5C1410` | `Unit::attackAt`: entry for every attack order | verified |
| `0x5C0E20` / `0x5C1210` | validation / dispatch below it | verified |
| `0x4A3A70` | the ranged attack (shipped bombard), callers `0x478AFF` (network), `0x5C1481` | verified |
| `0x4A1FA0` | target selection and shot loop for the units on a tile | verified |
| `0x4A36C0` | counts the legal targets on a tile (all classes) | verified |
| `0x4A2650` | city strike `(attacker, city, mode, rolls)`, `ret 0x10` | verified |
| `0x4A2550` | collateral strike hook (tile or city), `ret 8`; effect `0x5B4DC0` | city part verified, `0x5B4DC0` open |
| `0x4A2B40` | cruise missile (callers `0x470291`, `0x5B4D40`) | verified |
| `0x4A3320` | legacy ranged attack (special-action bit 17), mid-function `0x4A3440`; dormant | verified |
| `0x4A1AE0` / `0x4A3280` | defensive bombard: victim check / one shot (section 8.9) | verified |
| `0x4A4520` / `0x4A46A0` | interception duel / report (`air.md`) | verified |
| `0x4C0C70` / `0x4C1000` | land / naval bombard defense of a city | verified |
| `0x4C1320` / `0x4C1470` | destroy the building that holds that defense | verified |
| `0x4C1590` | pick and destroy a random non-wonder building (strike mode 1) | called |
| `0x4ACB50(city, bld, flag)` | the building is in the city / acts on it | verified |
| `0x4ACF40` | remove a building from a city | called |
| `0x4C11E0` / `0x4C1280` | SAM / coastal-fortress strength sums (`air.md` 1.2) | verified |
| `0x4A7280` | AI estimator of a fight's outcome (calls `0x4A0ED0` at `0x4A731E`, `0x4A7343`) | identified, not decoded |

**After the fight (section 6.2) and around it (section 14)**

| address | role | state |
|---|---|---|
| `0x5BEF00(winner, loser, flag)` | victory bookkeeping: promotion, Great Leader, golden age, enslave | verified |
| `0x5BBBC0` | kill a unit | cargo recursion verified (6.3); other bookkeeping open |
| `0x5631B0(otherCiv, amount)` | incident tally against `otherCiv` (14.4) | verified |
| `0x563410` | barbarian raid / city capture entry (section 14) | barbarian branch verified |
| `0x5B5790(unit, otherCiv, interactive)` | may this unit attack that civ (14.3) | verified |
| `0x5B5600(unit, otherCiv, flag)` | provoke: war declaration, allies, aggression counter (14.3) | verified |
| `0x501F20(player, civ, reason)` | `declareWar`; writes the at-war byte table | verified |
| `0x558F70(player, unit)` | `Player::atWarWith(unit)` | verified |
| `0x5C68A0`, `0x52C7A0` | air defense and its range test (`air.md`) | verified |

Callers of `0x4A0ED0` (arguments as pushed: `A, B, bombard, flag4`):

| site | arguments | where |
|---|---|---|
| `0x4A5AF9` | `(A, B, 0, 0)` | the duel, `0x4A53A0` |
| `0x4A731E` | `([this], [this+4], 0, 0)` | AI estimator `0x4A7280` |
| `0x4A7343` | `(eax, [this], 1, 1)` | AI estimator `0x4A7280` |
| `0x4A3621` | `(ebx, esi, 1, 0)` | legacy ranged attack `0x4A3320` |
| `0x4A2BE6` | `(.., .., 1, 0)` | cruise missile `0x4A2B40` |
| `0x4A3F5B` | `(.., .., 1, 0)` | ranged attack `0x4A3A70` |
| `0x4A3292` | `(edi, esi, 1, 1)` | defensive bombard `0x4A3280` |
| `0x4A4555` | `(esi, edi, 0, 1)` | interception duel `0x4A4520` |

## 3. The odds function `0x4A0ED0`

### 3.1 Formula (verified)

```text
0x4A11AE  add ebp,0x64        ebp = 100 + D
0x4A11B1  add ecx,0x64        ecx = 100 + P
0x4A11B4  imul eax,ebp        eax = (defStr + defArmyBonus) * (100 + D)     = X
0x4A11B7  imul ecx,edi        ecx = (100 + P) * (atkStr + atkArmyBonus)     = Y
0x4A11BA  add ecx,eax         ecx = X + Y
0x4A11BC  shl eax,10 ; cdq ; idiv ecx          1024 * X / (X + Y), truncating
          clamp to [1, 0x3FF]
```

* `atkStr = 0x5BE6E0(A)`, or `PRTO[A].+0x48` (bombard strength) when `bombard != 0`.
* `defStr = 0x5BE820(B)`.
* Army bonuses: `0x5BCAE0(A, 0)` and `0x5BCAE0(B, 1)` (section 5). The attacker's
  bonus is added even in bombard mode.
* The clamp's **upper** edge fires only when `Y == 0` (raw value exactly 1024):
  for `Y > 0` the raw value is below 1024. The lower edge fires when `Y > 1023 X`
  (including `X == 0`). So a zero-attack attacker still wins 1 round in 1024, and a
  zero-defense worker still wins 1 round in 1024.
* `X + Y == 0` divides by zero (the original traps); the Rust returns `None`.

### 3.2 Defender percentage `D`

| term | amount | condition | where |
|---|---|---|---|
| terrain | `TERR[t].+0x58` (mem), see 4.1 | always | `0x56CD80` |
| river | `[0x9C72B8]` (25) | defender's tile has a river edge toward the attacker | `0x56CD80` |
| tile | city / fortress / barricade, see 4.2 | always | `0x56CEB0` |
| radar | 25 | the defender's own civ has a radar bit on its tile | `0x56CEB0` tail |
| fortify | `[0x9C72F0]` (25) | land kind, tile not water, order == 1, movement left > 0 | `0x4A0F79..0x4A0FC9` |
| fortify (army) | `[0x9C72F0]` (25) | defender is loaded in a container (army): the **outermost container** is tested instead (4.4) | `0x4A0F67`, `0x42B5B0` |
| barbarian | `DIFF[difficulty(B.owner)].+0x64` (+100 with the barbarian-bonus wonder) | **attacker** `A.owner == 0` | `0x4A0FCF..0x4A1025` |

### 3.3 Attacker percentage `P`

| term | amount | condition | where |
|---|---|---|---|
| radar | 25 | A's civ has a radar bit on the **defender's** tile | `0x4A1028..0x4A105E`, `0x4A8320` |
| barbarian | `DIFF[difficulty(A.owner)].+0x64` (+100 with the wonder) | **defender** `B.owner == 0` | `0x4A1063..0x4A10C1` |
| amphibious | 25 | see 4.6 | `0x4A10C6..0x4A1187` |

### 3.4 Modes

| mode | `bombard` | `flag4` | what changes |
|---|---|---|---|
| duel | 0 | 0 | everything above |
| bombard | 1 | 0 | attacker strength is `PRTO +0x48`; terrain/tile terms are called with **no attacker position and no civ** (no river, no defender radar) |
| raw | any | 1 | `0x4A0F19 jne 0x4A0FCF` skips terrain, river, tile structure, defender radar and fortify. **Kept:** army bonuses, both barbarian terms, the attacker's radar, amphibious |

## 4. The terms in detail

### 4.1 Terrain and river, `0x56CD80(ax, ay, bx, by)`

* Terrain: the term comes from `0x5DBFB0(cellB)` and is `TERR[id].+0x58` (body
  `+0x54`) of the defender's terrain. If the cell's overlay bit 29 (vtable slot `0x78`,
  `0x5EA980`) is set it reads `TERR[id].+0xA4` instead; that field is **open** (unmapped,
  and the bit's meaning too). Table for `conquests.biq`, identical to the game's
  Civilopedia "Defender Combat Bonus" list (`Text/Civilopedia.txt`, entry
  `GCON_Terrain_Combat`):

  | id | 0 Desert | 1 Plains | 2 Grass | 3 Tundra | 4 Floodpl. | 5 Hills | 6 Mount. | 7 Forest | 8 Jungle | 9 Marsh | 10 Volcano | 11 Coast | 12 Sea | 13 Ocean |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | % | 10 | 10 | 10 | 10 | 10 | 50 | 100 | 25 | 25 | 20 | 80 | 10 | 10 | 10 |

* River: `+[0x9C72B8]` iff `cellB.vfunc(0x94)() & (1 << dir)` where `dir` is the
  direction **from the defender toward the attacker**: `0x5F3F50(bx,by,ax,ay,9)`,
  or the fallback wrap-normalized `0x5E6DA0(dx, dy)` with 8 folded to 0. Skipped when
  the attacker coordinates are out of bounds (bombard passes -1).
* `0x5E6DA0(dx, dy)` classifier, `N0 NE1 E2 SE3 S4 SW5 W6 NW7`: with `ax = |dx|`, `ay = |dy|`,
  if `ax > ay` and `2ax > 3ay` the result is E (`dx >= 0`) or W; if `ax <= ay` and
  `2ay > 3ax` it is S (`dy >= 0`) or N (the binary returns 8, folded to 0); otherwise the
  diagonal `dx >= 0 ? (dy >= 0 ? SE : NE) : (dy >= 0 ? SW : NW)`. Neighbour deltas:
  N (0,-2), NE (1,-1), E (2,0), SE (1,1), S (0,2), SW (-1,1), W (-2,0), NW (-1,-1).
* **Which byte combat reads (verified).** Cell vtable slot `0x94` is `0x5EAA70:
  mov al, [ecx+4]`. The river bonus tests `(byte[cell+4] >> dir) & 1` with `dir` in 0..7 as
  indexed above, so `byte[cell+4]` is the river-edge set gameplay uses (an eight-direction
  index; the generator's last loop ORs `1 << (i & 0x1F)` into it for four neighbours,
  `NOTES.md` section 14.3). The sibling slot `0x98` (`0x5EAA80: mov al, [ecx+5]`), which
  `rivers.md` took for a river mask read by the renderer, is **the tile's owner civ id**
  (0 = unowned), see 9 and 14.3: so the two bytes are not two views of the rivers. How
  the bit order of `byte[cell+4]` maps onto the four iso edges (for example bits 1, 3, 5,
  7) is open.

### 4.2 Tile structures, `0x56CEB0(x, y, civ)` (verified)

```text
if the tile has a city (id = word[cell+0x1A] != 0xFFFF; lookup 0x437870 in pool 0xA52E68):
    resisters = 0x4BB2A0(city, -1)
    L = 0                                          if resisters > 0
    L = [0x9C72D8 + idx*4] + 0x4C10B0(city)        otherwise      idx = size > cityMax ? 2 : size > townMax ? 1 : 0
else (no city, also when the lookup is null):
    L = [0x9C726C]            if overlay bit 4  (slot 0x34)       Fortress
    L = 2 * [0x9C726C]        if overlay bit 28 (slot 0x38)       Barricade
    L = 0                     otherwise
if civ != -1 and 0 <= civ < 32 and (cell+0xD4 >> civ) & 1:  L += 25          radar (4.7)
```

A city tile never reads the overlay. Fortress is tested before barricade.
Defender-side values for `conquests.biq`: Town (size <= 6) 0, City (7..12) 50,
Metropolis (13+) 100, Fortress 50, Barricade 100. These match the Civilopedia
"Structural Bonuses" table, including its sentence "Cities with resisters do not
give defensive bonuses."

**Resisters** `0x4BB2A0(city, filter)`: counts entries of the list at `city+0xE0`
(count `[city+0xEC]`) whose byte `+0x20` is set and whose `+0x140 == filter`;
`-1` counts the resisting ones. Evidence: the `RESISTANCEENDS` text path at
`0x4AC11E`.

### 4.3 City building bonus, `0x4C10B0` (verified)

The **maximum** over the buildings present (not the sum) of `BLDG +0xA4` (body
`+0xA0`) times a multiplier:

* skip the building if `BLDG +0xE0 >= 0` and the owner knows that tech (obsolete);
* skip if the city is above town size (`size > townMax`, or above `cityMax`) and
  `BLDG +0x9C != 0`;
* multiplier 2 only when `BLDG +0x9C > 0` and the owner has a wonder with ability
  bit `0x1000` in the flag dword (body `+0xF4`); no row in `conquests.biq` has that
  bit, so the multiplier is 1 there.

`conquests.biq`: **Walls** (row 7, body `+0x98 = 8`, `+0xA0 = 50`) apply only up to
town size ("Town Walls +50%"); **Civil Defense** (row 75, `+0x98 = 0`, `+0xA0 = 50`)
applies at every size. Both present gives 50, not 100. **Coastal Fortress** (row 23)
stores its numbers one dword later (body `+0x9C = 8`, `+0xA4 = 50`). The data
for the other BLDG fields that combat reads are (mem offsets, body = mem - 4):

| mem | body | read by | meaning | `conquests.biq` |
|---|---|---|---|---|
| `+0x9C` | `+0x98` | `0x4C0C70`, `0x4C10B0`, `0x4C1320` | land bombardment defense; also exempts the building from the town-size rule of 4.3 | Walls 8 |
| `+0xA0` | `+0x9C` | `0x4C1000`, `0x4C1470` | naval bombardment defense | Coastal Fortress 8 |
| `+0xA4` | `+0xA0` | `0x4C10B0` | defender percentage | Walls 50, Civil Defense 50 |
| `+0xA8` | `+0xA4` | none of the combat code read | the `biq` crate's "naval defense bonus %"; no reader of the `0x4C10B0` shape was found (**open**) | Coastal Fortress 50 |
| `+0xC4` | `+0xC0` | `0x4C11E0` | SAM strength, summed (`air.md` 1.2); the `biq` crate calls it `num_buildings_required` | SAM Missile Battery 8 |
| `+0xC8` | `+0xC4` | `0x4C1280` | coastal-fortress strength, summed; the crate calls it `air_power` | Coastal Fortress 8 |
| `+0xD4` | `+0xD0` | `0x4ACB50` | required government | |
| `+0xE0` | `+0xDC` | all of the sums | obsolete-by tech (`-1` none) | |

(The older "`0x4C11E0` naval sibling" label was wrong: it is the SAM sum. The
naval number combat uses is the bombardment defense `0x4C1000`, section 8.6.)

### 4.4 Fortify

`[0x9C72F0]` (25) iff the PRTO kind (`+0x9C`) is 0 (land), the tile is not water
(vtable `0x8C`), `[unit+0x64] == 1` (fortified order) and the remaining movement
(`0x467CE0`) is positive. A defender that is loaded in a container (an army) is not
tested itself: `0x4A0F67` calls `0x5BCA90` (container-of) and, when it is non-null,
`0x42B5B0`, which follows `0x5BCA90` to the **outermost** container and applies the
same four tests to that unit (`maxMove - [unit+0x50]` negative or zero fails, anything
above zero passes, including values above 9999). So a fortified army fortifies its
members; a fortified unit inside an unfortified army gets nothing.

### 4.5 Barbarians and difficulty (verified)

Owner 0 is the barbarian civ. Only the **non-barbarian** side gets a term, and
it is the same shape on both sides:

```text
bonus(civ) = DIFF[difficulty(civ)].+0x64  +  (civ owns the barbarian-bonus wonder ? 100 : 0)
barbarian attacks B  ->  D += bonus(B.owner)          0x4A0FCF..0x4A1025
A attacks barbarian  ->  P += bonus(A.owner)          0x4A1063..0x4A10C1
```

`difficulty(civ)` is read at `0xA52EC8 + civ * 8420`; the DIFF table is
`[0x9C40C0]` (stride 124). `conquests.biq`: Chieftain 800, Warlord 400, Regent 200,
Monarch 100, Emperor 50, Demigod 25, Deity 0, Sid 0. The wonder test is
`0x55A8D0(civ, 4, 0)`: BLDG flag dword body `+0xF4` value 4, held only by **The
Great Wall** (row 36).

### 4.6 Amphibious attack, `+25` on `P` (verified)

All of: attacker has ability bit 6 (`0x5BC8B0(A, 6)`); `atkStr > 0`; if
`[A+0x48] & 4` then also ability bit 2 (blitz); PRTO kind is land; the
**defender's tile is not water**; the **attacker's tile is water**. The meaning of
status bit 2 (`[unit+0x48] & 4`) is **HYPOTHESIS** ("already attacked this turn").

### 4.7 Radar towers: `cell+0xD4` is a per-civ mask (verified)

`cell+0xD4` is a 32-bit mask with one bit per civ id. `Player::0x55AEF0(x, y)` sets
bit `player.id` (`[ecx+0x1C]`), `0x55AF40` clears it. The object constructor
`0x5DB850` (its only caller is `0x567384`, inside the Player creator
`0x567080(x, y, id)`: `new(0x30)`, ctor `0x5DB7D0`, FOURCC `"radt"`, pool `0xA52E24`)
sets the bit over the 25 offsets `spiralOffset(0..24)` (a 5x5 block, range 2,
wrap-aware); a sibling `0x55AE90` sets a second mask at `cell+0x60`. Creators:
`0x5B3866` (a Unit method: build at the unit's tile) and `0x5D258A` (logs "Placed
tower for %d"; its neighbour `0x5D255A` logs "Placed airfield"). The clear
routine `0x5DBB31..0x5DBC86` keeps the bit while another tower of that civ still
covers the tile.

The Civilopedia entry (`GCON_Radar_Towers`) agrees: range 2, "+25% attack and defense",
"can only be applied once, even if multiple towers overlap", "can only be used by
the player who created them", destroyed in foreign territory, available after Radio.
In code: the attacker tests **its own** civ bit on the **defender's** tile
(`0x4A8320`, adds to `P`); the defender tests **its own** civ bit on **its** tile
(tail of `0x56CEB0`, adds to `D`). Once per side, never stacked.

## 5. Strength, army, hit points

* **Attack** `0x5BE6E0`: army with carried members: `(Sum + n/2) / n` (idiv, truncating)
  over the members' strengths; otherwise PRTO `+0x60`.
* **Defense** `0x5BE820`: army with members: the same rounded average, returned as is;
  otherwise PRTO `+0x58`, and **halved by `sar 1` when `[unit+0x1ED] != 0` and the value is > 1**
  (`0x5BE953..0x5BE962`). The flag is what bombard sets (section 8).
  Members are the units on the same tile whose `+0x60` equals the army's unit id.
* **Army bonus** `0x5BCAE0(unit, defending)`: resolve the army (the unit if it
  has ability bit 18, else the unit `[unit+0x60]` points at); sum the members' strengths
  (`0x5BE820` if `defending` else `0x5BE6E0`) in a float; zero if there are no
  members; else `_ftol(sum * 0.16666667f)`, the constant `0x3E2AAAAB` at `0x66FB50`.
  The constant is just above 1/6, so the result is exactly `Sum / 6` (floor) for any
  realistic sum.
* **Maximum HP** `0x5BE5B0`: `max(1, B + PRTO.+0xA4)` where `B` is the sum of the
  members' own maximum HP for an army with members, else `EXPR[level].+0x24` with
  `level = [unit+0x44]`. `[unit+0x44]` is the experience-level index, **not shields**.
* **Damage** is `[unit+0x4C]`; remaining HP is `maxHP - damage`.

`conquests.biq` EXPR (stride 44, table `[0x9C40CC]`; body `+0x20` / `+0x24`):

| level | base HP | retreat % |
|---|---|---|
| Conscript | 2 | 34 |
| Regular | 3 | 50 |
| Veteran | 4 | 58 |
| Elite | 5 | 66 |

## 6. The duel loop, `0x4A53A0`

Per round (verified; the function is 2 235 disassembly lines, mostly bookkeeping,
animation and logging around this core):

```text
roll = next(1024) & 0xFFFF                                   0x4A5B3C
if roll >= odds:            # setge at 0x4A5B56
    defender.damage = max(0, defender.damage + 1)            0x4A5BA4..0x4A5BB5
    if maxHP(defender) - damage <= 0: attacker wins          0x4A5C44..0x4A5C59
    else defender-retreat check (section 7)
else:
    attacker.damage = max(0, attacker.damage + 1)            0x4A66F9
    if maxHP(attacker) - damage <= 0: defender wins
    else attacker-retreat check
```

* One hit point per round, no damage scaling by strength or level. **Strength acts
  only through the odds.**
* The death test is `remaining <= 0`. The `cmp eax,0x270F` branches that look like an
  "invulnerable above 9999" case are redundant: both arms go to the same target. This
  corrects an earlier carried-over claim.
* The odds are recomputed only when the participants change (army front unit via
  `0x5BCA90`, `[army+0x1D0]`, `0x4A4B30`), so a plain duel uses one value for all rounds.
  The reselection logic is **open**.
* The attacker-round-win hook `0x4A5BBE..0x4A5C37` calls `0x4A2550(this, A, D)` when
  the attacker has bombard strength > 0 (`PRTO +0x48`) and `PRTO +0xAC & 0x10080000`
  is non-zero. `PRTO +0xAC` is the special-actions word, and the mask is its **bit 19,
  Collateral Damage** (bit 28 is unused); **no unit in `conquests.biq` has it, so the
  hook is dormant**. A unit with positive attack strength always fires it; one with
  none needs a city on the target tile (`0x4A5C18`). The strike itself is section 8.5
  (a city: one facility strike, mode 1) and 8.8 (an empty tile).
* Every terminal path sets the done flag `[esp+0x11]` (`0x4A717E`).

### 6.1 Choosing the defender (`0x4A47C0`, verified)

The step object picks the defender among the units on the target tile, with the
legality filter `0x4A1590` and one of two loops (`rust/src/combat.rs`
`defender_eligibility`, `defender_rating`, `pick_best_defender`,
`pick_weakest_defender`, `is_better_defender`):

* **Filter** `0x4A1590..0x4A1739`: a loaded unit never defends on its own (`0x5BCA90`
  non-null); a **sea** unit needs a water tile, a **land** unit a land tile, and an
  **air unit is never eligible** (`0x4A1664`: aircraft are fought only through
  interception, `air.md`). A unit with defense 0 is eligible whatever its HP,
  otherwise it needs HP left; defense above 0 with HP at or below 0 is a "zombie":
  the filter queues a kill event (`0x474140`) and reports it ineligible.
* **Rating** `0x4A1823..0x4A186B`: `(100 + fortify) * defense * clamp(remaining, 0, 9999) / 100`, where
  `fortify` is `[0x9C72F0]` when `0x42B5B0` grants it (4.4) and `defense` is `0x5BE820`.
* **Strongest** `0x4A1740` starts at 0 and takes the **highest** rating; **weakest**
  `0x4A1910` starts at 1000, skips zero ratings and takes the lowest. Comparators
  `0x4A12D0` (strongest) and `0x4A1430` (weakest, the mirror): a **King** (ability 29)
  is the worst defender to pick against a real incumbent (and the one the weakest
  search prefers); equal positive ratings are broken by cargo count (an Army counts
  0, otherwise `0x5BE9E0`), attack strength, bombard strength and maximum HP, the
  strongest search keeping the **smaller** of each, the weakest the larger; equal on
  all four keeps the incumbent. The other callers of `0x4A12D0`/`0x4A1430`
  (`0x44AD06`, `0x44EEAA`, `0x4E416E..0x4E41B4`, `0x5BCE25`, `0x5BD13C`, `0x5BD170`)
  are unread.

### 6.2 After a victory (`0x5BEF00(winner, loser, flag)`, verified)

Run for the winning unit when a fight ends in a kill (`flag` is 1 when the attacker
won, 0 when the defender did; callers include the ranged-attack kill path of
`0x4A1FA0` and the interception report `0x4A46A0`). Draws, in order:

1. **Golden Age** (`0x5BEF93..0x5BF005`): the winner (or the prototype of the Army it
   belongs to, `0x5BC6D0`) has ability 15 (the unique-unit Golden-Age ability), the loser
   is not a barbarian and the civ has none scheduled (`Player +0x3C == -1`):
   `0x55C8C0` starts one, lasting RULE `[0x9C7308]` (body `+0x140`) turns. No dice.
2. **Promotion** (`0x5BF01B..0x5BF0D1`): for a winner at experience level 0, 1, 2 the die is
   `[2, 4, 8][level]`, **doubled** when the loser is a barbarian, **halved** when the
   winner's civ is Militaristic (trait bit 0 of `RACE +0x948`, verified, `capture.md` section 11).
   The unit promotes when `next(die) == 0`;
   a unit whose status bit 2 is set (it failed earlier this turn) is promoted without
   a die, and a failed roll sets that bit. Level 3 does not roll for promotion.
3. **Great Leader** (`0x5BF4DF..0x5BF5A5`): an elite (level 3+) land winner, not in an
   army, whose loser is not a barbarian, which has not made a leader yet (status bit
   `0x20`), in a civ with no Leader unit (`[0x9C728C]`): `next(die) == 0` with die 16
   (12 for the owner of the Heroic Epic, BLDG flag `+0xF4` bit 0), **doubled when the
   defender won**.
4. **Enslave** (`0x5BFA05..0x5BFA15`): a winner with the Enslave special action (token
   bit 18) whose prototype names a result unit: `next(100) < 33` converts the loser.

`0x5BBBC0` (kill) is called afterwards by the callers; its cargo recursion is
specified below. Other destruction bookkeeping remains open.

### 6.3 Death does not wipe an ordinary defending stack (verified)

The melee victory paths call `0x5BBBC0` on the defeated fighter only:
`0x4A63BE..0x4A63EF` loads the defender from `[combat+4]`, while
`0x4A6EC1..0x4A6EF2` loads the attacker from `[combat+0]`. In network mode
each instead queues that unit's id through `0x474140`.

Inside the kill routine, `0x5BBFA5..0x5BC0AB` scans the unit pool. It clears
references at `Unit +0x1C4` that match the dead unit's id
(`0x5BBFE9..0x5BBFFB`). The recursive destruction/detachment branch is gated
by **`other.+0x60 == dead.id`** (`0x5BC000..0x5BC009`), not by matching tile
coordinates. `+0x60` is the carrier id (`unit-turn.md`, unit layout). Thus
ordinary soldiers, civilians and artillery on the same tile survive.

For linked cargo, the branch tests the dead unit's Army ability 18
(`0x5BC00F..0x5BC01A`), its domain at `PRTO +0x9C`, a caller flag for air
carriers (`0x5BC034..0x5BC03F`), and whether a sea carrier is in a city
(`0x5BC041..0x5BC077`). It either detaches cargo through `0x5C59B0(-1,-1)`
or recursively kills it at `0x5BC09D`. The complete transport policy and
remaining kill bookkeeping are not specified here. This finding establishes
the negative: there is no whole-tile casualty rule for ordinary melee stacks.

Game regressions cover a surviving soldier, Worker and supporting Catapult,
and a later move capturing the Worker after its defending fighter dies.

## 7. Retreat

**Eligibility, `0x4A47C0`** (flags `[this+9]` attacker, `[this+8]` defender):

```text
[this+9] = maxMove(A) > [0x9C72C8]        # more than one full move (3 units)   0x5BE470
[this+8] = maxMove(D) > [0x9C72C8]
if both: clear both
if [this+8] and the defender's tile has a city (0x5EA6C0): clear [this+8]
```

**Defender retreat**, evaluated only after an *attacker* round win (block `0x4A647B`),
each condition failing straight back to the loop:

1. `[this+8]` set;
2. `D.owner != 0` (barbarians never retreat);
3. `remaining(D) == 1` exactly;
4. `remaining(A) > 1`;
5. roll `next(R_A + 50) < R_D`, with `R = EXPR[level].+0x28` (the retreat %), at `0x4A650A`;
6. `0x5BFB60(D, [this+0xC])` must succeed in moving the defender; if it fails the
   fight continues; if it succeeds the duel ends with **nobody dead**.

**Attacker retreat** (block `0x4A6F84`) is the mirror image: after a *defender*
round win, `[this+9]`, `A.owner != 0`, `remaining(A) == 1`, `remaining(D) > 1`,
roll `next(R_D + 50) < R_A` (at `0x4A7013`). No move is attempted; the duel just ends.

Consequences: each side gets **at most one** retreat roll per fight, at the moment
it reaches exactly 1 HP. A Regular unit against a Regular unit passes
`next(100) < 50` half the time; an Elite defender against a Conscript attacker passes
`next(84) < 66`, about 79%. Only fast units can retreat, and if both are fast neither can.

## 8. Ranged attacks, city strikes, defensive bombard

Every shipped bombard-type unit (Catapult, Cannon, Artillery, ships, bombers) goes
through the **ranged attack** `0x4A3A70`, which writes real damage. The older notes
described the legacy routine `0x4A3320` (flag-setting bombard) as if it were the
shipped path; it is not (section 8.10).

### 8.1 Entry chain (verified)

```text
Unit::attackAt(x, y)                 0x5C1410   16 callers (UI, AI, network)
  0x5C0E20  validate the order
  0x5C1210  dispatch
  special-action word bit 17 (0x10020000, "charm"): legacy 0x4A3320     0x5C1464 (dormant, 8.10)
  otherwise 0x4A3A70                 the ranged attack  (callers 0x5C1481, and 0x478AFF in the network handler)
```

Cruise missiles use their own routine `0x4A2B40` (callers `0x470291`, `0x5B4D40`, 8.7).
The network handler (`0x478AFF`) enters `0x4A3A70` as well.

### 8.2 The ranged attack `0x4A3A70` (verified)

```text
tile = target; city = cityAt(tile)                                    0x56D2C0
if city:
    wall roll by ATTACKER domain (8.6):
        land:  defense = 0x4C0C70(city)   facility = 0x4C1320
        sea:   defense = 0x4C1000(city)   facility = 0x4C1470
        air:   no wall roll
    skipped -> the units are attacked at once; hit -> one building destroyed, FACDESTROYED,
    the attack goes on only if the facility was "reported" (a player-wide building); missed -> the attack ENDS
air attacker over a city: mode = next(k), k = 4, +1 above 4 targets, +1 more above 8 (counted by 0x4A36C0);
    mode 0 or 1 -> city strike 0x4A2650(attacker, city, mode, 0) (8.5); a strike that does not happen
    falls through to the units
units path 0x4A1FA0: choose the target (8.3), then the shot loop (8.4)
no unit to shoot at in a city: mode = next(2) -> city strike 0x4A2650(attacker, city, mode, 0)
```

The chance that an aircraft hits the city rather than its garrison is `2 / k`:
50 % (up to 4 units), 40 % (5..8), 33 % (more than 8).

### 8.3 Choosing the target, `0x4A1FA0` (verified)

The classes of target are scanned in this order (list built at `0x4A1FA6..0x4A20A7`;
`0x4A36C0` counts the same lists over all classes without stopping):

* the **AI cruise-missile flag** (AI word `PRTO +0x8C` bit 5, `shr eax,5` at `0x4A203C`)
  or a **sea** attacker: sea, air, land;
* an **air** attacker: air, sea, land;
* any other **land** attacker: sea units if the target tile is water, land units if
  not, and nothing else.

A candidate must (`0x4A2113..0x4A22F8`): not be loaded in another unit; have defense
above 0 (a Worker is never shot at); not be at or below 1 HP unless the attacker is
lethal against its domain (8.4); be hostile to the attacker (`0x5BB650`); and stand
on a land tile if it is a land or air unit, on a water tile or in a city if it is a
sea unit, in a city if it is an aircraft. The first class with a legal candidate
supplies the target, chosen with the strongest-defender comparator `0x4A12D0`
(6.1; called with ties-need-positive at `0x4A23DB`). `0x4A1FA0` is also the target
selector the AI calls (`0x44C5D8`, `0x44CD5C`).

### 8.4 The shot loop (verified)

`odds = 0x4A0ED0(attacker, target, 1, 0)` (bombard mode: attacker strength is
`PRTO +0x48`; the terrain and tile terms are evaluated without the attacker's position
and civ, so there is no river term and no defender radar, 3.4). A **sea unit inside a
city** is harder to hit by the ranged attack (not by the missile):
`odds = (odds + 1) / 2` truncated (`0x4A3FF2..0x4A4006`, a float multiply by 0.5
and `_ftol`). Then up to `rate_of_fire` (`PRTO +0x6C`) shots, each `next(1024)`:

* a die **below** `odds` is a miss; otherwise the target takes **1 damage**;
* after a hit, remaining HP **exactly 1** ends the volley unless the attacker is
  lethal against the target's domain (ability 27 Lethal Land Bombardment for land
  targets, 28 Lethal Sea Bombardment for sea targets, `0x4A40E8` / `0x4A40DF`;
  aircraft have no floor); a target left at exactly 1 HP also books a weight-1
  incident against its civ (`0x5631B0`, `0x4A40DA`);
* remaining HP of zero or less **kills**: `0x5BEF00(attacker, target, 1)` (6.2)
  then `0x5BBBC0`;
* shots are never skipped for an earlier hit, so a volley draws `rate_of_fire` dice
  unless it ends early.

So a bombard unit **never kills** a land or sea unit unless it has ability 27 / 28,
and that is how ordinary artillery softens a stack. Shipped ability words (PRTO `+0x88`,
read from `conquests.biq`): the **Bomber**, Stealth Bomber, F-15, Stealth Fighter,
Cruise Missile and Hwach'a have bits 27 **and** 28 (lethal against both); the **Fighter**
and Jet Fighter have bit 28 only (they can sink ships, but leave land units at 1 HP);
Catapult, Cannon, Artillery, Radar Artillery and every ship have neither. Each shot
is `P(hit) = (1024 - odds) / 1024`, the same per-round chance as in a duel. Sample
bombard values: Catapult 4 (rate of fire 1, range 1), Cannon 8 (1, 1), Artillery 12
(2, 2), Radar Artillery 16 (3, 2), Destroyer 6 (2, 1), Battleship 8 (2, 2), Bomber
12 (3), Stealth Bomber 18 (3), Cruise Missile 16 (3, 4).

### 8.5 City strike `0x4A2650(attacker, city, mode, rolls)`, `ret 0x10` (verified)

`rolls = max(PRTO[attacker].+0x6C, rolls)`; any `mode` other than 0 or 1 returns false
without drawing (`0x4A2684..0x4A2693`).

```text
mode 0  population      needs city.+0x138 (size) > 1             base = [0x9C7298] (16)
mode 1  facility        destroys one random non-wonder building  base = [0x9C7294] (16)
v    = ((terrain + tile + 100) * base) / 100       terrain = 0x56CD80(-1,-1,x,y), tile = 0x56CEB0(x,y,-1)
odds = clamp(1024 * v / (v + PRTO[attacker].+0x48), 1, 1023)
up to `rolls` dice next(1024); the FIRST die >= odds succeeds (no further dice)
success, mode 0: incident tally 0x5631B0, citizen killed 0x4BA230, text POPDESTROYED
success, mode 1: 0x4C1590 picks and removes a building (0x4ACF40), text FACDESTROYED
failure: BOMBFAILED / THEIRBOMBFAILED (air) or BOMBARDFAILED (artillery)
```

An empty city is therefore struck at implicit strength 16 plus its tile terms: with
Walls on hills (50 + 50 + 100 = 200 %) `v = 32`, and a Bomber (bombard 12, rate of fire 3)
needs a die of at least `1024 * 32 / 44 = 744` on any of its three dice: 27 % each, 62 % per
run.

`0x4A2550(this, A, D)` (the collateral hook of 6, `ret 8`) forwards a city on the
target tile to `0x4A2650(A, city, 1, 1)`. Without a city, if `0x5B3AB0(A, x, y)`
holds, it strikes the tile itself (8.8).

### 8.6 Walls and ports (verified)

* **Land bombard defense** `0x4C0C70(city)`: 0 above town size (`pop > [0x9C72E8]`
  or `pop > [0x9C72E4]`); else the **largest** BLDG `+0x9C` over the buildings that
  act on the city (`0x4ACB50(city, i, 1)`) and are not obsolete, times
  `wonders + 1` (`0x55A8D0(owner, 0x1000, 0)`, none in `conquests.biq`). Walls: 8.
* **Naval bombard defense** `0x4C1000(city)`: the largest BLDG `+0xA0`, any city size,
  no multiplier. Coastal Fortress: 8.
* **Wall roll** (ranged attack on a city by a land or sea unit): `v = ((terrain + tile + 100)
  * defense) / 100`; `v <= 0` skips the roll; else `odds = clamp(1024 * v / (v + bombard),
  1, 1023)` and up to `rate_of_fire` dice stop at the first success. Success removes
  one building (`0x4C1320` land: pass 0 over the buildings the city holds, the strictly
  largest defense, first of equals; pass 1 over buildings that merely act on it only
  *reports*; `0x4C1470` naval: strictly positive, in the city only). A failed roll ends
  the whole attack and the garrison is not touched.
* The "Cities with resisters do not give defensive bonuses" rule is inside `0x56CEB0`
  (4.2), so a city in disorder is struck at `v = 16 * (terrain + 100) / 100`.

### 8.7 Cruise missile `0x4A2B40` (verified)

The same shot loop with the 1-HP floor removed, without port halving, and
`odds = 0x4A0ED0(A, target, 1, 0)` at `0x4A2BE6`. It sets the missile's order to 0 at
`0x4A2BD9` and consumes it. The missile is a land-domain unit with the AI flag
(`PRTO +0x8C` bit 5), so its target classes are sea, air, land (8.3).

### 8.8 Strike on an empty tile `0x4A2550` (verified; effect in `colonies.md` 8)

If `0x5B3AB0(A, x, y)` holds (verified: the unit either has not attacked yet this turn or has
Blitz, status bit 2 and ability 2; the tile is neither a city (`0x5EA6C0`) nor flagged by
`0x5EA6E0`; and the tile's overlay word, cell vtable slot `0xA8`, has any of bits 0 to 4
or 28 set, mask `0x1000001F`, which includes Fortress (4) and Barricade (28)):

```text
v    = ((terrain + tile + 100) << 4) / 100          terrain = 0x56CD80(-1,-1,x,y), tile = 0x56CEB0(x,y,-1)
odds = clamp(1024 * v / (v + PRTO[A].+0x48), 1, 1023)
roll = next(1024); if roll >= odds: 0x5B4DC0(A, 0, x, y, A.owner)
```

So a tile defends with an implicit strength of **16** scaled by its terrain and
structure percentages (`0x51EB851F` is the signed divide by 100). The precondition says
the target is a tile that carries a destroyable improvement. The prioritised
destruction in `0x5B4DC0` is verified in `colonies.md` 8: Barricade degrades to
Fortress first, Railroad is removed next, otherwise roads, mines and irrigation
are cleared together before the structure-removal fallback.

### 8.9 Defensive bombard, `0x4A1AE0` / `0x4A3280` (verified)

Once per duel, **before the first round** (`0x4A5708` in `0x4A53A0`, also `0x4A7330` in
the AI estimator), a unit on the **defender's** tile may shoot the **attacker**:

* `0x4A1AE0(A, B)` (victim = the attacker): the victim needs defense above 0 and, if it
  is a land or sea unit, more than 1 HP (so the shot never kills; aircraft have no
  floor). The shooter is another unit on the defender's tile (not the defender, not
  loaded in it) of the **same domain as the victim**, with bombard strength > 0,
  without ability 3, and whose status bit `0x40` ("already fired") is clear; the
  strictly highest bombard strength wins and the first of equals stays.
* `0x4A3280`: **one** die against `odds = 0x4A0ED0(shooter, victim, 1, 1)` (raw mode:
  no terrain terms). A die at or above the odds adds one damage; a miss changes
  nothing. The shooter's status bit `0x40` is set either way (`0x4A3309`). A hit that
  leaves the victim at exactly 1 HP books a weight-1 incident from the shooter's civ
  against the victim's civ (`0x5631B0`, `0x4A32E0`).
* In multiplayer the shot goes through the network (`0x4748C0`, replayed by `0x4788ED`).
* The start of every duel (`0x4A569F`) sets the **attacker's status bit 2**
  (`[+0x48] |= 4`: "has attacked this turn", the meaning used by the amphibious test,
  4.6) and then wakes the defender: `0x4A56F1` calls `Unit::setOrder(0)` on it unless
  its order is 1 (fortified) or 15 (interception).

### 8.10 Legacy ranged attack `0x4A3320` (dormant)

`Unit::attackAt` calls it (`0x5C146E`) for a unit whose special-action word has bit 17
(`0x10020000`, the "charm" bit). **No unit in `conquests.biq` has it.** Its unit pass
(`0x4A3440..0x4A3661`, formerly documented here as the bombard) draws exactly
`rate_of_fire` dice at `0x4A0ED0(A, u, 1, 0)` with no early exit and, if any die is at
or above the odds, sets `[u+0x1ED] = 1`, which halves that unit's defense (section 5)
until cleared. It writes no damage. The earlier statement that the shipped bombard
"writes no damage" was therefore wrong: it applied to this dormant path.

### 8.11 The interception duel `0x4A4520`

The second, simpler duel loop is the **air-interception fight**: `odds =
0x4A0ED0(interceptor, aircraft, 0, 1)` (raw mode), one `next(1024)` per round, the
same damage and death rule, **no retreat logic**; it returns at once when the
interceptor's attack and the aircraft's defense are both zero. Callers: `0x5C719E`
(inside the air-defense routine `0x5C68A0`) and `0x478A35` (the network replay). It is
described, with the SAM and flak layers around it, in `air.md`.

## 9. Where the numbers live

RULE object `0x9C71E4` (scenario `0x9C3508 + 0x3CDC`); the "body" column is the
offset in the decoded RULE record of `conquests.biq` (720 bytes), and every value
below was read from that record and matches the global.

**Address to body offset.** The reader `0x5E78E0` (called from `0x5963A0` with
`this = 0x9C71E4`) fills the object from the stream in this order: `this+4` the record
length, `this+8` 96 bytes of names, `this+0xC4` a count `N` followed by `this+0x68 =
new int[N]` read from the stream, then the dwords `this+0x6C..0xC0`, then (the dword at
`this+0xC4` is the count already read and is skipped) `0xC8..0xF0`, `0xF4` (12 bytes),
`0x100` (12 bytes), `0x10C`, `0x118`, `0x114`, `0x11C..0x130`. With `obj = global -
0x9C71E4` the record offset is `body = obj + 0x20` for `obj <= 0xC0` and `obj + 0x1C`
for `obj >= 0xC8` (the skipped dword accounts for the 4-byte difference).

| Rust field | global | body offset | `conquests.biq` | meaning |
|---|---|---|---|---|
| `size_bonus_pct[0..3]` | `0x9C72D8..0x9C72E0` | `+0x110..+0x118` | 0 / 50 / 100 | Town / City / Metropolis |
| `town_max` | `0x9C72E4` | `+0x11C` | 6 | largest town (the size classes of `0x427540`, the Aqueduct gate: `economy.md`) |
| `city_max` | `0x9C72E8` | `+0x120` | 12 | largest city (the Hospital gate: `economy.md`) |
| `fortify_pct` | `0x9C72F0` | `+0x128` | 25 | fortified units |
| `fort_pct` | `0x9C726C` | `+0xA8` | 50 | fortress (barricade is twice) |
| `river_pct` | `0x9C72B8` | `+0xF0` | 25 | river crossing |
| `move_unit` | `0x9C72C8` | `+0x100` | 3 | movement units per full move |
| `city_strike_base[1]` | `0x9C7294` | `+0xD0` | 16 | implicit strength of a city facility strike (8.5) |
| `city_strike_base[0]` | `0x9C7298` | `+0xD4` | 16 | implicit strength of a population strike (8.5) |
| `air_intercept_pct` | `0x9C72A0` | `+0xDC` | 50 | chance each defender engages an ordinary aircraft (`air.md`) |
| `stealth_intercept_pct` | `0x9C72A4` | `+0xE0` | 5 | the same for a Stealth aircraft (`air.md`) |
| (array count) | `0x9C72A8` | `+0x60` | 10 | the count `N` of the int array behind it: the ten dwords at body `+0x64..+0x8B` (all 1 in the shipped file) are read into `new int[N]`, the pointer `[0x9C724C]`; indexed by the spaceship part, it is how many of that part one player may build (`buildable.md`) |
| food per citizen | `0x9C72B4` | `+0xEC` | 2 | the multiplier in food eaten `(size - resisters) * 2` (`0x4B05A4`, `0x4B1155`) and the fixed food of a city centre tile (`0x5D74AE`): `yields.md` sections 4.1 and 5.3 |
| shield cost per gold | `0x9C7268` | `+0xA4` | 4 | Wealth divisor (`0x4B0AFE`, halved with Economics) and the sale price of an improvement (`0x4B32F0`): `yields.md` section 5.6, `economy.md` |
| golden age length | `0x9C7308` | `+0x140` | | turns of a golden age (`0x55C8C0`, 6.2) |

Unresolved RULE words: `[0x9C7260]`, `[0x9C72F8]`, `[0x9C7300]`, `[0x9C7304]`,
`[0x9C730C..0x9C7318]`. The `biq` crate's `golden_age_duration` (body `+0xD8`) and
`intercept_stealth_missions_pct` (`+0xDC`) disagree with this table: the exe reads
`+0xDC` (50) as the ordinary-aircraft chance and `+0xE0` (5) as the stealth chance, and
the golden-age length at `+0x140`.

Other tables: DIFF `[0x9C40C0]` stride 124 (`+0x64` mem, body `+0x60`); EXPR
`[0x9C40CC]` stride 44; TERR defense mem `+0x58` (body `+0x54`); BLDG stride `0x110`,
count `[0x9C3D80]`, table `[0x9C40AC]` (fields in 4.3); PRTO row `[0x9C71E0] + type * 0x138`.
For BLDG and PRTO **mem = body + 4** (the loader inserts one leading dword).

**Unit record** (`+0x20` id; object = pool node - 0x1C, pool `[0xA52E84]`, max id
`[0xA52E90]`):

| offset | field |
|---|---|
| `+0x20` | unit id |
| `+0x24` / `+0x28` | x / y |
| `+0x34` | owner civ (0 = barbarian) |
| `+0x40` | PRTO type |
| `+0x44` | experience-level index (0 Conscript .. 3 Elite) |
| `+0x48` | status bits: `0x04` already attacked this turn (set at `0x4A569F`, verified), `0x20` has produced a Great Leader, `0x40` already fired a defensive bombard; the rest open |
| `+0x4C` | damage taken |
| `+0x50` | movement used, in units of `[0x9C72C8]` (3 per full move) |
| `+0x60` | id of the unit carrying this one (army or transport), `-1` none |
| `+0x64` | order: 0 none, 1 fortified, 15 interception; 16, 34 and others are set by `Unit::setOrder` callers (`air.md` 2, open) |
| `+0x1DC` | display flag set on the defensive-bombard shooter |
| `+0x1ED` | defense-halved flag (legacy bombard, 8.10) |

**Cell record** (the map cell for tile `(x, y)` is `0x5D16A0(((width >> 1) * y + (x >> 1)) & 0xFFFF)`
on the map object `0x9C736C`; `[0x9C74D4]` is the map width in cells; all slots are `thiscall`
virtual methods):

| vtable slot | what | evidence |
|---|---|---|
| `0x34`, `0x38` | overlay bit 4 (Fortress), overlay bit 28 (Barricade) | `0x56CEB0` (4.2) |
| `0x78` | overlay bit 29 (alternate terrain defense, open) | `0x56CD80` (4.1) |
| `0x8C` | is the tile water | 4.4, 8.3 |
| `0x94` | `byte[cell+4]`: river-edge bit set, eight directions | `0x56CD80` (4.1) |
| `0x98` | **`byte[cell+5]`: owner civ id of the tile (0 = none)**; setter slot `0x108` = `0x5EAD20` (callers include `0x596DBB`, the cell reset, which stores 0, and `0x5D3AFB`) | 14.3 |
| `0xA0` | handle of the tile's unit list | 8.3 |
| `0xA8` | overlay word: bits 0 to 4 and 28 are the improvements a strike can destroy (mask `0x1000001F`) | `0x5B3AB0` (8.8) |
| `0xB8` | continent id (word) | `0x563410` (14.1) |
| `+0x1A` (word) | city id, `0xFFFF` none | 4.2 |
| `+0xD4` (dword) | per-civ radar bit mask | 4.7 |

**PRTO row** (mem offsets; body = mem - 4): attack `+0x60`, defense `+0x58`, bombard
strength `+0x48`, bombard range `+0x4C`, rate of fire `+0x6C`, movement `+0x70`,
operational (air) range `+0x64`, domain `+0x9C` (0 land, 1 sea, 2 air), hit-point bonus
`+0xA4`, ability word `+0x88`, AI word `+0x8C` (bit 5 = cruise missile), standard
orders `+0xA8`, special actions `+0xAC`, worker actions `+0xB0`, air missions `+0xB4`,
and, in long records only, `+0x128` and `+0x130` (no reader found), `+0x12C` (float, read
in the round loop at `0x4A5DC0` / `0x4A6896` and at `0x5B3499`) and `+0x134` (flak
strength, read only by `0x5C68A0`, `air.md`). Shipped record lengths in `conquests.biq`: 255 bytes (134 rows), 327 (3), 619
(3), 623 (1); only the long ones carry the tail fields and the legal-terrain and
stealth-target lists. The `biq` crate's offsets for bombard strength / range, rate of
fire, movement and domain disagree with the shipped data (true body offsets:
`0x44`, `0x48`, `0x68`, `0x6C`, `0x98`); its `upgrade_to` (mem `+0xF4`) is the
"enslave results in" unit.

**PRTO ability bits** (`Unit::hasAbility(n)` = `0x5BC8B0`, PRTO form `0x5E4EF0`; word `+0x88`).
Those the combat code reads: 2 Blitz (4.6), 3 Cruise Missile (such a unit never
fires a defensive bombard, 8.9), 6 Amphibious (4.6), 15 unique-unit Golden Age (6.2), 17
**Hidden Nationality** (never provokes a war declaration, 14.3; the `biq` crate's
`1 << 0x13` is wrong), 18 Army, 21 Stealth (`air.md`), 27 Lethal Land Bombardment,
28 Lethal Sea Bombardment (8.4), 29 King (6.1). Seen in shipped data but not read by
combat: 0 Wheeled, 1 Zone of Control, 4 all-terrain-as-road, 5 Radar, 7
Invisible, 8 Carrier, 9 Draft, 10 (set on every air unit; unresolved, the crate calls it
"Immobile"), 16 Nuke, 19 Leader, 20 ICBM, 22 sees submarines, 23 tactical missile, 24 carries
tactical missiles, 25 ranged-attack animation, 26 turn to attack, 30.

**Action tokens** `(wordIndex << 28) | mask`, tested by `Unit::canDoAction` `0x5C1AD0`
against PRTO `+0xA8 + 4 * wordIndex` (it also requires movement left > 0). Word 1 is
the special-actions word (`editor.md` has the full bit list); combat reads its bit 17
(charm, the legacy ranged attack, 8.10), bit 18 (Enslave, 6.2) and bit 19 (Collateral
Damage, the hook of section 6). Word 3 is the air-missions word: bit 0 Bombing, 1
Recon, 2 Interception, 3 Re-base, 4 Precision Bombing (`air.md`).

## 9.1 Verified against the shipped text

`Text/Civilopedia.txt` and `script.txt` agree with the decoded numbers: Coastal Fortress
"naval bombardment defense of eight" (BLDG naval `+0xA0 = 8`), SAM "attack enemy air units
that attempt to attack the SAM site's city" (`air.md`), Stealth Bomber "very difficult to
intercept" (the 5 % word).

## 10. Worked example

Attack 8 against defense 4, the defender fortified (25) on hills (50) in a size-5
town (base 0) with Walls (50):

```text
D = 50 + 0 + 50 + 25 = 125        X = 4 * 225 = 900        Y = 8 * 100 = 800
odds = 1024 * 900 / 1700 = 542     the attacker wins a round on 482 of 1024 dice (47.1%)
```

This is the test `worked_example_from_combat_md` in `rust/src/combat.rs`.

## 11. Corrections to earlier documents

| document | earlier claim | now |
|---|---|---|
| `ai.md`, `README.md`, `rng.rs` | combat randomness is MSVC `rand()`; AI/combat and map randomness are different algorithms | combat uses the `Random` class through the global instance `0xA526B4`; MSVC `rand()` rolls no combat die; the map generator uses separate instances of the same class (section 1) |
| `ai.md` | the first draw is saved to `[0xA526B4]` | `[0xA526B4] = timeGetTime()` at start-up (`0x56C1F2`) |
| `ai.md` | "All randomness flows through `0x64A20E`" | false (section 1) |
| `ai.md` | unit `+0x44` is shields, `+0x4C` a stack-summed accumulator | `+0x44` is the experience-level index; `+0x4C` is damage taken, pooled additively when stacks merge |
| `ai.md` | combat odds math is open | resolved here |
| earlier notes | a remaining value above 9999 is "invulnerable" | the `0x270F` compares are redundant (section 6) |
| earlier notes | the hook mask `PRTO +0xAC & 0x10080000` is unknown | token `0x10080000` is word 1 bit 19, Collateral Damage; no shipped unit has it (section 6) |
| earlier notes | `0x56D2C0(x, y)` returns a flag | it returns the **City** standing on the tile (0 if none) |
| earlier notes | the promotion roll is at `0x5C6968` | the roll is in `0x5BEF00` (6.2); `0x5C6968` sits inside the air-defense routine `0x5C68A0` |
| earlier notes | `0x4A3440` is the bombard routine | it is the middle of the legacy ranged attack `0x4A3320` (8.10); the shipped bombard is `0x4A3A70` (8.2) |
| earlier notes | "bombard writes no damage, it only sets a flag" | true only of the dormant legacy routine; the shipped bombard writes one point of damage per hit (8.4) |
| earlier notes | `0x4C11E0` is the naval sibling of the building bonus | it sums the SAM strength (`air.md` 1.2); the naval number combat uses is `0x4C1000` (8.6) |
| `ai.md`, `rivers.md` | `0xA53BC8` is an overlay gate | it is the base of every player's **at-war byte table** (`Player +0xD30 + civ`), read by `0x558F70`, `0x5B5790` and written by `0x501F20` (14.3) |
| earlier notes | status bit 2 of a unit (`+0x48 & 4`) may mean "already attacked" | verified: every duel sets it on the attacker at `0x4A569F` (8.9) |
| `air.md` | the `PRTO +0x134` word may be a general stat | its only reader is the flak test (`air.md` 1) |
| `rivers.md`, `NOTES.md`, `rust/src/rivers.rs` | `byte[cell+5]` (slot `0x98`, `0x5EAA80`) is a river mask and the renderer gate `0xA53BC8` indexes it | `byte[cell+5]` is the tile's **owner civ**; `0xA53BC8` is the at-war table, so the renderer overlay shows where the viewer is at war with the tile's owner (14.3). The river data in gameplay is `byte[cell+4]` (4.1) |

## 12. Open

* `0x4A4B30` and `[army+0x1D0]`: choosing the participant a stack or army puts forward
  (the odds are recomputed when it changes, section 6).
* `0x5BBBC0` (kill a unit): remaining bookkeeping and complete cargo policy;
  ordinary defending-stack survival is verified in 6.3.
* `0x5BFB60` (move a retreating defender): internals.
* `0x4A7280`: the AI estimator of a fight (calls `0x4A0ED0` twice); a dynamic program of
  its own, not decoded.
* `0x5B4D40`; the meaning of the special tile flag checked by `0x5B3AB0`.
  Improvement destruction in `0x5B4DC0` is specified in `colonies.md` 8.
* `TERR` mem `+0xA4` and overlay bit 29 (slot `0x78`): an alternate terrain defense value.
* The Pikeman's bonus against mounted units (manual): no term for it was found in
  `0x4A0ED0` or the strength getters; its mechanism is not located.
* BLDG `+0xA8` (mem; Coastal Fortress 50) has no reader of the `0x4C10B0` shape, and the
  consumers of `+0xC4` / `+0xC8` outside the two sums (`0x421E9F`, `0x4B2814`, `0x4CA87D`)
  are unread; whether the Coastal Fortress ever bombards on its own is open.
* RULE words `[0x9C7260]` (body `+0x9C`, 20), `[0x9C72F8]`, `[0x9C7300]`, `[0x9C7304]`,
  `[0x9C730C..0x9C7318]` (consumers unread).
* PRTO ability bit 10 (set on every air unit), the meaning of unit order 34, unit status bits
  other than `0x04`, `0x20`, `0x40`.
* The writer of `[0x9903B8]` (the shared seed) and the meaning of `[0xA5267C]` bit 8.
* Classification of the remaining `0xA526B4` call sites (riot, the one-in-sixteen event near
  `0x43EC39`, disease and so on); Military Academy consumers (BLDG flag `+0xF4`).
* Section 14.5: the capture path is decoded in `capture.md`, including the AI's raze and
  accept decisions (`Player.vtable +0x18` / `+0x14`, section 12 there); its open list (the
  helper routines) is section 13 there.

## 13. Verification record

* Every data constant in `rust/src/combat.rs` (RULE, DIFF, EXPR, TERR, BLDG values) was
  re-read from the decoded `conquests.biq` rows; the literals (25, 50, 100, 16, 1024,
  the clamp) come from the instructions cited beside each one.
* The terrain list, structure list (fortified 25, fortress 50, barricade 100,
  Town Walls 50, Civil Defense 50, City 50, Metro 100, river 25), the resisters sentence and the
  radar tower text of `Text/Civilopedia.txt` agree with the decoded numbers.
* The ranged-attack, city-strike, defensive-bombard and interception chapters (8) were
  read twice from disassembly; the unit statistics quoted in 8.4 and 8.5 come from the
  decoded `conquests.biq` PRTO rows (bombard strength, range, rate of fire, ability word).
* Dynamic confirmation (an actual fight under Wine) was not needed and has not been done;
  the shipped text plus the code agree, so the open items above are the remaining risk.

## 14. After the fight: the raid, the capture routine, and the diplomatic gates

### 14.1 `0x563410`: one routine for every change of city ownership

`Player::0x563410(city, unit, capture, convert)`, `ret 0x10`, `this` = the player that takes the
city, 7 callers (`0x469B3F`, `0x476C76`, `0x4B2DC0`, `0x5034B9`, `0x5281D3`, `0x5B9D81`,
`0x5C4D47`). It first reads the city's owner (`city +0x28`), its tile (`+0x24` / `+0x26`) and
its continent (tile vtable slot `0xB8`), then branches on `this +0x1C` (the civ id):

* **`this` is the barbarian civ (0)**: the **raid**, 14.2. The city never changes hands.
* any other civ: the capture or conversion path from `0x5638E3` (14.5, `capture.md`).

### 14.2 The barbarian raid (verified)

A barbarian "capture" of a city is a plunder: the city keeps its owner, one loss is applied,
and the raiding unit is removed (`0x5BBBC0(unit)`, `0x5638D2`). The loss is chosen in this
order (`B = city`, `O = its owner`, `T = O's treasury`, see below):

| # | condition | loss | message (`script.txt`) |
|---|---|---|---|
| 1 | `0x427540(B) == 0` (a **town**: size <= `[0x9C72E4]`, the tier test of 4.2) and `0x4C0C70(B) > 0` (it has walls) | `0x4C1320(B, 0)` removes the building with the highest land bombard defense (8.6); nothing if it finds none | `BARBARIAN_DESTROY_WALLS` |
| 2 | `[B +0x44] > 10` (the city's **shield stock**) | the stock is set to `min(0, cost of the current item)`, i.e. 0 (`0x4ACD70` is the item cost, 0 if none) | `BARBARIAN_CAPTURE_CITY_PRODUCTION` ("Our work on X has been destroyed!") |
| 3 | `0x4BB410(B, race(O)) > 1` (at least two citizens of the owner's race) | `0x4BA230(B, 1, race(O), 0)`: one random such citizen is killed (start index `next(count + 1)` on the gameplay `Random`, then a cyclic scan) | `BARBARIAN_CAPTURE_CITY_POPULATION` |
| 4 | `T != 0` (and not 3) | gold: see below | `BARBARIAN_CAPTURE_CITY_GOLD` |
| 5 | otherwise | as 2: the stock is set to 0 | `BARBARIAN_CAPTURE_CITY_PRODUCTION` |

Rows 2 to 5 are one chain: the shield test comes first (`0x5635C7`), then the citizen count
(`0x5635F1`, evaluated twice), then the treasury (`0x5635F6`); `0x5637C9` is the production
exit shared by 2 and 5. The messages are shown only when `O` is the local human player
(`[0x9FD4BC]`); the effects always apply. In multiplayer (`0x47B530`) the message goes through the
network dispatcher `0x47A430` / `0x495840` with a flag.

**Gold** (`0x56369C..0x563788`). `T = [P +0x44] + [P +0x48]` for the owner's player record `P`
(the treasury is stored as two shares, `economy.md`). The loot is
`T / [P +0x194]` (signed division by the city count); if that is 0 it is 1 when `T > 1`, else
`T` itself. The treasury is rewritten with the writer's own split: the remainder `R = T - loot`
becomes `[P +0x44] = timeGetTime() % R - 0x3039`, `[P +0x48] = R - [P +0x44]`
(`R <= 0` stores `timeGetTime() % 0xD431 - 0x8235` and its negation, so the sum is 0), the same
algorithm as `0x4C2350`. The barbarians do not receive the gold.

### 14.3 The diplomatic gates in front of an attack (verified)

Two civ masks appear in the gates and in hundreds of other tests of the form
`mask & (1 << civ)`: **`[0xA526BC]` is the mask of the human civs** (`1 << [0x9FD4BC]`,
`0x4F556E` in single-player; built from the player-slot types in multiplayer,
`0x494841`) and **`[0xA526C0]` the mask of the civs in play** (restored from saves,
`0x59FFC3`..`0x5A2419`; HYPOTHESIS).

**The attack validator `0x5C0E20`** (8.1) carries the gate for an attack on a tile held by civ `V`
(`0x5C10C0..0x5C11AE`): first its own copy of the alliance refusal (`NOALLIANCE_AGGRESSION`, as in
step 1 below), then

```text
if multiplayer and the attacker's civ is in the human mask:
    if not owner.atWar(V): owner.declareWar(V, reason 0)                      0x501F20
else:                                                      # always, in single-player
    if not unit.mayAttack(V, interactive = 1): refuse                          0x5B5790
    unit.provoke(V, flag = (owner of the target tile == V))                    0x5B5600
if the unit's domain is air: refuse when the air defense stops it              0x5C68A0 (air.md)
```

**`0x5B5790(otherCiv, interactive)`** (`ret 8`; "may this unit attack that civ"):

1. `owner +0xF30 + 4 * otherCiv` has the `0x04` flag (the two civs are **allied**): refuse. When
   `interactive` and the local player is one of the two, the tile is marked and `NOALLIANCE_AGGRESSION`
   ("No aggression against alliance.") is logged (`0x4ED220`).
2. Allow without a question when the unit has **Hidden Nationality** (ability 17), when
   `otherCiv <= 0` or equals the owner, when the owner is already at war with `otherCiv`
   (`byte [0xA53BC8 + 8420 * owner + otherCiv]`), or when the owner is not the local human player.
3. Otherwise the human is about to attack a civ it is at peace with: a confirmation box
   (`DECLARE_WAR`: "this will cause war with the X people. Are you sure?") decides; "No" refuses.
   On "Yes" in multiplayer the choice is sent (`0x473D00`); the war itself is declared by the
   next routine.

**`0x5B5600(otherCiv, flag)`** (`ret 8`; "provoke"):

```text
if otherCiv <= 0 or otherCiv == unit.owner: return
hidden = unit has ability 17, or the prototype shared by its army (0x5BC6D0) has it
if not hidden:
    if single-player, or the owner's civ is not in the human mask:      # humans declared in the validator
        owner.declareWar(otherCiv, reason 0)                                  0x501F20
    if flag:
        for each civ c = 1.. (31 words from +0xF34), c != owner, c != otherCiv,
                with c's civ id in the in-play mask [0xA526C0]:
            w = otherCiv.relation[c]                         player + 0xF34 + 4*(c - 1)
            if (w & 4) or (w & 1):             # allied with, or (flag 1, unnamed) tied to, the victim
                player[c].declareWar(owner, reason otherCiv + 2)
aggression counter: player[otherCiv].pair[owner].+0x00 += 1               0x5B577D (always, hidden or not)
```

So an attack that breaks the peace makes the attacker's civ declare war on the victim, then the
victim's allies declare war on the attacker. The unit action at `0x5B34C0` (a worker-style
action on the unit's own tile, animation 12) also provokes, passing the **tile's owner**
(`0x5B358F`, flag 0). The same call to arms also runs **every turn without any attack**: the
second pass of `0x500AD0` makes the allies of a territory owner declare war on a civ it is at war
with whose military unit stands on the owner's land (`government.md` 5.2). In both routes the
relation word is `Player +0xF30 + 4 * civ`: bit `0x01` mutual protection, `0x02` right of
passage, `0x04` alliance.

**The tile-owner byte.** The civ id passed there is `byte[cell+5]` (cell vtable slot `0x98`,
`0x5EAA80`). The evidence that this byte is the territory owner and not a river mask:
(1) it is the argument of `provoke` at `0x5B358F` and `0x5C119A`; (2) `0x5BEC03` (in the
movement-permission test around `0x5BEB60`) treats a non-zero value different from the unit's own
civ as a foreign border, and lets the unit through only for armies (ability 18, also when the unit is
carried by one), when `owner.relation[tileOwner]` has the `0x02` flag, or when
`0x55AA10(owner, 0x100, 0)` is positive; (3) `declareWar` compares it with the target civ when
it scans the player's units for ones standing in the victim's territory (`0x502027`, only when
the relation has the `0x02` flag or the pair counter has reached `0x200`); (4) `0x4B2A9F` counts
tiles whose owner equals a given civ. The `0xA53BC8` gate the renderer applies to it is the at-war
table below, so the render path of `rivers.md` shows an overlay for tiles whose owner the viewer is
at war with.

**`0x501F20(player, civ, reason)`** (`declareWar`): returns at once for `civ <= 0`, `civ == player.civ` or when
the war already exists (`player.+0xD30[civ] != 0`); it then runs `0x501CD0(civ, 0)`, counts a treaty
violation in a pair record when it finds one of the player's units hostile to `civ` on a tile that
`civ` holds (the scan over the unit pool at `0x501FA2..0x50204D`), counts the declaration in
the victim's pair record, and sets **both** at-war bytes (`player.+0xD30[civ]` and
`civ.+0xD30[player.civ] = 1`). The tail is decoded in `government.md` 5.5: the relation-bit
reset, the war-weariness kicker (60 or 30 for the declarer, minus 30 for the victim, only for a plain
declaration against a civ it has done nothing hostile to), the AI's war memory `Player +0xBB0` and
the deal settlement `0x500830`.

**`0x558F70(unit)`** (`Player::atWarWith`, `ret 4`): `idx = unit.owner`, except that a **Hidden
Nationality** unit of a third civ is looked up as civ **0**: the result is `byte [this +
0xD30 + idx]`. A hidden-nationality unit is therefore at war with exactly the civs that are at war
with the barbarians, which is how privateers can always be attacked.

### 14.4 The pair record and the incident tally

`Player +0x1C4 + 0x4C * other` is a 19-dword record per ordered pair (the player's record
about civ `other`). The array really starts 0x14 bytes earlier, at `Player +0x1B0 + 0x4C *
other`: four more dwords are used there (declarations, deals broken, treaty violations, hostile
acts); `government.md` 5.1 has the table in both numberings.

| offset | field |
|---|---|
| `+0x00` | aggression counter: incremented in the **victim's** record about the aggressor by `0x5B5600` |
| `+0x14` | tech trades |
| `+0x20`, `+0x24` | two accumulators of "incident" weight, written by `0x5631B0` |

`Player::0x5631B0(victimCiv, amount)` (`ret 8`; `this` = the actor): returns at once for the
barbarian actor (`this +0x1C == 0`), for a victim whose `+0x1C` id is 0 and for `victim == actor`;
otherwise it adds `amount` to **both** `+0x20` and `+0x24` of `player[victim]`'s record about the
actor. The combat code calls it with weight 1 when a hit leaves a unit at exactly 1 HP (8.4, 8.9)
and when a city strike kills population (8.5). The readers of the tally are decoded in
`government.md` 5: the second accumulator is added whole to the war-weariness counters every turn
(`0x500AD0`), the first one feeds the AI's mobilization (`0x444B80`), and peace (`0x5025B0`) halves
the first and zeroes the second. The dwords also enter the AI's attitude score `0x440100`
(not decoded).

### 14.5 Capture by a non-barbarian civ (decoded in `capture.md`)

Everything after `0x5638E3` is documented in [`capture.md`](capture.md): the four stack
arguments are `(city, unit, capture, convert)`; a culture conversion (`convert`) asks the
converting civ to accept or rebuff; a military capture plunders gold (the victim's last city
yields the whole treasury, otherwise `treasury / cities` scaled 1/2, 3/4, 1 by size class),
shrinks or destroys the city, then asks keep or raze (an AI decides with `0x443B60`, a
conversion offered to an AI with `0x443A60`); the transfer `0x564800` destroys the
Palace of a lost capital, every culture building and small wonder, and 25 percent of the
other ordinary buildings of a capture into a city without a citizen of the capturer's race.
Reference code: `rust/src/capture.rs`.
