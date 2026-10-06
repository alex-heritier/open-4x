# Movement: movement points, the per-tile effects of placing a unit, the mover and the entry of its step evaluator (clean-room specification, partial)

What is specified here: the movement-point scale and each unit's allowance, the terrain cost fields, everything
`Unit::setPosition` does when a unit leaves and enters a tile, the teleport used by air missions, and the stages, return codes and
cost accounting of the mover `0x5B8FC0`. What is **not** specified (section 10): the body of the step evaluator `0x57F360` beyond its
first guards, the path finder `0x580540`, zones of control, and the order executors. Those routines are several thousand bytes of
branching code that were read only in part, and the document says so where it matters.

Tags: **V** read from the raw disassembly (the function named was read to the addresses cited); **H** hypothesis; **O** open.
No code was run for this document; the golden values are arithmetic on the formulas.

Companion documents: `vision.md` (sight, refreshed by every placement), `stacking.md` (the per-tile unit list),
`worker-jobs.md` (the terrain fields and `Cell::moveCost`), `colonies.md` 6, `barbarians.md` 9, `goody-huts.md` 5,
`victory.md` 5.3 (the blocks of `setPosition` that are specified there), `capture.md` (`0x563410`), `air.md`, `combat.md` 14.3
(`0x5B5600`), `unit-turn.md` (the per-unit turn and order codes), `primitives.md` 3.3 (the wonder counters).

## 1. The movement-point scale

* One **full move** is `M = RULE.move_unit` (`[0x9C72C8]`; 3 in the shipped rules). Every movement quantity below is an integer in these
  units. A unit's **used** movement this turn is `U.+0x50`; the per-unit turn epilogue resets it (`unit-turn.md` 3.7).
* Remaining movement is `clamp(maxMove(U) - U.+0x50, 0, 9999)` (**V**, `0x5B9DE3..0x5B9DFE` for the clamp). The gate idiom of the order
  code is `remaining > 0` (`unit-upgrades.md` 3, `ai.md` "Unit action gate").
* `U.+0x54` and `U.+0x58` are the progress and the layer of a multi-turn job in progress (`worker-jobs.md` 6); `setPosition` sets them to
  `0` and `-1` (section 5, step 3).

## 2. A prototype's allowance `protoMoves(prto, owner)` `0x5CDDF0` (cdecl) **V** (read in full)

```
m = PRTO.movement * M                                   // PRTO memory +0x70
if owner > 0 and PRTO.unit_class == SEA (1):            // unit_class at memory +0x9C: 0 land, 1 sea, 2 air
    if owner.countWonders(mask 8)      > 0:  m += M     // 0x55A8D0: a wonder with BLDG wonder_flags bit 3 ("+1 ship movement")
    if owner.countWonders(mask 0x4000) > 0:  m += 2*M   // bit 14 ("+2 ship movement")
    if RACE[owner.race].hasTrait(7 = Seafaring):  m += M        // RACE table [0x9C71D0], row = Player +0x20, stride 0x974
return m
```

`countWonders` (`primitives.md` 3.3) counts wonders that are active (not obsolete), under the player's government gate, and held by one of
the player's cities, so each of the two wonder bonuses is a single flat addition however many wonders carry the flag. All three additions
apply to sea units only; land and air units get `PRTO.movement * M` and nothing else.

## 3. A unit's allowance `maxMove(U)` `0x5BE470` (`ret` plain, `ecx` = unit) **V** (read in full)

```
if U's own prototype has ability 18 (Army):
    m = -1
    if U.tile is on the map:
        for each unit v on U's tile with v.+0x60 == U.id:        // the army's members
            mv = maxMove(v);  m = (m == -1) ? mv : min(m, mv)
    m += M
    if m != -1:  return m
return protoMoves(PRTO[U.type], U.owner)
```

An Army therefore moves at the speed of its **slowest member plus one full move**. An Army with no members (or off the map) returns
`M - 1`, a quirk that cannot occur in play because an empty army is destroyed (**H**). Golden (`M = 3`): members of 1 move (`3`) and 2 moves
(`6`) give `3 + 3 = 6`; a lone Knight with `movement 2` gives `6`; a Galleon with `movement 3` under a Seafaring nation with one active
"+1 ship movement" wonder gives `(3 + 1 + 1) * 3 = 15`.

