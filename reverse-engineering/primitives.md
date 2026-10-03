# Shared primitives: containers, handles and the query functions the rules call

Standalone specification, recovered from the raw disassembly of `Civ3Conquests.exe` (PE32, MSVC 6,
image base `0x400000`). Every rule document (`happiness.md`, `yields.md`, `economy.md`,
`government.md`, `buildable.md`, `capture.md`, `combat.md`, `air.md`) calls the functions below by
address and never restates them. This file does, so that a reader who has no access to the binary can
implement them. Evidence level for every entry is **A** (decoded from the disassembly) unless a line
says otherwise; nothing in this file was run under emulation.

Notation: `this` is `ecx`. `ret N` means N/4 stack arguments, listed in call order after a semicolon:
`f(this; a, b)`. Offsets are in-memory offsets. For BLDG and TECH rows the in-memory offset is the
file-body offset plus 4 (the `biq` crate documents both columns). "Bit `k` of `m`" is `(m >> k) & 1`.

## 1. Containers and handles

### 1.1 Global tables

| what | address | layout |
|---|---|---|
| Player array | `0xA52E98` | 32 records of `0x20E4` (8 420) bytes. Record `+0x1C` is its own slot number; slot 0 is the barbarians (`biq-format.md`: the game uses `LEAD` index + 1 as the player slot) |
| highest used slot | `[0xA5279C]` | `0x539D60` only matches slots `0..=[0xA5279C]` |
| City pool | base `[0xA52E6C]`, last index `[0xA52E78]` | 8-byte slots; the node pointer is the dword at slot `+4`; the city object is **node − 0x1C** |
| Cell table object | `0x9C736C` | pointer array at `[obj+0x148]`, count `word [obj+0x40]` |
| BLDG table | `[0x9C40AC]` | stride `0x110`, count `[0x9C3D80]` (`N` below) |
| TECH table | `[0x9C7320]` | stride `0x74`, count `[0x9C3DBC]` (`T` below) |
| GOOD table | count `[0x9C3DA4]` | row `+0x3C` is the good's class (1 luxury, 2 strategic; `0x5E3720`, `0x5E3730`) |
| per-tech knowledge | `[0xA52B4C]` | dword per tech; **bit p = player slot p knows the tech** |
| map width | `[0x9C74D4]` | in x units, i.e. twice the number of columns; height `[0x9C74C0]` |
| map wrap flags | `[0x9C755C]` | bit 0 wraps x, bit 1 wraps y (the cell table object keeps a second copy at `+0x1F0`, read by `0x5EEDB0`) |
| Game object | `0xA52658` | `+0x4F8`: dword array indexed by building, the **city pool index of a great wonder** (`0x539030(this; b)`); `+0x4FC`: byte array indexed by building, non-zero when that great wonder has been built by anyone (`0x538FE0(this; b)`; `buildable.md` section 2 step 7 refuses a great wonder when it is non-zero). Its other fields are in the documents that read them |
| human-player mask | `[0xA526BC]` | bit p set: slot p is human. `[0xA526C0]` is the mask of slots in play; `[0x9FD4BC]` the local player's slot |
| gameplay RNG | `0xA526B4` | one dword of state; `0x60BAB0(this; n)` is the die every document calls `rand(n)` (1.6) |
| current turn | `[0xA526AC]` | incremented once per round, at the very end of the round (`turn.md`) |

### 1.2 Pool access (`0x437870`, and the inline copies)

`pool.get(i)`: if the base pointer is null, or `i < 0`, or `i > last`, return null; otherwise take the
dword at `base + i*8 + 4`; if it is 0 return null; otherwise return it minus `0x1C`. The generic form
`0x437870(this; i)` uses `[this+4]` as the base and `[this+0x10]` as the last index. The first dword
of each slot is not read by any caller seen here.

### 1.3 Cells

`cellIndex(x, y) = ((W >> 1) * y + (x >> 1)) & 0xFFFF`, with `W = [0x9C74D4]`, an arithmetic shift of
`W` and a logical shift of the 16-bit `x`. `0x5D16A0(0x9C736C; index)` returns the cell pointer
`table[index]`, or the address of a static empty cell (`0xCAA330`) when the table is null, `index` is
negative or `index >= count`. Cell virtual slot `+0xB8` (no arguments) returns the cell's **continent
number** as a signed 16-bit value. A cell has one signed word per player at `cell +0x6E + 2*p`: the id
of the city that tile is connected to for player `p` (4.3).

