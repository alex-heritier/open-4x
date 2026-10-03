# Vision: sight sources, the line-of-sight mask, the unit sight refresh (clean-room specification)

What a civilization sees at any moment, and how the engine keeps that set up to date as units move, die, fortify and
change owner. Everything here is the behaviour of `Civ3Conquests.exe`; nothing depends on a reference implementation.

Tags: **V** read from the raw disassembly and cross-checked; **E** executed: the real routine was run inside the
emulator (`tools/emu/`) on four shipped maps and on randomised terrain, and the specification below was compared with
the result bit for bit (counts in section 4.4); **H** hypothesis; **O** open. The scratch harnesses are not committed.

Companion documents: `borders-culture.md` (tile geometry, the neighbour step, the owner write that grants territory
sight), `colonies.md` (the structure sources, `discover`, the remembered overlay), `research.md` 10.3 (`discover`),
`unit-turn.md` (the unit record, the pending reveal), `air.md` (missions).

## 1. Data

### 1.1 Tile geometry (shared with every other document)

* Tiles use the doubled grid: `(x, y)` is a real tile when `x + y` is even. Cell index `= ((W >> 1) * y + (x >> 1)) & 0xFFFF`
  with `W = [0x9C74D4]`; the cell is `0x5D16A0(map = 0x9C736C; index)` (`ret 4`), which returns the dummy cell `0xCAA330`
  for an out-of-range index. `0x437A70(x, y)` is the same lookup from coordinates. Map extents: `map +0x168` = width,
  `map +0x154` = height; wrap flags `map +0x1F0` (bit 0 wrap in x, bit 1 wrap in y; it is also the byte `[0x9C755C]`).
* The **spiral** `0x5E6E50(n; &dx, &dy)` gives the offset of the n-th tile around a centre; closed form in
  `borders-culture.md` 4. The first 49 entries, which are all that vision uses:

  | n | offset | n | offset | n | offset | n | offset |
  |---|---|---|---|---|---|---|---|
  | 0 | (0, 0) | 9 | (1, -3) | 25 | (1, -5) | 37 | (-3, 3) |
  | 1 | (1, -1) | 10 | (2, -2) | 26 | (2, -4) | 38 | (-4, 2) |
  | 2 | (2, 0) | 11 | (3, -1) | 27 | (3, -3) | 39 | (-5, 1) |
  | 3 | (1, 1) | 12 | (3, 1) | 28 | (4, -2) | 40 | (-5, -1) |
  | 4 | (0, 2) | 13 | (2, 2) | 29 | (5, -1) | 41 | (-4, -2) |
  | 5 | (-1, 1) | 14 | (1, 3) | 30 | (5, 1) | 42 | (-3, -3) |
  | 6 | (-2, 0) | 15 | (-1, 3) | 31 | (4, 2) | 43 | (-2, -4) |
  | 7 | (-1, -1) | 16 | (-2, 2) | 32 | (3, 3) | 44 | (-1, -5) |
  | 8 | (0, -2) | 17 | (-3, 1) | 33 | (2, 4) | 45 | (0, -6) |
  | | | 18 | (-3, -1) | 34 | (1, 5) | 46 | (6, 0) |
  | | | 19 | (-2, -2) | 35 | (-1, 5) | 47 | (0, 6) |
  | | | 20 | (-1, -3) | 36 | (-2, 4) | 48 | (-6, 0) |
  | | | 21 | (0, -4) | | | | |
  | | | 22 | (4, 0) | | | | |
  | | | 23 | (0, 4) | | | | |
  | | | 24 | (-4, 0) | | | | |

  (**E**: the table was produced by calling the routine for `n = 0..80`.) `n < 9` is "the tile and its 8 neighbours"
  (ring 1); `9 <= n < 25` is ring 2; `25 <= n < 49` is ring 3 (a 7 x 7 diamond is the first 49 entries).
