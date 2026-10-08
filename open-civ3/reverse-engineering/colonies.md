# Colonies, airfields, radar towers and outposts (clean-room specification)

Executable: `Civ3Conquests.exe` (PE32, MSVC 6, image base `0x400000`). This document specifies the four **tile
colony kinds** and nothing else: how a site is judged legal, how the object is created and indexed, what each kind
contributes to a civilization's *vision*, how each kind is destroyed or transferred, and every trigger that calls those
routines. It is a specification for a clean-room port; no source from the reference implementation is needed to use it.
Companion documents: `worker-jobs.md` (the worker jobs that end in these creators, and the order tokens),
`trade-network.md` (a plain Colony is what connects a luxury or strategic resource; the network rebuild `0x57E450`),
`world-events.md` (resource disappearance and lava, which destroy colonies), `borders-culture.md` (a tile changing owner
destroys or transfers colonies), `barbarians.md` (camps; units stepping on foreign colonies), `NOTES.md` section 8
(the spiral enumerator `0x5E6E50`).

Tags: **V** read from the instructions with the function body opened; **H** hypothesis; **O** not decoded;
**E** executed. Every address is an absolute virtual address in the original image.

Notation: `f(this; a, b)` is an MSVC `__thiscall` (`ecx` = `this`, stack arguments after the semicolon, the callee pops).
`P` is a `Player` record (`0xA52E98 + slot * 0x20E4`, slot at `P+0x1C`). `cell(x,y)` is the map cell (`map.vt+0x30(x,y)`,
the cell index is `(x>>1) + (W>>1) * y`, see `NOTES.md`). `spiral(k)` is `0x5E6E50(k; &dx, &dy)`; the first 9, 25, 49 and
81 entries are the 3x3, 5x5, 7x7 and 9x9 blocks around the centre (entry 0 is the centre). `rebuild` means
`0x57E450(0xB72888; 0)`, the full trade-network rebuild (`trade-network.md`).

---

## 1. The four kinds

| kind | name in this document | overlay bit (plane 0) | pool (array / free head / free count / last / capacity) | object size | per-player counter (dword in `Player`) | world counter |
|---|---|---|---|---|---|---|
| 0 | **Colony** (plain) | none; "plain" is defined as colony id valid and bits 29-31 clear (`0x5EA6E0`) | `0xA52E54 / E58 / E5C / E60 / E64`, header `0xA52E50` | `0x30` | `+0x198` | `[0xA52694]` |
| 1 | **Airfield** | bit 29 (`0x20000000`) | `0xA52E3C / E40 / E44 / E48 / E4C`, header `0xA52E38` | `0x34` | `+0x11C8` | `[0xA527A0]` |
| 2 | **Radar Tower** | bit 30 (`0x40000000`) | `0xA52E24 / E28 / E2C / E30 / E34`, header `0xA52E20` | `0x30` | `+0x11DC` | `[0xA527A8]` |
| 3 | **Outpost** | bit 31 (`0x80000000`) | `0xA52E0C / E10 / E14 / E18 / E1C`, header `0xA52E08` | `0x30` | `+0x11E0` | `[0xA527AC]` |

(All **V**: the sizes are the `operator new` arguments in the creators `0x566A20`, `0x566D50`, `0x567080`, `0x5673A0`;
the counters are the `inc` / `dec` instructions in the creators and destroyers of section 6. `Player +0x194` was
earlier read as the city count; it is not touched by any routine here.)

A **tile can host at most one colony object**: the tile's *colony id* is a single word per cell (`cell +0x1C`; read by
`cell.vt+0xBC`, written by `cell.vt+0xF8(id)`, `-1` = none). The overlay bit of a kind tells which pool the id indexes.
The validity predicates used everywhere (cell vtable `0x6701C8`, **V**):

| predicate | address | meaning |
|---|---|---|
| `vt+0x68` | `0x5EA910` | the cell has a colony id (any kind) |
| `0x5EA6E0` | | plain Colony: colony id valid **and** overlay bits 29, 30, 31 all clear |
| `vt+0x18(0)` | | overlay bit 29 (airfield) |
| `vt+0x84(0)` | | overlay bit 30 (radar tower) |
| `vt+0x4C(0)` | | overlay bit 31 (outpost) |
| `vt+0x118` | `0x5D9EF0` | **owner slot of the colony** on the tile: the object is taken from the airfield pool when bit 29 is set, from the radar / outpost pool for bits 30 / 31, else from the plain pool; the owner byte is object `+0x2C` (`-1` none). It is *not* the border owner `vt+0x98`. |
| `0x5EA6C0` | | the cell has a city |

### 1.1 The object

