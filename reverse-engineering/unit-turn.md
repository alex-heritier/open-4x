# The per-unit turn, healing and unit death

Clean-room specification of what the executable does to **one unit once per round** (`Unit::turn`, `0x5C7700`),
of the healing rule it calls (`0x5BEB60`, `0x5BECE0`), and of the routine that removes a unit from the game
(`Unit::kill`, `0x5BBBC0`). Image base `0x400000`; `Civ3Conquests.exe` (PE32, MSVC 6). Companion documents:
`turn.md` (where the per-unit turn is called from), `combat.md` (unit record, PRTO words, abilities),
`victory.md` (the score paid by a kill), `capture.md`, `barbarians.md`, `primitives.md`.

Evidence tags: **V** read in the disassembly for this document (address given), **I** inferred from a
consistent set of reads, **H** hypothesis (do not implement as fact), **O** open.

This file does **not** contain unit movement, orders, the AI decisions about units, or the combat engine. Those
are elsewhere (`combat.md`, `ai.md`) or still open (section 9).

## At a glance

* `Unit::turn(U)` is `thiscall`, plain `ret`, `0x5C7700..0x5C7E88`. It is called once per unit and per round
  (section 6). Order inside the call:
  1. clear the "defense halved" flag `U.+0x1ED`;
  2. if the unit is **not carried** (`U.+0x60 == -1`): the **sea hazard** (a ship can sink), then the **jungle
     disease** roll; either one ends the call with the unit's death;
  3. **healing** of a damaged unit, then the **auto-wake** of a human's fortified unit that finished healing;
  4. a fortified or intercepting ship refreshes its vision;
  5. pending reveal block (`0x5C7570`);
  6. **end of turn bookkeeping**: movement used is reset to 0 and the per-turn status bits are cleared.
* Two random draws can occur, both from the gameplay generator `0xA526B4` (`rand(n)` = `0x60BAB0(0xA526B4; n)`
  returns a value in `0..n-1`, the code tests the low 16 bits): the **sea roll** `rand(2)` or `rand(4)`, and the
  **disease roll** `rand(1000)`. Neither draw happens unless every earlier condition of its block holds.
* Healing per round, for a unit with damage and (movement used = 0 or status bit 0): in the open 1 hit point,
  in a city 2, in a city with the matching building all (stock rules, section 4). Armies heal by thirds.
* A unit's death goes through `Unit::kill` (section 5), which also kills the cargo, pays the kill score,
  updates the owner's counters, frees the pool slot and may eliminate the civilization.

## 1. Fields and tables used

Unit record fields (`combat.md` has the base table; the additions here are marked **new**).

