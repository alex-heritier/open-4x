# The trade network: road labels, the city connection matrix, and resource availability

Clean-room specification of what decides **which cities are connected for which civilization**
(`0x57D980`, `0x57D840`, `0x57DE90`, `0x57DEF0`, `0x57E320`, `0x57E3D0`), of the breadth-first label
fill that feeds it (`0x580540` in its network mode), and of the resource-availability rebuild that is
chained behind every matrix update (`0x57E450`, `0x57EDD0`) together with the resource-deal records it
maintains (`Player +0x1614`). Every claim cites the address it was read at (raw disassembly; `r2`).
Tags: **V** read from the instructions, **H** hypothesis (data flow read, meaning inferred), **O**
open (not decoded). No reference implementation is attached: the ordered steps and the golden vectors
in section 9 are the contract.

Neighbours (not repeated): `primitives.md` 4 (the matrix **queries** `0x57F0A0`, `0x57F130`, the
resource predicates `0x4ADE30`, `0x55E730`, `0x55E850`), `capture.md` 12.2 (the AI's use of the
resource scan), `city-buildings.md` 6 (the call from the building routine), `turn.md` 195 (the per-turn
call), `diplomacy.md` 280 (resource deals in the diplomacy item table), `yields.md` 3 (cell vtable
slots), `rivers.md`.

The object `0xB72888` is **two things in one**: the connection matrix (this document) and the
general A\* path finder of the whole game (`0x580540` + the step test `0x57F360`; movement, goto, AI
reach, sea routes). Only the parts the network uses are specified here; the rest belongs to
`movement.md` (sections 7 and 8 give the interface and the entry guards of `0x57F360`; the body and `0x580540` stay open).

## 1. Data

### 1.1 The network object `0xB72888` (`this` of every function below)

| offset | meaning | evidence |
|---|---|---|
| `+0x04` | map width in tiles (the row stride in cells is `width / 2`, signed halving) | `0x580550..0x580573`, `0x57F372..0x57F399` (V) |
| `+0x08, +0x0C, +0x10, +0x14, +0x18` | the path-query cache key (`x1, y1, unit, player, flags` with bit 8 ignored); **reset by every matrix rebuild** to `-1, -1, 0, -1, 0` | `0x57D991..0x57D99D` (V); compare `0x5807B9..0x5807F7` (V) |
| `+0x20, +0x24`, `+0x2C, +0x30` | pointers to the two scratch arrays of the path finder (flag 1 selects the first pair) | `0x58079F`, `0x58082A` (V) |
| `+0x38` | the **connection matrix**: `512 x 512` dwords, address `+0x38 + ((A.id << 9) + B.id) * 4`; bit `p` set = "cities A and B are connected for civilization `p`" (`p` = slot 1..31) | `0x57D840`, `primitives.md` 4.3 (V) |

City ids index the matrix directly. The loops below run over the **pool index** `0..[0xA52E78]` of the
city pool `[0xA52E6C]` (8 bytes per entry, node pointer - `0x1C` = city), and use the loop counter as
the matrix index, so a city's pool index and `+0x20` id coincide (**H**: no code path observed where
they differ).

### 1.2 Per-cell words (map cell record, see `yields.md` 3 and `combat.md` 14.3)

| field | meaning | evidence |
|---|---|---|
| word at `cell +0x6E + 2*p`, `p` = 1..31 (bytes `0x70..0xAC`) | the **network label** of the tile for civilization `p`: the id of the city at which the fill that reached this tile started; `0xFFFF` (-1) = unlabelled | `0x580A01..0x580A06`, `0x57DA26`, `0x57DB6E` (V) |
| `dword [cell +0x58]` | bit `p` = tile is **seen** by civ `p` | `0x57DDAC`, `0x57F9C8` (V) |
| `byte [cell +5]` | owner civ of the tile (cell vtable slot `0x98`), 0 = none | `0x5DA06B..0x5DA0AD` (V) |
| `word [cell +0x6C]` | city whose citizen works the tile (not a network field; listed to avoid confusion) | `yields.md` 3 |

Cell index of tile `(x, y)`: `((width >> 1) * y + (x >> 1)) & 0xFFFF` with `width = [0x9C74D4]`, fetched
by `0x5D16A0` on the map object `0x9C736C`. The cell count is the word `[0x9C73AC]`.

### 1.3 Per-city and per-player data touched

* `city +0x9C` (dword): bit mask of **goods available to this city** (bit `good` for `good < 32`),
  rebuilt by `0x57E450` (section 7). Read by `0x4ADE30` (`primitives.md` 4.1).