## 4. Terrain movement cost

`Cell::moveCost` `0x5DBF60` (`worker-jobs.md` 1.3): if the cell's landmark flag (vtable `+0x78`) is set, the TERR row field `+0xA0`, else
`+0x5C`; the row is `[0x9C7328] + 0xF0 * terrainId` (`cell.vt+0xC8`). Shipped values per terrain id (from `worker-jobs.md` 1.3):

| id | 0 Desert | 1 Plains | 2 Grass | 3 Tundra | 4 Flood Plain | 5 Hills | 6 Mountains | 7 Forest | 8 Jungle | 9 Marsh | 10 Volcano | 11 Coast | 12 Sea | 13 Ocean |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| move cost | 1 | 1 | 1 | 1 | 1 | 2 | 3 | 2 | 3 | 2 | 3 | 1 | 1 | 1 |

Normal road and river costs are decoded in section 14. Railroad and special
prototype costs remain outside the implemented subset (section 9, item 1).
The only consumer documented so far is the worker's job length (`worker-jobs.md` 7).

## 5. `Unit::setPosition(U; x, y)` `0x5BD220` (`ret 8`) **V** (the whole routine was read; the blocks that other documents specify are cross-referenced)

The routine places a unit, or removes it when `(x, y)` is off the map (callers pass `-1, -1`). It is the only writer of the per-tile
unit list (`stacking.md`) and of the previous-tile fields. Addresses are those of the first and last instruction of each step.

```
 1  if (U.x, U.y) == (x, y): return                                                           // 0x5BD231..0x5BD239 (jumps to the epilogue 0x5BE460: no sight refresh)
 2  if (x, y) is off the map (0x426BD0): refreshSight(U; 1, 0, 1)                             // 0x5BD246..0x5BD257  vision.md 6: unmark the old position
 3  U.prev := U.tile;  U.tile := (x, y);  U.+0x54 := 0;  U.+0x58 := -1                       // 0x5BD25C..0x5BD27D
 4  UI only: the animation-path object at U+0x27C receives (x, y) four times (0x403A50 twice, 0x403A70 twice) when
    (U.+0x38D != 0 and not multiplayer 0x47B530) or (multiplayer and U.id is not in the table [0x74AF60 + 0x44268 .. 0x74AF60 + 0x4C268) stride 4)   // 0x5BD265..0x5BD2FE
 5  UI only: if U has ability 18 (Army) and U.+0x1D0 names a live unit, that unit's +0x280 object gets 0x403CC0(1)     // 0x5BD304..0x5BD344
 6  LEAVE the old tile, if U.prev is on the map:                                              // 0x5BD349..0x5BD65B
      a  unlink U's node from the old tile's unit list and return it to the link pool         // 0x5BD35D..0x5BD527  stacking.md
      b  if the old tile's "worked by" word (cell +0x6C) names a live city K and Player[K.owner].isHostileTo(U) (5.1):  K.vt+0x38(0)   // 0x5BD41F..0x5BD4AE
      c  cap = PRTO[U.type].transport_capacity (memory +0x50); if U has ability 18 and
         Player[U.owner].countSmallFlagWonders(mask 4, 0) > 0 (0x55AA10, Larger Armies): cap += 1                    // 0x5BD4B1..0x5BD4F8, 0x5BD52C..0x5BD53E
      d  if cap > 0: for every unit c of the unit pool (index 0 .. last) with c.+0x60 == U.id:
             if c.+0x64 != 15: c.setOrder(1)  (0x5B3040)
             Unit::setPosition(c; x, y)                                                       // 0x5BD54C..0x5BD5A6  (the cargo follows; recursion)
      e  if U.+0x60 != -1 and the unit with that id (its carrier) exists and (U.x, U.y) != (carrier.x, carrier.y):
             U.0x5C59B0(-1, -1)                                                               // 0x5BD5A8..0x5BD5E5  (H: U leaves its carrier)
      f  if attack(U) > 0 (0x5BE6E0) or defense(U) > 0 (0x5BE820), and GOVT[Player[U.owner].+0xA0].+0x1AC > 0 (military_police_limit):
             K = cityAt(U.prev) (0x56D2C0); if K: 0x4BCFF0(K) (happiness recompute), then K.vt+0x38(0)    // 0x5BD5EC..0x5BD658
 7  if the new tile is off the map: return                                                    // 0x5BD668..0x5BD66F  (jump to 0x5BE460)
 8  ENTER the new tile:
      a  push a node for U on the front of the new tile's list                                // 0x5BD67A..0x5BD763  stacking.md
      b  if the new tile's "worked by" word names a live city K and Player[K.owner].isHostileTo(U):
             n = 0x5F3F50(map; K.x, K.y, U.x, U.y, 21)        // index of the tile inside K's 21-tile area (K.x, K.y are the words at K +0x24, +0x26)
             K.0x4BBC80(n)                                     // K stops working tile n (n == 0, the city tile, is a no-op that returns 1)
             K.vt+0x38(0)                                                                     // 0x5BD78B..0x5BD84A
      c  martial law, entering: the test of 6f with K = cityAt(U.tile)                        // 0x5BD84D..0x5BD8BB
      d  colonies, airfields, radar towers, outposts of the new tile                          // 0x5BD8C3..0x5BDD6A  colonies.md 6, barbarians.md 9
      e  barbarian camp (cell.vt+0x1C) and U.owner != 0                                       // 0x5BDD8F..0x5BDE38  barbarians.md 9 (the camp is collected)
      f  goody hut (cell.vt+0x3C) and U.owner != 0: Player[U.owner].0x55C6B0(x, y, U)          // 0x5BDE58..0x5BDE90  goody-huts.md 5
      g  flag modes, only when U.+0x1EC != 0xFF (U carries a flag; +0x1EC = flag owner's slot)  // 0x5BDE95..0x5BE443  victory.md 5.3
         (U.+0x1EC := 0xFF and U.+0x1EA := 0xFFFF are written only after a delivery, at 0x5BE443/0x5BE44A)
 9  refreshSight(U; 0, 1, 1)                                                                  // 0x5BE453..0x5BE45B  vision.md 6: mark the new position
```

