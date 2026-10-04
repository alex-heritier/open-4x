# Borders, tile ownership and the culture flip

Clean-room specification of the cultural side of `Civ3Conquests.exe` (PE32, MSVC 6, image base `0x400000`): the
per-turn culture accumulation of a city, the culture level and its thresholds, how every tile's owner is
recomputed from the cities' border levels (claim, tie-break, orphan resolution, release), the side effects of an
ownership change (vision, colonies, camps, huts, trade network, units), the empire-wide culture total, and the
**culture flip** (a city defecting to a neighbouring civ). Tags: **V** read from the instructions with the body
opened, **E** executed in an emulator over the unmodified exe (scratch harness, not committed), **H** hypothesis,
**O** open. No reference code is given on purpose; every algorithm is stated as data and rules.

Conventions (`primitives.md`): `__thiscall` (`ecx` = this), `ret N`, the last pushed argument is the first
parameter. City = `City` record (pool `[0xA52E6C]`, last index `[0xA52E78]`); Player at `0xA52E98 + 0x20E4*slot`;
map object `0x9C736C` (fields used here: `+0x40` word tile count, `+0x148` cell-pointer array, `+0x154` y extent, `+0x168` x extent,
`+0x1F0` wrap flags). Tiles use the doubled grid: `(x, y)` is valid when `x + y` is even; the tile index is
`((W >> 1) * y + (x >> 1)) & 0xFFFF` with `W = [0x9C74D4]`; from an index `i`: `y = i div (W >> 1)`,
`x = 2 * (i mod (W >> 1)) + (y & 1)`. Map vtable slot `+0x30(x, y)` returns the cell; the wrap moduli are the two
extents; wrap flag bit 0 = wrap in x, bit 1 = wrap in y. An out-of-range index yields the dummy cell `0xCAA330`.

**Neighbour step used everywhere below** (inline in every routine of this file): given `(x, y)` and an offset
`(dx, dy)`: `nx = x + dx`; if wrap-x: `nx += Wx` when `nx < 0`, `nx -= Wx` when `nx >= Wx`; the same for
`ny` with wrap-y and `Wy`; then the tile exists only when `0 <= nx < Wx` and `0 <= ny < Wy` (`Wx = [map+0x168]`,
`Wy = [map+0x154]`). A single fold is applied, so offsets larger than one extent are not folded twice.

## 1. Data

| datum | where | meaning |
|---|---|---|
| `City +0x20` id; `+0x24`/`+0x26` x/y (words); `+0x28` owner slot (byte) | city record | |
| `City +0x30` | flags | bit 0 disorder, bit 1 celebration (`happiness.md`); **disorder doubles and celebration halves the flip score (section 9)** |
| `City +0x58` | flip cooldown | decremented once per call of the flip test while positive; set by the capture routine through `0x4BCDE0(city, n)`, `n = 1` for a plain capture, else 10 (`capture.md` step 12) |
| `City +0x5C` | **culture (border) level** 1..6 | section 3 |
| `City +0xC4` | builder-record array, 12 bytes per building; `+8` = the culture that building has produced in this city | `city-buildings.md` |
| `City +0xDC / +0xE0 / +0xEC` | citizen pool; last index `+0xEC` | citizen record: `+0x20` byte non-zero = resisting; `+0x140` race (`yields.md`, `hurry.md`) |
| `City +0x13C` | culture produced this turn | section 2 |
| `City +0x140 + 4*civ` | accumulated culture of civ `civ` in this city (32 dwords) | |
| `City +0x358, +0x35C, +0x360, +0x364` | four words read only by the border tie-break (section 5.4); meaning and writers **O** | |
| `Player +0x1C` slot; `+0x20` race; `+0x2C` capital city id; `+0x30` difficulty level; `+0xA0` government; `+0xD30 + q` at-war byte against slot `q`; `+0x15C8` capital-lost counter; `+0x181C` culture sub-object; `+0x183C` total culture | player record | the at-war byte is non-zero while the civ is at war with `q` |
| culture sub-object `S = Player +0x181C` | `S+0x1C` rating index; `+0x20` accumulated total (= `Player +0x183C`); `+0x24` culture produced this turn; `+0x28` the owner slot | section 8 |
| `GOVT +0x18C` class (4 = Anarchy), `GOVT +0x24` xenophobic flag | `[0x9C71D8] + 0x1E8 * govt` | |
| `DIFF[level] +0x78` | `[0x9C40C0] + 0x7C * level + 0x78` = body `+0x74` `citizens_quelled_by_military` | section 9 |
| `[0x9C7300]` | RULE border factor (10 stock) | section 3 |
| table `0x670540` | 7 dwords `{0, 4, 10, 20, 36, 52, 82}`, the squared reach per level (**E**: read from the exe) | section 4 |
| `[0xA5267C] & 0x8000` | game rule **Allow Cultural Conversions** (`biq-format.md`) | gate of the flip |
| `[0xA526C0]` | in-play civ mask (bit = slot) | |
| `[0xA526B4]` | gameplay RNG; `0x60BAB0(0xA526B4; n)` returns a value in `0..n-1` | |
| cell | `+0x08` resource id (dword, -1 none); vtable `0x6701C8`: `+0x64(0)` road predicate, `+0x8C` water predicate (terrain >= 11), `+0x98` owner byte, `+0xAC` feature-flag word, `+0xE0(2, mask, -1, -1)` set / `+0xCC(2, mask, -1, -1)` clear a flag, `+0x108(owner)` write the owner, `+0xC8` terrain id | |
| cell flag `0x40000` (feature word, group 2) | **owner-lapsed mark**, set and cleared only by `0x5D4830` | section 5.2 |
| cell word `+0x64` | per-civ bit set: tiles the civ currently sees through owning territory | section 6 |