Every colony object is addressed through the **pool node** pointer `n`; the object base is `n - 0x1C`. Fields
(offsets from the object base, **V**: written by the inits of section 5):

| offset | size | field |
|---|---|---|
| `+0x20` | dword | id in its pool (equal to the cell's colony id) |
| `+0x24` | word | x |
| `+0x28` | word | y |
| `+0x2C` | byte | owner slot |
| `+0x30` | byte | airfield only; set to `0` by the airfield init; no reader found (**O**) |

### 1.2 The pool protocol (`0x566A20`, identical in the other three creators with their own globals) **V**

A pool is an array of 8-byte entries `{ next, node }`: `node` is the node pointer (`0` = no object) and `next` is `-1`
for a live entry, or the index of the next free entry for a free one. Header fields: array pointer (`E54`), free head
(`E58`, `-1` empty), free count (`E5C`), `last` (`E60`, highest index in use or free), capacity (`E64`).

*Allocate with `id = -1`* (what every gameplay path passes):
1. If the array pointer is null, initialise the header to capacity 100 (`0x56BC60(header; 100)`).
2. If `last == capacity - 1` and the free count is `0`, grow the array (`0x4C22C0(header)`; growth rule **O**).
3. If the free count `> 0`: `id = freeHead`; `freeHead = entry[id].next`; `freeCount--`. Else `id = ++last`.
4. Allocate the object (`new`, size from section 1), run its constructor (`0x5DA860` / `0x5DAD00` / `0x5DB7D0` /
   `0x5DB230`), store `node = object + 0x1C`, `entry.next = -1`.

*Allocate with `id >= 0`* (used when an id must be reproduced): if `entry[id]` exists and holds an object, the creator
**fails (returns null)**. Otherwise, if `id > last`, indices `last+1 .. id-1` are threaded onto the free list (each gets
`next = freeHead`, `freeHead = that index`, `freeCount++`) and `last = id`; if `id <= last` the entry is unlinked from
the free list (head or interior; `freeCount--`). Then the object is created and stored at `entry[id]`.

*Release* (done by the destroyers, section 6): destructor `0x5B0380(object)`, `entry.node = 0`, `entry.next = freeHead`,
`freeHead = id`, `freeCount++`. `last` is never lowered, so loops over a pool run `i = 0 .. last` and must skip entries
whose `node` is `0` (and nodes whose `node - 0x1C` is `0`). **V** for the destroyers; the growth step is **O**.

---

## 2. The site-legality gate for founding a Colony (`0x5D7080`)

Only the plain Colony has a rule-driven site test (airfield, radar and outpost use the worker-job gates of
`worker-jobs.md`). `Map::colonySiteCheck(map; x, y, civ)` = `0x5D7080`, `ret 0xC`; returns **0 = allowed**, otherwise a
reason code. Callers (all treat nonzero as "refuse"): the order tester `Unit::canDoAction` `0x5C1AD0` (`0x5C209E`, token
`0x20000001`), the UI handler `0x4DAA70` (`0x4DB069`, followed by the build `0x5B36E0`), and the automation / AI code
`0x45D814` (`0x45E1C8`, `0x45EBB0`) and `0x4620D0` (`0x4620EC`).

```text
r = 0x5F3060(map; x, y, civ, 1)        // = cellColonyGate(cell(x,y), civ, 1)   0x5F3090, section 2.1
if r != 0: return r
res = cell.+0x08                        // the resource id of the tile, -1 none
if res == -1:                                                          return 1
G = GOOD[res]                           // table [0x9C71D4], stride 0x5C
if !P[civ].knowsTech(G.+0x4C):                                          return 1     // 0x561440; -1 means "known"
if !(G.+0x3C == 1 or G.+0x3C == 2):                                     return 1     // 0x5E3700: luxury or strategic
if cell.vt+0x98() != 0:                                                 return 6     // the tile has a border owner
return 0
```

`knowsTech(-1)` is true (`trade-network.md`, `primitives.md`), so a luxury resource (whose reveal tech is `-1`) is always
visible; a strategic resource needs its revealing technology. (**V**: `0x5D7080..0x5D717A`.)

### 2.1 `cellColonyGate` `0x5F3090` (`ret 0xC`, called with `(cell, civ, flag)`; `civ` is overwritten and unused) **V**

```text
good = GOOD[cell.vt+0x9C()]            // the resource record of the cell, null if none
if good == null or !(good.class in {1, 2}):                             return 1     // no luxury / strategic resource on the tile
if cell has a city (0x5EA6C0):                                          return 2
if flag != 0 and cell is a plain Colony (0x5EA6E0):                     return 2
if cell has a barbarian camp (vt+0x1C(0)):                              return 2
T = TERR[cell.vt+0xC8()]                // the terrain record, stride 0xF0
if T.+0x79 != 0 (allow colonies):                                       return 0
return cell.vt+0x8C() (is water) ? 3 : 4
```

Return codes (as both functions yield them): **0** allowed; **1** no visible luxury / strategic resource; **2** a city,
a colony or a camp is already there; **3** water terrain that does not allow colonies; **4** land terrain that does not
allow colonies; **6** the tile has a border owner (any civilization, including the founder's own). A port that shows
text for a refusal must map these codes; the strings are **O**.

Consequences (derived): a Colony may be founded **only** on an unowned tile that carries a visible luxury or strategic
resource, whose terrain has `allow_colonies` set. `TERR +0x79` is the second byte of the allow-flag block `+0x78..+0x7F`
(`worker-jobs.md` section 1). In the shipped `conquests.biq` (decoded dump, 14 TERR rows) `allow_colonies = 1` for terrain
ids 0-8 (Desert, Plains, Grassland, Tundra, Flood Plain, Hills, Mountains, Forest, Jungle) and `0` for ids 9-13 (Marsh,
Volcano, Coast, Sea, Ocean).

---

## 3. Founding and building

The unit methods (all **V**, they differ only in the creator they call):

| method | creator called | arguments |
|---|---|---|
| `0x5B36E0` Unit::foundColony | `0x566A20(P; x, y, -1, 1)` | `P = Player[unit.+0x34]`, `(x, y) = (unit.+0x24, unit.+0x28)`; `rebuild = 1` |
| `0x5B3790` Unit::buildAirfield | `0x566D50(P; x, y, -1, 1)` | same |
| `0x5B3840` Unit::buildRadar | `0x567080(P; x, y, -1)` | |
| `0x5B38F0` Unit::buildOutpost | `0x5673A0(P; x, y, -1)` | |

After the creator each method: (1) calls the UI hook `0x4E69D0(0x9F8700; unit, 0, 0)`; (2) if the hook returned nonzero and
the tile's discovered-by mask (`cell +0x58`) has the bit `1 << Player[[0x9FD4BC]].slot` (the **local human** slot), sets the
redraw flag `[0xA281C4] = 1`; (3) **kills the unit**: `Unit::kill(unit; 0, 1, 0, 0, 0, 0, 0)`
(`0x5BBBC0`, `unit-turn.md` section 5). So the unit that founds a colony or completes an airfield, radar or outpost job is
**consumed**. The creator's result is not tested: the unit dies even if the creator refused (for example a second Colony
on a plain-Colony tile).

How the airfield, radar and outpost methods are reached: the worker-job completion `Cell::applyOverlay(cell; layer, mask,
x, y, unit)` `0x5DA240` (`ret 0x14`): if `layer == 0` and the mask has bit 29, call `0x5B3790(unit)`; bit 31,
`0x5B38F0(unit)`; bit 30, `0x5B3840(unit)`; then in every case `cell.vt+0xE0(layer, mask, x, y)` (set the overlay bits).
(`worker-jobs.md` section 8.) The Colony is reached only through the order `0x20000001` (UI `0x4DAA70`).

The scenario placer `Map::placeColony(map; record, ownerSlot)` `0x5D24B0` (`ret 8`): clears overlay mask `0xE0000000` at
`(word record+8, word record+0xC)`, then by `dword record+0x10` (0 Colony, 1 Airfield, 2 Radar, 3 Outpost; other values do
nothing) calls the creator with `id = -1` for the owner slot argument (`rebuild = 0` for the Colony and the Airfield) and
prints a debug line (`"\tPlaced colony for %d..."` and siblings). This is how a scenario's `CLNY` records become objects
(`biq-format.md`). **V**.

---

## 4. The creators

All four refuse (return `0`) when the tile already has the kind's marker, allocate by the pool protocol of 1.2, increment
the two counters, write the cell's colony id, then run the init. Common order (**V**):

```text
create<kind>(P; x, y, id, flag):
    cell = cell(x, y)
    if refuse-test(cell): return 0                          // per kind below
    obj = poolAlloc(kind, id)                               // 1.2; null -> return 0
    P.counter[kind]++ ; world.counter[kind]++               // section 1
    cell.vt+0xF8(id)                                        // write the colony id
    init<kind>(obj; id, x, y, P.slot, flag)                 // section 5
    return obj
```

| creator | arguments | refuse test |
|---|---|---|
| `0x566A20` Colony, `ret 0x10` | `(x, y, id, rebuild)` | the cell already is a plain Colony (`0x5EA6E0`) |
| `0x566D50` Airfield, `ret 0x10` | `(x, y, id, rebuild)` | overlay bit 29 already set |
| `0x567080` Radar, `ret 0xC` | `(x, y, id)` | overlay bit 30 already set (`0x567080..0x5670D0`; the register left in `eax` is returned, which is not a valid object; every caller discards the result) |
| `0x5673A0` Outpost, `ret 0xC` | `(x, y, id)` | overlay bit 31 already set (`0x5673E8..0x5673ED`, same shape) |

The refuse tests look only at the *marker*, not at ownership, terrain or neighbours; legality is the job gate's
(`worker-jobs.md` section 3, `0x55EFA0`). Every caller discards the creators' results except the pool protocol's own
failure paths, so a refused creator is silent.

---

## 5. Initialisation: what each kind sets and sees

The init routines write the object fields (section 1.1), set the overlay bits, optionally rebuild the trade network, and
**grant vision**. Vision uses three `Player` primitives (`this` = the owning player; all `ret 8`, arguments `(x, y)`):

| primitive | address | effect |
|---|---|---|
| `seeTile` | `0x55AE90` | `cell +0x60 |= bit(slot)` ("currently seen" mask), then `discover(P; x, y)` `0x55B1A0` (sets the `+0x58` discovered bit and redraws if the tile is new; `research.md` 10.3) |
| `radarOn` | `0x55AEF0` | `cell +0xD4 |= bit(slot)` (the "radar sees" mask) |
| `radarOff` | `0x55AF40` | `cell +0xD4 &= ~bit(slot)` |
| `unseeTile` | `0x55ADF0` | if the player is the local human, schedule a redraw; `cell +0x60 &= ~bit(slot)`; `cell[+0xAE + slot] = low byte of the overlay word` (the **remembered overlay** the player will keep seeing) |

`bit(slot)` is `1 << P.slot` (`P+0x1C`, which is the dword at `0xA52EB4 + slot * 0x20E4`); the per-player cell masks
(`+0x58`, `+0x60`, `+0xD4`) are dword bit sets indexed by slot. Every enumeration below skips tiles outside the map (wrap rules of `0x426C00` /
`0x426C40` / `0x426BD0`, `NOTES.md`).

| kind | init (`ret`) | sets | vision granted |
|---|---|---|---|
| Colony | `0x5DA900(obj; id, x, y, owner, rebuild)` `ret 0x14` | fields; overlay mask `3` (road + railroad) is **set** on the tile, then mask `0xE0000000` is cleared; if `rebuild` then `0x57E450` | `seeTile` over `spiral(0..8)` (3x3) |
| Airfield | `0x5DAD80(obj; id, x, y, owner, rebuild)` `ret 0x14` | fields; byte `+0x30 = 0`; overlay bit 29; if `rebuild` then `0x57E450` | `seeTile` over `spiral(0..8)` |
| Radar | `0x5DB850(obj; id, x, y, owner)` `ret 0x10` | fields; overlay bit 30 | for `k = 0..24` (5x5): `radarOn` **and** `seeTile` |
| Outpost | `0x5DB2B0(obj; id, x, y, owner)` `ret 0x10` | fields; overlay bit 31 | `seeTile` over the first `N` spiral entries, `N = 49` if the tile's terrain is Mountains (6) or Volcano (10), `25` if Hills (5), else `9` |

(**V** for the shapes. The relative order of the overlay write and the vision loop inside each init was not recorded;
it matters only if `discover` `0x55B1A0` reads the overlay word, which is **O**.)

Notes: the Colony's mask-3 write gives the tile a road and a railroad, which is what makes the resource connectable by the
network (`trade-network.md`); it overrides the owner's lack of the Road / Railroad technology, and the destroyer undoes it
(6.1). The road and railroad bits are set through the overlay setter without the `0x57DEF0` incremental update; the
explicit `rebuild` argument replaces it.

### 5.1 The observation test `0x55AC10` (`ret 0xC`) **V**

`Player::isObserved(P; x, y, skipColonyPlain)` is used by the destroyers to decide which tiles to *unsee*. It enumerates
`k = 0..80` (the 9x9 block around `(x, y)`) and returns 1 as soon as one tile `T = (x,y) + spiral(k)` satisfies any of:

1. `k < 25` and `T` has a radar tower owned by `P` (`cell(T).vt+0x118 == P.slot`);
2. `skipColonyPlain == 0` and `k < 9` and `T` has a plain Colony **or an Airfield** owned by `P`;
3. `T` has an outpost owned by `P`, with `k < 49` if `T`'s terrain is **Mountains (6)** only, else `k < 25`.

The result is `0` otherwise. (Note the asymmetry with the init: the init gives an outpost 49 tiles on Mountains **or
Volcano** and 25 on **Hills**; the observation test gives 49 on Mountains only and 25 on everything else. A faithful port
reproduces both; the visible effect is that destroying a neighbouring structure can leave Hills / Volcano outpost
vision with a different footprint than it was created with.)

---

## 6. The destroyers

All four run the same shape: update counters, free the pool slot, clear the tile's colony id (`cell.vt+0xF8(-1)`), clear
the kind's overlay bit, optionally remove road / railroad (Colony only), optionally `rebuild`, then **unsee** every tile
of the kind's footprint that is no longer observed by any remaining structure, and set the redraw flag `[0xA281C4] = 1`.

### 6.1 Colony `0x5DAA90(obj; rebuild)` `ret 4` **V**

```text
P = Player[obj.owner]
cell.vt+0xF8(-1)                                    // colony id cleared
P.+0x198-- ; [0xA52694]--
pool release (1.2)                                  // destructor 0x5B0380
if !P.knowsTech([[0x9C7324] + 0x1A4]):  cell.vt+0xCC(0, 1, x, y)    // clear the road bit   (TFRM row 3 = Road, field +0x48)
if !P.knowsTech([[0x9C7324] + 0x218]):  cell.vt+0xCC(0, 2, x, y)    // clear the rail bit   (TFRM row 4 = Railroad)
if rebuild: 0x57E450
for k in 0..8: T = (x,y)+spiral(k); if !isObserved(P; T, 0): unseeTile(P; T)
[0xA281C4] = 1
```

Clearing the road bit also clears the rail bit (`0x5EAB40`), so for an owner without the Road technology both go. If the
owner knows Road but not Railroad, only the rail bit is removed. If the owner knows both, the tile keeps its road and rail
(the *tile improvement outlives the colony*). The plain-Colony overlay has no marker bit, so nothing else is cleared.

### 6.2 Airfield `0x5DAEC0(obj; rebuild)` `ret 4` **V**

Same shape: `P.+0x11C8--`, `[0xA527A0]--`, release, `cell.vt+0xF8(-1)`, clear overlay bit 29 (`vt+0xCC(0, 0x20000000, ...)`),
no road removal, optional `rebuild`, `unseeTile` over `spiral(0..8)` for tiles with `!isObserved(P; T, 0)`, redraw flag.

### 6.3 Radar Tower `0x5DB990(obj)` plain `ret` (no arguments, no rebuild) **V**

```text
R = (obj.x, obj.y) ; o = obj.owner ; id = obj.id ; P = Player[o]
cell(R).vt+0xF8(-1)                                 // colony id cleared first
P.+0x11DC-- ; [0xA527A8]--
pool release (1.2)                                  // destructor 0x5B0380, node = 0, free list
cell(R).vt+0xCC(0, 0x40000000, R.x, R.y)            // overlay bit 30 cleared: the tower is gone before the scans
for t in 0..24:                                     // the 5x5 of the destroyed tower
    T = R + spiral(t) with the wrap of 0x426C00 / 0x426C40; skip if T is off the map
    covered = false
    for u in 1..24:                                 // the 5x5 of T, T itself excluded
        S = T + spiral(u) wrapped; skip if S is off the map
        if S.x == R.x or S.y == R.y: continue       // quirk 3: compared with the raw coordinates of the destroyed tower
        if cell(S) has overlay bit 30 and cell(S).vt+0x118() == o:  covered = true     // another tower of the same owner
    if !covered: radarOff(P; T)
    if !isObserved(P; T, 0): unseeTile(P; T)
[0xA281C4] = 1
```

(The `covered` flag is only ever set, never tested early; the inner loop always runs to `u = 24`. **V**,
`0x5DB990..0x5DBCD1`.)

### 6.4 Outpost `0x5DB4E0(obj)` plain `ret` **V**

`P.+0x11E0--`, `[0xA527AC]--`, release, `cell.vt+0xF8(-1)`, clear overlay bit 31. Then, with `N` chosen by terrain exactly
as in the init (49 for terrain 6 or 10, 25 for 5, else 9): for `k = 0..N-1`, `T = (x,y)+spiral(k)`, if
`!isObserved(P; T, 0)` then `unseeTile(P; T)`. Redraw flag set.

### 6.5 The map-level wrappers `0x5D6360` (radar) and `0x5D6430` (outpost) **V**

`ret 8`, arguments `(x, y)` (the cell index is `(x>>1) + (W>>1) * y`). If the cell has the kind's overlay bit
(`vt+0x84(0)` / `vt+0x4C(0)`), read the colony id (`vt+0xBC`) and fetch the object from that kind's pool (the id must be
`>= 0`, `<= last`, with a non-null node and object); if present call `0x5DB990(obj)` / `0x5DB4E0(obj)`. A missing object
or bit is silently ignored. The Colony and Airfield have no such wrapper; their callers do the pool lookup inline
(`pool[id]` with the same checks) and pass `rebuild`.