* `Player +0x1614`: pointer to the **supply-record table**, `GOOD count x 32 x 3` bytes, record
  address `table + (good * 32 + civ) * 3` (section 8). `Player +0x1618`: pointer to a byte array
  `[GOOD count]`, the **spare-copy counter** of each good. `Player +0x2C`: capital city id (-1 none).
  `Player +0x40`: bit `0x20` = the "strategic resource" notice was shown, bit `0x40` = the "luxury"
  notice was shown.
* Globals: `[0xA526C0]` in-play mask (bit = slot), `[0xA526BC]` human mask, `[0x9FD4BC]` local
  slot, `0xA53BC8` the at-war table (byte at `0xA53BC8 + a * 0x20E4 + b` = `Player[a].+0xD30[b]`, non-zero = `a`
  at war with `b`; `diplomacy.md` 1; read at `0x57DD22`, `0x5DA0AD`, `0x57E35F`), `[0x9C755C]` map wrap flags (bit 0 wraps X, bit 1
  wraps Y), `[0x9C74D4]` width, `[0x9C74C0]` height, `[0x9C3D80]` BLDG count, `[0x9C3DA4]` GOOD
  count, `[0x9C3DB0]` PRTO count, `[0x9C3DBC]` tech count.

## 2. The three predicates that define connectivity

### 2.1 `Cell::roadFor(this = cell; p)` `0x5DA060`, `ret 4`

True when **both** hold (V, `0x5DA064..0x5DA0BC`):

1. cell vtable slot `+0x64` with argument 0 is true: the **road predicate** `0x5D9FF0` of `yields.md` 3
   ("overlay bit 0 set and the tile owner knows the Road job tech");
2. `p == 0`, or the tile has no owner (`byte [cell +5] == 0`), or the at-war byte
   `Player[p].+0xD30[tileOwner]` (table `0xA53BC8`; `diplomacy.md` 1: the only war flag) is 0 (the
   tile owner is **not at war with p**).

Rivers, railroads and sea are **not** part of this predicate; sea links come from section 5.

Whether a city centre tile always satisfies slot `+0x64` (an implicit road under every city) is
**H**: nothing in this routine special-cases cities, yet the fill (3) must start from a city tile whose
own `roadFor` is true or it labels nothing (`0x58070C..0x580733`, V), and every map in the corpus
labels its cities.

### 2.2 `City::hasAirTradeBuilding(this)` `0x4ADF90` and `City::hasWaterTradeBuilding(this)` `0x4AE030`

Both `ret` a bool (`setg`) and have no stack argument (V). They scan BLDG rows `b = 0..[0x9C3D80]-1`
(stride `0x110`, base `[0x9C40AC]`) and count rows for which (`0x4ADF90` loop `0x4ADFA9..0x4AE01C`):

* `0x4ACB50(city; b, 1)` (the city has the building; `primitives.md` 3.1) is true, **and**
* the row is not obsolete for the owner: `BLDG +0xE0` (obsolete-by tech) is `< 0`, or
  `Player::knowsTech(owner; BLDG +0xE0)` (`0x561440`) is false, **and**
* `BLDG +0xEC` (`improvement_flags`) has bit `0x200000` (**air trade**, `0x4ADFFD`) for `0x4ADF90`,
  bit `0x100000` (**water trade**, `0x4AE09D`) for `0x4AE030`.

The result is "count > 0".

## 3. The label fill: `0x580540` in network mode

`0x580540(this = 0xB72888; x1, y1, x2, y2, unit, p, flags, out)` (`ret 0x20`, eight arguments, V
from every call site). The network builder calls it as
`(x, y, -1, -1, 0, p, 4, 0)` (`0x57DAF1`): **destination `(-1, -1)`, no unit, flag bit 2**. In this mode
the function is a pure flood fill (V, `0x58070C..0x580A06` and the step test `0x57F949..0x57F964`):

1. Start tile `(x1, y1)`. The start cell must satisfy `roadFor(cell; p)` (`0x58070C..0x580733`), else
   return 0 and label nothing.
2. The **label** is `[0x56D2C0(x1, y1)] + 0x20`: the id of the city standing on the start tile
   (`0x580743..0x58074E`; `0x56D2C0` is the tile-to-city lookup).
3. Breadth-first over the 8 neighbours of every popped cell (direction table at `0x67057C`, offsets
   from `0x5E6E50(dir)`, the same ring as the city spiral). Neighbour coordinates wrap on X when
   `[0x9C755C] & 1` and on Y when `& 2` (`0x580A43..0x580A8A`); tiles outside
   `0 <= x < width`, `0 <= y < height` are skipped.
4. The step from a cell to a neighbour is allowed exactly when the **neighbour's** `roadFor(cell; p)`
   is true (step test `0x57F360`, no-unit branch at `0x57F949..0x57F964`: it returns `0` if the
   predicate holds, `-1` otherwise; the preceding gates reject out-of-range coordinates, direction
   codes outside `1..8` and non-adjacent tile pairs, `0x57F3A8..0x57F44A`).