| field | meaning |
|---|---|
| `+0x20` | unit id (index in the pool `[0xA52E84]`, last index `[0xA52E90]`, node pointer minus `0x1C` = object) |
| `+0x24` / `+0x28` | x / y (raw map coordinates; cell index = `(W/2) * y + (x >> 1)`) |
| `+0x34` | owner civ slot (0 = barbarian) |
| `+0x38` | nationality (**new**, section 5 step 6; used as a RACE row there and as a player slot in step 7: **O**) |
| `+0x40` | PRTO row (stride `0x138`, table `[0x9C71E0]`) |
| `+0x44` | experience level (0 to 3) |
| `+0x48` | status bits (section 3.7) |
| `+0x4C` | damage taken (dead when it reaches the unit's maximum hit points) |
| `+0x50` | movement used, in movement fractions (`[0x9C72C8]` per full move) |
| `+0x5D` | byte, **new**: nonzero while the unit is registered with the controller `0xA268B8` (cleared in section 5 step 4) |
| `+0x60` | id of the unit carrying this one (army or transport), `-1` none |
| `+0x64` | order: 0 none, 1 fortified, 15 interception |
| `+0x68` | byte, **new**: cleared by the cancel-orders method; tested by the network dispatcher (`0x476FB0`), meaning **O** |
| `+0x6C` / `+0x70` | **new**: a saved tile (x, y) used by the pending reveal `0x5C7570`; `-1, -1` when unused |
| `+0x74` | custom unit name (NUL-terminated; empty means the PRTO name) |
| `+0x1C4` | **new**: id of a linked unit (`-1` none); cleared when that unit dies |
| `+0x1E8` | byte, **new**: nonzero while an activity record is attached (`0x5CD300` detaches it) |
| `+0x1EA` | word, **new**: with `+0x1EC` the identity of a captured-flag token (section 5 step 7) |
| `+0x1EC` | byte, **new**: race id of that token, `0xFF` none |
| `+0x1ED` | byte: defense-halved flag (cleared first thing in the turn) |
| `+0x22C` | multiplayer: nonzero means "owned by a remote machine, skip" (tested by the callers) |

Other tables and globals:

| item | where | meaning |
|---|---|---|
| player record | `0xA52E98 + 0x20E4 * slot` | `+0x1C` slot, `+0x20` race id, `+0x188` int16 army count, `+0x18C` unit count, `+0x190` military unit count, `+0x108` int16[20] units per AI strategy bit, `+0x15F0` pointer to an int16 array indexed by PRTO row: live units per type, `+0xF30 + 4 * civ` treaty word (bit `0x02` right of passage) |
| RACE table | `[0x9C71D0]`, stride `0x974` | `vtable[0]` `0x53A080(trait)` tests a trait bit; trait 7 is Seafaring |
| EXPR table | `[0x9C40CC]`, stride 44 | mem `+0x24` base hit points of the level (2, 3, 4, 5); mem `+0xA8` is therefore the base hit points of level 3 (Elite) |
| local player | `[0x9FD4BC]` | slot of the human at this machine |
| human mask | `[0xA526BC]` | bit `slot` set for human civs |
| game flags | `[0xA5267C]` | `0x4000` Capture the Unit, `0x20000` Reverse Capture the Flag, `0x800` Regicide, `0x1000` Mass Regicide (`victory.md` 1.1) |
| `0x47B530` | | multiplayer-mode gate (tail jump to `0x499FE0`; `primitives.md` 6) |
| world unit count | `[0xA5268C]` | decremented by every kill |

## 2. Predicates used (all **V**)

* **`Unit::hasAbility(U; n)` `0x5BC8B0`, `ret 4`.** For `n == 18` (Army) it is the PRTO test alone. For every other
  `n` it is true when `PRTO[U.type]` has ability `n` **or**, if `m = 0x5BC6D0(U) != -1`, when `PRTO[m]` has it.
  `0x5BC6D0(U)` is `-1` unless `U` is an army; for an army it scans the units on the army's tile whose carrier
  (`+0x60`) is `U` and returns the PRTO row they all share (`-1` if there is none or if two members differ). So
  **an army has the abilities of the unit type it carries**. The PRTO test is `0x5E4EF0(row; n)` (bits 0 to 31
  of the ability dword, higher bits in the extended dword). Ability numbers used in this file (`biq` crate
  `prto::ability`): 3 Cruise Missile, 11 Sinks in Sea, 12 Sinks in Ocean, 13 Flag Unit, 16 Nuclear Weapon,
  18 Army, 29 King.
* **`Unit::maxHP(U)` `0x5BE5B0`.** `hp = EXPR[U.level].baseHP`, but for an army with members `hp` is the **sum** of
  the members' `maxHP` (recursive; members are the units on the tile whose `+0x60` is the army's id). Then
  `hp += PRTO[U.type].hitPointBonus` (mem `+0xA4`) and the result is `max(hp, 1)`.
* **`Player::knowsTechWithFlag(P; mask)` `0x561480`**: true when the player has a TECH whose flag word has the bits
  (`research.md` section 2 table). `0x2000` is "Trade over Sea Tiles", `0x4000` "Trade over Ocean Tiles".
* **`Player::countWonders(P; value, 0)` `0x55A8D0`**: number of the player's great wonders whose wonder-flag
  dword (BLDG body `+0xF4`) contains `value` (`combat.md` 4.5). Value 1 is "Safe Sea Travel" (the Great Lighthouse).
* **`Player::countSmallWonders(P; mask, 0)` `0x55AA10`** (`primitives.md` 3.3.1): small wonders with the flag. Mask
  `0x100` is "Allows Healing in Enemy Territory".
* **`City::countBuildingsWithFlag(C; mask)` `0x4B1F90`** (`primitives.md` 3.5): improvement flag test; masks
  `0x2` (barracks-type), `0x20000` (port-type) and `0x40000` (airport-type) are used by the heal rule.
* **City at a tile** `0x56D2C0(x, y)`: the city object whose id is in the cell's city word (`cell vtable +0xB4`), or
  `0` (also `0` for coordinates outside the map).