### 6.6 Airfield transfer `0x5DB0D0(af; newOwner)` `ret 4` **V**

```text
if newOwner == 0:  0x5DAEC0(af; 1) ; return                    // a capture by slot 0 destroys the airfield
old = af.owner (byte +0x2C)
Player[old].+0x11C8-- ; Player[newOwner].+0x11C8++
af.owner = newOwner & 0xFF
0x57E450                                                        // network rebuild
for k in 0..8: T = (af.x, af.y) + spiral(k)                     // existing tiles only
    seeTile(Player[newOwner]; T)
    if !isObserved(Player[old]; T, 0): unseeTile(Player[old]; T)
```

The world counter `[0xA527A0]` is unchanged (the airfield stays); the cell's colony id and overlay bit 29 are unchanged.

---

## 7. Every trigger of a creator, destroyer or transfer (callers found by a whole-image scan)

| caller | what it does with colonies |
|---|---|
| `0x4DAA70` (UI "found colony") | gate `0x5D7080`, then `0x5B36E0` |
| `0x5C1AD0` `Unit::canDoAction` | gate for order `0x20000001` (`worker-jobs.md` 2) |
| `0x45D814`, `0x4620D0` | AI / automation gate calls (`0x5D7080`) |
| `0x5DA240` `Cell::applyOverlay` | airfield / outpost / radar creation (section 3) |
| `0x461470` `Unit::workOnTile` | Fortress and Barricade completion destroy airfield (`0x5DAEC0`), radar (`0x5D6360`), outpost (`0x5D6430`) before clearing `0xE0000000` (`worker-jobs.md` 7 step 3) |
| `0x4AE2A0` `City::init` | founding a city on a tile destroys, in this order: the plain Colony (`0x5EA6E0` then pool lookup then `0x5DAA90(obj; 1)`), the Airfield (`vt+0x18`, `0x5DAEC0(obj; 1)`), the Radar (`vt+0x84`, `0x5D6360`), the Outpost (`vt+0x4C`, `0x5D6430`) |
| `0x5D3AB0` tile-owner change | `borders-culture.md` section 6: a plain Colony is destroyed; an airfield of another owner is **captured** by `0x5DB0D0` if the new owner knows the technology `[[0x9C7324] + 0x45C]` (TFRM row 9 = Airfield, field `+0x48`; Flight in the shipped rules) else destroyed; radar and outpost of another owner are destroyed (`0x5DB990`, `0x5DB4E0`); every air unit on the tile is re-based to the nearest city of the old holder (`0x5C71C0`) or killed |
| `0x5BD220` `Unit::setPosition` | a unit entering a tile with a colony of **another owner** (`unit.+0x34 != colony owner`; **no war or treaty test was found in these blocks**, **H** that the movement rules keep peaceful units out): incident `0x5631B0(Player[unit.owner]; colonyOwner, 2)`. A **plain Colony** (`0x5BD8E5..0x5BDB41`): if the colony's owner is the local human a popup is shown (`COLONY_BARBS` when the entering owner's slot is `0`, else `COLONY_CIV`, both with an optional multiplayer flag `0x4000`), and in every case the colony is destroyed, `0x5DAA90(obj; 1)`. An **airfield**: captured (`0x5DB0D0`) when the entrant knows the Flight-row technology, else destroyed; **radar** and **outpost** are destroyed (`barbarians.md` 8) |
| `0x5D5D00` resource disappearance | a plain Colony on a tile whose resource disappears is destroyed (`world-events.md` 2.3 step 4) |
| `0x4F4040` lava | the airfield, outpost and radar on a lava tile are destroyed (`world-events.md` 5.4 steps 3-4) |
| `0x5B4070` nuclear blast | per tile in the blast: plain Colony, Airfield, Radar and Outpost are destroyed (`0x5DAA90`, `0x5DAEC0`, `0x5D6360`, `0x5D6430`) |
| `0x5B4DC0` bombard / improvement destruction | section 8 |
| `0x568950` civ destroyed | for each pool, `i = 0 .. last` (re-reading `last` after every destruction), every object whose owner byte equals the destroyed slot is destroyed (`0x5DAA90(obj; 0)`, `0x5DAEC0(obj; 0)`, `0x5DB990`, `0x5DB4E0`); a "network rebuild needed" flag is raised for the Colony and Airfield cases |
| `0x567C80` player initializer | after killing every unit and destroying every city owned by the slot, destroys every **plain** Colony of that owner, `0x5DAA90(obj; 1)` (`0x567F83..0x567FD0`). It is the only colony call in the function: airfields, radar towers and outposts of the slot are **not** removed by it (quirk 9; `government.md` 8) |
| `0x5D24B0` scenario placer | creation only (section 3) |