`cell.vt+0x64(0)` (`0x5D9FF0`) is "the tile has a road": the group-0 overlay bit 0 (`0x5EA8F0`), except that a tile that
has a city or a colony counts as roadless when the owner of that city/colony (`vt+0x114` / `vt+0x118`) lacks the
technology `[[0x9C7324] + 0x1A4]`. (Name of that technology **O**.) `vt+0x8C` is the water predicate
(`barbarians.md` 1.2).

## 2. Per-turn culture accumulation `City::accumulateCulture` `0x4B2680` (plain `ret`; caller `0x4BEBDB` in the city sequencer) **V**

```
this.+0x13C = 0
for b in 0 .. BLDG.count-1:                                         # [0x9C3D80]
    if not this.hasBuilding(b, 1) (0x4ACB50):  continue
    if this.isObsolete(b) (0x4ACCC0) and (BLDG[b].+0xF0 & 4) != 4:  continue        # obsolete and not a Great Wonder: no culture
    this.+0x13C = max(0, this.+0x13C + P.cultureOf(b, this))        # 0x4F8CE0(&P.+0x181C; b, this), city-buildings.md 6
if GOVT[P.+0xA0].+0x18C == 4:  return                               # Anarchy: no stock change, no level update
if GOVT[P.+0xA0].+0x24 != 0:                                        # xenophobic government
    X = 0x4BB410(this; P.+0x20);  Y = 0x4BB300(this)                # citizens of the owner's race; city size
    if Y > 0 and (float)X / Y < 0.51f:  return                      # float compare with the single 0.51f at 0x669060 (0x3F028F5C)
for b in 0 .. BLDG.count-1:                                         # per-building tally, same two gates, NO clamp
    if hasBuilding(b, 1) and not (isObsolete(b) and (BLDG[b].+0xF0 & 4) != 4):
        this.+0xC4[b*12 + 8] += P.cultureOf(b, this)
this.+0x140[owner] = max(0, this.+0x140[owner] + this.+0x13C)
this.recalcCultureLevel(force = 0, quiet = 0, noBorderUpdate = 0)  # 0x4B0C60, section 3
```

Notes: the first loop clamps the running sum at 0 after every building, the second does not clamp. A city with
`Y <= 0` (or a non-xenophobic government) is not stopped by the ratio test. The ratio `X / Y` is computed on the x87
stack and compared with the single-precision constant `0.5099999905` (`0x3F028F5C`); the city is stopped when the
ratio is strictly below it. Anarchy keeps the stock and the level untouched.

## 3. Culture level `City::recalcCultureLevel(this; force, quiet, noBorderUpdate)` `0x4B0C60` (`ret 0xC`) **V**

```
t = [0x9C7300];  a = this.+0x140[owner];  level = 1;  limit = t
while a >= limit and level < 6:  level += 1;  limit *= t            # 32-bit imul; limit = t^level
prev = this.+0x5C
if force == 0 and level == prev:  return
this.+0x5C = level
if noBorderUpdate == 0:
       map.recomputeBorders()                                        # 0x5D4830, section 5 (takes no arguments)
       0xA0E270.notify(x, y, level)                                  # 0x578E90 (UI/minimap, H)
if prev <= 2 and level <= 2:  this.vtable[+0x38](1)                  # city refresh (O)
if force == 0:
    if level > prev:
        if quiet == 0 and owner == [0x9FD4BC] (the local slot):
            if (Player.+0x40 & 8) == 0 and not multiplayer (0x47B530):
                 dialog CULTUREBORDERVERBOSE (modal), then Player.+0x40 |= 8
            else: message CULTUREBORDER at the city
    elif quiet == 0 and owner == local slot:  message CULTUREBORDEROVER at the city          # level < prev
r = max(level, prev);  if UI region check 0x4E69F0(0x9F8700; x, y, -r, -r): [0xA281C4] = 1   # redraw flag
```