* **Cell methods used** (`world-events.md` 1.2): `+0x8C` is water, `+0x98` tile owner byte, `+0xC8` terrain id
  (`8` jungle, `11` coast, `12` sea, `13` ocean), `+0xA0` unit list, `+0xB4` city id, `0x5EA6C0(cell)` has a
  city, `0x5EA6E0(cell)` hosts a plain colony.

## 3. `Unit::turn(U)` `0x5C7700` step by step

### 3.1 Prologue **V**

```
U.+0x1ED = 0                                         // 0x5C7714
if U.+0x60 != -1: goto POST                          // 0x5C771B: a carried unit skips every hazard
```

### 3.2 The sea hazard **V** (`0x5C7721..0x5C7869`)

```
sinkable = false
if hasAbility(U, 11):                                          // Sinks in Sea
    if not knowsTechWithFlag(owner, 0x2000)
       and countWonders(owner, 1, 0) == 0                      // no Safe Sea Travel wonder
       and terrain(x, y) == 12:                                // the tile is Sea
        sinkable = true
if not sinkable and hasAbility(U, 12):                         // Sinks in Ocean
    if not knowsTechWithFlag(owner, 0x4000) and terrain(x, y) == 13:
        sinkable = true
if sinkable:
    n = 2
    if RACE[owner.race].hasTrait(7) then n = 4                 // Seafaring (0x5C785E..0x5C7864)
    if rand(n) == 0: goto LOSS
goto DISEASE                                                   // 0x5C7A9B
```

Reading notes: the Sea test is on the **tile's terrain id**, not on a "deep water" flag; a Sinks-in-Ocean ship
is safe on Sea and Coast, a Sinks-in-Sea ship is safe on Ocean (it cannot normally be there). The tech and
wonder immunity is per owner. The draw is made only for a sinkable unit, so a unit that is immune consumes no
random number. Chance per round: 1/2 normally, 1/4 for a Seafaring civilization.

### 3.3 The jungle disease **V** (`0x5C7A9B..0x5C7B0A`)

```
if PRTO[U.type].populationCost != 0: goto POST             // mem +0x68 (body +0x64)
if U.order != 1:                    goto POST              // only fortified units
if terrain(x, y) != 8:              goto POST              // jungle
if rand(1000) != 0:                 goto POST              // 0.1 % per round
goto LOSS (disease variant)
```

Units that cost population (Settlers and the like) are immune. The unit does not need to be a land unit; the
three tests are the whole condition. In the shipped game only land units are fortified on jungle.

### 3.4 Delivering the loss **V** (`0x5C787D..0x5C7CFF`)

Both hazards share this block; only the message key differs (`SHIPSUNK` for the sea hazard,
`UNITJUNGLEDISEASE` for the disease).

```
if owner != [0x9FD4BC] (the unit is not the local player's):
    goto END_LOSS                                         // no message, no queue
if 0x47B530():                                            // network game
    append a record to the pending-event table (below); goto END_LOSS
// single machine game, the local player's unit:
0x4DF6B0(0x9F8700; x, y)                                  // bring the tile into view (UI; sea hazard only,
                                                          //  the disease path at 0x5C7C7B has no such call)
name = (U.+0x74 non-empty) ? U.+0x74 : PRTO[U.type].name  // mem +8
0x61C5A0(0, name, -1, -1)                                 // message text slot 0 = the unit name
0x4ED220(0x9F8700; x, y, key, 1)                          // the popup with the key
[0xA281D0] = 1; 0x4F00F0(0xA268B8; U, 6, 0); [0xA281D0] = 0   // controller notification, kind 6
END_LOSS:
if 0x47B530(): return                                     // network: the unit dies when the record is handled
Unit::kill(U; 0, 1, 0, 0, 0, 0, 0)                        // 0x5BBBC0, section 5; then return
```

So on a single machine **every** sunk or diseased unit is killed through `Unit::kill` with `killer = 0`
and `a2 = 1`, whether or not it belongs to the human. In a network game only the local player's own unit gets
a pending-event record, and nothing else happens on this machine (the unit stays until the record is handled;
the handler is not decoded).