* `0x5E6D20(dx, dy, limit)` (cdecl, plain `ret`) is the inverse: the first `n` in `0 .. limit-1` whose offset equals
  `(dx, dy)`, else `-1`. Linear search (**V**; **E** round trip for `n = 0..80`).
* **Neighbour step** (`borders-culture.md` header): `nx = x + dx`; with wrap-x, `nx += W` when `nx < 0`, `nx -= W` when
  `nx >= W`; the same for `y`; the tile exists only when `0 <= nx < W` and `0 <= ny < H`. One fold only. The helpers
  are `0x426C00(x)` / `0x426C40(y)` (fold when the wrap bit is set; `ret 4`) and `0x426BD0(x, y)` (in bounds, `ret 8`).
* **Tile distance** `0x44A8D0(map; x1, y1, x2, y2)` (`ret 0x10`): `dx = |x1 - x2|`, and when wrap-x is set and
  `dx > W / 2` (integer division, `W = map +0x168`) then `dx = W - dx`; the same for `dy` with wrap-y (bit 1 of `map +0x1F0`)
  and `H = map +0x154`; the result is `(dx + dy) / 2` (the sum is never negative; **V**, `0x44A8D0..0x44A948`).
  One step in any of the eight directions has distance 1.
* The cell accessor `0x5D16A0(map; index)` (`ret 4`) returns `cellArray[index]` (`map +0x148`) when the array exists and
  `0 <= index < word map +0x40`, else the shared dummy cell `0xCAA330` (**V**).

### 1.2 The per-cell sight masks

Each mask is a dword whose bit `s` belongs to the civilization in slot `s` (`Player +0x1C`). The cell is the object
returned by `0x5D16A0`; its vtable is `0x6701C8`.

| offset | meaning | set (and `discover`) | clear (and remember overlay) | source |
|---|---|---|---|---|
| `+0x58` | **discovered** (ever seen) | `0x55B1A0` | never | all |
| `+0x5C` | seen by a **unit** | `0x55AD90(P; x, y)` | `0x55AB70(P; x, y)` | section 6 |
| `+0x60` | seen by a **structure**: plain Colony, Airfield, Radar Tower, Outpost | `0x55AE90` | `0x55ADF0` | `colonies.md` 5 |
| `+0x64` | seen through **territory**: the 3 x 3 block around every tile the civ owns | `0x55B030` | `0x55AF90` | the owner write `0x5D3AB0` (`borders-culture.md` 6) |
| `+0xD0` | seen by an air **reveal** (the recon mission) | `0x55B140` | `0x55B090` | section 7 |
| `+0xD4` | covered by a **radar** tower | `0x55AEF0` | `0x55AF40` | `colonies.md` 5 (readers **O**) |
| `+0x68` | **line-of-sight mask** of this tile: 25 bits, section 4 | `0x5D6500` | (zeroed on entry) | map creation / load |
| `+0xAE + slot` | per-player **remembered overlay** byte | `discover` (and every clear primitive) | | |

All the primitives are `Player` methods (`this` = the player record `0xA52E98 + 0x20E4 * slot`), `ret 8`, arguments
`(x, y)` (raw coordinates, converted to the cell inside). (**V**: bodies of `0x55AD90`, `0x55AB70` read in full for this
document; `0x55AE90` / `0x55ADF0` / `0x55AEF0` / `0x55AF40` as in `colonies.md` 5 and `combat.md`; `0x55B030` / `0x55AF90` as in
`borders-culture.md` 6; `0x55B140` / `0x55B090` as used in section 7. `workers.md` calls `0x55AD90` a cell-index helper: that
is wrong, it is the unit-sight set primitive.) The two directions (clear, then redraw order) differ slightly per primitive;
section 1.2's clear description is the one read from `0x55AB70`.

* **Set** primitive: `cell.mask |= 1 << P.slot`, then `P.discover(x, y)` `0x55B1A0` (always, even when the bit was
  already set; `discover` itself only acts when the `+0x58` bit is clear: it sets it, counts the tile in `P.+0xA8`,
  stores `cell.vt+0xA8(0)` into `cell[+0xAE + slot]` and refreshes the display, `research.md` 10.3).