### 1.4 City fields these functions read

| offset | meaning |
|---|---|
| `+0x20` | city id, `0..511` (index into the connection matrix, 4.3) |
| `+0x24`, `+0x26` | x, y (words) |
| `+0x28` | owner slot (byte) |
| `+0x290` (= set at `+0x274`, whose bytes start at `+0x1C`) | **the city's own building bitmap**: building `b` is present iff bit `b & 7` of the byte at `city + 0x290 + (b >> 3)` is set (`0x5DF8D0(city+0x274; b)`) |
| `+0x9C` | resource bit mask the city can use locally (bit g = good g) |

### 1.5 Player fields these functions read

| offset | meaning |
|---|---|
| `+0x1C` | own slot number |
| `+0x20` | civilization (race) index; `RACE +0x91C` holds the same number |
| `+0x2C` | capital city pool index, `-1` for none |
| `+0xA0` | government index |
| `+0x3C` | the turn on which the **Golden Age** ends: a Golden Age runs while `[0xA526AC] < Player.+0x3C` (`government.md` section 2, `yields.md` section 3) |
| `+0x15D0` / `+0x15D4` | **Science Age** (the timed research bonus): flag byte (bit 0) / last turn (section 6) |
| `+0x15E8` | pointer to a dword array, one entry per building: the pool index of the city that holds that **small wonder** for this player |
| `+0x1614` | resource supply table (`capture.md` 12.2) |
| `+0x20D0` | the **building reach set** (3.2); its array pointer is at `+0x20D4` |

### 1.6 The gameplay random generator `rand(n)` (`0x60BA80`, `0x60BAB0`)

State `s` is the dword at `0xA526B4` (unsigned, wrapping arithmetic).

```
draw():   s = s * 0x41C64E6D + 0x3039   (mod 2^32);   k = (s >> 16) & 0x7FFF        // 0x60BA80, k as k / 32768.0
rand(n):  k = draw();   return (k * (n & 0xFFFF)) >> 15                              // 0x60BAB0, ret 4
```

`rand(n)` therefore returns an integer in `[0, n & 0xFFFF)`; it **always consumes exactly one draw**, also for
`n = 0` (result 0) and for `n = 65536` (result 0). The x87 product of the 15-bit `k * 2^-15` and a 16-bit
integer is exact, so the `_ftol` truncation (`0x64A230`) is a plain integer shift; the result is **not** a
modulus: for `n` that does not divide 32768 some values have one more preimage than others, and a port that
wants the original's draws must use this formula, not `k % n`. A roll such as "`rand(100) < p`" is
`(k * 100) >> 15 < p`. Every gameplay roll in every document uses this single stream (171 call sites with
`ecx = 0xA526B4`), so the order of draws inside a turn is part of the behaviour (each document lists its draw
order). The map generator's private generators and the MSVC `rand()` (`0x64A20E`, 70 call sites, used by
advisor and AI scans) are separate streams. Seeding sites are listed in `combat.md` (section "Seeding").

## 2. Technology queries

### 2.1 `Player::knowsTech(this; t)` `0x561440`, `ret 4`

* `t == -1` returns **true** (no requirement).
* `t == T` (the tech count, used as a "never available" sentinel) returns **false**.
* Otherwise bit `Player.+0x1C` of the dword `[0xA52B4C] + 4*t`.

### 2.2 `Player::knowsTechWithFlags(this; mask)` `0x561480`, `ret 4`

True when some tech `t` in `0..T-1` has `(TECH[t].+0x68 & mask) == mask` and the player knows it.
`TECH +0x68` is the `flags` dword (`biq::sections::tech::flags`: bit 6 Recycling, bit 13 Trade over
Sea, bit 14 Trade over Ocean, bit 12 Double Wealth, and so on). The check is "all bits of `mask`",
so every caller seen passes a single bit.

### 2.2.1 The research predicates (`research.md` 9.3 is their owner)

`Player::canResearch(this; t)` `0x561580`, `Player::reachable(this; t)` `0x5614E0` and
`Player::techDepth(this; t, d)` `0x561620` are specified in `research.md` section 9.3; they use the same
`t == -1` and `t == T` conventions as 2.1.