### 5.1 "Hostile to U"

`Player::isHostileTo(P; U)` `0x558F70` (`ret 4`) **V**: `owner = U.owner`, except when U has ability 17 (Hidden Nationality) and `P.slot` is a real
civilization (not `-1`, not `0`) different from `U.owner`, in which case `owner := 0` (the barbarian slot). The result is the byte
`P.+0xD30[owner]` (the at-war flag against `owner`, `diplomacy.md` 3). So a unit with Hidden Nationality counts as a barbarian for every
other civilization's worked-tile test while it moves.

### 5.2 What the city effects mean

An at-war unit standing on a tile that a city works takes the tile away from the city: on entry the city releases the tile
(`0x4BBC80`, `city-founding.md`) and re-plans through its vtable `+0x38` (**H** for the name; it takes one boolean argument, always `0` here);
when the enemy leaves, the city re-plans again (that method alone does not reassign the tile). Martial-law units (`happiness.md`) that
step on or off a city tile make the city recompute its moods (`0x4BCFF0`).

### 5.3 Quirks worth reproducing

1. A call with the unit already on `(x, y)` does nothing: no sight refresh, no list change.
2. Cargo recursion (step 6d) runs inside the carrier's *leave* block, before the carrier is inserted on the new tile (step 8a); the cargo reaches
   the new tile, and triggers huts, colonies and camps, before the carrier does.
3. Cargo that is not on interception (order 15) is set to order 1 (sentry) every time its carrier moves.
4. The hostility test of 6b/8b treats Hidden-Nationality units as barbarians only for that test; combat and diplomacy use the real owner.
5. The leave block 6f requires the *moving* unit to have attack or defense above 0 and the owner's government a martial-law limit above 0, but
   not that the city is its own; any city on the tile is recomputed.

## 6. Teleport `Unit::teleport(U; x, y)` `0x5C5040` (`ret 8`) **V** (read in full)

Used by the air missions (`air.md`). It first marks the **source** tile: if there is a city at `(U.x, U.y)` (`0x56D2C0`), `city.+0x30 |= 4`; else if
that tile has an airfield (`cell.vt+0x18(0)`), the airfield record (pool `[0xA52E3C]`, index `cell.vt+0xBC`) gets byte `+0x30 := 1`
(readers of both marks: **O**; `colonies.md` open item 2). Then `setPosition(U; x, y)` (section 5) and `U.+0x50 := maxMove(U)`: the unit has no
movement left.