* **Clear** primitive (`0x55AB70`, read in full): for the local human (`P.slot == [0x9FD4BC]`) first schedule a redraw of the
  tile (`0x4ED0C0(0x9F8700; x, y)`), then `cell.mask &= ~(1 << P.slot)`, then store `cell.vt+0xA8(0)` (the tile's current
  group-0 overlay word, `colonies.md` 5) as a byte into `cell[+0xAE + slot]` (`slot & 0xFF`): **the civilization keeps seeing
  the tile as it was when it last had sight of it**.
* The clear primitives do **not** test the other masks. Whoever clears a bit is responsible for checking that no other
  source still covers the tile (sections 6 and 7 do it for units and reveals; `colonies.md` 6 for structures).

### 1.3 The visible-now test **V**

A tile is currently visible to civilization `s` iff
`(cell.+0x5C | cell.+0x60 | cell.+0x64 | cell.+0xD0) & (1 << s) != 0` (the OR is formed in `0x5BA1D0`, `0x5BA3D2..0x5BA43D`;
the radar mask `+0xD4` is not part of it). `+0x58` is "ever seen". No other composite was found in this
routine; consumers elsewhere (rendering, combat, AI) are listed as open in section 9.

### 1.4 Unit fields used

`+0x20` id, `+0x24 / +0x28` current tile, `+0x2C / +0x30` **previous tile** (written by `setPosition`, section 6.3), `+0x34` owner,
`+0x40` PRTO row, `+0x48` status bits (bit `0x08` = pending reveal), `+0x60` id of the carrying unit (`-1` none), `+0x64`
order (`0` none, `1` fortified/sentry, `15` interception), `+0x6C / +0x70` the reveal anchor tile. `PRTO.unit_class`
(`[0x9C71E0] + 0x138 * row + 0x9C`): `0` land, `1` sea, `2` air. Ability bits (`PRTO` ability dword, helper
`0x5E4EF0(row; bit)`): bit 5 = **Radar**, bit 18 = **Army**.

## 2. Ability inheritance of armies

`Unit::hasAbility(U; bit)` `0x5BC8B0` (`ret 4`) **V**:

```
if bit == 18:  return PRTO[U.type].ability(18)                       // an Army is only what its own row says
v = U.passengerType()                                                // 0x5BC6D0
if v == -1:    return PRTO[U.type].ability(bit)
return PRTO[U.type].ability(bit) or PRTO[v].ability(bit)
```

`Unit::passengerType(U)` `0x5BC6D0` (plain `ret`) **V** (read in full):

```
result = -1
if not U.ability(18) (own type) or U.tile is off the map:  return -1
for each unit u on U's tile (cell.vt+0xA0 first unit, step 0x426C80 over pool 0xA52DD4), u != U:
    c = unit with pool index u.+0x60 (if the index is in the unit pool and the node exists) ; if c.hasAbility(18): carrier = c else carrier = none
    if carrier == U:
        if result == -1:  result = u.type
        elif result != u.type:  return -1
return result
```

An Army holding only units of one type therefore **has that type's abilities** (a Radar unit in an Army gives the Army radar
sight); a mixed-type Army inherits nothing.

## 3. The sight predicate `canSee(U; x, y)` `0x5BA010` (`ret 8`) **V**

```
d = distance(U.tile, (x, y))                                         // 0x44A8D0
if PRTO[U.type].class == SEA and U.order in {1, 15} and d <= 3
       and cell(x, y).vt+0x8C()                                      // the tile is water
       and cell(x, y).vt+0xB8() == cell(U.tile).vt+0xB8():           // same water body (signed word)
    return true                                                     // fortified / intercepting ships see 3 tiles over their water body
if d > 2: return false
if U.hasAbility(5):  return true                                     // Radar: everything within 2, no line-of-sight test
k = 0x5E6D20(x - U.x, y - U.y after the toroidal fold, 25)           // spiral index 0..24, -1 if none
return (cell(U.tile).+0x68 >> k) & 1                                 // k = -1 shifts by 31: bit 31 is never set
```