5. Every cell **popped** gets `word [cell +0x6E + 2*p] = label` (`0x5809F5..0x580A06`). A cell that
   already carries the visited mark is skipped (`0x580AED`, bit `0x40000000` of the work word).
6. No cost accumulates (all steps cost 0); the return value is not used by the builder.

The war rule means a road tile owned by a civ at war with `p` is a wall **for p's network only**: the
same road is a link for a third civ.

## 4. The matrix builder: `0x57D980(this; p, relabel, flag, cityId)`

`ret 0x10` (V: `0x57DE89`). `p` = civilization slot, `relabel` and `flag` are bytes, `cityId` a dword
(-1 = all). Argument roles are read from the code (`relabel` is tested at `0x57D9F9`, `flag` becomes
bit `0x1000` of the sea query at `0x57DE07`, `cityId` is compared at `0x57DA0E`).

**Step 0, always.** Reset the query-cache key (`+0x08 = -1, +0x0C = -1, +0x10 = 0, +0x14 = -1,
+0x18 = 0`, `0x57D991..0x57D99D`). Then **clear bit `p` in every matrix word**
`M[a][b]` for `a, b` in `0..lastCity` (`0x57D9AF..0x57D9E3`). So a rebuild for `p` is always from
scratch; nothing of the old bits survives.

Then three passes `pass = 0, 1, 2` (`0x57D9ED`, loop test `0x57DE75`).

**Pass 0 (roads).**

* *0a, only if `relabel != 0`* (`0x57D9F9`): clear labels. With `cityId == -1` set the word
  `cell +0x6E + 2p` of **all** cells to -1; otherwise only cells whose label equals `cityId`
  (`0x57DA0E..0x57DA7B`). Then for every city `c` in pool order (`0x57DA8B..0x57DAFF`): if the label of
  `c`'s own tile for `p` is -1, run the fill `580540(c.x, c.y, -1, -1, 0, p, 4, 0)` (section 3). The
  fill therefore labels each road component with the id of the **lowest-pool-index city** that
  reaches it first among cities whose tile is still unlabelled.
  Without `relabel` this whole step is skipped and the existing labels are trusted.
* *0b, always* (`0x57DB01..0x57DC49`): for each city `i` in pool order with label `L = label(i's
  tile, p)`, `L != -1`: for every city `j > i` whose own tile label for `p` equals `L`, set bit `p`
  in `M[i][j]` and `M[j][i]` (`0x57DC02..0x57DC13`). The matrix diagonal is not set here.

  Effect: two cities are connected by road for `p` iff their tiles carry the same label.

**Pass 1 (air) and pass 2 (water).** `useAir = (pass == 1)` (`0x57DC53`). For every city `i` in pool
order, with `hasAir(i)` (`0x4ADF90`) when `useAir`, else `hasWater(i)` (`0x4AE030`) (`0x57DCAC..0x57DCB3`);
for every city `j > i`:

| condition (all must hold) | evidence |
|---|---|
| `Player[owner(i)].+0xD30[owner(j)] == 0` (not at war) | `0x57DD1D..0x57DD2A` (V) |
| the same building predicate holds for `j` | `0x57DD3E` / `0x57DDC7` |
| bit `p` of `M[i][j]` is not already set | `0x57DD4B..0x57DD62`, `0x57DDD0..0x57DDE7` |
| *air*: bit `p` of `cell(j's tile) +0x58` is set (the tile of `j` is **seen** by `p`; `i` is not checked) | `0x57DD68..0x57DDAF` |
| *water*: the path finder `580540(i.x, i.y, j.x, j.y, 0, p, 9 | (flag ? 0x1000 : 0), 0)` returns `> 0` | `0x57DDF5..0x57DE30` |