## 7. The mover `Unit::tryMove(U; dir, flagB, a3, a4)` `0x5B8FC0` (`ret 0x10`)

4 176 bytes. `dir` is a spiral index `1..8` (`vision.md` 1.1); `flagB` is a byte; `a3` (stack `+0x54` at the body, **O**) and `a4` (byte, stack `+0x58`) are used
below. Stages, in code order (all **V** except where tagged):

**Stage 0, guards** (`0x5B8FC0..0x5B9079`):

```
if flagB != 0 and byte[0xA52B6C] == 0:                     return 0
if multiplayer (0x47B530 = [0x9AFD74] in {4, 5}) and U.+0x22C != 0:  return 10      // busy with a pending network action
v = passengerType(U)  (0x5BC6D0, vision.md 2)
if PRTO[U.type] has ability 10 (Immobile) or (v != -1 and PRTO[v] has ability 10):    return 1
if dir == 0:                                               return 1
```

**Stage 1, the destination** (`0x5B9081..0x5B91BF`): `(dx, dy) = 0x5E6E50(dir)`; `(tx, ty)` = wrap of `(U.x + dx, U.y + dy)` by `0x426C00` / `0x426C40`
(`vision.md` 1.1); if `(tx, ty)` is off the map (`0x426BD0`): skip to the epilogue (nothing moves). Locals computed: `A = 0x56D630(tx, ty, U.owner, 0)` (owner of the first
non-barbarian unit on the tile, `-1` if none, `victory.md` 2), `B = 0x56D7D0(tx, ty, U.owner, 0)` (the tile's occupant, `goody-huts.md` 6.1: city owner, else unit or colony owner,
`0` camp or barbarian, `-1` empty), `C = 0x56D2C0(tx, ty)` (city on the destination), `srcWater` and `dstWater` (`cell.vt+0x8C` of the two tiles).

**Stage 2, domain transitions** (`0x5B91C3..0x5B923D`):

* destination is land, `C` absent and `PRTO.unit_class == 1` (a ship): `return 0x5C5420(U; tx, ty)` (**V**, passenger disembark dialog, section 9.2; the ship does not enter the land tile);
* destination is water and `PRTO.unit_class == 0` (a land unit): `carrier = 0x5C5F70(U; tx, ty, f)` with `f = 1` when not multiplayer and `a4 == 0`, else `0`
  (`air.md`: picks the transport on the water tile and shows the SELECT_TRANSPORT dialog to the local human when several qualify); a result of `0` ends the call with
  nothing moved; the chosen carrier is remembered for stage 7;
* otherwise nothing happens at this stage.

**Stage 3, occupant of the destination** (`0x5B9243..0x5B9416`):

```
if A != -1 and A != U.owner:
    if allianceNo(U.owner) != 0 and allianceNo(U.owner) == allianceNo(A):  return 10        // 0x5E23C0(0x9C40D0; race) = team number, victory.md 1.3 and 6.2
    r = 0x5B61E0(U; tx, ty, B, dir)                                                              // the attack on the tile (combat.md)
    attackFlag = 1                                                                                // byte local [esp+0x13]
    if multiplayer: doneFlag = 1                                                                  // byte local [esp+0x12]
    if r == 3:  U.+0x50 += M;  goto the boarding tail of stage 7 (0x5B9FB3)                          // no step follows
    if r != 2:  goto epilogue (nothing moves)                                                     // r == 2: the defender is gone, the unit advances (goto stage 4)
elif B != -1 and B != U.owner:                                                                     // a foreign occupant that is not a unit stack: city, colony, camp
    same alliance test on B: return 10
    0x5B5600(U; B, tileOwner(tx, ty) == B)                                                        // "provoke" (combat.md 14.3); the move continues
```

**Stage 4, the cost** (`0x5B9416..0x5B94E9`):