The offset fold in `0x5F3F50(map; x1, y1, x2, y2, limit)` (`ret 0x14`, read in full) is applied **without testing the wrap
flags**: `dx = x2 - x1`; if `dx > W / 2` then `dx -= W`, elif `dx < -(W / 2)` then `dx += W`; the same for `dy` with `H`;
then `0x5E6D20(dx, dy, limit)`. (`canSee` calls it with the unit's tile first and the target second, so the index is the
spiral position of `target - unit`.) (Harmless: with `d <= 2` the offset is at most 4 in either coordinate.)
Ring 1 (`k < 9`) is always visible (the mask has those nine bits set for every tile, section 4). Ring 2 depends on the mask.
Nothing here depends on the unit's domain except the sentry-ship clause; **air units get no extra range**.

## 4. The line-of-sight mask `Map::computeSightMask(x, y)` `0x5D6500` (`ret 8`)

### 4.1 Contract

Called once for every real tile when a world is created or loaded: `0x5D16F0` (call at `0x5D17B0`), `0x5D1840` (call at
`0x5D18F3`) and the cell-chunk loader `0x5D9B30` (call at `0x5D9E78`). `0x5D16F0` and `0x5D1840` are called from `0x48DDE0`
(`0x48E332`, `0x48E38D`), `0x494630` (`0x494A07`, `0x494A74`) and `0x54E140` (`0x54E862`, `0x54E912`); `0x5D9B30` from the
save loader `0x5D9740` (`0x5D990C`). A search of the whole disassembly finds **no other caller** (**V** by absence: the
address is only ever the target of those three `call`s), so the mask is **not refreshed when a tile's terrain changes later**
(terraforming, a volcano). Result: `cell(x, y).+0x68` = 25-bit mask; the function returns the loop counter `25`.

`terr(t)` below is the terrain id of a tile, `(cell.+0x2C >> 12) & 0xF` (`cell.vt+0xC8` `0x5EAB30`): `0` Desert, `1` Plains,
`2` Grassland, `3` Tundra, `4` Flood Plain, `5` Hills, `6` Mountains, `7` Forest, `8` Jungle, `9` Marsh, `10` Volcano, `11` Coast,
`12` Sea, `13` Ocean. `water(t)` is `terr >= 11` (`cell.vt+0x8C`). `step(base, i)` is the neighbour step of 1.1 applied to
`base + spiral(i)`; a tile that fails the bounds test is skipped wherever it appears.

### 4.2 Tables (read from the image, **E**)

```
T1[m] (0x6705A0 + 4m), m = 0..8 :  0, 8, 1, 2, 3, 4, 5, 6, 7
T2[m] (0x6705C4 + 4m), m = 0..8 :  0, 2, 3, 4, 5, 6, 7, 8, 1
PAIR[k] (0x6705C8 + 8k), k = 9..24 : 9:(8,1) 10:(0,1) 11:(1,2) 12:(2,3) 13:(0,3) 14:(3,4) 15:(4,5) 16:(0,5)
                                     17:(5,6) 18:(6,7) 19:(0,7) 20:(7,8) 21:(8,0) 22:(2,0) 23:(4,0) 24:(6,0)
```

(`T1`/`T2` index `m` is a ring-1 spiral index; for `m` they give the two ring-1 neighbours adjacent to `m` on either side.
`PAIR[k]` lists, for the ring-2 tile `k`, the one or two ring-1 tiles in front of it; `0` stands for "the centre tile".)

### 4.3 Algorithm **V, E**