(**V** for the call sites; the preconditions of `0x5D3AB0` and `0x5BD220` are specified in the documents cited.)

---

## 8. Improvement destruction by bombard: `0x5B4DC0` (`ret 0x10`) — the destructive tail **V**, the roll head **V**

`Unit::destroyImprovement(unit; rollFlag, x, y, victimCiv)`; the attacker of `combat.md` 8.8 calls it with
`rollFlag = 0` after its own tile-defence roll has already failed (`0x5B3AB0` / `0x4A2460`).

1. If `rollFlag != 0`: `ok = 0x4A2460(0x9C7348; unit, x, y)`, the **same tile-defence roll as `combat.md` 8.8**
   (`v = ((terrain + tile + 100) * 16) / 100`, `odds = clamp(1024 * v / (v + PRTO[unit].+0x48), 1, 1023)`, then a 1024-roll).
   On failure call `0x5CAF80(unit; x, y, 0)` (UI refresh), and if the attacker or the victim is the local human show
   `BOMBFAILED` / `THEIRBOMBFAILED` when the unit's `PRTO +0x9C == 2` (air) else `BOMBARDFAILED` /
   `THEIRBOMBARDFAILED`; return.
2. Record an incident: `0x5631B0(Player[unit.owner]; victimCiv, 1)` (`combat.md` 14.4).
3. Remove **one class** of improvement from the tile, the first that applies, in this order (cell vtable calls):
   1. **Barricade** (`vt+0x38`): clear mask `0xF000001F`, then set the saved overlay word with bit 28 cleared and bit 4
      (**Fortress**) set. A Barricade therefore *degrades to a Fortress*; (the saved word is the pre-clear word, so
      bits 0-3 and 29-31 are written back unchanged).
   2. **Railroad** (`vt+0x5C`): clear mask `0x2`.
   3. **Any of road / railroad / mine / irrigation** (`overlay & 0xF != 0`): clear mask `0xF` (all four at once).
   4. Otherwise: if the tile has an **airfield**: look it up in the airfield pool (the id must be in range with non-null
      node), `0x5DAEC0(obj; 1)`; then, **without stopping**: if it has an **outpost**, `0x5D6430(map; x, y)`; if it has a
      **radar**, `0x5D6360(map; x, y)`; finally clear mask `0xF000001F` (fortress, the four low bits, and bits 28-31).
      A plain Colony is never removed by this routine.