```
if srcWater == dstWater:  0x4A76B0(0x9C7348; U, ..., tx, ty)                          // UI (H): the move animation
cost = stepCost(0xB72888; U.x, U.y, tx, ty, U, U.owner, 0x80000080, dir, 0)            // 0x57F360, section 8
free = (cost == 0)                                                                      // byte local [esp+0x11]
if cost == -1:
    if 0x5B5CD0(U; dir, 0) != 0:  return 1                                              // O: the step is refused
    cost = maxMove(U)                                                                   // else the step is made at the price of all movement
if attackFlag and cost <= M:   cost = M
if flagB != 0 and cost == 0:   cost = 1
doneFlag = 1
U.+0x50 += cost
if srcWater and not dstWater and PRTO.unit_class == 0 (land):  U.+0x50 = maxMove(U)      // landing from a ship uses up the turn
```

**Stage 5, presentation** (`0x5B94EC..0x5B9D49`): `0x5BB650(U; localHuman, 1)` (visible to the local human) gates a long block of UI work (the map scroll `0x4A8350`, sound, the move animation
through `0x5392D0`, sentry-wake calls `0x4F0CB0`, multiplayer sync `0x4DF6B0`/`0x4EEB20`); its only gameplay-relevant call is `0x5BAE60` (**O**, a state clear at the unit when it
is "waking" from fortify/sentry). Not specified.

**Stage 6, the city capture** (`0x5B9D49..0x5B9D93`): `K = cityAt(tx, ty)`; if `K` exists and `K.owner != U.owner` (byte at `K +0x28`): `ok = Player[U.owner].0x563410(K, U, capture = 1, convert = 0)`
(`capture.md` 2; for a barbarian owner the routine takes the raid path); if `ok`, `return 0` (the capture moved the unit; nothing else happens).

**Stage 7, the commit** (`0x5B9D96..0x5B9DA3`): `Unit::setPosition(U; tx, ty)` (section 5). After it (`0x5B9DA5..0x5B9F7F`): the human-owner "out of moves" message block
(**O**, UI), then the boarding tail (`0x5B9FB3..0x5B9FEC`): if a carrier was chosen at stage 2: `U.+0x60 := carrier.id`, `U.setOrder(1)`, and if the carrier's prototype has
ability 18 (Army), `carrier.0x5BCC90(U)` (**H**: joins the army).

**Epilogue** (`0x5B9FF1`): returns `1` when the byte local `doneFlag` is `0`, else `0`. So the **return value is `0` when a step or an attack was paid for and `1` when nothing
was**; the explicit exits above return `0` (flag filter), `1` (immobile, no direction, refused step), `10` (busy or allied destination), or the result of `0x5C5420`.
Callers read `10` as "refused for a diplomatic reason" (`unit-turn.md` 3.8).

Not decoded: the interior of stage 5, the messages, `0x5B61E0` (combat, `combat.md`). Selected transport and refusal clauses are now specified in section 9; the rest of `0x5B5CD0` and `0x5C5420` remains open.

## 8. The step evaluator `stepCost(this = 0xB72888; fx, fy, tx, ty, U, owner, flags, dir, a9)` `0x57F360` (`ret 0x24`): interface and entry guards **V**, body **O**

`this+4` is the map's doubled width (`idx = this.+4 * y + x`). Return: `-1` blocked, `0` free, or a cost in movement points. Verified guards in order:

1. `flags & 1` is kept in a byte local. Any of `fx, tx` outside `0 <= x < [0x9C74D4]`, `fy, ty` outside `0 <= y < [0x9C74C0]`, or `dir` outside `1..8` -> `-1` (`0x57F3AC..0x57F415`, exit `0x57FE05`).
2. Adjacency: `0x441ED0(...) + 0x437970(...) == 1` is required, else `-1` (`0x57F422..0x57F44A`); the arguments of the two distance helpers were not traced (**O**).
3. `U == 0` (a pure query): the branch at `0x57F881` (**O**). `flags & 1` set: the branch at `0x57FA3E` (**O**).
4. `U` given and `flags & 1` clear, `U.owner` a human (`[0xA526BC]` mask) and byte `U.+0x68 == 0` (`0x57F4A0..0x57F5FF`):
   * if any of `cell(to).+0x5C | +0x60 | +0x64 | +0xD0` has the owner's bit (the tile is visible now): `occ = 0x56D7D0(tx, ty, owner, 1)`; `occ >= 0 and occ != owner` -> `-1` (an enemy stack blocks the plain move; attacks go through the mover's stage 3);
   * else if the owner has **discovered** the tile (`cell(to).+0x58` bit of `Player.+0x1C`): a remembered foreign city (`0x56D770`), a remembered foreign colony or unit owner (`0x56D7A0`) other than the owner -> `-1`; a camp (`cell.vt+0x1C(owner)`) -> `-1`;
   * not discovered: no restriction from this block.
