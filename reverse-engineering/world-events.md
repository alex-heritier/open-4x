# World events: resource upkeep, pollution, global warming, volcanoes, plague

Owns: everything the round processor does to the *world* rather than to a player: the every-5th-turn
**resource upkeep** (`0x4F4CB0`: strategic and luxury resources disappear, strategic ones are re-placed,
`0x5D5D00`, site test `0x5F3320`), the **pollution model** (the two per-city pollution numbers `0x4B1A50` /
`0x4B1B40` / `0x4B1C10`, the tile-pollution event `0x4B2F80`, the nuclear **meltdown** `0x4B4970`, the nuclear
counter), **global warming** (second half of `0x4F4380`), **volcanoes** (first half of `0x4F4380`, lava
`0x4F4040`) and the **plague** (`0x4F5250` scheduler, `0x4B3C30` per-city step, `0x4B3700` infection,
`0x4B4090` cure). When each of them runs is in `turn.md` section 4. Image base `0x400000`; static VA = runtime
VA. This is a **clean-room specification**: it contains no reference code. Every rule carries the address
that shows it.

Evidence tags: **V** = the whole routine body was read in the disassembly and the statement is what the
instructions do; **I** = inferred from how a value is used; **H** = **HYPOTHESIS** (do not build on it
without a test); **O** = open (not decoded). Nothing here was checked by running the game.

## At a glance

| event | routine | runs | skipped when | randomness |
|---|---|---|---|---|
| resource upkeep | `0x4F4CB0` | end of a round when `turn mod 5 == 0` (turn counter before the increment) | never gated by multiplayer | one `rand(p)` per eligible tile, plus a tile shuffle per relocation |
| city tile pollution | `0x4B2F80` | every city, inside the city turn `0x4BE970` | pollution value `<= 0` | `rand(100)`, `rand(20)` |
| nuclear meltdown | `0x4B4970` | every city, inside the city turn, right after the previous | city not in disorder, or no reactor | `n` times `rand(100) < 10` |
| global warming | `0x4F4380` (second half) | every round, **also in multiplayer** | never | `rand(T)` twice per attempt |
| volcanoes | `0x4F4380` (first half) | every round | multiplayer game; fewer than 2 calendar years since the last pass | `rand(P)`, `rand(4)`, `rand(100)` |
| plague | `0x4F5250` (+ `0x4B3C30` per city) | every round | multiplayer game; `permit_plagues` clear; all occurrences used | `rand(V)`, `rand(100)`, `rand(1024)`, `rand(2048)`, `rand(1768)` |

* Everything in this file uses the gameplay generator `rand(n)` of `primitives.md` (`0x60BAB0`, state
  `0xA526B4`), so the **order of the draws** is part of the specification and is given for each routine.
* Multiplayer means `0x47B530` returns true (`turn.md` section 1). A second, network-mode implementation of
  the round (`0x476330`) calls the same world routines through `0x4F5160` (plague, volcano/warming, upkeep,
  cultural marks, trade network, score means, turn increment); the event routines themselves are shared.
* Nothing in this file touches the treasury. Every effect is on tiles, cities, units or the resource
  counters of the map.

## 1. Shared definitions

### 1.1 Tile index and coordinates **V**

`W = [0x9C74D4]` (map width in x units), `H = [0x9C74C0]`, `N = word [0x9C73AC]` (tile count, 16 bit).
The tile table is indexed by `i = (W/2)·y + (x >> 1)` taken modulo 65536 (`primitives.md` 1.3); x and y have
the same parity for every real tile, so the inverse is

```
y = i div (W/2)        x = 2 · (i mod (W/2)) + (y and 1)
```

Every loop "over all tiles" in this file runs `i = 0 .. N-1` and decodes `(x, y)` this way
(`0x4F4CB0` at `0x4F4CFA..0x4F4D0D`, `0x4F4380` at `0x4F4460..0x4F4487`, `0x4F4380` warming at
`0x4F49DC..0x4F49EF`).

Neighbour offsets come from the spiral `spiralOffset(n)` of `NOTES.md` section 8 (`0x5E6E50`): `n = 0` is the
tile itself, `1..8` the ring around it, `9..24` the next ring, ring `r` ends at `(2r+1)² - 1`. A neighbour is
then normalized with the wrap flags of the map (`0x426C00` for x, `0x426C40` for y) and accepted by the
bounds test `0x426BD0(x, y)`; the resource test `0x5F3320` open-codes the same wrap (one add or subtract of
the width or height when the matching wrap flag, bit 0 / bit 1 of `[map+0x1F0]`, is set).

### 1.2 Cell facts used here **V** (accessors through the Cell vtable `0x6701C8`)