4. Cases 1-3 jump to the common tail; case 4 falls into it. Tail: if bit `1 << Player[unit.owner].slot` is set in the
   tile's discovered mask (`cell +0x58`), refresh the tile with `0x55B1A0(Player[unit.owner]; x, y)`; then
   `0x5CAF80(unit; x, y, 1)`; if the attacker or the victim is the local human **and** `rollFlag != 0`, show
   `BOMBSUCCEEDED` / `THEIRBOMBSUCCEEDED` (air) or `BOMBARDSUCCEEDED` / `THEIRBOMBARDSUCCEEDED` (the `THEIR` form when the
   unit's owner is not the local human); a final victim-side notification when `victimCiv > 0`, `victimCiv != unit.owner`
   and `victimCiv` is the local human is **O** (its call was not followed).

This resolves the hypothesis of `combat.md` 8.8 ("pillage-style destruction"): it **is** a prioritised improvement
destruction. Which of the classes can be present is gated by the caller's precondition (`overlay & 0x1000001F != 0`,
Fortress and Barricade included).

---

## 9. Golden vectors

All hand-derived consequences of the text (not captured runs), **V** where the underlying read is **V**.

1. **Footprints.** Colony / Airfield: 9 tiles (3x3, `spiral(0..8)`), all `seeTile`. Radar: 25 tiles (5x5), `radarOn` +
   `seeTile`. Outpost: 9 (flat terrain), 25 (Hills, id 5), 49 (Mountains id 6, Volcano id 10).