5. Otherwise (AI-controlled owner, or `U.+0x68 != 0`) (`0x57F61E..0x57F68C`): `occ = 0x56D7D0(tx, ty, owner, 1)`; for a foreign occupant the result is `-1` unless flag bit 31 is set **and** `U.vt+0x3C(occ, 0)` (H: may attack) is true,
   `attack(U) != 0`, and `0x5A6060(...; 6, owner, ...)` returns 0 (**O**, not identified).
6. The remainder (domain rules at `0x57F692..`, terrain and treaty tests, the cost proper; the success exit tail-calls `0x5CCBB0` at `0x57FE12`) was not read.

## 9. Transport selection, loading and shore attack gates

### 9.1 Carrier selection `0x5C5F70(U; x, y, dialog)` **V**

Read `0x5C5F70..0x5C6283`. It enumerates the destination tile's unit list,
requires the candidate's owner equal U's owner (`0x5C6030..0x5C6036`), and
calls `candidate.0x5C5BD0(U)`. With `dialog == 0`, it returns the first eligible
candidate (`0x5C625F`). With dialog enabled, a local human gets a
SELECT_TRANSPORT list when several qualify; one candidate returns directly
(`0x5C6209..0x5C6279`). No result returns a null pointer. It does not spend
movement or assign the carrier link.

`carrier.0x5C5BD0(passenger)` (`0x5C5BD0..0x5C5F3E`, **V**) rejects self,
no free capacity (`capacity(carrier) - countCargo(carrier) <= 0`), and a carrier
already carried by another unit. Effective capacity is `0x437F20`; cargo count
is `0x5BE9E0`. For ordinary ships, the passenger's domain must equal 2 when
the carrier has Only Aircraft ability 8, otherwise 0 (`0x5C5D30..0x5C5DA9`).
Only Foot Units ability 14 additionally requires a land passenger with Foot
Unit ability 1 (`0x5C5E14..0x5C5E82`). Only Tactical Missiles ability 24
requires Tactical Missile ability 23 (`0x5C5E88..0x5C5F10`); a Tactical Missile
passenger requires an Only Tactical Missiles carrier (`0x5C5F12..0x5C5F2A`).
Army handling adds capacity/weight and member restrictions; these are outside
the ordinary ship rules summarized here. The eligibility routine itself does
not test the passenger's Load command bit.

### 9.2 Explicit Load and same-tile Unload **V**

The command eligibility routine `0x5C1AD0` first requires positive remaining
movement and the requested command bit (`0x5C1AD0..0x5C1B2B`). Load uses
command mask `0x10000001`, Unload `0x10000002`. The Load eligibility case
queries `0x5C5F70` on the unit's current tile without a dialog (`0x5C1BA3`).
Unload counts cargo and refuses a water tile or an Army (`0x5C1BBB..0x5C1C5A`);
thus the explicit Unload command is a **port** command, not a water command.

The UI's Load branch `0x4D8AE0..0x4D8B77` checks that gate, calls the selector
with dialog enabled, and in single-player calls `U.0x5C5110(carrier)`.
The whole latter routine (`0x5C5110..0x5C514E`) sets `U.+0x60 := carrier.id`
and order 1, with an additional Army-member call for an Army. It never
changes `U.+0x50`: explicit loading preserves movement.

The UI's Unload branch (`0x4DAC7D..0x4DAC9B`) checks the gate and calls
`0x5C5420(ship; ship.x, ship.y)`. That routine offers a DISEMBARK passenger
list to the local human and calls `0x5C59B0` on the chosen passenger. The
same-tile path of the latter (`0x5C59B0..0x5C5AD2`, read in full) clears
`passenger.+0x60 := -1`, sets order 0 and calls the position method at its
unchanged coordinates (`0x5C5A8C..0x5C5AA8`). This path does not charge
movement. Its different-tile path invokes the mover, which pays the landing
cost.