### 2.3 Obsolescence convention

A building (BLDG `+0xE0`, `rendered_obsolete_by`) is obsolete for a player when that field is not
`-1` and not `T` and `knowsTech` is true. `0x4B1F90` and `0x4B0160` test it as "field `< 0`, or the
owner does not know the tech" and treat the building as active. `0x55A8D0` and `0x55A560` also skip
the value `T`.

## 3. Buildings, wonders and their reach

### 3.1 `City::hasBuilding(this; b, withReach)` `0x4ACB50`, `ret 8`

```
if b == -1                               return true
req = BLDG[b].+0xD4                      // required government
if req != -1 and req != Player[owner].+0xA0   return false
if withReach:
    S = Player[owner].reachSet
    if S is allocated and S contains b                              return true
    k = (continent(city) + 1) * N + b
    if S is allocated and S contains k                              return true
return bit b of the city's own building bitmap
```

`continent(city)` is cell virtual `+0xB8` of the city's cell (1.3). `withReach = 0` skips both set
tests. So a building counts as present in a city if the city built it, or if a wonder that this owner
holds grants it to every city (`b`), or to every city on the wonder's continent (`k`). The government
check applies to the whole answer: a building whose required government is not the current one is
never present, even if it was built.

### 3.2 The reach set

`Player +0x20D0` is an open-addressing hash set of dwords (8-byte slots: key, value; empty key `-1`;
slot count at `+0xC`, used count at `+0x8`, slot array at `+0x4`; the hash is virtual slot `+0x18`;
probing is linear with wrap). Only membership of a key is ever asked, so any set of integers is an
equivalent implementation. It holds two kinds of key:

* a plain building number `b` (BLDG `+0x88`, `gain_in_every_city`), and
* a continent key `(continent + 1) * N + b'` (BLDG `+0x8C`, `gain_in_every_city_on_continent`).

**Rebuild** `Player::rebuildReach` `0x55A560` (no arguments): empty the set, then for every building
`B` in `0..N-1`:

1. `B.+0xF0 & 4` (a Great Wonder, `other_characteristics` bit 2) must be set;
2. `B.+0x88` and `B.+0x8C` must not both be `-1`;
3. `B.+0xD4` is `-1` or equals the player's government;
4. `B.+0xE0` is `-1`, or `T`, or the player does not know that tech (a wonder is switched off by
   obsolescence);
5. the wonder must exist and be held by one of this player's cities: the wonder owner table
   (`0x539030(0xA52658; b)` returns the city pool index) must name a valid city whose owner byte is
   this player's slot.

For a wonder that passes: if `B.+0x88 != -1` insert the key `B.+0x88`; if `B.+0x8C != -1` insert
`(continent(wonderCity) + 1) * N + B.+0x8C`. The "building" a wonder grants is therefore a *building
number*: *Sun Tzu* grants *Barracks* on its continent, *Hoover Dam* grants *Hydro Plant* on its
continent (`biq` crate evidence C). Small wonders never enter the set.

**Callers (the events after which the set is rebuilt; `city-buildings.md` specifies `0x4ACF40`)**: `0x4ACF40` (a building is added to or removed
from a city, call at `0x4AD90D`), `0x55CD60` (government change, `0x55CDEA`), `0x561860` (a player
gains a technology, `0x561A5C`), `0x564800` (a city changes hands, two calls at `0x564F5A` and
`0x564F6D`), and two game-load/setup routines (`0x5D1EA0`, `0x5D2150`). A wonder built elsewhere takes
effect for another civ's cities only through its own owner's set.

### 3.3 `Player::countWonders(this; mask, onlyCity)` `0x55A8D0`, `ret 8`

Counts the buildings `B` for which all of the following hold; returns the count:

* `B.+0xD4` is `-1` or the player's government;
* `(B.+0xF8 & mask) != 0` (`wonder_flags`);
* **Great Wonder** (`B.+0xF0 & 4`): not obsolete (2.3) and held by one of this player's cities (the
  wonder owner table, as in 3.2 step 5); **Small Wonder** (`B.+0xF0 & 8`): the city is
  `pool.get(Player.+0x15E8[b])` and must belong to the player; other categories never count;
* `onlyCity == 0`, or the holding city is `onlyCity`.