| vtable slot | target | meaning |
|---|---|---|
| `+0x9C` | | resource id on the tile, `-1` none |
| `+0xEC(r)` | `0x5DA1F0` | set the resource id (`-1` removes) |
| `+0xC8` | `0x5EAB30` | terrain nibble: `(terrainWord >> 12) and 0xF`; `0` desert, `1` plains, `2` grassland, `3` tundra, `4` flood plain, `5` hills, `6` mountains, `7` forest, `8` jungle, `9` marsh, `10` volcano, `11` coast, `12` sea, `13` ocean |
| `+0x8C` | `0x5EAA30` | **is water**: `terrain >= 11` |
| `+0xC4` | | base-terrain class of the tile (the terrain a forest, jungle or marsh lies on) |
| `+0xB8` | | continent number (signed 16 bit word) |
| `+0xBC` | `0x5EAB00` | word `[cell+0x1C]`: the **colony id** of the tile, `-1` none (`savegame.md`; a colony object is a Colony, Airfield, Radar Tower or Outpost, `biq-format.md` `CLNY`); the colony's kind is the overlay bit set on the tile |
| `+0xA8(plane)` | `0x5DC300` | plane value; plane `0` is the **overlay word** |
| `+0x18(0)`, `+0x84(0)`, `+0x4C(0)` | `0x5EA610`, `0x5EA9D0`, `0x5EA830` | overlay bit 29 (airfield), bit 30 (radar tower), bit 31 (outpost) |
| `+0x98` | `0x5EAA80` | byte `[cell+5]`: **tile owner**, civ slot, `0` = unowned (`combat.md` 14.3) |
| `+0x118` | `0x5D9EF0` | **owner of the colony on the tile**: if the colony id is valid, the colony object is taken from the airfield pool `[0xA52E3C]` when overlay bit 29 is set, else from the radar pool `[0xA52E24]` (bit 30), else from the outpost pool `[0xA52E0C]` (bit 31), else (`0x5EA6E0` true) from the plain-colony pool `[0xA52E54]`; the result is its owner byte `+0x2C`, `-1` when there is none |
| `0x5EA6E0(cell)` | | **the tile hosts a plain Colony**: colony id `!= -1` and overlay bits 29, 30, 31 all clear (the three slot tests above) |
| `+0xCC(plane, mask, x, y)` | `0x5DA3E0` | **clear** the bits `mask` in the plane (here plane 0, the overlay) |
| `+0xE0(plane, mask, x, y)` | `0x5DA2A0` | **set** the bits `mask` in the plane (`NOTES.md` 11.9: goody hut is `(0, 0x20)`) |
| `+0xFC` / `+0x100(v)` | `0x5DC170` / `0x5DC180` | dword `[cell+0x34]`: the **volcano stage** (section 5) |
| `+0x120` | `0x5E9A60` | terrain row `[TERR + 0x74]` of the tile's terrain, the **pollution effect**: the terrain id the tile degrades to, `-1` none, `14` = "the base terrain" (then the answer is slot `+0xC4`) |
| `+0xA0` | `0x5EAA90` | dword `[cell+0x0C]`: head of the tile's unit list |
| `word [cell+0x6C]` | | id of the city whose citizen **works** the tile, `0xFFFF` none (`yields.md`) |
| `word [cell+0x6E+2p]` | | id of the city the tile is trade-connected to for player slot `p`, `-1` none (`primitives.md` 4.3) |
| `dword [cell+0x58]`, `+0x5C`, `+0x60`, `+0x64`, `+0xD0` | | per-slot bit sets (bit = player slot): what each player sees / remembers; only used here to decide whether the human is told |
| `dword [cell+0xD8]` | | pointer to the animation object attached to the tile (section 5) |