`level = 1 + #{ k in 1..5 : a >= t^k }`. With `t = 10`: level 1 for `a < 10`, 2 for `10..99`, 3 for `100..999`,
4 for `1000..9999`, 5 for `10000..99999`, 6 from `100000`. The level is recomputed from the stock each time and
can fall when the stock fell (the stock itself is only ever increased by section 2; capture/loader paths write it
directly). Other callers: the capture routine
(`0x56513F`, `force = 1`), the loader/setup `0x5D25F0` (`0x5D2A5A`), `0x5C326C` in `0x5C2E80` (**O**).

## 4. The border shape

### 4.1 The offset enumerator `0x5E6E50(k; &dx, &dy)` (cdecl) **E**

Maps an index `k` in `0..168` to an offset in the doubled grid. Closed form, **E**-verified against all 169
executed entries: let `(u, v)` be rotated coordinates with `dx = u - v`, `dy = u + v`.

```
k = 0:                 (u, v) = (0, 0)
k = 1..8  (ring r=1):  (u, v) = (0,-1) (1,-1) (1,0) (1,1) (0,1) (-1,1) (-1,0) (-1,-1)
for r = 2 .. 6 in turn (ring r has 8r entries, in this order):
    top edge     v = -r, u = -r+1 .. r-1            (ascending u)
    right edge   u =  r, v = -r+1 .. r-1            (ascending v)
    bottom edge  v =  r, u =  r-1 .. -r+1           (descending u)
    left edge    u = -r, v =  r-1 .. -r+1           (descending v)
    corners      (-r,-r) (r,-r) (r,r) (-r,r)
```

Ring `r` occupies `k = (2r-1)^2 .. (2r+1)^2 - 1` (ring 1: 1..8, ring 2: 9..24, ring 3: 25..48, ring 4: 49..80, ring 5:
81..120, ring 6: 121..168). Executed table:

```
0       (0,0)
1..8    (1,-1) (2,0) (1,1) (0,2) (-1,1) (-2,0) (-1,-1) (0,-2)
9..24   (1,-3) (2,-2) (3,-1) (3,1) (2,2) (1,3) (-1,3) (-2,2) (-3,1) (-3,-1) (-2,-2) (-1,-3) (0,-4) (4,0) (0,4) (-4,0)
25..48  (1,-5) (2,-4) (3,-3) (4,-2) (5,-1) (5,1) (4,2) (3,3) (2,4) (1,5) (-1,5) (-2,4) (-3,3) (-4,2) (-5,1) (-5,-1) (-4,-2) (-3,-3) (-2,-4) (-1,-5) (0,-6) (6,0) (0,6) (-6,0)
49..80  (1,-7) (2,-6) (3,-5) (4,-4) (5,-3) (6,-2) (7,-1) (7,1) (6,2) (5,3) (4,4) (3,5) (2,6) (1,7) (-1,7) (-2,6) (-3,5) (-4,4) (-5,3) (-6,2) (-7,1) (-7,-1) (-6,-2) (-5,-3) (-4,-4) (-3,-5) (-2,-6) (-1,-7) (0,-8) (8,0) (0,8) (-8,0)
81..120 (1,-9) (2,-8) (3,-7) (4,-6) (5,-5) (6,-4) (7,-3) (8,-2) (9,-1) (9,1) (8,2) (7,3) (6,4) (5,5) (4,6) (3,7) (2,8) (1,9) (-1,9) (-2,8) (-3,7) (-4,6) (-5,5) (-6,4) (-7,3) (-8,2) (-9,1) (-9,-1) (-8,-2) (-7,-3) (-6,-4) (-5,-5) (-4,-6) (-3,-7) (-2,-8) (-1,-9) (0,-10) (10,0) (0,10) (-10,0)
121..168 (1,-11) (2,-10) (3,-9) (4,-8) (5,-7) (6,-6) (7,-5) (8,-4) (9,-3) (10,-2) (11,-1) (11,1) (10,2) (9,3) (8,4) (7,5) (6,6) (5,7) (4,8) (3,9) (2,10) (1,11) (-1,11) (-2,10) (-3,9) (-4,8) (-5,7) (-6,6) (-7,5) (-8,4) (-9,3) (-10,2) (-11,1) (-11,-1) (-10,-2) (-9,-3) (-8,-4) (-7,-5) (-6,-6) (-5,-7) (-4,-8) (-3,-9) (-2,-10) (-1,-11) (0,-12) (12,0) (0,12) (-12,0)
```

`k = 0..8` is the 3x3 block around a tile, `k = 0..20` the 21-tile city radius, `k = 1..20` the 20 tiles used by the
flip.

### 4.2 Squared reach per level

A tile at offset `(dx, dy)` from a city of level `L` is in reach when `dx*dx + dy*dy <= R2[L]`,
`R2 = {0, 4, 10, 20, 36, 52, 82}` (table `0x670540`; index 0 is never used by a city, whose level is 1..6). **E**
tile counts per level: 1 -> 9 tiles (3x3, max `k` 8), 2 -> 21 (max `k` 20), 3 -> 37 (max `k` 43), 4 -> 61 (74),
5 -> 89 (113), 6 -> 137 (161). This **corrects** the "17, 26, 37" figures for levels 4 to 6 in `economy.md` (a guess).