The wonder flag bits are `biq::sections::bldg::wonder_flags` (bit 4 doubles research, bit 5 +1 trade,
bit 6 halves upgrade cost, bit 7 pays trade-installation upkeep, bit 8 allows nuclear devices, bit 9
growth +2, bit 11 reduces war weariness everywhere, and so on).

### 3.3.1 `Player::countSmallFlagWonders(this; mask, onlyCity)` `0x55AA10`, `ret 8`

The same loop as 3.3 over the **small-wonder flag word** `BLDG +0xF4`. Returns the number of buildings `B` in
`0..N-1` for which all of the following hold (there is **no obsolescence test**):

* `B.+0xD4` is `-1` or the player's government (`Player +0xA0`);
* `(B.+0xF4 & mask) != 0`;
* a city `K` is found: for a Small Wonder (`B.+0xF0 & 8`) it is `pool.get(Player.+0x15E8[B])`; for a Great
  Wonder (`B.+0xF0 & 4`) it is `pool.get(0x539030(0xA52658; B))`; any other category is skipped; `K` must exist;
* `onlyCity == 0` or `K == onlyCity`;
* `K`'s owner byte (`+0x28`) equals the player's slot.

The masks passed by the callers read so far: `0x2` (the Military Academy only in the shipped rules: an Army can
be built in that city, `buildable.md` 3.1 step 5) and `0x10` (Build Spaceship Parts, `buildable.md` 2 step 6;
the call there passes `onlyCity = 0`).

### 3.4 `Player::ownsDoublingWonder(this; b)` `0x55A7E0`, `ret 4`

True when some Great Wonder `B` (`B.+0xF0 & 4`) has `B.+0x84 == b` (`doubles_happiness_of`), a
permitted government (`B.+0xD4` is `-1` or current), is not obsolete (2.3) and is held by one of the
player's cities. *Sistine Chapel* names *Cathedral* in the shipped file.

### 3.5 `City::countBuildingsWithFlag(this; mask)` `0x4B1F90`, `ret 4`

The number of buildings `b` in `0..N-1` with `hasBuilding(b, 1)`, not obsolete for the owner (2.3) and
`(BLDG[b].+0xEC & mask) != 0` (`improvement_flags`). The result is a count, not a bit.

### 3.6 `City::countOtherCitiesWithBuilding(this; b)` `0x4BDDE0`, `ret 4`

Over every city in the pool (indices `0..last`, null slots skipped): count the cities that have the
same owner, a **different** city id (`+0x20`), the same continent number as `this`, and
`hasBuilding(b, 1)`.

### 3.7 `City::buildingUpkeep(this; b)` `0x4ACDF0`, `ret 4`

Gold per turn for building `b` in this city:

```
S = Player[owner].reachSet
if S contains b                                   return 0
if S contains (continent(city)+1)*N + b           return 0     // both tests skipped when S is unallocated
if countWonders(0x80, 0) > 0 and (BLDG[b].+0xF0 & 0x40)   return 0   // pays_trade_maintenance and the Commercial trait
return BLDG[b].+0xAC                                           // maintenance
```