```
mask = 0
O    = (x, y);   tO = terr(O)
for k in 0 .. 24:
    T = step(O, k);  if T is off the map: continue
    if k < 9:  mask |= 1 << k;  continue                              // ring 1 and the centre: always

    via(P, m) = ( T == step(P, m) or T == step(P, T1[m]) or T == step(P, T2[m]) )     // P + spiral(m), etc.

    visible = false
    // A. over water: an adjacent water tile never blocks
    for m in 1 .. 8:  P = step(O, m);  if P on the map and water(terr(P)) and via(P, m):  visible = true;  break
    // B. from high ground: look over the adjacent tiles that are lower than the origin
    if not visible:
        if tO in {6, 10}:           blockers = {6, 10}
        elif tO == 5:               blockers = {6, 10, 5, 7, 8}
        else:                       blockers = none (skip B)
        for m in 1 .. 8:  P = step(O, m);  if P on the map and terr(P) not in blockers and via(P, m):  visible = true;  break
    // C. tall terrain is seen from afar: the target itself is high
    if not visible:
        tT = terr(T);  (a, b) = PAIR[k]
        if tT in {6, 10}:   for q in (a, b): Q = step(O, q);  if Q on the map and Q != O and terr(Q) not in {6, 10}:  visible = true;  break
        elif tT == 5:       for q in (a, b): Q = step(O, q);  if Q on the map and Q != O and terr(Q) not in {6, 5, 10, 7, 8}:  visible = true;  break
    if visible:  mask |= 1 << k
```

Order matters only in that every phase can only add the bit; the result is the OR. Reading the three phases:
**A** water is transparent (a ship sees two tiles; a land unit on a coast sees across the water); **B** a unit on Hills sees
over Plains, Grassland, Desert, Tundra, Flood Plain, Marsh and water, but not over Hills, Mountains, Volcano, Forest, Jungle;
a unit on Mountains or a Volcano sees over everything except Mountains and Volcano; **C** a Mountain, Volcano or Hills tile
two steps away is visible whenever a tile in front of it (on the line from the centre) is not itself an obstruction of the
same or higher class; flat origin with flat neighbours sees only ring 1.

(`Q != O` is a skip test `cmp ebx, [esp+0x30]; cmp edi, [esp+0x34]` at `0x5D6E4A` and the matching hills block `0x5D6F75`:
the centre does not count as a "tile in front".)

### 4.4 Verification **E**

The real `0x5D6500` was run in the emulator for every real tile of four shipped saves (9 240, 5 000, 5 000 and 32 768 tiles; the
5 000-tile save "1111110 BC.SAV" contains every terrain id 0..13, 2 Volcano tiles) under each of the four wrap-flag settings
(`map +0x1F0` = 0..3): 208 032 tile computations, and for 3 000 random 9 x 9 terrain assignments at random positions (map edges
included, random wrap modes, four terrain-weight palettes). **Zero mismatches** with the pseudocode above in all of them.
Terrain was read through the game's own accessor (`cell.vt+0xC8`), and written by replacing bits 12..15 of `cell +0x2C`.

### 4.5 Golden vectors **E** (all other tiles of the 9 x 9 block are Plains, id 1, unless stated; bit `k` = spiral index `k`)

| # | terrain layout | mask | ring-2 bits set |
|---|---|---|---|
| V1 | all Plains | `0x00001FF` | none |
| V2 | origin Hills | `0x1FFFFFF` | all of 9..24 |
| V3 | origin Mountains | `0x1FFFFFF` | all |
| V4 | all ring 1 Hills (origin Plains) | `0x00001FF` | none |
| V5 | all ring 1 Coast (11) | `0x1FFFFFF` | all |
| V6 | origin Hills, ring 1 Forest | `0x00001FF` | none |
| V7 | origin Hills; ring 1 tile 1 Plains, tiles 2..8 Mountains | `0x0000FFF` | 9, 10, 11 |
| V8 | rest of the block Mountains, origin and ring 1 Plains | `0x1FFFFFF` | all (C) |
| V9 | rest Hills, origin and ring 1 Plains | `0x1FFFFFF` | all (C) |
| V10 | rest Hills, ring 1 Hills (origin Plains) | `0x00001FF` | none |
| V11 | rest Forest, origin and ring 1 Plains | `0x00001FF` | none |
| V12 | origin and ring 1 Mountains except tile 5 Plains, ring 2 Hills | `0x00381FF` | 15, 16, 17 |