The pending-event table (network games only, **I** from `0x5C789A..0x5C7A46`): parallel arrays of `0x2000` entries
at `0x74F1C8` (unit id), `0x7571C8` (kind), `0x75F1C8`, `0x7671C8` (byte), `0x7691C8` (byte), `0x76B1C8`,
`0x7731C8`, `0x77B1C8` (all `-1` initially), `0x7831C8` (byte), `0x7851C8` (the time stamp, nonzero means
in use) and `0x7BD1CC`. The new entry is inserted in **time-stamp order** using `timeGetTime()`: the first
unflagged entry (byte `0x74D1C8[i]` zero) with a later time stamp is shifted up together with everything
behind it, and the new entry (unit id, kind `6`, time stamp = now) goes to its place; if none is later it is
appended after the last used entry.

### 3.5 Healing **V** (`0x5C7D03..0x5C7D36`)

```
POST:
if U.damage > 0 and (U.movesUsed == 0 or (U.status & 1)):
    if healEligible(U, x, y):                               // 0x5BEB60, section 4.1
        heal(U)                                             // 0x5BECE0, section 4.2
```

The movement test is exactly `U.+0x50 == 0` or bit 0 of `U.+0x48` (the setter of bit 0 is not known, **O**). A
unit that moved this round and has no bit 0 does not heal; the moves counter is cleared at the end of the call
(3.7), so moving ends the healing of that round only.

### 3.6 Auto-wake of a healed fortified unit **V** (`0x5C7D3D..0x5C7E2F`)

Right after `heal(U)` the following chain runs; every test must pass for `0x5BEB10(U)` to be called.

```
owner is human                      ([0xA526BC] has bit Player.+0x1C)
not 0x47B530()                      // not a network game
U.damage == 0                       // fully healed now
U.order == 1                        // fortified
U.+0x60 == -1                       // not carried
if PRTO[U.type].domain == 0 (land): the tile is not water, has no city (0x5EA6C0) and hosts no plain colony (0x5EA6E0)
    // domain 1 and 2 units skip the tile test
then 0x5BEB10(U)
```

`0x5BEB10(U)` (**V**): remember `s = (domain == 1 and order is 1 or 15)`; call the unit's virtual method `+0x68`
(vtable `0x66DCF0`, body `0x56BE70`: cancel orders; below); if `s`, run `0x5BA1D0(U; 0, 0, 1)` (the vision
update, 3.8). The method `0x56BE70` does: `0x5B3040(U; 0)` (set order none), `U.+0x48 &= ~0x180`, `U.+0x68 = 0`,
detach the activity record when `U.+0x1E8` is set (`0x5CD300`), `U.+0xB8 = U.+0xBC = U.+0x1C8 = -1`, and
`0x5B2F10(U; -1)` (clear the link `+0x1C4`). The visible effect in play: **a human's fortified land unit that
heals to full outside a city wakes up**; units fortified in cities, ships, and AI units are not woken here.

### 3.7 Epilogue and the status bits **V** (`0x5C7E34..0x5C7E88`)

```
if PRTO[U.type].domain == 1 and (U.order == 1 or U.order == 15): 0x5BA1D0(U; 0, 0, 1)   // 0x5C7E5A
if U.status & 8: 0x5C7570(U)                                                             // pending reveal
U.+0x50 = 0                                  // movement used
U.+0x48 = U.+0x48 & 0xB8                     // clears bits 0x01, 0x02, 0x04, 0x40
```

Status bits of `U.+0x48`: `0x04` already attacked this round (`combat.md`), `0x40` already fired a defensive
bombard, `0x01` and `0x02` per-round flags whose setters are not known (**O**), `0x08` pending reveal,
`0x10`, `0x20` (has produced a Great Leader) and `0x80` survive the end of the round. Everything the unit did
in the round is forgotten here: the new round starts with **full movement** (used = 0) and no attack flag.

### 3.8 Vision helpers

Fully specified in `vision.md`: `0x5BA1D0(U; a, b, c)` is the unit sight refresh (`a` = withdraw this unit's sight, `b` = tidy
around the previous tile, `c` = request a redraw; section 6 there), `0x5BA010` is the sight predicate (section 3), `0x5C74A0` /
`0x5C7570` are the air reveal begin / end (section 7). Calls made from this document's routines:
`0x5BEB10`, `0x5C6290`, `0x5C6570` and the epilogue `0x5C7E62` all use `(a, b, c) = (0, 0, 1)` and only for a sea unit whose order was
1 or 15.

## 4. The healing rule

### 4.1 Eligibility `healEligible(U; x, y)` `0x5BEB60`, `ret 8` **V**