## 5. Recomputing every tile's owner `Map::recomputeBorders` `0x5D4830` (plain `ret`, `this` = map `0x9C736C`) **V**

No arguments: every call rescans the whole map. Callers: level change `0x4B0CBD`, city founding `0x4AE5EE`
(`borderFlag`), city destruction `0x4AF260`, loader `0x5D25F0` (`0x5D2A96`). Local state: `needRebuild` (byte, a
trade-network/resource rebuild is needed), `maskA`, `maskB`, `maskLost` (32-bit civ masks), `changed`.

### 5.1 Phase 1: claim by cities (tile by tile, index order)

```
for i in 0 .. tileCount-1:
    (x, y) = tile of index i;  ocean = (cell(x,y).vt+0xC8() == 13)         # only terrain 13 (Ocean); Coast 11 and Sea 12 are not restricted
    best = none;  bestK = 0
    for k in 0 .. 168:
        (nx, ny) = (x, y) + off(k) with the neighbour step; skip if the tile does not exist
        if ocean and k >= 21:  skip
        C = cityAt(nx, ny) (0x56D2C0);  skip if none
        if off(k).dx^2 + off(k).dy^2 > R2[C.+0x5C]:  skip                  # not in reach of C
        if best == none:  best = C;  bestK = k;  continue
        if tieBreak(best, C) == best:  continue                             # section 5.4: the incumbent keeps the tile
        L = smallest index 0..5 with d2(off(bestK)) <= R2[L], or 6 if none  # the bracket of the incumbent's distance
        if d2(off(k)) <= R2[L]:  best = C;  bestK = k                       # R2[6] = 82; the challenger must lie in the same bracket
    if best == none:
        if cell.vt+0x98() != 0:  cell.vt+0xE0(2, 0x40000, -1, -1)           # owned but unclaimed: set the owner-lapsed mark; the owner is NOT cleared here
    else:
        old = cell.vt+0x98();  Map::setTileOwner(x, y, best.owner)           # section 6, returns at once when old == new
        if old != best.owner:  noteChange(cell, old, best.owner)             # section 5.3
```

Because `k` enumerates the offset from the **tile** to the candidate city, the nearest rings are tried first: the
first city found keeps the tile unless a later city beats it in the tie-break **and** its offset lies within the
smallest radius bracket (`R2` entry) that contains the incumbent's offset. A city whose tile is farther than
the incumbent's bracket can never take the tile by culture alone. Ownership of a tile therefore never depends on
the other tiles' owners.

### 5.2 Phase 2: resolve unowned and lapsed tiles (repeat until stable)

```
repeat:
    changed = false
    for every tile i:
        owner = cell.vt+0x98()
        if owner != 0 and not (cell.vt+0xAC() & 0x40000):  continue            # a settled tile
        n = orphanOwner(x, y)                                                   # 0x5D4370(map; x, y, 0, 0), section 5.5
        if n == 0:  continue                                                    # stays unowned / marked
        cell.vt+0xCC(2, 0x40000, -1, -1)                                        # clear the mark
        if n != owner:  Map::setTileOwner(x, y, n);  noteChange(cell, owner, n)
        changed = true
until not changed
```

Each processed tile either gains an owner or loses its mark, so the loop terminates. Neighbours that still
carry the mark do not count (section 5.5), so a tile whose neighbours are still marked can only be resolved in a
later pass.

### 5.3 Phase 3 and the epilogue

```
for every tile with the mark 0x40000:                                         # nobody could take it
    old = cell.vt+0x98();  cell.vt+0xCC(2, 0x40000, -1, -1)
    Map::setTileOwner(x, y, 0)
    if cell.vt+0x64(0) (road):   (hasCity ? needRebuild = true : resource test with owner old);  maskA |= { j in play : atWar(j, old) }
    elif cell.vt+0x8C() (water): maskB |= { j in play : atWar(j, old) }
for j = 1 .. 31 with bit j in [0xA526C0]:
    if j in maskLost:  Player[j].refreshUnitVision()                           # 0x55B4B0: for every unit owned by j, 0x5BA1D0(unit; 0, 0, 0) (unit-turn.md, vision update); UI refresh for the local slot
    if j in maskA:  0xB72888.0x57D980(j, relabel = 1, flag = 0, cityId = -1);  needRebuild = true
    elif j in maskB:  0xB72888.0x57D980(j, relabel = 0, flag = 0, cityId = -1);  needRebuild = true
if needRebuild:  0xB72888.0x57E450(quiet = 0)                                   # resource availability, trade-network.md section 7
```

**`noteChange(cell, old, new)`** (duplicated inline in phases 1 and 2, structure checked in both):