The ship mover calls this same routine for a land destination without a
city (`0x5B91C5..0x5B91EC`), before ordinary movement costs or combat.
The carrier stays on its source tile. The DISEMBARK template in
`Conquests/Text/script.txt:1693..1698` has fixed choices "Never mind." and
"Unload all."; passenger rows are appended with value `unit.id + 2`
(`0x5C575A..0x5C576C`). Return 0 cancels (`0x5C57C8`), return 1 enters the
all-passenger loop (`0x5C57D0..0x5C5835`), and higher values select one unit
and call `0x5C59B0(destination)` (`0x5C57D5..0x5C5821`). The all-passenger
loop is also the AI path, calling that same mover for units linked to this
carrier (`0x5C58ED..0x5C5924`).

Different-tile passenger moves require positive remaining movement
(`0x5C5A13..0x5C5A1C`). Passenger terrain and foreign-occupant checks occur
while constructing the human list (`0x5C54E2..0x5C5578`) and again in the
passenger routine (`0x5C59D7..0x5C5A45`). The full interiors of those
terrain helpers and multiplayer details remain open.

### 9.3 Foreign destination from water **V** for these refusal clauses

`0x5B5CD0` checks remaining movement (`0x5B5D5C..0x5B5D89`), then for a
foreign stack and a land attacker on water (`0x5B5D8C..0x5B5E22`) requires:
Amphibious ability 6, attack strength `0x5BE6E0(U) > 0`, and either status
bit 2 clear (`U.+0x48 & 4 == 0`) or Blitz ability 2. Failure returns refusal
code 5 at `0x5B5E13`. The foreign non-stack occupant clause uses the same
ability/attack/status tests (`0x5B5FE1..0x5B603C`).

The cargo destination predicate `0x5C5160` repeats those conditions for
foreign destinations (`0x5C5380..0x5C53C5`). Source **terrain**, rather than
just a carrier link, controls the water restriction: cargo in a port is on
land and may leave to attack. A successful different-tile placement detaches
cargo (`section 5`, `0x5BD5A8..0x5BD5E5`); a land unit leaving water spends its
remaining movement (`section 7`, mover stage 4).

`0x5B64C0` contains similar terrain/amphibious tests but finishes with PRTO
stealth-target list membership. Its caller `0x5B6820` is the assassin selector;
this is not the general attack-refusal gate.

## 10. Open items (what a port still has to read)