2. **Colony on a luxury tile.** Tile `(10, 10)`, resource Wine (luxury, class 1), terrain Grassland (allow_colonies set),
unowned, no city: `colonySiteCheck` returns `0`. With a border owner set: `6`. With the resource a strategic one whose
revealing tech the player lacks: `1`. On Ocean (water, `allow_colonies = 0`): `3`. On Marsh (land, `allow_colonies = 0`): `4`.
3. **Colony then destroy.** Player without Road and Railroad, `rebuild = 1`: the tile gains mask `3`; destruction clears
   `1` (and `2`) and unsees the 3x3 tiles not seen by another structure. A player with both technologies keeps the road
   and railroad on the tile.
4. **Radar overlap (two towers, owner P).** Towers at `R = (10, 10)` and `S = (12, 12)`. Destroy `R`. For `T = (11, 11)`:
   `S` is at offset `(1, 1)` from `T`, within `spiral(1..24)`, and shares neither coordinate with `R`, so `T` is covered
   and `radarOff` is **not** called. For `T = (12, 12)` (the tile of `S` itself): the scan excludes `T`, no other tower is
   found, so `radarOff(P; (12, 12))` **is** called: the surviving tower loses the radar bit on its own tile (quirk 3);
   `isObserved(P; (12, 12), 0)` is true (tower at `k = 0 < 25`), so the tile is not unseen.