```
if domain(U) == 1 (sea) and cell(x, y) is water: return false          // a ship at sea never heals
r = 0x56D7D0(x, y, -1, 1)                                               // civ occupying the tile: city owner, else unit/colony owner, 0 camp/barbarian, -1 empty
if r != -1 and r != U.owner: return false
t = tile owner byte of cell(x, y)
if t == 0 or t == U.owner: return true
// foreign territory:
if hasAbility-18 (PRTO only) of U: return true                          // an army
carrier = unit with id U.+0x60; if it exists and has ability 18: return true
if (Player[U.owner].+0xF30 + 4 * t) & 2: return true                    // right of passage with the tile owner
return countSmallWonders(owner, 0x100, 0) > 0                           // "Allows Healing in Enemy Territory"
```

`0x56D7D0(x, y, viewer, vis)` is the tile **occupant** resolver, not the border owner (specified in `goody-huts.md` 6.1, **V**): with `viewer = -1` it returns the owner of the city on the tile, else the owner of the first unit (or the colony owner when the tile hosts a colony), `0` for a barbarian or a camp, `-1` for an empty tile. So the first test says: a unit does not heal on a tile occupied by another civ (a foreign unit sharing the tile, or a foreign city or colony); the territory test follows.

### 4.2 Amount `heal(U)` `0x5BECE0`, plain `ret` **V**

```
if 0x47B530() and 0x469590(0x74AF60; U.id): return        // network: unit is queued elsewhere (0x5BED05 test)
isArmy = hasAbility-18(U) (PRTO only)
W      = isArmy and countSmallWonders(owner, 0x100, 0) > 0
base   = isArmy ? maxHP(U) : EXPR[3].baseHP                // [[0x9C40CC] + 0xA8]
A      = base - 1
city   = 0x56D2C0(x, y)
if city:
    mask = { land: 0x2, sea: 0x20000, air: 0x40000 }[PRTO.domain]      // other domain values skip the next test
    if mask and city.countBuildingsWithFlag(mask) > 0: h = A
    else if W:                                              h = trunc(2 * A / 3)
    else:                                                   h = trunc(A / 2)
else if not isArmy:                                         h = trunc(A / 4)
else:
    t = tile owner byte
    foreign = (t != 0 and t != owner and ((Player[owner].+0xF30 + 4 * t) & 2) == 0)
    if foreign: h = W ? trunc(A / 3) : trunc(A / 4)
    else:       h = W ? trunc(A / 2) : trunc(A / 3)
U.damage = max(0, U.damage - h)
```

All divisions truncate toward zero (the code uses `0x55555556` multiplies and `cdq/sar`). The result is applied
once per round. There is no bonus for fortifying, for experience level, or for terrain.

Stock values (`conquests.biq`, Elite base hit points 5, so `A = 4` for an ordinary unit):

| situation, ordinary (non-army) unit | hit points healed per round |
|---|---|
| open country, any owner the unit may be on | 1 |
| city without the matching building | 2 |
| city with the matching building (`0x2` land, `0x20000` sea, `0x40000` air) | 4 (and damage never goes below 0) |

Golden vectors for an army (`maxHP = 10`, `A = 9`):

| situation | no wonder `W` | with wonder `W` |
|---|---|---|
| city with the building | 9 | 9 |
| city without it | 4 | 6 |
| open country, own, neutral or right-of-passage territory | 3 | 4 |
| open country, foreign territory without right of passage | 2 | 3 |

Hit-point cap: the maximum of a non-army unit is `EXPR[level].baseHP + PRTO.hitPointBonus` (section 2), which can
exceed 4, so a unit of base 5 with a +3 bonus needs two rounds in a city with the building.

## 5. `Unit::kill(U; a1..a7)` `0x5BBBC0`, `ret 0x1C` **V**