A building supplied by a wonder is free even if the city also built one. The mask `0x80` is
`wonder_flags` bit 7 (*Smith's Trading Company*); `other_characteristics` bit 6 is the Commercial
trait bit.

### 3.8 `City::startingExperience(this; prto)` `0x4B0160`, `ret 4`

Returns **2** (veteran) or **1** (regular), by the unit's domain `PRTO +0x9C`:

| `PRTO +0x9C` | needs a building with `improvement_flags` | meaning |
|---|---|---|
| 0 | `0x2` | veteran ground units (*Barracks*) |
| 1 | `0x20000` | veteran sea units (*Harbor*) |
| 2 | `0x40000` | veteran air units (*Airport*) |
| other | none | always 1 |

The building must satisfy `hasBuilding(b, 1)` and be active (obsolete tech `< 0`, or not known by the
owner). `happiness.md` section 9 stores the result in `[unit+0x44]` (clamped to 0..3): the new unit's
experience level.

## 4. Resources and the connection matrix

### 4.1 `City::resourceUsable(this; good)` `0x4ADE30`, `ret 4`

Specified in `capture.md` section 12.2; restated: false for `good == -1` or `good >= [0x9C3DA4]`. If
the owner has no capital (`Player.+0x2C == -1`) or the connection test `0x57F0A0` (4.3) says this city
is not connected to the capital, the answer is bit `good` of the city's own mask `city +0x9C`. If
connected, the answer is `0x55E730(owner; good)`: some civ `j >= 1` has a supply record
`owner +0x1614[(good * 32 + j) * 3]` whose first two bytes are both non-zero. The tail that tests
`city +0x9C` and calls `0x55E850` returns false after the call.

### 4.2 Good classes

`0x5E3720(good row)` is true when `row +0x3C == 1` (luxury); `0x5E3730` when it is `2` (strategic);
`0x5E3700` is the combined test used by the AI (luxury or strategic).

### 4.3 The connection matrix `0xB72888` (queries only; the maintainer is **open**)

The object at `0xB72888` (constructor `0x57F210`, destructor `0x57F330`) holds, among five heap
arrays of `[0x9C73AC] & 0xFFFF` entries, a table of `0x40000` dwords at `+0x38`: a 512 x 512 matrix
indexed `(cityA.id << 9) + cityB.id`. The dword is a **bit set of player slots**: bit p says "city A
and city B are connected for player p".

* `0x57F0A0(this; A, B, p)`, `ret 0xC`: false if `A` or `B` is null; true if `A == B`; with `p == -1`
  the answer is "any of the two owners' bits is set", i.e. `matrix[A,B] & ((1 << A.owner) | (1 <<
  B.owner)) != 0`; otherwise bit `p` of `matrix[A,B]`.
* `0x57F130(this; city, x, y)`, `ret 0xC`: look up the cell of `(x, y)`, read its per-player word at
  `cell +0x6E + 2*city.owner` (a city id, or `-1`); if that city exists, true when it is `city` itself
  or when the matrix says the two are connected (with `p` = the owner, or the "either owner" rule if
  the owner byte is `-1`).

How the matrix is filled (the breadth-first search `0x57F360`, driven by `0x580540`, which consults
the techs *Trade over Sea* / *Trade over Ocean* through `0x561480`) is not specified here; see
`trade-network.md` if present, otherwise it is an open item of this file.

## 5. City output accessors

`0x4ACAE0(this; kind, flag)` returns `[city + 0x25C + 4*kind]` and `0x4AC9E0(this; kind, flag)`
returns `[city + 0x268 + 4*kind]`, where `kind` 0 is luxury, 1 is science, 2 is tax (`yields.md` 5.1:
the city's shares and the specialists' shares). Only `kind == 1` with `flag != 0` is special: if the
player's Science Age is active (section 6) the value is multiplied by the single-precision constant
`1.25` (`0x66905C`) and truncated toward zero (`0x64A230`, an x87 `ftol`).

## 6. Small tests

* `0x55C890(this)`: **Science Age active** (the owner's timed research bonus): `Player.+0x15D0` has
  bit 0 set and `[0xA526AC] <= Player.+0x15D4`. (The Golden Age is a different state, `Player.+0x3C`.)
* `0x539D60(this = RACE row)`: the slot `p` in `0..=[0xA5279C]` with `Player[p].+0x20 == RACE.+0x91C`,
  or `-1`.
* `0x437D10(this = Player)`: **is human**: bit `Player.+0x1C` of `[0xA526BC]`, as 0 or 1.
* `0x56AAB0(this = Player; prto)`: **race may build the prototype**: `PRTO[prto].+0x90 & (1 << (Player.+0x20 &
  31))` is non-zero, as 0 or 1 (`shl` masks the count to five bits).
* `0x5DF900(x)`: population count of the 32-bit argument (loop of `shr` and `adc`).
* `0x4ABE70(this = city)`: `[city+0x13C] != [0x9C3D64]`.
* `0x47B530`: a tail jump to `0x499FE0`, the multiplayer-mode gate used by the message code.

## 7. Open

* The maintainer of the connection matrix and of the per-player cell words (`0x57F360`, `0x580540`,
  `0x57E450`, `0x57EDD0`, `0x55E9B0`).
* The first dword of a pool slot and the stable meaning of the unlabelled bytes of the reach set slots
  (the value dword is always 0 when inserted).
* Whether `0x55A560` is also reached through a virtual slot (only direct calls were counted:
  7 `E8` sites by the call-target census over the linear-sweep disassembly).