```
if old == 0:                                                   # newly owned
    if road(cell):    if hasCity(cell) or resourceKnown(cell, new):  needRebuild = true;   maskA |= { j in play : atWar(j, new) }
    elif water(cell):                                                                       maskB |= { j in play : atWar(j, new) }
else:                                                          # changed hands (old != new)
    maskLost |= 1 << old
    if road(cell):    if hasCity(cell) or resourceKnown(cell, old) or resourceKnown(cell, new):  needRebuild = true
                      maskA |= { j in play : atWar(j, old) != atWar(j, new) }
    elif water(cell): maskB |= { j in play : atWar(j, old) != atWar(j, new) }
resourceKnown(cell, p) = cell.+8 != -1 and Player[p].knowsTech(GOOD[cell.+8].+0x4C)       # GOOD at [0x9C71D4], stride 92; tech -1 counts as known (0x561440)
```

`atWar(j, p)` is `Player[j].+0xD30[p] != 0`. The masks name the civs whose road (`maskA`) or sea (`maskB`)
connectivity changed because an enemy-border rule (`trade-network.md` section 2) now includes or excludes the tile.

### 5.4 The claimant tie-break `0x5D3850(map; incumbent A, candidate C)` (`ret 8`), returns the winner **V**

Notation: `cu(X) = X.+0x140[X.owner]`; `z(X) = X.+0x364` if non-zero else 1; `s(X) = X.+0x35C + 1`;
`m = A.+0x358` (the incumbent's mode word); `rat(X) = Player[X.owner].+0x183C`. Every case of the vectors T1..T9
(section 10) was **E**-executed on the real routine in the emulator.

```
if cu(A) > cu(C): return A
if cu(A) < cu(C): return C
# equal culture: stage 1 (can return A)
m == 0:  if z(A) < z(C): return A
m == 1:  if z(A) < z(C): return A;  if z(A) == z(C) and s(A) < s(C): return A
m == 2:  if z(A) == z(C): return A
# stage 2 (can return C or go to the final test)
m == 0:  if z(A) == z(C): goto final;  return C
m == 1:  if z(A) != z(C): return C;  if s(A) == s(C): goto final;  return C
m == 2:  return C                      # stage 2 is only reached with z(A) != z(C); the code also tests +0x360 there (0x5D394C..0x5D3962) but that arm is unreachable
other m: return C
final:   if rat(A) > rat(C): return A;  if rat(A) != rat(C): return C
         return (A.owner < C.owner) ? A : C                      # equal ratings: the lower slot wins, ties go to C
```

In ordinary play the first two lines decide. The words `+0x358..+0x364` act as a deterministic city-specific
tie-break chain; their writers were not found (**O**).

### 5.5 Orphan resolution `0x5D4370(map; x, y, k0, hint)` (`ret 0x10`), the `k0 = 0` path **V**

`k0 > 0` (with `hint`) is only used by the interface preview `0x4DD800`; the border code always passes `0, 0`.

```
if cell(x,y).vt+0xC8() == 13:  return 0                                   # open ocean is never settled by adjacency
if cell(x,y).vt+0x98() != 0 and not (cell.vt+0xAC() & 0x40000):  return 0
nb(k) = owner of the tile (x,y)+off(k) if it exists and does not carry the 0x40000 mark, else 0
A = (nb(7) == nb(3)) ? nb(7) : 0               # the NW and SE diagonal neighbours agree
B = (nb(1) == nb(5)) ? nb(1) : 0               # the NE and SW diagonal neighbours agree
if A > 0:  if B <= 0 or A == B: return A;   return (rat(A) >= rat(B)) ? A : B      # rat = Player[.].+0x183C
if B > 0:  return B
return 0
```

`off(1) = (1,-1)`, `off(3) = (1,1)`, `off(5) = (-1,1)`, `off(7) = (-1,-1)`.

## 6. The owner write `Map::setTileOwner(x, y, newOwner)` `0x5D3AB0` (`ret 0xC`) **V**

```
old = cell(x,y).vt+0x98();  if old == newOwner:  return
cell.vt+0x108(newOwner)                                                  # write the owner byte
0xC88588.0x58B5D0(6, newOwner, x, y, 0, old)                             # event record, kind 6 (H: tile-owner change)
if old > 0:
    for t in 0..8:  T = (x,y) + off(t) (existing tiles only)
        if no tile U in the 3x3 block around T (off(0..8) from T, existing) has owner == old:
            Player[old].forgetTile(T)                                    # 0x55AF90, below
if newOwner <= 0:  return                                                # releasing a tile stops here
for t in 0..8:  T = (x,y) + off(t) (existing tiles only):  Player[newOwner].revealTile(T)    # 0x55B030
c = cell(x,y)
if c has a colony (0x5EA6E0) and its pool entry (id c.vt+0xBC, pool [0xA52E54], last [0xA52E60]) exists:  0x5DAA90(colony; 1)   # destroyed
if c has a barbarian camp (vt+0x1C(0)):  Player[newOwner].0x565A00(x, y)                   # barbarians.md 9
if c has a goody hut (vt+0x3C(0)):       Player[newOwner].0x55C6B0(x, y, 0)                  # goody-huts.md trigger 2
if c has an airfield (vt+0x18(0)) and c.vt+0x118() != newOwner:                              # the improvement's owner differs
    nearest = 0x56D040(x, y, c.vt+0x118(), -1, -1, -1, 0)                                    # a city of the old holder (H: nearest)
    for every unit u (pool [0xA52E84]) with u.+0x24 == x and u.+0x28 == y and PRTO[u.+0x40].+0x9C == 2 (air), any owner:
        nearest != 0 ? (trace "setTerritory"; 0x5C71C0(u; nearest.x, nearest.y)) : 0x5BBBC0(u; 0,0,0,0,0,0,0)   # re-base or kill
    airfield A = pool [0xA52E3C][c.vt+0xBC()]  (last [0xA52E48]), if present:
        Player[newOwner].knowsTech([[0x9C7324] + 0x45C]) ? 0x5DB0D0(A; newOwner) (capture) : 0x5DAEC0(A; 1) (destroy)
if c has a radar tower (vt+0x84(0)) and c.vt+0x118() != newOwner:  pool [0xA52E24] (last [0xA52E30]) entry c.vt+0xBC():  0x5DB990(entry)
if c has an outpost (vt+0x4C(0)) and c.vt+0x118() != newOwner:     pool [0xA52E0C] (last [0xA52E18]) entry c.vt+0xBC():  0x5DB4E0(entry)
```

(The airfield, radar and outpost bodies `0x5DB0D0`, `0x5DAEC0`, `0x5DB990`, `0x5DB4E0` are specified in
`colonies.md` sections 6.2 to 6.6.)

**`Player::forgetTile(this; x, y)`** `0x55AF90`: redraw the tile when `this.slot` is the local slot (`0x4ED0C0(0x9F8700; x, y)`);
clear bit `slot` in cell word `+0x64`; set the remembered byte `cell[+0xAE + slot]` to the low byte of the group-0 overlay
word (`vt+0xA8(0)`). **`revealTile`** `0x55B030`: set bit `slot` in cell word `+0x64`, then `0x55B1A0(this; x, y)` (the
reveal/refresh body, **O**). Together they implement "a civ sees the 3x3 block around every tile it owns".

## 7. Culture stock of a city versus the empire total

* City stock `City +0x140[civ]` (section 2): feeds the level (section 3), the tie-break (5.4) and the flip (9).
* Empire culture `Player +0x183C` (the "rating" read by the tie-break, the orphan resolution, the flip and the AI
  attitude code `capture.md`): section 8.

## 8. The empire total `0x4F8E20(S)` with `S = Player +0x181C` (plain `ret`) **V**

```
if GOVT[P.+0xA0].+0x18C == 4:  return                                  # Anarchy: nothing
S.+0x24 = sum of City.+0x13C over every city owned by S.+0x28
S.+0x24 = max(0, S.+0x24)
S.+0x20 = max(0, S.+0x20 + S.+0x24)                                    # Player +0x183C
S.+0x1C = 0x5E72D0(0x9C71E4; S.+0x20)                                  # lookup in the table at 0x9C71E4 (rating index, O)
```

This corrects `capture.md` 7 (which concluded `Player +0x183C` is never written): the writes go through the
sub-object (`S.+0x20`), not by the absolute offset. It runs once per player turn (`turn.md`/`research.md` 2.1 step 6).

## 9. The culture flip `City::cultureFlip(this)` `0x4B28D0` (plain `ret`, returns bool in `al`) **V**

Called once at the start of every city turn (`0x4BE983`, `city-turn.md` step 1); **when it returns true the rest of
that city's turn is skipped** (`0x4BEC3F`).

```
if this.+0x58 > 0:  this.+0x58 -= 1;  return false
if ([0xA5267C] & 0x8000) == 0:  return false
owner = this.+0x28
if this.id == Player[owner].+0x2C:  return false                               # a capital never flips
ownCap = cityPool[Player[owner].+0x2C] if present (0 otherwise)
for j = 1 .. 31:
    if bit j of [0xA526C0] is clear or j == owner:  continue
    raceJ = Player[j].+0x20
    A = #citizens c of this city with c.+0x140 == raceJ
    B = #citizens c with c.+0x20 != 0 (resisting) and (raceJ == -1 or c.+0x140 == raceJ)
    T = A + B                                                                    # a resister of that race counts twice
    C = #{ k in 1..20 : the tile this + off(k) exists (neighbour step) and cell.owner == j }
    S = T + max(C - 2, 0)
    if S >= 10:  S = 10   elif S <= 0:  continue
    if this.+0x30 & 1:  S *= 2                                                   # city in disorder
    if this.+0x140[j] > this.+0x140[owner]:  S *= 2                              # civ j has the larger culture stake here
    Ro = Player[owner].+0x183C + 1;  Rj = Player[j].+0x183C + 1
    if Rj > 4 * Ro:  S *= 4   else:  S = trunc(Rj * S / Ro)                       # signed divide, toward zero
    if this.+0x30 & 2:  S = trunc(S / 2)                                          # (sar after sign fix)
    S -= 0x5A6060(x, y, 4, -1, 0, -1) * DIFF[Player[owner].+0x30].+0x78         # martial-law units on the city tile * citizens_quelled_by_military (happiness.md 3.4)
    if S <= 0:  continue
    D = 2000
    if ownCap != 0 and capital_j (cityPool[Player[j].+0x2C]) != 0:
         D = clamp(2000 * dist(this, capital_j) / dist(this, ownCap), 500, 8000)  # signed divide, toward zero; the clamp is inside this branch
    if Player[owner].0x558F50(j) != 0:  S = (int)(S * 1.4f)                      # j != owner always, so this is Player[owner].+0x15C8 != 0; fild S, fmul dword [0x666AB8] (= 1.39999997615814), then _ftol 0x64A230 (forces round-toward-zero)
    if (rand(D) & 0xFFFF) >= S:  continue                                         # rand(D) = 0x60BAB0(0xA526B4; D)
    Player[j].0x563410(this, 0, 1, 1)                                             # capture.md; arguments (city, 0, capture = 1, convert = 1)
    if this.+0x28 == j:  return true
return false
```

`dist(a, b)` (the game's own metric, not the exact move count): `dx = 0x441ED0(a.x, b.x)`, `dy = 0x437970(a.y, b.y)`,
where `0x441ED0` is `|a - b|` folded to `Wx - |a - b|` when wrap-x is on and `|a - b| > Wx/2` (signed half), and
`0x437970` the same in y with wrap-y and `Wy`; then with `mx = max(dx, dy)`, `mn = min(dx, dy)`:
`dist = mx - ((((dx + dy) / 2) - mn + 1) / 2)` (each `/ 2` truncates toward zero; `dist >= 1` for distinct cities).

Consequences:

* Per civ and turn the flip probability is `min(1, S / D)` (`rand(D)` is uniform on `0..D-1`, the flip happens when it is below `S`).
* `S` is capped at 10 before the modifiers; disorder and the larger stake double it each (at most 40), the rating ratio
  multiplies it by at most 4 (at most 160); the 1.4 factor can raise that to 224.
* The roll is drawn **only** for civs whose `S` is positive after every deduction; the first success ends the loop; a
  failed capture attempt (owner unchanged) continues with the next civ.
* The 1.4 factor is the single-precision constant `0x3FB33333` (= 1.39999997615814, slightly below 1.4), the product is
  formed in the x87 (53-bit or wider, so it is exact for these magnitudes) and `_ftol` truncates toward zero: `S = 5, 10, 15, 20, 80, 160`
  give `6, 13, 20, 27, 111, 223` (**E**: the real `0x4B28D0` and the real `_ftol` were run with the Windows default control word `0x27F`).
  A rounding implementation would give `7, 14, 21, 28, 112, 224` and is wrong.
* The counter `Player +0x15C8` is set to 1 when the civ loses its capital and decremented once per round (`victory.md`), so the
  1.4 factor applies for about one round after the capital is lost.
* `City +0x58` (armed by the capture routine with 1 or 10) only counts down when the flip test is reached, once per
  city turn, and no roll is made while it is positive.
* `D` cannot be zero: `dist` of two distinct cities is at least 1.

## 10. Golden vectors

Hand-derived from the text unless marked **E**.

| id | input | result |
|---|---|---|
| B1 | factor 10, stock 0, 9, 10, 99, 100, 99999, 100000 | levels 1, 1, 2, 2, 3, 5, 6 |
| B2 | factor 3, stock 80 | limits 3, 9, 27, 81: `80 >= 27` and `< 81` -> level 4 |
| B3 | tiles per level, **E** | 9, 21, 37, 61, 89, 137 |
| B4 | the enumerator, **E** | closed form of 4.1 equals the executed 169 entries |
| P1 | one level-2 land city at `(10,10)`; tile `(10,12)` (`d2 = 4`) and tile `(14,10)` (`d2 = 16`) | `(10,12)` claimed; `(14,10)` not (`16 > 10`) |
| P2 | one level-3 city; tile at offset `(4,0)` (k = 22) is Ocean (13) / is Plains | Ocean not claimed (`k >= 21`); Plains claimed (`16 <= 20`) |
| P3 | cities A `(0,0)` stock 50 and B `(4,0)` stock 80, both level 2; tile `(2,0)` | B is met first (offset `(2,0)`, k 2); A challenges at k 6: `cu(B) > cu(A)` -> B keeps |
| T1 | `cu(A) = 5`, `cu(C) = 3` | A |
| T2 | `cu(A) = 3`, `cu(C) = 5` | C |
| T3 | equal, `m = 0`, `z(A) = 1`, `z(C) = 2` | A (stage 1) |
| T4 | equal, `m = 0`, `z(A) = 2`, `z(C) = 1` | C |
| T5 | equal, `m = 0`, `z(A) = z(C) = 1`, `rat(A) = 9`, `rat(C) = 7` | A; with `rat` 7 and 9: C |
| T6 | as T5, equal ratings, owners 2 (A) and 3 (C) | A; owners 3 and 2: C; same owner: C |
| T7 | equal, `m = 2`, `z(A) == z(C)` | A (whatever `+0x360` holds); with `z(A) != z(C)`: C |
| T8 | equal, `m = 1`, `z` equal, `s(A) < s(C)` | A; `s` equal: final test; `s(A) > s(C)`: C |
| T9 | equal, `m = 5` | C |
| O1 | orphan tile; `nb(7) = nb(3) = 2`, `nb(1) = nb(5) = 0` | 2 |
| O2 | `nb(7) = 2`, `nb(3) = 3`, `nb(1) = nb(5) = 4` | A = 0 (disagree), B = 4 -> 4 |
| O3 | `nb(7) = nb(3) = 2`, `nb(1) = nb(5) = 3`, `rat(2) = 10`, `rat(3) = 12` | 3; with `rat(2) = 12`: 2; equal: 2 |
| D1 | `dist` with `(dx, dy)` = (0,0), (2,0), (1,1), (4,0), (6,2), (4,4), (8,0), (3,1), (0,2) | 0, 1, 1, 3, 5, 4, 6, 2, 1 (helpers `0x441ED0`/`0x437970` **E**; the combining formula by hand) |
| D2 | `0x441ED0(a, b)` with `Wx = 100`, wrap-x on: `(96,0)`, `(60,0)`, `(50,0)`; wrap-x off: `(96,0)` | 4, 40, 50; 96 (**E**) |
| F1 | `A = 1, B = 0, C = 0` | `S = 1` |
| F2 | `A = 2, B = 1, C = 5` | `S = 3 + 3 = 6` |
| F3 | `S = 6`, disorder, civ j has the larger stock | `6 -> 12 -> 24` |
| F4 | `S = 24`, `Ro = 11`, `Rj = 12` | `trunc(12 * 24 / 11) = 26` |
| F5 | `Rj = 50`, `Ro = 11` (`50 > 44`) | `S * 4` |
| F6 | `S = 5` after all deductions, capital lost (**E**) | `(int)(5 * 1.39999997615814) = 6`, flip when `rand(D) < 6`; `S = 80 -> 111` |
| F7 | `dist(city, capital_j) = 3`, `dist(city, ownCap) = 12` | `D = 2000 * 3 / 12 = 500` |
| F8 | `dist(city, capital_j) = 20`, `dist(city, ownCap) = 2` | `D = 20000` -> clamped to 8000 |
| F9 | no capital for j (or none for the owner) | `D = 2000`, no clamp |
| F10 | **E** the whole of section 9: the real `0x4B28D0` run in the emulator with the three callees stubbed (`0x5A6060` martial-law count, `0x60BAB0` RNG, `0x563410` capture) and a fake cell vtable | 300 random scenarios (citizens, resisters, tile counts, disorder, bit 1, stakes, ratings, capitals, difficulty, capital-lost counter) agreed on both `S` and `D` with section 9; plus: a tile count that depends on the wrap fold (city at the edge, wrap-x/wrap-y on and off) agrees; two candidate civs are tried in ascending slot order and the first success ends the loop; a failed roll continues; the rule bit `0x8000` clear, the owner's capital, and a positive cooldown (decremented, returns false) all return false before any roll |

## 11. Open items

1. **O** Meaning and writers of city words `+0x358/+0x35C/+0x360/+0x364` (5.4). **E** (2026-10-04): they are not
   zero in play; every city of the shipped `yolo.SAV` loaded in the emulator holds `+0x358 = 1`, `+0x364 = 1450`,
   `+0x35C` 0 or 6, `+0x360 = 0`. Corruption's rank ties read the same chain (`economy.md`).
2. **O** Name of the road-gating technology `[[0x9C7324] + 0x1A4]` in `cell.vt+0x64`; the reveal body `0x55B1A0`.
3. **O** `0x56D040(x, y, owner, -1, -1, -1, 0)` (used for the airfield re-base; **H**: nearest city of that owner).
4. (Resolved in `colonies.md`: the airfield/radar/outpost bodies `0x5DB0D0`, `0x5DAEC0`, `0x5DB990`, `0x5DB4E0` and the colony destroyer `0x5DAA90`.)
5. **O** The unit side of `0x5BA1D0` beyond the vision walk; `0x5C326C` (a caller of the level recompute inside `0x5C2E80`).
6. **O** `0x578E90` (`0xA0E270.notify`), `City vtable +0x38(1)` effects; the table behind `0x5E72D0(0x9C71E4)`.
7. **H** Every border-related effect of "tiles with the mark `0x40000`" outside `0x5D4830`/`0x5D4370` (the flag is read through
   `cell.vt+0xAC`; the interface preview `0x4DD800` is the only other reader of `0x5D4370`).