When they hold, call `0x57D840(this; p, i, j)` (section 5). So an air or water link needs the
building **in both cities** (not obsolete for each city's owner), no war between the two owners, and
- air: visibility of the far tile; water: a sea route for the generic (unit-less) path query. The
sea query's flag `9` and the `0x1000` extension are path-finder modes (**O**: `movement.md`); the
`flag` argument of `0x57D980` (= the argument of `0x57DE90`) only toggles `0x1000`.

Pass order matters: a pass-2 query can see the bits pass 0 and pass 1 already set because pass 0/1
results are tested through "bit `p` of `M[i][j]` already set" (skips pairs that are already connected)
but the path finder itself does not read the matrix.

## 5. The pair connect with transitive closure: `0x57D840(this; p, a, b)`

`ret 0xC` (V, `0x57D972`). `bit = 1 << p`; `last = [0xA52E78]`.

1. `M[a][b] |= bit; M[b][a] |= bit` (`0x57D857..0x57D87D`).
2. For `k = 0..last`: if `M[b][k] & bit`: `M[a][k] |= bit; M[k][a] |= bit` (`0x57D89E..0x57D8D5`).
   (`a` inherits every neighbour of `b`; this includes `k = a`, so `M[a][a]` becomes set.)
3. For `i = 0..last`: if `M[a][i] & bit`: for `j = 0..last`: if `M[a][j] & bit`: `M[i][j] |= bit;
   M[j][i] |= bit` (`0x57D8FE..0x57D96B`). The set of cities that now relate to `a` becomes a **full
   clique**, diagonal included.

Result: if the old relation was an equivalence (a disjoint union of cliques), the new one is the
equivalence with the two classes merged. Cities of other owners take part; the matrix is "which cities
`p`'s ships/roads/aircraft join", regardless of owner.

## 6. Entry points and when they run

| function | what it does | evidence |
|---|---|---|
| `0x57DE90(this; flag)` `ret 4` | for every slot `s = 1..31` with bit `s` in `[0xA526C0]`: `0x57D980(this; s, relabel = 0, flag, -1)`; then `0x57E450(this; 0)` | `0x57DE9F..0x57DED5` (V) |
| `0x57E320(this; city)` `ret 4`, called once from the city-founding routine `0x4AE6ED` | for every in-play slot `s` with `Player[s].+0xD30[city.owner] == 0` (not at war with the city's owner, `0x57E351..0x57E35F`): let `L` = the label word of the city's tile for `s`; if `L == -1` use `city.id` instead; call `0x57D980(this; s, relabel = 1, flag = 0, L-or-id)` (`0x57E388..0x57E39F`); then tail-jump `0x57E450(this; 0)` | `0x57E33C..0x57E3C9` (V) |
| `0x57E3D0(this; cityId, civ)` `ret 8`, called once from the city-destruction routine `0x4AF1AD` (pushes `ebp` then `edi`; the callee reads `cityId` from the first argument and `civ` from the second: **H** for the identity of the two registers) | for every slot `s = 1..31` that is in play **or** equals `civ`, and has `Player[s].+0xD30[civ] == 0` (not at war with `civ`; the byte address is `0xA55CAC + civ + (s-1)*0x20E4`, `0x57E3EB`, `0x57E409`): `0x57D980(this; s, relabel = 1, flag = 0, cityId)` (clears the labels equal to the destroyed city's id and re-fills from the remaining cities); then `0x57E450(this; 0)` | `0x57E3D1..0x57E43F` (V) |
| `0x57DEF0(this; x, y)` `ret 8` | **incremental update** after the road state of tile `(x, y)` changed, section 6.1 | `0x57DEF0..0x57E26C` |
| `0x57E450(this; quiet)` `ret 4` | resource availability (section 7) | `0x57E450` |

Callers of the entry points (call sites found by a direct-call census over the linear sweep of
`.text`; indirect calls are not covered):

| callee | callers (function: reason, **H** unless noted) |
|---|---|
| `0x57DE90` | `0x4ACF40` (building add/remove with air/water trade or `SAFE_SEA_TRAVEL`; V, `city-buildings.md` 6), `0x4F5201` in `0x4F5160`, `0x4F614E` in `0x4F5EF0`, `0x561B0B` in `0x561860` (acquiring an advance; `research.md` 10) |
| `0x57DEF0` | `0x5DA308`, `0x5DA449` in `0x5DA240` (tile road built / pillaged), `0x4762F1` in `0x4761E0` |
| `0x57D980` (direct) | `0x501F20` (two sites, `0x502262`, `0x502276`) and `0x5025B0` (`0x5026F6`, `0x50270A`) (diplomacy executors: war and peace, borders open or close), `0x561B42` in `0x561860` (`research.md` 10: `P.slot, 1, 0, -1`), `0x5D206C` in `0x5D1EA0` and `0x5D21B3` in `0x5D2150` (tile ownership change), `0x5D59A4` in `0x5D4830` (border recompute, `borders-culture.md` 5.3) |
| `0x57E450` | all of the above plus `0x476330` (`0x47668F`), `0x55CB20` (capital registration, `city-buildings.md` 4), `0x568950` (`0x568D3B`, civ destroyed), `0x5D5D00`, `0x5DA900`, `0x5DAA90`, `0x5DAD80`, `0x5DAEC0`, `0x5DB0D0` (tile-improvement / colony changes) |

The per-turn call is `0x57DE90(this; 1)` in the turn driver (`turn.md` 195, step 7): it passes
`flag = 1` and `relabel = 0`, i.e. it rebuilds **only the matrix** from the existing labels (the road
labels are kept current by the incremental update below and by the founding / destruction / diplomacy
paths).

### 6.1 `0x57DEF0(this; x, y)`: tile road changed

For each in-play slot `p` (loop `0x57DF14..0x57E254`, label word offset starts at `0x70` and steps 2):

* `cell = cell(x, y)`.
* **Case A: the tile no longer satisfies `roadFor(cell; p)`** (`0x57DF5F..0x57E0E4`):
  * if the tile's label is -1, nothing to do;
  * otherwise look at the 8 neighbours. For each neighbour `n` (offsets `5E6E50(1..8)`, wrapped,
    bounds-checked) record `same[n]` = "the neighbour's label equals the tile's label". Count the
    true ones (`0x57E043`, `cmp [esp+0x20], 2; jle`). If the count is `<= 2`, the tile cannot be a
    cut vertex: set its label to -1 and leave the matrix alone (`0x57E0C0..0x57E0E4`). If the count is
    `> 2`, test a fixed pattern over the eight booleans (the neighbours alternate between the four
    orthogonal and four diagonal positions; the test asks whether the "same" neighbours form one
    contiguous arc, `0x57E04A..0x57E0BC`; **O**: the exact contiguity predicate was read as a
    boolean network of the eight bytes at `esp+0x35..esp+0x3c` and not reduced further); if the
    pattern says the removal may split the component, run the full rebuild
    `0x57D980(this; p, relabel = 1, flag = 0, label)` (`0x57E21C..0x57E229`), otherwise clear the
    label as above.
* **Case B: the tile satisfies `roadFor`** (`0x57E0E9..0x57E31C`): scan the 8 neighbours with
  `roadFor(neighbour; p)`, collecting their labels `lab`:
  * the first neighbour sets the candidate `cand = lab` (can be -1);
  * a neighbour with label -1 sets `sawUnlabelled`; one with a label sets `sawLabelled`; if both are
    set the tile bridges a labelled and an unlabelled road piece: **full rebuild**
    `0x57D980(this; p, 1, 0, -1)` (`0x57E21A..0x57E229`);
  * two **different** labels `cand != lab`: relabel every cell whose label is `lab` to `cand`
    (`0x57E273..0x57E2B3`), and unless `M[cand][lab]` already has bit `p`, call
    `0x57D840(this; p, cand, lab)` (`0x57E2B3..0x57E2D4`);
  * finally the tile's own label is set to `cand` (`-1` when no road neighbour exists,
    `0x57E2F4..0x57E317`).
* After the slot loop, `0x57E450(this; 0)` (`0x57E260`).

## 7. Resource availability: `0x57E450(this; quiet)`, `ret 4`

Recomputes, for every civilization, **which luxury and strategic goods each city can use** and the
supply records of section 8. `quiet` (byte, `[esp+0x44]`) suppresses the three notifications. The
procedure is destructive: it clears and rebuilds everything it owns.

`good` classes: `0x5E3700(GOOD row)` = luxury or strategic; `0x5E3720` = luxury; `0x5E3730` =
strategic (`primitives.md` 4.2). GOOD row stride `0x5C`, base `[0x9C71D4]`.

**Pass 1: reset records (`0x57E48C..0x57E57C`).** For every `good` (`0 .. [0x9C3DA4]-1`) with
`0x5E3700` true, for every in-play civ `a` (slot 1..31) and every in-play civ `b` (slot 1..31):

* `a.rec(good, b).byte2 = 1`;
* if `a == b` (own record): `a.spare[good] = 0` (`Player +0x1618` array) and
  `a.rec(good, a).byte0 = 0`;
* else if `a.rec(good, b).byte0 != 0`: `a.rec(good, b).byte2 = 0` (the record is a deal that must be
  re-confirmed by pass 3's scan).

**Pass 1b (`0x57E5A3..0x57E5E4`):** for every city: `city +0x9C = 0`.

**Pass 2: scan all map cells (`0x57E5E9..0x57E94C`).** For cell index `i = 0 .. [0x9C73AC]-1`:

1. `owner` = if `0x5EA6E0(cell)` is true (the callee calls cell slot `+0xBC` and compares the word
   with `0xFFFF`, then calls slot `+0x18(0)`; the exact test is **O**) then cell slot `+0x118`, else
   slot `+0x98` (`0x57E609..0x57E638`). Skip if the result is 0. (**H**: slot `+0x98` is the tile
   owner byte, `combat.md` 14.3; slot `+0x118` is the same civ id read through the city on the tile,
   the sibling of slot `+0x114` in `yields.md` 3.)
2. `label = word [cell +0x6E + 2*owner]`; skip if -1 (`0x57E659..0x57E665`): the tile must be inside a
   labelled road component of its **owner**.
3. `good = 0x5DA1A0(cell)` (the resource id of the cell, `dword [cell +8]`, as a byte, with checks:
   skip `-1`, `>= [0x9C3DA4]`, or a class that is neither luxury nor strategic, `0x57E682..0x57E6BB`).
4. `0x57EDD0(this; good, owner, label)` (section 7.1).
5. Notification scan, only when `owner == [0x9FD4BC]` (local human) and the local player's `+0x40`
   lacks at least one of the bits `0x60` (`0x57E6D1..0x57E6F9`). For a luxury good (`0x5E3720`) and
   bit `0x40` not yet set: scan every city `c` (pool order): if `c.owner == local` and `c` is the
   label city or `M[c][label]` has either owner's bit, remember `c` (first hit) and set
   `Player +0x40 |= 0x40` and `found = 1` (`0x57E715..0x57E818`). Strategic good (`0x5E3730`): the
   same with bit `0x20` (`0x57E81D..0x57E931`).

**Pass 3: cancel unbacked deals (`0x57E951..0x57EA9E`).** For each `good` with `0x5E3700`, for every
in-play `a`, for every in-play `b`: let `r = a.rec(good, b)`. If `r.byte2 == 0` and `r.byte1 != 0`
(a *received* resource that no physical copy confirmed): set `a.rec(good, b).byte0 = 0` and
`b.rec(good, a).byte0 = 0`, set both `byte2 = 1`, and call
`0x503A10(Player[b]; a, kind, good, 1)` with `kind = 5` for a strategic good and `6` for a luxury
(`0x57EA33..0x57EA50`). `0x503A10` is the agreement cleanup of `victory.md` 12.4 (V: signature
`this, other, kind, payload, cancelFlag`; effect = end the deal, **H**).

**Pass 4 (`0x57EAB4..0x57EAE8`):** for every city, `0x4BCFF0(city)` (the city recompute; `yields.md`):
the resource set changes building availability and yields.

**Pass 5: the notices (`0x57EAF4..0x57ED72`).**

* If a luxury was found (`found_lux`): `Player[local] +0x40 |= 0x40` again. Unless `quiet`, `[0x9C757C]`
  (byte; the "loading a game" guard) is zero, and the multiplayer gate `0x47B530` is false: build the
  message `HAVE_LUXURY` (`0x72CFFC`) with the good's name (`GOOD row +4`, argument 0) and the found
  city's name (city `+0x1E0`, argument 1) via `0x61C5A0`, send it to the dialog host `0x49FC50()`
  (vtable `+0x170`; flags `0x4000` in single player), then `0x611530`.
* If a strategic was found: `Player[local] +0x40 |= 0x20`; same guards; scan the PRTO rows
  (`0 .. [0x9C3DB0]-1`, stride `0x138`, the three required-resource slots at `+0x7C`, `+0x80`, `+0x84`)
  for a row requiring `good`; for the first such row ask `0x4C04E0(city; prto, 1, 0, 0)` (can the found
  city build it, `buildable.md`). If one is found the message is `HAVE_RESOURCE` (`0x72CFD4`) with the
  good and the unit names (PRTO name at `+8`); otherwise `HAVE_RESOURCE_NO_UNIT` (`0x72CFE4`)
  (`0x57ECA7..0x57ED72`). **V** for the string selection and guards.

### 7.1 `0x57EDD0(this; good, slot, label)`, `ret 0xC`

`L` = the city with id `label` (null if out of range). `O` = Player `slot`.

1. **City masks.** For every city `c` (pool order) with `c.owner == slot` and `L != null`: if
   `c == L`, or `M[c][L]` has bit `slot` (for `slot == -1`: has either owner's bit), then
   `c +0x9C |= 1 << (good & 31)` (`0x57EE44..0x57EED8`; the shift count is the good index masked to
   five bits, as the hardware `shl` does).
2. **Capital gate.** Let `K` = the city with id `O.+0x2C` (null → return). If `K != L` and `M[K][L]`
   does not connect them (bit `slot`), **return** (`0x57EEF4..0x57EF86`): copies in a network that
   does not include the capital do not count toward trade or the supply records. (The city mask of
   step 1 is already set.)
3. **Consume the copy.** Scan civs `q = 1..31`, `q != slot`, in play, until one is claimed:
   if `O.rec(good, q).byte2 == 0` and `O.rec(good, q).byte1 == 0`, and `0x55E9B0(O; q, 1)` is true
   (can-trade test, section 8.1): set `O.rec(good, q).byte2 = 1` and `Q.rec(good, slot).byte2 = 1`
   (`Q` = Player `q`) and stop scanning (`0x57EFA3..0x57F01F`). A claimed copy backs one outstanding
   gift.
4. **No claim.** Otherwise, with `r = O.rec(good, slot)`: if `r.byte0 == 0` then `r.byte0 = 1;
   r.byte1 = 1` (`O` now has the good itself); else `O.spare[good] += 1` (`0x57F04A..0x57F085`).

So each map tile carrying a good, inside a road component of its owner that reaches the owner's
capital, is **one copy**; copies first re-confirm outstanding gifts of that good in civ order, then
make the owner's own record, then spares.

## 8. The supply record `rec(good, civ)` of Player `X`

Address `X.+0x1614 + (good * 32 + civ) * 3`: three bytes `{byte0, byte1, byte2}`.

| state | meaning (**H** for names; V for transitions below) |
|---|---|
| `X.rec(good, X)` = `{1, 1, *}` | `X` has the good itself |
| `X.rec(good, j)` = `{1, 1, *}`, `j != X` | `X` **receives** the good from `j` by a deal (supplier test of `0x55E730`, `capture.md` 12.2) |
| `X.rec(good, j)` = `{1, 0, *}`, `j != X` | `X` has **given** the good to `j` (the mirror of the previous line) |
| `byte2` | scratch of section 7: "confirmed this rebuild" |
| `X.spare[good]` (`+0x1618`) | extra copies beyond the first |

Readers: `0x55E700(P; good)` true iff `P.rec(good, P.slot)` has byte0 and byte1 both non-zero (V,
`0x55E717..0x55E728`); `0x55E730(P; good)`: any civ in play `j >= 1` with byte0 and byte1 non-zero
(V, `0x55E75D..0x55E79D`); `0x55E7B0(P; good)`: the **count** of such civs `j != P.slot` (V,
`0x55E7EE..0x55E83E`, argument -1 → 0); `0x55E850(P; good)`: count of records with byte0 set and
byte1 clear (`primitives.md` 4.1; V); `0x55E8E0(P; civ)`: whether any good row has `P.rec(good, civ)`
byte0 non-zero, stepping `0x60` bytes per good (V, `0x55E919..0x55E93B`); `0x55E940(P; civ)` the same
with byte0 **and** byte1.

### 8.1 `0x55E9B0(this = P; q, flag)`, `ret 8`: may `P` and `q` trade

False unless (V, `0x55E9E6..0x55EAC8`): `P` and `q` are in play; `q != 0`, `P.slot != 0`; `P.+0x2C != -1`
and `q`'s capital id (`Player[q] +0x2C`) `!= -1`; `P.+0xEB0 + 4*q` has bit 0 (**contact**,
`diplomacy.md` 1.1); if `flag != 0` then `P.+0xD30[q] == 0` (not at war); `P.+0x1030 + 4*q == 0` and
`Player[q].+0x1030 + 4*P.slot == 0` (no embargo word either way, `diplomacy.md` 1: `0x55EA48`,
`0x55EA64`); and finally the capitals are connected: `0x57F0A0(this = 0xB72888; capitalP, capitalQ,
-1)` (`primitives.md` 4.3, either-owner rule). Returns 1 or 0.

### 8.2 The deal API (the three writers of byte0/byte1)

* `0x55EAE0(this = P; good, to, a, b, c)`, `ret 0x14`: **may P give `good` to `to`**. V parts: `good != -1`;
  the good's prerequisite tech `GOOD +0x4C` is either `-1` or known by `P` (`0x55EB0A..0x55EB45`);
  `P` does not itself receive that good from anyone (no `j` with `P.rec(good, j)` byte0 and byte1 both
  set, `0x55EB78..0x55EBC2`); if the flag argument is set, `0x55E9B0(P; to, ...)` must hold
  (`0x55EBC6..0x55EBDB`); `P.rec(good, to).byte0 == 0` (no deal yet, `0x55EBEF`); then true if
  `P.spare[good] != 0` (`0x55EBFC`), else true if the flag (`[esp+0x20]`) is clear and
  `P.rec(good, P.slot).byte0 != 0` (`0x55EC0C..0x55EC29`), else false. Argument roles beyond the
  first two are **H**.
* `0x55EC80(this = P; good, to)`, `ret 8`: **give**. Requires `0x55EAE0(P; good, to, 0, 1, 1)`; then
  (V, `0x55ECB9..0x55ED57`): if `P.spare[good] != 0` decrement it, else clear `P.rec(good, P.slot).byte0`;
  `P.rec(good, to).byte0 = 1`; `Q.rec(good, P.slot).byte0 = 1`; `P.rec(good, to).byte1 = 0`;
  `Q.rec(good, P.slot).byte1 = 1`; return 1. (`0x5032BD` in the diplomacy executor, `diplomacy.md` 280.)
* `0x55ED60(this = P; good?, to?)`, `ret 8`: **take back / cancel**. First test: `P.rec(...).byte0 != 0`
  else return 0 (V, `0x55ED9C`); the rest (message to the local human, `0x49FC50`, `0xA360A7` guard,
  string lookup) is **O**.

## 9. Golden vectors

**G1. Pair closure.** Empty bit `p = 2` in a 4-city world (ids 0..3). Apply `0x57D840(p, 0, 1)`:
`M[0][0], M[0][1], M[1][0], M[1][1]` have bit 2 (diagonal included). Then `0x57D840(p, 2, 3)`: the
same for `{2, 3}`. Then `0x57D840(p, 1, 2)`: step 2 gives `M[1][2], M[2][1], M[1][3], M[3][1]`; step 3
makes row 1 relate to `{0, 1, 2, 3}`, so **all 16** entries have bit 2, including `M[0][3]`, a pair
never connected directly.

**G2. Rebuild is from scratch.** After G1, `0x57D980(p = 2, relabel = 0, flag = 0, -1)` with no
labels, no air and no water buildings clears bit 2 everywhere and leaves it clear: connectivity does not
persist across a rebuild unless the labels or the buildings reproduce it.

**G3. Road labels.** Two cities A (id 3) and B (id 7) on road tiles of a single 8-connected road
component owned by a civ not at war with `p`, plus a city C (id 9) on a separate component. A full
rebuild with `relabel = 1` labels the component of A with 3 (A has the lower pool index) and B's tile
with 3 too; C with 9. Matrix: `M[3][7]` and `M[7][3]` have bit `p`; `M[3][9]`, `M[7][9]` do not.
If the owner of the single tile joining A's and B's parts declares war on `p`, `roadFor(tile; p)` is
false for that tile: a rebuild leaves A and B with different labels and unconnected.

**G4. Road built between two components** (Case B of 6.1): the new tile has neighbour labels 3 and 9.
The result: every cell labelled 9 is relabelled 3, `0x57D840(p, 3, 9)` runs (unless already connected),
the tile gets label 3. A tile next to one labelled and one unlabelled road neighbour triggers a full
rebuild.

**G5. Gifts and copies.** Civs A=1, B=2, strategic good g. A owns one connected, capital-reaching tile
of g. A gives g to B (`0x55EC80(A; g, 2)`; precondition `0x55EAE0` true: A's own record `{1,1}`, no
spare). After the give: `A.rec(g,A) = {0,*}`, `A.rec(g,B) = {1,0}`, `B.rec(g,A) = {1,1}`.
Rebuild (`0x57E450`): pass 1 sets all `byte2 = 1`, then `A.rec(g,B).byte2 = B.rec(g,A).byte2 = 0` (byte0
set); `A.rec(g,A).byte0 = 0`. Pass 2 finds A's tile: `0x57EDD0(g, A, L)` claims q=B (byte2 == 0,
byte1 == 0 for `A.rec(g,B)`, trade allowed): both `byte2 = 1`; A's own record stays `{0}`; B can use
g (`0x55E700(B; g)`... false for own, `0x55E730(B; g)` true). Pass 3 finds nothing to cancel.
If A has a **second** tile of g: second call `0x57EDD0`: no record claimable (byte2 already 1), so
`A.rec(g,A) = {1,1}`: A keeps one copy. A **third** tile: `A.spare[g] = 1`.
If A then loses its only tile: pass 2 produces no claim, pass 3 sees `B.rec(g,A)` with `byte2 == 0`
and `byte1 != 0`: both `byte0` cleared and `0x503A10(Player A; 2, 5, g, 1)` ends the deal.

## 10. Open items

1. The exact contiguity predicate of Case A in 6.1 (`0x57E04A..0x57E0BC`, boolean network on eight
   bytes); a port may instead **always** run the full rebuild when the count of same-label
   neighbours is `> 2`, which is a superset (extra rebuilds, never a missed split) and yields the same
   labels.
2. Whether a city centre has an implicit road for `roadFor` (2.1) and the semantics of cell slot
   `+0x118` used in pass 2 step 1.
3. Which register is which in the `0x4AF1AD` call of `0x57E3D0` (city id and owner), and the argument
   roles of `0x55EAE0`, `0x55ED60`.
4. The sea-route query `0x580540(..., flags 9 | 0x1000, ...)`, the path-finder modes, and the unit
   step test `0x57F360` (`movement.md` 8, body open): they decide when two harbour cities are connected
   and how the *Trade over Sea / Ocean* techs (`0x561480` with `0x2000`/`0x4000`) gate water tiles.
5. The callers' individual reasons for calling `0x57D980` / `0x57E450` (section 6 table) beyond the
   evidence in the cited function names.
6. The 9 `byte0/byte1` combinations not listed in section 8 (e.g. `{0, 1}`) are never produced by the
   observed writers; if present in a save file they behave as "no deal" except for pass 3's
   `byte2 == 0 && byte1 != 0` test.