5. **Radar overlap (same column).** Towers at `R = (10, 10)` and `S = (10, 13)`. Destroy `R`. For `T = (10, 12)`: `S` is at
   offset `(0, 1)` and would cover it, but `S.x == R.x` skips it, so `radarOff(P; (10, 12))` is called; the tile stays
   *seen* (`isObserved` finds `S` at `k < 25`) but its `+0xD4` radar bit is cleared.
6. **Airfield capture by slot 0.** Any capture whose new owner slot is `0` destroys the airfield.
7. **Pool.** Allocate A, B, C (ids 0, 1, 2), destroy B (free list `1`), allocate D: D gets id `1`; `last` stays `2`.

---

## 10. Quirks a faithful port must reproduce (all **V** unless noted)

1. The founding / building unit is **always consumed**, even when the creator refused (section 3).
2. The Colony site gate reads the *resource's reveal tech* and the *border owner*, not the founder's territory: a Colony
   can never be founded inside anyone's borders, including the founder's own (code `6`).
3. Radar destruction skips candidate towers on the same raw `x` or the same raw `y` as the destroyed tower and never counts
   a tower standing on the tile under test (section 6.3). A surviving tower in the destroyed tower's row or column therefore
   does not keep the tiles it covers radar-visible, and a surviving tower loses the radar bit on its own tile when that tile
   lies in the destroyed tower's 5x5. (`isObserved` still sees the surviving tower, so ordinary vision is not lost.)