## 5. Which units are seen and how the radius works (summary)

* Land and air units: ring 1 always; ring 2 per the mask of their own tile, or all of ring 2 with Radar (also when the
  Radar unit is the only type inside an Army).
* Ships: as above, and a fortified (order 1) or intercepting (order 15) ship sees every water tile of the same water body
  within distance 3 (ring 3 of the spiral, first 49 entries, whose maximum tile distance is 3).
* Units carried by another unit (`+0x60 != -1`) contribute nothing while carried (section 6, first test).
* Cities have no sight routine of their own (none was found): a city sees through the territory it owns, **the 3 x 3 block around every owned tile** (`+0x64`, `borders-culture.md` 6). **H** for "no other city sight".

## 6. The unit sight refresh `Unit::refreshSight(U; a, b, c)` `0x5BA1D0` (`ret 0xC`; `a`, `b`, `c` are bytes) **V**

`a` = **remove** this unit's contribution (the unit is leaving the map); `b` = use the **previous tile** `+0x2C / +0x30`
instead of the current one when tidying up; `c` = request a redraw. `P = Player[U.+0x34]`, `s = P.slot`.

```
if a == 0:
    if U.+0x60 != -1:  return                                        // carried: neither marks nor unmarks (also skips the redraw flag)
    for n in 0 .. 48:
        T = step(U.tile, n);  if T off the map: continue
        if canSee(U, T):  P.markSeen(T)                              // 0x55AD90: cell.+0x5C |= bit(s); discover
for j in 0 .. 48:
    base = (a == 0 and b != 0) ? U.prevTile : U.tile
    T = step(base, j);  if T off the map: continue
    if a != 0:
        if not canSee(U, T):  continue                              // only tiles this unit was feeding
    else:
        if ((cell(T).+0x5C | .+0x60 | .+0x64 | .+0xD0) & bit(s)) == 0:  continue     // not visible now: nothing to remove
        if canSee(U, T):  continue                                   // still seen by this unit
    // does another unit of the same civilization still see T?
    for m in 0 .. 48:
        N = step(T, m);  if N off the map: continue
        for each unit u standing on N, u != U, u.+0x34 == U.+0x34:
            if canSee(u, T):  goto next j
    P.unmarkSeen(T)                                                  // 0x55AB70: redraw (local human), cell.+0x5C &= ~bit(s), remember overlay
if c and U.+0x34 == [0x9FD4BC]:  [0xA281C5] = 1                      // redraw request
```

Notes **V**:

* Only **units** are consulted for the "does anyone else still see it" test. A tile that is also covered by territory,
  a structure or a reveal keeps its other bits because `unmarkSeen` clears `+0x5C` only; the tests of the clears are the
  separate masks.
* The scan is over 49 tiles (radius 3) around `T`, which is the largest reach of any sight source (the sentry ship).
* The inner test uses each other unit's own order, ability and tile mask, so an Army with a Radar passenger counts.
* `a != 0` ignores `b`; the loop then runs around the current tile.

### 6.1 Call sites (`(a, b, c)`; every `call 0x5BA1D0` in the image: `0x55B4F3`, `0x5BD257`, `0x5BE45B`, `0x5BEB55`, `0x5C647D`, `0x5C65F9`, `0x5C7E62`)

| caller | when | `(a, b, c)` |
|---|---|---|
| `0x5BD220` `Unit::setPosition(x, y)` (`0x5BD257`) | the destination is **off the map**, before the tile fields change | `(1, 0, 1)` |
| `0x5BD220` tail (`0x5BE45B`) | after every placement | `(0, 1, 1)` |
| `0x5BEB10` (`0x5BEB55`), `0x5C6290` (`0x5C647D`), `0x5C6570` (`0x5C65F9`) | the unit **had** class Sea and order 1 or 15 on entry (tested before the cancel); the order-cancel method (vtable `+0x68`) runs, then | `(0, 0, 1)` |
| `0x5C7700` (`0x5C7E62`), the per-unit turn, at the end | class Sea and order 1 or 15 | `(0, 0, 1)` |
| `0x55B4B0` `Player::refreshAllSight` (`0x55B4F3`) | once for every unit owned by the player | `(0, 0, 0)` |