Arguments (stack, first to last): `a1` killer slot (`0` none), `a2` (see step 6), `a3` air cargo dies too, `a4`, `a5`,
`a6` suppress the elimination check (`a6` also suppresses the regicide announcement), `a7` combat-loss flag for
the flag branch (step 7). Call sites pass, for example: `Unit::turn` `(0, 1, 0, 0, 0, 0, 0)`; city capture
`(P.civ, 0, 0, 0, 0, 0, 0)` (`capture.md`); the elimination sweep `(killer, 0, 0, 1, 0, 0, 0)` (`victory.md`, `0x568D19`); cargo of a dying carrier `(a1, 0, 0, 0, 0, 0, 0)`. There are 45 call sites. Constant patterns
seen in their pushes: `a2 = 1` at `0x5B366C`, `0x5B3786`, `0x5B3836`, `0x5B38E4`, `0x5B3994`, `0x5BC6BC`, `0x5C0415`, `0x5C048D`,
`0x5C17DA`, `0x5C3245` and `0x5C7CF8`; `a3 = 1` at `0x4A472D`, `0x4A47AA` and `0x5C702B`; `a4 = 1` at `0x568D19`;
`a5 = 1` at `0x566775`; `a6 = 1` at `0x5C0944`; `a7 = 1` at `0x4F42DF` and `0x5C17DA` (a register at `0x4A4431`).

```
id, owner, type, cont = U.+0x20, U.+0x34, U.+0x40, continent word of the tile (cell vtable +0xB8, sign-extended)
1. if 0x47B530() and 0x46BE30(0x74AF60; U): 0x46BE70(0x74AF60; id)         // network: drop the pending record
2. king = hasAbility(U, 29)
3. if not a6 and king and (flags & 0x1800):                                  // Regicide or Mass Regicide
       announce 0x58B5D0(0xC88588; 7, owner, x, y, name, 0)
4. if U.+0x5D: 0x4F02C0(0xA268B8; U); U.+0x5D = 0
   if U.+0x1E8: 0x5CD300(U)
5. (reserved: the result of hasAbility(U, 13) at 0x5BBC38 is not used)
6. if a2:
       s = 0x539D60(RACE[U.+0x38])                        // slot of the civ of that race, -1 none (primitives.md 6)
       if s != owner: Player[s].+0x1F4 + 0x4C * owner += 1  // the pair record "+0x44" of government.md 5.1: "units of s's nationality lost under owner"
7. if flags & 0x24000 (Capture the Unit or Reverse Capture the Flag):                 // 0x5BBCFA..0x5BBEDD
       see 5.1
8. 0x4EF740(0xA268B8; U)                                  // controller: remove U from its lists
   U.+0x4C = max(0, maxHP(U))                             // damage := maximum: the unit has 0 hit points
   0x5B2F10(U; -1)                                        // clear U.+0x1C4 link
9. if flags & 0x26000 and a1 != 0 and a1 != owner:       // victory-point modes (victory.md 4)
       Player[a1].(+0x11CC)  += trunc(PRTO[type].shieldCost / 10) * [0xA529B0]
       Player[a1].(+0x15BC)  += the same amount
10. for every unit v in the pool, index 0..[0xA52E90]:
       if v.+0x1C4 == id: 0x5B2F10(v; -1)
       if v.+0x60 == id:                                  // v rides in U
           if hasAbility-18(U) (U is an army): Unit::kill(v; a1, 0, 0, 0, 0, 0, 0)
           else d = PRTO[U.type].domain
               if d == 2 and a3:                          Unit::kill(v; a1, 0, 0, 0, 0, 0, 0)
               else if d == 1 and cell(U.x, U.y) is water: Unit::kill(v; a1, 0, 0, 0, 0, 0, 0)
               else                                        0x5C59B0(v; -1, -1)        // v leaves the carrier
11. 0x5BD220(U; -1, -1)                                   // Unit::setPosition: remove U from its tile
    [0xA5268C] -= 1                                       // world unit count
    Player[owner].+0x18C -= 1                             // unit count
12. military count: Player[owner].+0x190 -= 1 when any of:
       0x5BE6E0(U) > 0 (attack) or 0x5BE820(U) > 0 (defense) or PRTO[type].bombardStrength (mem +0x48) > 0
       or PRTO[type] has ability 16 or the army's member type (0x5BC6D0) has it
13. 0x55A020(Player; type, 1): for each AI strategy bit k in 0..19 set in PRTO[type].+0x8C: Player.+0x108[k] = max(0, Player.+0x108[k] - 1)  (int16)
    Player.+0x15F0[type] (int16) -= 1 when nonzero
    if hasAbility-18 (PRTO only): Player.+0x188 (int16) = max(0, Player.+0x188 - 1)
14. 0x4EF740(0xA268B8; U) again; 0x406200(0x73DA40; &U.+0x27C) (O: releases the object embedded at +0x27C from the list 0x73DA40); U.+0x1DC = 0
15. free the pool slot: 0x5B0380(object) (destructor); array[id].obj = 0; array[id].next = [0xA52E88];
    [0xA52E88] = id; [0xA52E8C] += 1                      // LIFO free list (ids are reused newest first)
16. if [0x9FD474] == U: 0x4DBA70(0x9F8700; 0, 0)           // the selected unit died: clear the selection
17. if not (a4 or a5 or a6):
       force = king and (flags & 0x800)                  // Regicide
       0x568950(Player[owner]; cont, a1, force)           // civilization elimination check (victory.md 11)
```