4. The outpost vision size differs between creation / destruction (Mountains and Volcano 49, Hills 25) and the observation
   test (Mountains only 49; everything else 25) (section 5.1).
5. Destroying a Colony keeps the tile's road and railroad if the owner knows the respective technologies; the overlay
   marker is not separately cleared (a plain Colony has none).
6. A capture of an airfield by slot 0 destroys it instead of transferring it (6.6); the airfield re-base loop of the
   tile-owner change moves **any** air unit on the tile, whatever its owner (`borders-culture.md` 6).
7. Clearing overlay bits 29-31 through the generic clear (`0x5DA3E0`, for example the job rows' `B` masks) does **not** call
   the destroyers by itself; callers that need the counters and pools consistent call the destroyers first (`0x461470`
   does, section 7). Other generic clears were not audited (**H** that none leaves a stale pool entry).
8. `0x5B4DC0` degrades a Barricade to a Fortress and never touches a plain Colony (section 8).
9. The player initializer `0x567C80` removes only plain Colonies (section 7); a civilization restarted in a slot keeps
   foreign-held airfield, radar and outpost objects that the slot owned (**H** that the initializer is only used on a fresh
   slot, where none exist).
10. Entering a tile that holds another owner's plain Colony destroys it unconditionally (no war test in the arrival block).

---

## 11. Open items

1. **O** The growth routine `0x4C22C0` and the initial-capacity routine `0x56BC60` (array sizing only; observable only as a
   limit on the number of colonies).
2. **O** Whether any code reads the airfield byte `+0x30`.
3. (Settled in `movement.md` 5.) The head of `0x5BD220` was read in full: before the colony block (`0x5BD8C3`) it only does the list update, the
   worked-tile eviction for an at-war unit and the martial-law recompute; **no war, treaty or stack-ownership test precedes the plain-Colony block**.
4. **O** The tail of `0x5B4DC0` after the tile refresh (a victim-side message).
5. (Settled in `vision.md`: the masks `+0x5C` (units), `+0x60` (structures), `+0x64` (territory) and `+0xD0` (air reveal) are
   independent; `unseeTile` clears only `+0x60`; nothing re-marks it, and a tile is visible while the OR of the four has the civ's
   bit, `vision.md` 1.3.) Remaining **O**: the readers of `+0xD4`.
6. **O** The strings shown for the gate codes of section 2.
7. **O** The meaning of the `rebuild` flag raised by `0x568950` (it is collected for the Colony and Airfield cases only; its
   consumer was not followed).