(`a` is the last argument pushed. `0x5BEB10` is "cancel the order of this unit, and when it was a sentry/intercepting ship, shrink
its sight"; the two bytes pushed before each call are `1, 0, 0` in call order for `(0, 0, 1)`.)

`0x55B4B0(P)` (`ret` plain, **V**, read in full): loop the unit pool (`[0xA52E84]`, last `[0xA52E90]`); for each live unit with
`owner == P.slot` call `0x5BA1D0(U; 0, 0, 0)`; afterwards, when `P.slot == [0x9FD4BC]` and bit `P.slot` of `[0xA526C0]` is set, tail-jump to
`0x6027C0(0x9FDB50)` (a UI refresh of the local player's view, **O**). Its callers are the world build `0x494630` (`0x494B61`) and the
tile-owner recompute `0x5D4830` (`0x5D597F`).

### 6.2 What it does not do

* It does not touch cities (they use `+0x64`), structures (`+0x60`) or reveals (`+0xD0`).
* It does not clear anything when a unit simply stays put and its radius is unchanged.
* There is **no** per-turn full recomputation of `+0x5C`; stale bits are possible only if a caller skips the refresh.

### 6.3 `setPosition` and the previous tile **V** (`0x5BD220`, `ret 8`; the full body is specified in `movement.md` 5)

The parts that matter for vision, read from the raw code:

```
Unit::setPosition(U; x, y):
    if U.x == x and U.y == y:  return                                  // 0x5BD231..0x5BD239: no change, no refresh at all
    if (x, y) is off the map (0x426BD0):  refreshSight(U; 1, 0, 1)      // 0x5BD257: withdraw sight from the old place first
    U.prev = U.tile            // +0x2C/+0x30 := +0x24/+0x28           // 0x5BD25C
    U.tile = (x, y)                                                    // 0x5BD26D, 0x5BD273
    U.+0x54 = 0;  U.+0x58 = -1                                          // 0x5BD276, 0x5BD27D
    ... (stack-list removal and insertion, colony / hut / barbarian-camp side effects, movement.md 5) ...
    if PRTO[U.type].transport_capacity > 0:                            // 0x5BD534: PRTO body +0x4C
        for every unit c in the unit pool with c.+0x60 == U.id:         // 0x5BD558..0x5BD5A6
            if c.order != 15:  c.setOrder(1)                           // 0x5B3040 (unit-turn.md 3.6: setOrder; 0 = none)
            Unit::setPosition(c; x, y)                                  // cargo follows (recursion)
    ...
    (flag modes, only when U.+0x1EC != 0xFF; the reset U.+0x1EC = 0xFF, U.+0x1EA = 0xFFFF at 0x5BE443/0x5BE44A is part of
     the delivery path, not of every placement: movement.md 5 step 8g)
    refreshSight(U; 0, 1, 1)                                           // 0x5BE45B: the last act
```

Each cargo unit's own `setPosition` ends with its own `refreshSight(c; 0, 1, 1)`, which returns at once for `a == 0` because
`c.+0x60 != -1`: a unit that is carried contributes no sight. The carrier's refresh at the end of its own `setPosition` is the
one that counts.

## 7. The air reveal (recon mission) **V**

`Unit::beginReveal(U; x, y)` `0x5C74A0` (`ret 8`; callers `0x4785C0` at `0x47866C` and `0x4E6DB0` at `0x4E7097`: the mission code
of `air.md`): `U.+0x48 |= 8`; `U.+0x70 = y`; `U.+0x6C = x`; for `n = 0 .. 24` (the 5 x 5 block) the tile `T = step((x, y), n)` on
the map gets `Player[U.owner].0x55B140(T)` (sets the `+0xD0` bit and discovers); finally `U.+0x50 = 0x5BE470(U)` (movement
used := the full movement allowance: the unit has no movement left this turn).

`Unit::endReveal(U)` `0x5C7570` (plain `ret`; its only caller is the end of the per-unit turn `0x5C7700`, `0x5C7E6F`, executed
when status bit `0x08` is set; the code at `0x5C7E67`). Read in full:

```
U.+0x48 &= ~0x08                                                    // first: U itself can no longer count as an anchor
for n in 0 .. 24:
    T = step((U.+0x6C, U.+0x70), n);  skip if T is off the map or fails 0x426BD0
    for each unit u standing ON T (cell.vt+0xA0 list, pool 0xA52DD4):
        if u.owner == U.owner and (u.+0x48 & 8):  goto next n      // some reveal plane parks on this very tile: keep it
    Player[U.owner].0x55B090(T)                                     // clear the +0xD0 bit, remember the overlay
U.+0x6C = U.+0x70 = -1
```

Quirk: the test looks at the units standing on the tile being released, **not** at the other planes' anchors. Two recon planes
whose 5 x 5 areas overlap do not protect each other's tiles unless a plane happens to stand on that very tile, so the tiles of the
overlap lose the bit when either plane's reveal ends. The unit turn then clears `U.+0x50` to 0 and `U.+0x48 &= 0xB8` (keeps bits
3, 4, 5, 7) at `0x5C7E77`.

## 8. Territory, structures, radar

* **Territory** (`+0x64`): `Map::setTileOwner` `0x5D3AB0` (`borders-culture.md` 6). When an owner `old > 0` loses tile `(x, y)`, for each
  of the nine tiles `T` of its 3 x 3 block, `Player[old].forgetTile(T)` (`0x55AF90`) is called unless some tile in the 3 x 3 block
  around `T` is still owned by `old`. When `newOwner > 0` takes it, `Player[newOwner].revealTile(T)` (`0x55B030`) is called for all
  nine tiles. So **a civilization sees every tile within one step of any tile it owns**; releasing a tile (`newOwner <= 0`) only
  withdraws, and a tile that no longer has any owned neighbour stops being seen.
* **Structures** (`+0x60`) and **radar** (`+0xD4`): `colonies.md` 5 (footprints) and 5.1 (the observation test used by the
  destroyers).
* **Remembered overlay**: section 1.2, clear primitives.

## 9. Open items

1. The consumers of the masks outside this routine: the rendering of fog, the combat and AI visibility tests (`0x4D05D0`,
   `0x56CEB0` read `+0xD4`; the 36 functions that read two or more of the masks are not individually identified).
2. The consumer of the redraw flag `[0xA281C5]` (display only).
3. `cell.vt+0xA8(0)` exactly (the remembered byte): it is the group-0 overlay word per `colonies.md` 5.
4. `0x6027C0(0x9FDB50)` at the end of `0x55B4B0` (UI refresh).
5. The mask is not rebuilt after terrain changes (4.1, by absence of callers): confirm in play that Transform Terrain leaves stale line-of-sight.
6. The vtable method `+0x68` of the unit (`call [vtable+0x68]` in `0x5BEB10`, `0x5C6290`, `0x5C6570`): the order-cancel; its body is not specified here (**H** that it zeroes `+0x64`).

## 10. Quirks worth reproducing

1. Fortified/intercepting ships see radius 3 over their own water body; every other unit sees at most radius 2.
2. Radar ignores line of sight but not distance 2; an Army with a single-type Radar cargo inherits it.
3. The sight mask is built once, never updated after terraforming.
4. The reveal release looks only at units on the tile, not at other reveal anchors.
5. `refreshSight` with `b = 1` tidies the *previous* tile's neighbourhood; with `b = 0` the current tile's.
6. A carried unit (`+0x60 != -1`) is ignored by every `a == 0` call, and removed with `a = 1`.