1. The body of `0x57F360` after section 8 item 5: remaining domain legality, railroad and special `PRTO` costs, border and treaty restrictions, and the ZOC test. The land-terrain arm of `0x5CCBB0` is specified in section 13 and normal road/river costs in section 14; `0x449810` remains the foreign-tile query described in section 11.
2. The interior of `0x5B8FC0` stage 5, and the remaining clauses of `0x5B5CD0` and `0x5C5420`, plus `0x5BAE60` and `0x5BCC90`. Section 9 specifies carrier selection and same-tile Load/Unload.
3. The path finder `0x580540` (3 568 bytes) and its callers; the order executors `0x461F90`, `0x4620D0`, `0x4622D0`, `0x462670` (go-to-and-build orders) and the AI worker automation, whose chooser was not located.
4. The city method `vt+0x38(0)` called from `setPosition` (H: refresh of the city's tile assignment).
5. The unit fields `+0x1D0`, `+0x22C`, `+0x280`, `+0x38D`, `+0x68`.
6. The readers of the teleport marks (`city.+0x30 & 4`, airfield `+0x30`).

## 11. Zone of control as implemented in the clone (HYPOTHESIS)

The executable's ZOC reader was not decoded: `0x449810` is a "find a foreign tile within a radius on the same continent" query called from the step evaluator `0x57F360`, and is not confirmed to be ZOC. `PRTO.zone_of_control` is the first body dword (memory row `+4`, stride `0x138`) and is true on 16 shipped rows. `src/zoc.rs` implements the manual's rule:

- A unit exerts a zone when its prototype has the flag, it is a land unit (class 0), and it is not carried.
- A land unit may not step from a tile to a tile when both lie within distance 1 of a zone unit of a civ it is at war with, unless the destination holds a friendly unit or city, or the step boards a ship.
- A refused step clears the path and any exploring order.

Verify against the executable before treating any of this as exact.

## 12. Transport choice (select-transport dialog)

When a human unit boards or loads and several friendly carriers with room share the tile, the game asks which (dialog `SELECT_TRANSPORT`, called from `0x5C5F70`). One candidate loads without asking; the computer takes the first. The clone lists carriers in entity order as "Name (aboard/capacity)" and stores the question as `Diplomacy.board_ask`; the Transport screen in `advisors.rs` answers it. The executable's candidate ordering was not read.

## 13. Land terrain legality and wheeled road exception (**V**)

`Unit::canEnter` at `0x5CCBB0` handles domains and embarkation before its
land-terrain arm at `0x5CCE9E`. The destination effective TERR row is resolved
through cell vtable +0xC8, with native TERR stride 0xF0.

- `0x5CCF2B` reads TERR +0x7A (impassable). If true, check the road exception.
- `0x5CCF35..0x5CCF87` checks prototype ability 0 (Wheeled) and TERR +0x7B
  (impassable to wheeled units). An unrestricted unit/tile pair returns 0,
  meaning entry allowed.
- A restricted pair calls the destination road predicate at `0x5CCFB1` and
  origin road predicate at `0x5CCFE4`. **Both tiles require roads**. If either
  lacks one, return 1 (blocked). This exception also overrides the generic
  impassable byte, although all stock Conquests rows have that byte clear.

The ability check uses the unit's own prototype first, then the shared Army
prototype from `0x5BC6D0`. That helper returns -1 for non-Armies, empty Armies,
or mixed prototype members (`0x5BC7D3`, mismatch return `0x5BC81A`); homogeneous
Armies return the common member row (`0x5BC810`). Consequently a homogeneous
Chariot Army inherits Wheeled, but a mixed Chariot/Warrior Army does not.
Experience levels do not affect prototype equality.

Stock TERR restricts Mountains, Jungle, Marsh and Volcano to wheeled units;
Mountains and Jungle cost 3 whole MP, Forest and Hills 2. The game's map
currently represents Mountains and Jungle, but not Marsh or Volcano. Artificial
Ice remains outside the native land table and is impassable in the clone.

`rust/src/movement.rs` implements the terrain-only gate, with golden vectors
for road combinations and both restriction bytes. `src/units.rs` uses this
before costs for route planning and actual movement; combat orders, retreats
and AI military routes use the same gate. This does not claim the full native
step evaluator, pathfinder, retreat direction choice or treaty gates are decoded.


## 14. Normal road costs, river crossings and bridges (**V**)

`0x57F360` reaches the cost evaluator `0x580070` after successful entry
legality (`0x57F889`). Its normal land-road arm checks roads at both ends
through cell vtable +0x64 (`0x5801BB..0x5801D9`). The source cell's river
mask, vtable +0x94, is tested in the step direction (`0x5801DF..0x580201`).
A crossing calls `Player::hasTechFlag(4)` at `0x58021D` through `0x561480`.
Stock TECH flag 4 is Enables Bridges; Engineering (row 23) is its sole row.

Both roads and either no crossing or known bridges return cost 1 at
`0x580286`, one third of a movement point. An unbridged crossing falls
through `0x580290` to the normal terrain branch: `Cell::movementCost`
`0x5DBF60` at `0x580381`, multiplied by RULE movement thirds at `0x580388`.
Thus it costs destination terrain movement, rather than universally using
all remaining movement. Bridges belong to the moving civilization.

`rust/src/movement.rs::land_step_cost` implements this subset with golden
vectors. Game movement, unit routes, AI civilian/land approach routes,
undefended city capture and route turn previews share it. Previews now use
the actual unit allowance, domain and owner technology. Generated
`rules_data::BRIDGES` comes from TECH flags. Railroad, special prototype
movement, foreign-road/treaty gates and ZOC remain outside this subset.

Rendered seed-1 Egyptian Chariot fixtures cross from (39,29) to (39,28),
with roads on both tiles and source river mask 135. Before Engineering,
remaining movement is 3 thirds; afterward it is 5 thirds. The latter fixture
grants row 23 to native research slot 1 (game civ 0), not slot 0, which is
reserved for barbarians. F5/F8 preserves technology and remaining moves.
Inspected `/tmp/open4x-bridge-{before,after}-map.png` shows the road, river,
Chariot and corresponding 1 / 1 2/3 movement readouts after load.