Notes: the cargo of a **sunk or destroyed ship** dies (same call with `a1` as killer, so the killer is paid for
each cargo unit and each one runs its own elimination check); a land carrier or a ship in port drops the cargo
(`0x5C59B0`, which places the unit without orders); an **army's** members always die with it. Step 11 uses the
position-change routine with `(-1, -1)`, which also runs the leaving side of the arrival code; step 12 is the
counterpart of the increment done by the factory (`0x5694D0`, not re-read here, **I**).

### 5.1 Capture modes, step 7 (partly decoded)

Only when a flag-capture mode is on (`flags & 0x24000`). `[0x9C5B40]` (byte, not identified, **O**) combined with
a non-zero killer different from the owner cancels the whole step.

* If `U.+0x1EC != 0xFF` (the unit holds a token of race `R = U.+0x1EC`, type `U.+0x1EA`): when the tile is
  water, or when `a7 != 0`, or when the owner's slot is 0: announce `0x58B5D0(0xC88588; 10, slotOf(R), x, y, 0, 0)` and
  call `0x56AFB0(Player[slotOf(R)]; U.+0x1EA)` (**H**: the token returns to its home player). Otherwise (land,
  `a7 == 0`, owner a real civ): create a new unit with `0x5694D0(Player[owner]; type = U.+0x1EA, x, y, -1, -1,
  1, 0, -1)` and set its `+0x38` to `U.+0x1EC` (the token is dropped on the tile as a unit of the dying
  unit's owner).
* If `U.+0x1EC == 0xFF`: only a Flag Unit (ability 13) with `a7 != 0` acts: announce event `10` with the
  unit's `+0x38` and call `0x56AFB0(Player[U.+0x38]; U.type)` (note: `+0x38` is used as a player slot here but
  as a RACE row in step 6, **O**).

## 6. Where `Unit::turn` is called