Overlay word bits (the file's tile overlay, `biq-format.md`): road 0, railroad 1, mine 2, irrigation 3,
fortress 4, goody hut 5, **pollution 6**, barbarian camp 7, craters 8, barricade 28, airfield 29, radar
tower 30, outpost 31. Mask values used below: `0x2` railroad, `0xC` mine + irrigation, `0xF` the four
terrain improvements, `0x40` pollution, `0xF000001F` bits 0 to 4 and 28 to 31.

"The human" is the slot `[0x9FD4BC]`; a message is shown only when that slot is the affected owner or has the
tile in `[cell+0x58]`. Messages go through the message log `0x4ED220(0x9F8700; x, y, key, 1)`; they never
change game state and are not repeated here beyond their keys.

### 1.3 Calendar year, "no year 0" **V**

The date object (class tag `DATE`, constructor `0x5DF250`, setter `0x5DF100(this; turn)`) has a mode byte at
`+0x5C` (0 years, 1 months, 2 weeks). In mode 0 (the default) `0x5DF100` stores in `+0x68`

```
year = [0x9C4108] + U(turn)          U = the seven-segment table of victory.md section 3
```

(`[0x9C4108]` is the scenario start year, `-4000` in a standard game.) Modes 1 and 2 feed the same
accumulated units through `(U + [0x9C4100] - 1) / 12` and `(U + [0x9C4104] - 1) / 52`, keep the remainder in
`+0x60` / `+0x64` and add the start year. All the world routines read the date of "now" by constructing a
local date and calling `0x5DF100(turn)`; `Y` below is its `+0x68`, and

```
Y1 = (Y == 0) ? 1 : Y            the executable treats year 0 as year 1 in every comparison
```

so a span that crosses from BC to AD is one year longer than the subtraction suggests (**H** that this is
deliberate). Years per turn are the segment values of victory.md section 3 (50, 40, 25, 20, 10, 5, 2, then 1).

### 1.4 Pools **V**

Cities: `[0xA52E6C]` array, last index `[0xA52E78]` (loop `i = 0 .. last`, entry `[array + 8i + 4]`, object =
entry `- 0x1C`, skip when any of them is null); units `[0xA52E84]`/`[0xA52E90]`. The four **colony pools**
(array / last index): plain Colony `[0xA52E54]`/`[0xA52E60]`, Airfield `[0xA52E3C]`/`[0xA52E48]`, Radar Tower
`[0xA52E24]`/`[0xA52E30]`, Outpost `[0xA52E0C]`/`[0xA52E18]`. A colony object holds its id at `+0x20`, x at
word `+0x24`, y at word `+0x28` and its owner at byte `+0x2C`. World colony count `[0xA52694]`, world
airfield count `[0xA527A0]`; the per-player counters are dwords in the Player record (`Player +0x198` for
plain colonies, `+0x11C8` for airfields). Players: base `0xA52E98`, stride `0x20E4`, slot field `+0x1C`.

**Colony destruction** `0x5DAA90(colony; 1)` (plain colony; `0x5DAEC0(colony; 1)` is the airfield twin and the
outpost / radar twins are `0x5D6430` / `0x5D6360`, called as `(map; x, y)`): clears the tile's colony id
(cell slot `+0xF8(-1)`), decrements the owner's and the world's counter, releases the object to the free list
of its pool, and then removes the tile improvements that only the colony justified: for a plain colony,
overlay mask `1` (road) when the owner does not know the technology at `[0x9C7324] +0x1A4` and the further
mask chosen by the technology at `+0x218` (the remaining ~60 instructions, **O**).

## 2. Resource upkeep `0x4F4CB0` (every 5th turn)

### 2.1 Trigger **V**

At entry the routine tests `[0xA526AC] mod 5` (signed `idiv`); only `0` continues. It is called at the end
of the round before the turn counter is incremented (`turn.md` section 4), so it runs at the end of the
rounds numbered `0, 5, 10, ...`. It also writes the debug string `performing resource upkeep...`. Nothing else
in the routine depends on the player count, the difficulty or multiplayer.

### 2.2 Which tiles are touched **V**

Loop `i = 0 .. N-1` (section 1.1). A tile is **eligible** when all of these hold, tested in this order
(the first failing test ends the tile):

1. `r = cell.resource` (`+0x9C`) `!= -1`.
2. The GOOD row of `r` (`[0x9C71D4]`, stride 92) has class `+0x3C` equal to `1` (luxury) or `2` (strategic)
   (`0x5E3700`). Bonus resources never disappear.
3. The tile has an owner `o`: if `0x5EA6E0(cell)` holds (the tile hosts a plain Colony, 1.2) the owner is the
   **colony's owner** (slot `+0x118`, `-1` means none); otherwise it is the tile owner byte of slot `+0x98`
   and `0` means none. A tile without an owner never loses its resource. (A colony is how a civilization
   gets a resource outside its borders; the colony's owner, not the border owner, is the one tested.)
4. The owner knows the resource's reveal technology: `knowsTech(Player[o], GOOD.+0x4C)` (`0x561440`; a
   `-1` technology is always known, `primitives.md` 2.1). The row field `+0x4C` is the editor's
   *prerequisite*.
5. The tile is **connected** for the owner: `word [cell + 0x6E + 2·o] != -1` (the tile is linked to one of
   the owner's cities by the trade network, `primitives.md` 4.3). A resource nobody can use is safe.
6. `p = GOOD.+0x44` (the editor's *disappearance probability*) `!= 0`.
7. `rand(p) == 0` (one draw, `0x60BAB0` with the argument `p`). Probability `1/p` per check.

An eligible tile calls `0x5D5D00(map; x, y, notify)` with `notify = (o == [0x9FD4BC])`. The draws of
different tiles are made in tile-index order, so a tile that is not eligible consumes **no** draw.

Shipped `conquests.biq` (GOOD rows, class / prerequisite TECH row / `+0x44`): Horses 2 / 4 / 0, Iron 2 / 7 /
800, Saltpeter 2 / 30 / 800, Coal 2 / 44 / 400, Oil 2 / 53 / 200, Rubber 2 / 57 / 0, Aluminum 2 / 64 / 400,
Uranium 2 / 65 / 100; all eight luxuries and all bonus resources have `0`. Per check a connected, owned
Uranium tile therefore loses its resource with probability 1 in 100, an Iron tile 1 in 800.

### 2.3 What happens to the tile, `0x5D5D00(map; x, y, notify)` **V**

`this` is the map object `0x9C736C`; `map+0x40` is the tile count (word), `map+0x168` the width, `map+0x154`
the height, `map+0x1F0` the wrap flags.

1. A scratch array of `tileCount` dwords is allocated; it is freed on every exit.
2. `r` = the tile's resource; if it is `-1` the call ends (nothing else happens, not even step 11).
3. The tile's resource is set to `-1` (slot `+0xEC`), and the per-resource placed counter of the map is
   decremented: `0x5F3E80(map; r, 0x5F3E70(map; r) - 1)` (`0x5F3E70` is the getter of the counter array the
   generator fills, `NOTES.md` 11.8).
4. If `0x5EA6E0(cell)` holds (the tile hosts a plain Colony), the colony (pool `[0xA52E54]`, index = the
   cell's colony id) is **destroyed**: `0x5DAA90(colony; 1)` (section 1.4). A colony exists to reach a
   resource, so losing the resource removes it.
5. If `notify`: the message `GOODDISAPPEARED` with the resource name (text register 0), at `(x, y)`.
6. If the GOOD class is not `2` (`0x5E3730`, strategic), the routine jumps to step 11: **luxury resources are
   removed and never re-placed**.
7. (strategic only) **Shuffle.** The scratch array is filled with `0 .. tileCount-1`; for `p = 0 .. tileCount-1`
   in order: `q = p + rand(tileCount - p)`; swap entries `p` and `q` (the draw is made even when it returns
   0, so `tileCount` draws).
8. **Pick a player.** Repeat `P = rand(32)` until bit `Player[P].+0x1C` of the in-play mask `[0xA526C0]` is
   set. `P` is therefore a random civilization that is in play (barbarians included only if slot 0 is flagged
   in play, which it is not in a normal game).
9. **Three passes over the shuffled tiles**, each stopping at the first success; a candidate tile `c`
   (decoded from the entry with the formula of 1.1) is accepted by `valid(c) = 0x5F3320(map; cx, cy, r, 1)`
   (section 2.4) *and*

   | pass | extra condition on the candidate |
   |---|---|
   | 1 | owner byte (slot `+0x98`) `== P` and the tile is worked: `word [cell+0x6C] != -1` |
   | 2 | owner byte `== P` |
   | 3 | none |

   On acceptance the resource `r` is written (slot `+0xEC(r)`), the counter incremented, and the search
   ends. The two "owner" passes use the map vtable slot `+0x44`, which is `0x5F3320` itself (`0x670164`).
10. **Message.** Pass 1 and 2: if `P` is the human and the human knows the resource's technology,
    `GOODAPPEARED` (resource name, `(x, y)`). Pass 3: the same, with the tile owner byte instead of `P`.
11. `0x57E450(0xB72888; 0)`: the trade-network recompute (`primitives.md` 4.3; the connection words of the
    cells change because a resource appeared or vanished).

If no tile in the map is valid, the resource is simply gone. The tile that lost the resource is part of the
shuffled list and can receive the resource back if it is valid (it is: the test of 2.4 only forbids a tile
that *has* a resource, and the resource was just removed), so a strategic resource can reappear on the very
tile it left; the probability of that depends on the shuffle only.

### 2.4 Site validity `0x5F3320(map; x, y, r, flag)` (`ret 0x10`) **V**

Returns a byte. `G` is the GOOD row of `r` (map vtable `+0x8C('GOOD', r)`), `cont` the tile's continent
(slot `+0xB8`). `flag` is `1` in the relocation above and `0` in the map generator (`NOTES.md` 11.8). In
order:

1. If `G` is a **luxury** (`0x5E3720`: class 1): the continent record (map slot `+0x84(cont)`, tile count at
   record `+0x24`) must hold at least `37` tiles (`flag = 0`) or `75` tiles (`flag = 1`); otherwise false.
2. The terrain row `T` of the tile's terrain must have a resource mask (`0x5E8C00(T)` tests `[T+8] != 0`; if it
   is null it is built empty by `0x5E8DB0`), and bit `r` of the mask must be set (`mask[r >> 3] and
   (1 << (r and 7))`): the editor's "possible resources" of the terrain. Otherwise false.
3. The tile has no resource (`+0x9C == -1`). Otherwise false.
4. Cell slot `+0x6C` (feature bit 16, the bonus-grassland shield; `yields.md`) must be clear. Otherwise false.
5. **Spacing.** The scan length depends on the class (`0x5E3700`: luxury or strategic):

   ```
   m   = trunc( trunc((width + height) / 2) / 50 )       -- the 0x51EB851F magic with sar 4
   R   = (2 · min(m, 4) + 5)²                             -- 25, 49, 81, 121, 169
   R   = 9                                                -- for a bonus resource
   ```

   For `n = 1 .. R-1`: take the spiral neighbour (wrapped, in bounds); skip it unless its continent equals
   `cont`; let `s` be its resource, skip it if `-1`. A **conflict** (return false) is

   * for `n <= 8` (the eight neighbours): `s == r` and `G` is strategic;
   * for `n >= 9`: if `G` is strategic, `s == r`; otherwise (luxury or bonus) `s != r` and the GOOD row of
     `s` is a luxury.

   So a strategic resource refuses the same strategic resource anywhere in the scan area on its continent;
   a luxury or bonus resource refuses a **different luxury** at spiral index 9 or more (and tolerates any
   neighbour in the inner ring, including the same luxury); a bonus resource scans only the inner ring and
   can therefore never conflict.
6. **Water edge rule.** If the tile is not water (slot `+0x8C` false) the answer is **true** here. If it is
   water, it is valid only when at least one of the spiral neighbours `n = 1 .. 20` (wrapped, in bounds) is
   **not** water; a tile with no land within that 5×5-minus-corners neighbourhood is refused.

The routine has no side effects and uses no random numbers.

### 2.5 Golden checks (data, not code)

* The scan length of 2.4 step 5 depends on `(width + height) / 2` only: 40 gives `m = 0`, `R = 25`; 120 gives
  `m = 2`, `R = 81`; 200 gives `m = 4`, `R = 169`; 350 gives `m = 7 -> 4`, `R = 169`.
* A luxury needs a continent of at least 75 tiles when the relocation asks (never in practice, because only
  strategic resources are relocated), 37 tiles in the generator.
* A relocation on a map with 3000 tiles consumes 3000 draws for the shuffle, then one `rand(32)` per
  rejected player candidate, then no draws for the three passes (the validity test uses none).
* A connected Uranium tile survives 10 consecutive checks (50 turns) with probability `0.99¹⁰ = 90.4 %`.

## 3. Pollution

### 3.1 Inputs **V**

* `size = city +0x138`; `cityMax = [0x9C72E8]` (12 in the shipped rules, the editor's *city max*).
* `city +0x48` = the **sum of `BLDG.pollution` (`+0xCC`) of every building the city owns**: added when a
  building is added (`0x4ACF40` at `0x4AD1D5..0x4AD1DE`) and subtracted when one is removed (`0x4AD7F4..0x4AD7FB`).
  Obsolescence does not change it.
* BLDG flags (`+0xEC`, `biq` `improvement_flags`): bit 5 `0x20` *removes population pollution* (Mass Transit),
  bit 6 `0x40` *reduces building pollution* (Recycling Center), bit 16 `0x10000` *can explode or meltdown*
  (Nuclear Plant). `+0xE0` is *rendered obsolete by* (TECH row, `-1` never).
* `has(c, b)` is `0x4ACB50(city; b, 1)` (the city has building `b`).
* A building is **active** for these tests when `has(c, b)` and either `BLDG.+0xE0 < 0` or the owner does
  not know that technology (`knowsTech(owner, BLDG.+0xE0)` false).

### 3.2 Population pollution `0x4B1A50(city)` **V**

```
if size <= cityMax: return 0
P = size - cityMax
if some active building has flag 0x20: P = 1
return max(P, 0)
```

### 3.3 Building pollution `0x4B1B40(city)` **V**

```
B = city.+0x48
if B <= 0: return 0
if some active building has flag 0x40: B = 1
return max(B, 0)
```

### 3.4 Total `0x4B1C10(city)` **V**

`pop(c) + bld(c)` with the two rules above (the routine open-codes both; the early exit for
`size <= cityMax` makes the population part `0`). It is called for every city by the global-warming sum
(section 4) and by the UI (`0x4191C0`); the city screen shows the two parts separately (`0x42C8A0` calls
`0x4B1A50` and `0x4B1B40`).

### 3.5 The pollution event `0x4B2F80(city)` (called from `0x4BE970` at `0x4BEAF9`) **V**

```
p = 0x4B1A50(c) + 0x4B1B40(c)
if p <= 0: return
if rand(100) >= p: return                        -- probability p percent, certain at p >= 100
start = rand(20)
for j = 1 .. 20:
    n  = ((start + j) mod 20) + 1                 -- every spiral index 1..20 exactly once, from a random start
    (x, y) = normalize(city.x + dx(n), city.y + dy(n))      -- city.x = word +0x24, city.y = word +0x26
    if (x,y) in bounds
       and word [cell+0x6C] == city.id (+0x20)    -- the tile is worked by this very city
       and the tile is not water:
        set overlay mask 0x40 on the tile (slot +0xE0(0, 0x40, x, y))
        if city owner == the human: message POLLUTION (city name, x, y)
        return
```

Notes: there is **one** draw of `rand(100)` and, if it passes, **one** of `rand(20)`; the scan itself is
deterministic. Only worked, non-water tiles of the city can receive pollution, so a city that works no land
tile pollutes nothing (the event is silently lost). The routine does **not** test whether the tile is already
polluted: a polluted worked tile can be picked again and the event then changes nothing. The pollution bit is
cleaned by a Worker (`improvements`, not specified here); an uncleaned bit stays forever.

### 3.6 Nuclear meltdown `0x4B4970(city)` (called from `0x4BE970` at `0x4BEB07`, right after `0x4B45A0`, **O**) **V**

```
if bit 0 of city.+0x30 is clear (the city is not in disorder): return
n = number of buildings b with  has(c, b)  and  not 0x4ACCC0(c, b)  and  BLDG[b].+0xEC has bit 16
if n <= 0: return
repeat n times:
    if rand(100) >= 10: continue            -- 10 percent per reactor, independent
    -- meltdown (at most one per call, then the routine ends)
    [0xA52698] += 1                          -- the nuclear event counter, section 3.7
    remove floor(size / 2) citizens: 0x4BA230(c; floor(size/2), -1, 1)       -- skipped when floor(size/2) >= size, i.e. never
    for n = 1 .. 8 (the ring around the city):
        (x, y) = normalize(city.x + dx(n), city.y + dy(n))
        if in bounds and not water: set overlay mask 0x40 on the tile        -- no ownership or worked test
    UI: if the owner is the human: MELTDOWN dialog (the dialog's result sets [0x9C34E4] = 1, which later
        centres the view on the city, 0x4BEC29); in a multiplayer game a network event {8, city id, -1, -1, -1}
        is queued instead (0x74CFC4..0x74CFD4).
    return
```

`0x4ACCC0(c, b)` is the **obsolescence test** (**V**, `0x4ACCC0..0x4ACD0F`, [`city-buildings.md`](city-buildings.md)
section 2): false when `BLDG[b].+0xE0` (the "rendered obsolete by" tech) is negative, else whether the
owner knows that tech (`0x561440`). It is not a "switched off" predicate. The probability of at least one
meltdown in a turn is `1 - 0.9ⁿ` for `n` active reactors. `0x4BA230(c; k, -1, f)` is the city shrink routine
(the plague uses `k = 1, f = 0`, the meltdown `f = 1`); its body is **O**.

### 3.7 The nuclear counter `[0xA52698]` **V**

A signed dword, cleared by the new-game reset `0x59FD80`. Incremented by one per meltdown (3.6) and by one
per nuclear weapon detonation (`0x5B4070` at `0x5B4138`; the attack's own effects are not specified here,
**O**). Its only reader is the global-warming sum (section 4), which uses its **square**.

## 4. Global warming (`0x4F4380`, second half, entered at `0x4F48EE`) **V**

Runs every round, also in multiplayer. Let `T = N` (the tile count, a 16-bit value).

```
S = sum over every city of the world of 0x4B1C10(city)  +  [0xA52698]²        -- pollution points
level [0xA5269C] = 3 if S > T/2 ;  2 if S > T/4 ;  1 if S > 0 ;  0          (T/2, T/4 by unsigned shift)
k = trunc(S / 10) + 1                                                           -- number of attempts
repeat k times:
    if rand(T) >= S: continue                       -- success probability min(S, T) / T
    i = rand(T) ;  (x, y) from i (section 1.1)
    if the tile is not water:
        t' = cell slot +0x120                        -- the terrain's pollution effect
        if t' != -1 and t' < 11:                     -- a land terrain
            0x5D59E0(map; t', x, y)                  -- change the tile's terrain (section 4.1)
            if the tile's terrain nibble now equals t':
                clear overlay mask 0xC on the tile   -- mine and irrigation are lost
                if (cell+0xD0 or +0x64 or +0x60 or +0x5C) has the human's bit:
                    message: t' < 4 -> GLOBALWARMING (text 0 = old terrain name, text 1 = new terrain name)
                             t' >= 4 -> GLOBALWARMING_FOREST_JUNGLE (text 0 = old terrain name)
```

Facts: the level `[0xA5269C]` (0 to 3) is **only** read by the UI (`0x554860`); the effect on the map depends
on `S` alone. The sum counts every city of every civilization, barbarian ones included. The attempt count
grows with `S`, but the per-attempt probability caps at 1 when `S >= T`. The random tile is not tied to a
polluted tile: global warming hits **any** land tile whose terrain has a pollution effect.

Shipped pollution effects (`TERR.pollution_effect`): desert `-1`, plains `0` (desert), grassland `1`
(plains), tundra `-1`, flood plain `-1`, hills `-1`, mountains `-1`, forest `14` (its base terrain), jungle
`14`, marsh `11` (coast: fails the `t' < 11` test, so marsh is **unaffected**), volcano `-1`, coast/sea/ocean
`-1`. With those rules forest and jungle revert to the terrain they lie on (`0..3`, so they use the first
message key); the `GLOBALWARMING_FOREST_JUNGLE` key is reached only if a ruleset names a target of 4 or more.

### 4.1 `0x5D59E0(map; terrain, x, y)` **I**

Sets the cell's terrain through cell slot `+0x128(terrain, x, y)`. If the base-terrain class (slot `+0xC4`)
of the tile changed, it then inspects the four diagonal neighbours (helper `0x5EBDC0`, a count compared with
4) and may call slot `+0x128` again with the neighbour-corrected value, then walks the spiral from
`n = 1` (**O**: the continent / coast fix-up this implements was not decoded). The caller's own check "the
nibble now equals `t'`" is what decides that the change took effect.

### 4.2 Golden checks (data)

With `T = 3200` tiles:

| `S` | level | attempts `k` | success probability per attempt |
|---|---|---|---|
| 0 | 0 | 1 | 0 (`rand(T) >= 0` always) |
| 25 | 1 | 3 | 25/3200 = 0.78 % |
| 800 | 1 | 81 | 25 % |
| 801 | 2 | 81 | 25.03 % |
| 1601 | 3 | 161 | 50.03 % |
| 4000 | 3 | 401 | 100 % |

A civilization with three cities of size 20 and no Mass Transit adds `3 · 8 = 24` points from population,
and a Factory (pollution 2) and a Coal Plant (pollution 2) in each adds `3 · 4 = 12` more: `S = 36`.
One meltdown adds `1² = 1`, three meltdowns `9`, ten `100`.

## 5. Volcanoes (`0x4F4380`, first half) **V**

Skipped entirely when `0x47B530` is true. `P = [0x9C5D70]` is the scenario's *maximum eruption period* (GAME
field `volcano_max_eruption_period`, 5000 by default). The list `0xA283CC` (3 pairs of dwords) is a static
scratch list for the lava tiles of the current eruption.

### 5.1 Pass gate

```
now  = date(turn).year, then Y1 (1.3)        prev = stored year [0xA52A30] of the stored date 0xA529C8, 0 -> 1
if the stored date's mode ([0xA52A24]) differs from the current date's mode: reset the stored date (0x5DF250)
dy = Y1(now) - Y1(prev)
if dy <= 1: skip the tile loop;  (the stored date is replaced only if dy < 0)
else:       dy = min(dy, 50); run the tile loop; afterwards copy the current date into the stored date
            (0x4F4C80 copy of the date object, 16 dwords to 0xA529E4, fields 0xA52A24..0xA52A34)
```

Consequences: with 2 or more calendar years per turn every round is a pass; at 1 year per turn the stored
year is not advanced when `dy = 1`, so a pass happens every second round with `dy = 2`. `dy` is the number
of years elapsed since the previous pass, capped at 50. With the shipped calendar (victory.md section 3) the
first 25 turns have `dy = 50` (the cap), then 40, 25, 20, 10, 5, 2 per turn up to turn 439, and from turn
440 on one year per turn, so volcanoes keep erupting every second round to the end of the game.

### 5.2 Stage machine (per volcano tile; stage `s` is `[cell+0x34]`)

The loop is over all tiles `i = 0 .. N-1`; a tile takes part only if its terrain nibble is `10` (volcano).
Let `human sees` = the human's bit is set in `[cell+0x58]`.

| stage `s` | action |
|---|---|
| `s <= 0`, or `s >= 5` | **dormant**: draw `rand(P)`. If it is `< dy`: draw `r = rand(4)`, set the stage to `r + 1` (1 to 4); if `r < 3` attach the *smoke* animation (type 9, `0x5DA5E0(cell; 9, x, y, 1, 5)`) and, if the human sees the tile, message `VLC_ACTIVE`; if `r == 3` (stage 4) nothing else. If the draw is `>= dy`: set the stage to 0 and detach any animation (`0x5DA7A0`). |
| `1 <= s <= 3` | if no animation is attached (`[cell+0xD8] == 0`) attach the smoke animation (type 9); set the stage to `s + 1`. |
| `s == 4` | **eruption** (5.3). |

A dormant volcano therefore becomes active with probability `dy / P` per pass (at most 50/5000 = 1 %), then
climbs one stage per pass (a draw of 3 starts directly at stage 4, so the delay from activity to eruption is
between 0 and 3 passes after the activating pass).

### 5.3 Eruption **V**

1. The lava list `0xA283CC` is reset to three entries `(-1, -1)`.
2. If the human sees the tile: message `VLC_ERUPTS` at `(x, y)`.
3. The stage is set to 0 and the *eruption* animation (type 10) is attached.
4. For `n = 1 .. 8` (the ring around the volcano) while fewer than 3 lava tiles have been chosen:
   * `(nx, ny)` = the wrapped neighbour; if it is not inside `0 <= nx < W`, `0 <= ny < H`: next `n`;
   * `chance = 15`; for `k = 1 .. 8`: `M` = the wrapped neighbour `k` of `(nx, ny)`; if `M` is in bounds
     (`0x426BD0`) and equals one of the stored list entries, `chance = 25` and the `k` scan stops;
   * draw `rand(100)`; if it is `< chance`: store `(nx, ny)` as the next list entry and apply **lava**
     (5.4) to it.
5. Finally lava is applied to the volcano tile itself.

So an eruption affects the volcano tile for certain and up to three of its eight neighbours, each with 15 %
(25 % when it touches an already chosen lava tile). The list is filled in ring order `n = 1, 2, ...`, so the
first lava tile is always tested before the later ones. Water neighbours are chosen and stored like any other
(the lava routine then does nothing to them).

### 5.4 Lava `0x4F4040(x, y)` **V**

In order, on the tile `c`:

1. If slot `+0x5C(0)` holds (**I**: "the tile has a railroad"; for a city tile the slot delegates to
   `+0x114`): clear overlay mask `0x2` (railroad).
2. If overlay bits 0 to 3 are not all zero (`(overlay and 0xF) != 0`): clear mask `0xF`.
3. If the tile has an airfield (bit 29): destroy it, `0x5DAEC0(airfield; 1)` (the object is in pool
   `[0xA52E3C]`, indexed by the cell's colony id; section 1.4).
4. If the tile has an outpost (bit 31): `0x5D6430(map; x, y)`; if it has a radar tower (bit 30):
   `0x5D6360(map; x, y)` (the outpost and radar destroyers; bodies **O**, same pattern as 1.4). A plain
   Colony on the tile is not destroyed by this routine (**O** whether anything removes it later).
5. Clear mask `0xF000001F` (road, railroad, mine, irrigation, fortress and bits 28 to 31).
6. **If the tile is water: stop** (the rest does not apply).
7. Set overlay mask `0x40` (the same bit as pollution).
8. If a city stands on the tile (`0x56D2C0(x, y)`): message `VLC_KILLS_CITY` if the human sees the tile, then
   destroy the city: `0x4AECC0(city; owner, 0)` (the city destroyer; it decrements the world city count).
9. Every unit in the tile's list (cell slot `+0xA0`, then the unit pool): if the human's slot is the unit's
   apparent owner (`unit +0x34`) and the message was not yet shown for this tile, `VLC_KILLS_UNIT`; then kill
   the unit with `0x5BBBC0(unit; 0, 0, 0, 0, 0, 0, 0)`. Every unit dies, regardless of owner, type or
   domain.

Terrain stays volcano; there is no terrain change in lava.

### 5.5 Golden checks (data)

With `P = 5000` and `dy = 50`: activation probability 1 % per pass and volcano. For `dy = 2` (late game) 0.04 %.
For a volcano tile whose eight neighbours are all land and unaffected by earlier choices, the expected number
of lava neighbours per eruption is about `8 · 0.15 = 1.2`, capped at 3 by the list length.

## 6. Plague

### 6.1 Scenario fields and state **V**

GAME fields (the editor's *Disasters* dialog; `biq-format.md`): `permit_plagues` byte `[0x9C5C47]`
(default 0: **random-map games never have a plague**), `earliest start` `[0x9C5C48]` (a calendar year; the
engine overwrites it during play), `variance` `V = [0x9C5C4C]`, `duration` `D = [0x9C5C50]` (years),
`strength` `S = [0x9C5C54]`, `grace period` `G = [0x9C5C58]` (years), `max occurrences` `M = [0x9C5C5C]`,
`schedule` `[0x9C5C60]` (`-1` = the first start year was not yet randomized). Runtime state: occurrences so
far `[0xA52A58]`; date `0xA52AD0` whose year `+0x68 = [0xA52B38]` is the year of the **previous pass**;
date `0xA52A60` whose year `[0xA52AC8]` is the **start year of the running plague**.

### 6.2 The scheduler `0x4F5250` (once per round, before the volcano) **V**

```
if multiplayer or permit_plagues == 0 or occurrences >= M: return
Y  = date(turn).year ;  Y1 = (Y == 0 ? 1 : Y) ;  E = [0x9C5C48]
if [0x9C5C60] == -1:                                      -- first pass ever
    [0x9C5C60] = rand(V)                                  -- stored, never used again except as the "drawn" flag
    u  = trunc(V · G / 100)
    w  = trunc(u · rand(100) / 100)
    c  = E + w - trunc(u / 2)
    if c > Y1: E = c                                      -- [0x9C5C48] is overwritten
Yp = [0xA52B38] (0 -> 1)                                   -- previous pass year
date 0xA52AD0 := date(turn)                                -- so [0xA52B38] now holds this pass's year
if Y1 < E:                 return                          -- not yet
if Y1 == E:                START
if Y1 > E and Yp < E:      START                           -- the start year fell between two passes
S0 = [0xA52AC8] (0 -> 1)
if S0 + D > Y1:            ACTIVE                          -- plague still running
END:   for every city: 0x4B4090(city)                      -- cure
       occurrences += 1
       if occurrences < M:
           u = trunc(V · G / 100) ; w = trunc(u · rand(100) / 100)
           [0x9C5C48] = Y1 + G + w - trunc(u / 2)          -- next earliest start
       return
START: date 0xA52A60 := date(turn)                         -- records the start year [0xA52AC8] = Y
ACTIVE: for every city (all civilizations): 0x4B3C30(city)
```

Draw order: first-pass `rand(V)`, `rand(100)`; per END `rand(100)`; per city the draws of 6.3. The routine
returns before any draw when `permit_plagues` is clear or the occurrences are used up.

Shape: a plague is active for the years `S0 .. S0 + D - 1` (the test `S0 + D > Y1`), is cured on the first
pass at or after `S0 + D`, and the next one cannot start before `Y1 + G` plus a jitter of
`[-trunc(u/2), +trunc(u/2) - 1]` years (`u = trunc(V·G/100)`; `w` is in `0 .. u-1`).

Quirk **H**: if the pass finds `Y1 > E` and `Yp >= E` while no plague has ever started (for example after
loading a game written with an `earliest start` already in the past) then `S0 = 1` (the date object's
zero is mapped to year 1) and the ACTIVE test fails for any later year, so the routine takes the END branch:
it cures every city and **uses up an occurrence without a plague ever having run**.

### 6.3 Per-city step `0x4B3C30(city)` (every city, every pass while ACTIVE) **V**

Let `sz = min(city.+0x138, 12)`, `c = city.+0x1D0` (the city's gross commerce running sum, `yields.md`),
`capital = Player[owner].+0x2C` (capital city id, `-1` none).

**Exposure.**

```
base = 5 · sz                         if the owner has no capital
base = 10 · sz                        if 0x57F0A0(0xB72888; city, capitalCity, owner) holds   -- the city is the capital or is trade-connected to it for its owner (primitives.md 4.3)
base = 5 · sz                         otherwise
e    = base + 5 · c
if 0x427540(city) <= 0 (the size class is "town", size <= [0x9C72E4]) and 0x4C0C70(city) > 0 (the city has a land bombard defense, i.e. walls):
        e += 4 · sz
T    = trunc(S · e / 100) + trunc(e / 2)
if knowsTech(owner, 8): T = T / 8   (signed, truncated toward zero)
```

**Infection.** `if city.+0x3B0 == 0 (not infected)`: `roll = rand(1024)`; if `roll < T` call `0x4B3700`
(infection). If the city is still not infected afterwards the step ends (no further draw). If it is infected
(just now or before) continue:

**Units on the city tile.** If the tile is in bounds, for **each unit in the tile's list** (all owners,
list order of slot `+0xA0`): `k = min(city.+0x3B4, 20)`; `roll = rand(2048)`; if `roll < S + 5·k`
and the unit's prototype domain `PRTO +0x9C == 0` (land) and the unit does **not** have ability 29
(`0x5BC8B0(unit, 29)`, the King) then: message `PLAGUE_KILL` (unit name) if the unit's apparent owner
(`+0x34`) is the human, kill it with `0x5BBBC0(unit; 0 ×7)`, and `city.+0x3B4 += 1`. One draw is made per
unit **before** the type test; later units see the incremented `+0x3B4`.

**Population.** `roll = rand(1768)`; `k = min(city.+0x3B4, 20)`; if `city.size > 1` and `roll < S + 5·k`:
`0x4BA230(city; 1, -1, 0)` removes one citizen, and if the owner is the human the message `SUMMARY_PLAGUE`
(city name, x, y) is shown. The draw is made even if the size is 1.

UI tail (no state change): if `[0x9C34E4]` is set the view is centred on the city and the flag cleared; the
plague icon animation (type 11, `0x4C0AC0`) is attached to infected cities the human sees.

Tech 8 is a **hard-coded technology row index**: in the shipped `conquests.biq` row 8 is *Writing* (rows 0
to 8: Bronze Working, Masonry, Alphabet, Pottery, The Wheel, Warrior Code, Ceremonial Burial, Iron Working,
Writing). **H**: the constant was meant to be the medical technology of an earlier rule set; a modified
ruleset gives whatever technology sits in row 8.

### 6.4 Infection `0x4B3700(city)` and cure `0x4B4090(city)` **V**

Infection (only the first pass of a city's epidemic does the first part):

1. If `city.+0x3B0 == 0`: `city.+0x3AC` := the current year `Y` (0 mapped to 1), via a local date object.
2. `city.+0x3B0 := 1`; `city.+0x3B4 := city.+0x3B4 + 1`.
3. If `city.size > 1`: `0x4BA230(city; 1, -1, 0)` (one citizen dies).
4. For a human owner: the plague dialog / message and, in a multiplayer game, the network event; the rest
   (about 600 bytes of UI) is **O**.

Cure `0x4B4090`: `city.+0x3B0 := 0`, `city.+0x3B4 := 0`, and if the plague icon (`city.+0xA4 == 11`) is
attached it is detached (`0x406200`, `city.+0xA4 := 0`). The year field `+0x3AC` is left as it is.

Infection therefore lasts until the global END, not for a per-city duration; the pressure on the city grows
with `+0x3B4` (one per infection plus one per unit killed, capped at 20 in every formula).

### 6.5 Golden checks (data)

* `S = 30`, size 8, commerce 30, not connected to the capital, no walls: `e = 40 + 150 = 190`,
  `T = 57 + 95 = 152`, infection probability `152/1024 = 14.8 %`; with tech 8 known `T = 19` (1.9 %).
* Same city connected to the capital: `e = 80 + 150 = 230`, `T = 69 + 115 = 184` (18.0 %).
* A town of size 6 with walls, commerce 10, not connected: `e = 30 + 50 + 24 = 104`; `S = 50`:
  `T = 52 + 52 = 104`.
* Unit kill threshold with `S = 30` and `+0x3B4 = 3`: `30 + 15 = 45`, probability `45/2048 = 2.2 %` per land
  unit that is not a King; at the cap `k = 20` it is `130/2048 = 6.3 %`. Citizen loss: `45/1768 = 2.5 %`.
* Scheduler: `V = 5`, `G = 1000`: `u = 50`; the jitter `w - 25` runs from `-25` to `+24` (for `rand(100) = 99`:
  `w = trunc(50·99/100) = 49`). Year `Y = 0` is compared as 1. `E = 1000` and a pass at `Y1 = 1000` starts
  the plague; `D = 30` keeps it active until the first pass whose `Y1 >= S0 + 30`.

## 7. Corrections to the other files (found while reading these routines)

* `resources.md` (GOOD `+0x44`): it is the **disappearance probability** `p`, tested as `rand(p) == 0` for
  each connected, owned luxury or strategic tile on every 5th turn; the shipped strategics (Iron 800,
  Saltpeter 800, Coal 400, Oil 200, Aluminum 400, Uranium 100; Horses and Rubber 0) are values of `p`, not
  "800/400/200/100 frequencies", and the generator's frequency is `+0x40`. The `biq` `good.rs` wording
  "1 in N each turn" should read "1 in N each 5th turn, per tile".
* `NOTES.md` 11.10 (`0x5F2090`, "placeBarbarianCamps"): the write `vfunc(0xE0)(2, 0x10000, ...)` goes to
  **plane 2**, bit 16, which is the feature **bonus grassland** (`biq-format.md`), and the tile test
  `vfunc(0xC4) == 2` is the base terrain *grassland*. It is the bonus-grassland placement of the generator,
  not a camp placement; camps (overlay bit 7) are created while the game runs (`barbarians.md`).
* `yields.md` / `economy.md`: the pollution numbers `0x4B1A50` / `0x4B1B40` and the city-turn calls
  `0x4B2F80`, `0x4B4970` are part of the city turn `0x4BE970`, after the happiness refresh and before
  `0x4B45A0`; they are not in `economy.md`'s upkeep.
* `NOTES.md` 11.8 places resources with `0x5F22A0`; `0x5F3320` (section 2.4 here) is the validity test the
  **running game** uses; the generator's own spacing logic lives in `0x5F22A0` and is not guaranteed to be the
  same function (**O**).

## 8. Verification status and open items

All of sections 2 to 6 were read from the full disassembly (`0x4F4CB0`, `0x5D5D00`, `0x5F3320`,
`0x4B1A50`, `0x4B1B40`, `0x4B1C10`, `0x4B2F80`, `0x4B4970`, `0x4F4380`, `0x4F4040`, `0x4F5250`, `0x4B3C30`,
`0x4B3700` head, `0x4B4090`, `0x5DF100`, `0x4ACF40` pollution lines). None was run.

Open (ranked by how much a faithful port needs them):

1. `0x4BA230(city; k, -1, f)` (the city shrink used by plague and meltdown): what it does to improvements,
   tile assignments and happiness, and what `f` changes.
2. `0x5D59E0` and cell slot `+0x128` (terrain change side effects, 4.1), including the continent and
   coast re-evaluation.
3. `0x4B45A0` (the step between pollution and meltdown). (`0x4ACCC0` is resolved: the obsolescence test,
   `city-buildings.md` section 2.)
4. The tail of the colony destroyers `0x5DAA90` / `0x5DAEC0` (which improvements they strip and why), the
   bodies of `0x5D6430` / `0x5D6360`, and which overlay bit (if any) marks a plain Colony.
5. The meaning of `[cell+0x34]` as a general cell counter (the volcano stage is its only reader found) and
   of cell slot `+0x5C`.
6. The UI tail of `0x4B3700` and `0x4B3C30` (plague dialog, network events).
7. The nuclear weapon's own terrain and pollution effect (`0x5B4070`).
8. The network round `0x476330` was not read; it shares these routines but may order them differently.
9. Dynamic confirmation of any rule above (for example a seeded run that forces a plague start year).