| caller | address | context |
|---|---|---|
| `0x561220(P)` | `0x561270` | the pool walk: for every unit with `unit.+0x34 == P.+0x1C`, in **pool index order**, skipping (network games) units with `+0x22C != 0`. `turn.md` 2.1 step 6 had named the owner byte `+0x2C`; the field is `+0x34` (corrected there). The walk re-reads the last index `[0xA52E90]` on every iteration, so units created during the walk (index within the bound) are visited, and a unit killed earlier is simply absent |
| `0x561220` callers | `0x4F5D85` (human turn driver `0x4F5550`), `0x4F5FC7` (round processor loop A) | `turn.md` sections 2 and 5 |
| network | `0x476533`, `0x476619` (`0x476330`), `0x476FF8` (`0x476FB0`), `0x46F945` (`0x46F8B0`) | the network round and the message handler; `0x476FB0` runs the turn, then a virtual call `+0x54`, then (for a human unit without an order and without `+0x68`) the dispatcher `0x470700` (a human's unit with order 0 and `+0x68 == 0`), otherwise the dispatcher `0x4708b0` (units with no activity record, `+0x1E8 == 0`); both are **O** |

Consequently every unit, human, AI and barbarian, gets its movement restored, its healing, its hazards and its
status reset exactly **once per round**, in unit-id order within its owner's pass.

## 7. Corrections to earlier documents

| where | said | now |
|---|---|---|
| `ai.md` "disease call-site sequence" | `0x61C5A0` sets an "action-record effect"; `0x4ED220` is a "game log"; `0x4F00F0` a "state change" | `0x61C5A0(0, name, -1, -1)` stores the unit's **name** into message text slot 0; `0x4ED220(0x9F8700; x, y, key, 1)` shows the popup for the message key; `0x4F00F0(0xA268B8; U, 6, 0)` is a controller-side notification (kind 6). The death itself is `0x5BBBC0` |
| `turn.md` 2.1 step 6 | owner byte `unit +0x2C` | `unit +0x34` |
| `barbarians.md` 10 | barbarian unit behaviour might be decided in `0x5C7700` | `0x5C7700` contains no AI at all; barbarian units are moved by the AI planners (`0x446840`, `0x445EA0`, `0x449B20`) like any other civ's, **O** |
| `capture.md` row 4 | per-continent arrays at `+0x15EC` / `+0x15F0` | `+0x1608` / `+0x160C` (corrected there) |
| `government.md` 5.1 | pair record fields up to `+0x40` | `+0x44` is also written: `Unit::kill` step 6 and the citizen-kill routine `0x4BA230` add 1 to `Player[nationalitySlot].pair[owner].+0x44`; the attitude evaluation `0x440100` reads this field |

## 8. Verification status

* **V** read in full: `0x5C7700` (all 537 lines), `0x5BBBC0` (all instructions), `0x5BC8B0`, `0x5BC6D0`, `0x5BE5B0`,
  `0x5BEB10`, `0x5BEB60`, `0x5BECE0`, `0x56BE70`, `0x56D2C0`, `0x55A020`, `0x5EA6C0`, `0x476FB0`, `0x561220`, `0x56D7D0`.
* **V** read in full: `0x5BA1D0` and `0x5C7570` (`vision.md` 6 and 7).
* Not read: `0x5BD220` leave-tile path, `0x5C59B0`, `0x5B2F10`, `0x4EF740`, `0x4F00F0`, `0x4F02C0`, `0x5CD300`,
  `0x56AFB0`, `0x58B5D0`, `0x469590`, `0x46BE30`, the factory's increments.
* No dynamic test was run. Golden vectors in section 4 are hand-computed from the decoded formulas.

## 9. Open items

1. Setters of status bits `0x01` and `0x02` of `U.+0x48`; whether bit `0x01` means "rested" or "fortified".
2. (Settled: `0x56D7D0` is the tile occupant resolver, `goody-huts.md` 6.1.) The border/territory owner code `0x5D4830` is now specified in `borders-culture.md` (the loader `0x5D25F0` remains open).
3. (Settled in `vision.md`: argument semantics, radius 2 for all units, 3 for fortified or intercepting ships over their water body.)
4. The network handler that consumes the pending-event table, and the purpose of the other columns.
5. Capture-mode details (section 5.1), the byte `[0x9C5B40]`, and the nationality field `U.+0x38`.
6. Unit movement, orders and ZOC (`0x5B3040`, `0x5C1AD0`, `0x5BD220`), and the AI planners that drive barbarian
   and AI units.

## 10. Clone jungle-disease integration (2026-10-04)

`rust/src/disease.rs::unit_jungle_loss` implements the carried-unit gate and
the literal PRTO population-cost / fortified order / TERR 8 / rand(1000)
branch. Re-read `0x5C771B` and `0x5C7A9B..0x5C7B0A` for this integration.
Only a zero low-word result kills; skipped units consume no draw. No city
disease strength or cure technology enters this unit rule.

`src/naval.rs::unit_hazards` now handles this after the existing sea hazard,
using the shared gameplay RNG and the tile's effective terrain row. Carried
units skip both hazards. Despawn uses the existing cargo cleanup and
elimination systems; this does not implement every side effect of native
`Unit::kill`, nor the native pool-index walk. Deferred losses are excluded
from later boundaries delivered in the same frame.

Regression coverage includes death/survival, population-cost immunity,
fortification, carried units, terrain and owner skips, exact RNG state and
multiple boundaries after death. The rendered clone was loaded from v8
fixtures with RNG seeds 0 and 1: respectively roll 0 / death / state 12345
and roll 513 / survival / state 1103527590. Settler, Worker and the foreign
Warrior survived in both. F5/F8 preserved the outcome and RNG state.
These are clone checks, not dynamic execution of the original executable.

Verified 473 game tests, a game build, 613 native library tests, 22 native
integration tests and two doctests. Release clippy has no disease-module
warnings (ten existing library warnings, plus existing test-target warnings).
Logs: `/tmp/open4x-unit-disease-{game-tests,updated-targeted,build-now,native,clippy}.log`.
Rendered cases: `/tmp/open4x-unit-disease-{loss,survive}.json` and corresponding
`-map.png` captures. City terrain disease and persistent citizen identity
remain separate pending work (`disease.md`, `hurry.md`).
