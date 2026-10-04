# Barbarians: camps, spawning, uprisings, tribes

Clean-room specification of everything the executable does for the barbarian civilization (player slot 0)
**except the movement and combat decisions of barbarian units**, which are not decoded (section 10).
Image base `0x400000`; `Civ3Conquests.exe` (PE32, MSVC 6). Companion documents: `turn.md` (the round, and
`Player::turn`, whose slot-0 branch is section 3 here), `world-events.md` (cell slot table, calendar, pools),
`combat.md` (barbarian combat terms, the raid on a city), `capture.md` (city capture), `primitives.md`.

Evidence tags: **V** read in the disassembly for this document (address given), **I** inferred from a
consistent set of reads, **H** hypothesis (not to be implemented as fact), **O** open.

## At a glance

| Mechanism | Address | When | Randomness |
|---|---|---|---|
| Spawn land units at camps (slot-0 branch of `Player::turn`) | `0x5604B0` (`0x5604D7..0x561200`) | every round, if the barbarian setting `S >= 1` and the world has `>= 2N - 2` cities | `rand(8)` per camp and per spawn kind |
| Spawn sea units at camps | same | same, only when the world has `> 4N - 4` cities | `rand(8)` per camp |
| Found a new camp | `0x55F9F0` | same round, when the camp count `< N - 1` | `rand(tiles)` per attempt, `rand(15)` once |
| Uprising ("barbarian explosion") | `0x55FD00` | when exactly two civs share an era (`research.md` 7.2) | none |
| Initial camps get 2 garrison units | `0x5D21E0` | map/scenario setup | none |
| Camp destroyed, +25 gold | `0x565A00` | a non-barbarian unit steps on the tile, or the tile's owner changes to a civ, or a nuclear blast | none |
| Tribe-name table | `0xA526C8` (75 bytes) | maintained by the four routines above | none |

`N` is the number of bits set in the in-play mask `[0xA526C0]` (`0x5DF900`, a popcount); slot 0 is normally a
member, so `N - 1` is the number of real civilizations. `S` is `[0x9C737C]`, the resolved barbarian setting.

## 1. State, constants, conventions

### 1.1 Settings and rules **V**

| Item | Where | Meaning |
|---|---|---|
| `S` | `[0x9C737C]` | resolved barbarian activity. The world-setup stores `-1 .. 3` (the editor's "random" value `4` is replaced by `rand(5) - 1`, `rust/src/options.rs`); the selected value is `[0x9C7378]`. **Everything in this document is disabled when `S <= 0`** (`0x5604E0`, `0x55FD1E`). Which label belongs to which value is **H** (`-1` none, `0` sedentary, `1` roaming, `2` restless, `3` raging). |
| advanced unit | `[0x9C7250]` | RULE `+0x6C` "advanced barbarian unit"; PRTO row 11 (Horseman) in the shipped rules |
| basic unit | `[0x9C7254]` | RULE `+0x70` "basic barbarian unit"; PRTO row 6 (Warrior) |
| sea unit | `[0x9C7258]` | RULE `+0x74` "barbarian sea unit"; PRTO row 29 (Galley) |
| tile count | word `[0x9C73AC]` | `T`, number of cells |
| width / height | `[0x9C74D4]` / `[0x9C74C0]` | `W`, `H` (raw x extent, row count); also `map+0x168` / `map+0x154` |
| wrap flags | byte `[0x9C755C]` = `map+0x1F0` | bit 0: x wraps, bit 1: y wraps |
| world city count | `[0xA52690]` | `Cw` (`turn.md`) |
| unit count by type | `Player +0x15F0` | pointer to an `int16` array indexed by PRTO row: live units of that type owned by the player (`+1` in the unit factory `0x569936..0x569942`, `-1` in the unit destroyer `0x5BC1E0..0x5BC1F5`) |
| PRTO domain | `PRTO +0x9C` | `0` land, `1` sea, `2` air |

### 1.2 Cell facts used here **V**

Tile index `i = (W/2)*y + (x >> 1)`; from an index: `y = i div (W/2)`, `x = 2*(i mod (W/2)) + (y and 1)`
(`W/2` is a signed shift). Cell slots (`world-events.md` 1.2): water `+0x8C` (`terrain >= 11`), continent word
`[cell+0x1E]` (slot `+0xB8`), owner byte `[cell+5]` (slot `+0x98`, `0` unowned), unit-list head
`[cell+0x0C]` (slot `+0xA0`, `-1` empty).

Barbarian-specific cell data:

| Item | Where | Meaning |
|---|---|---|
| **camp** | overlay word (plane 0) bit 7, mask `0x80`; predicate slot `+0x1C(0)` (`0x5EA630`) | the tile hosts a barbarian camp. Set with slot `+0xE0(0, 0x80, -1, -1)`, cleared with `+0xCC(0, 0x80, -1, -1)` |
| **tribe id** | word `[cell+0x18]`; getter slot `+0xB0` (`0x5EAAD0`), setter slot `+0xDC` | index into the barbarian race's name list (RACE row 0, 76 names of 24 bytes); `-1` (`0xFFFF`) none, `75` the default tribe "Barbarian". Saved in the TILE record (`biq-format.md`) |
| city id | word `[cell+0x1A]` (slot `+0xB4`) | `0xFFFF` none; predicate `0x5EA6C0` = id `!= 0xFFFF` |
| start location | feature word (slot `+0xAC`) bit 19; predicate slot `+0x80` (`0x5EA9C0`) | the tile is a civ start position |
| goody hut | overlay bit 5, predicate slot `+0x3C(0)` | |
| tile has units | predicate `0x5EA9F0` (`[cell+0x0C] != -1`) | |

`getCell(i)` is `0x5D16A0(map = 0x9C736C; i)`; `getCell(x, y)` is Map slot `+0x30` (arguments `x, y`).

### 1.3 Spiral offsets and wrap **V**

`0x5E6E50(n; &dx, &dy)` (cdecl, 3 arguments): `n = 0` gives `(0, 0)`; `n = 1 .. 8` the first ring
`(1,-1) (1,0) (1,1) (0,1) (-1,1) (-1,0) (-1,-1) (0,-1)`; ring `r` holds `n` in `(2r-1)^2 .. (2r+1)^2 - 1`
(`NOTES.md` section 8). So `n < 9` is the 3x3 block, `n < 25` the 5x5, `n < 49` the 7x7, `n < 81` the 9x9,
`n < 121` the 11x11, `n < 169` the 13x13, all in **raw** `(x, y)` coordinates: the neighbour is the tile
`(x + dx, y + dy)` looked up with the index formula above (so an offset with the wrong parity lands on the
cell whose `x >> 1` matches).

Wrapping is a single step: `0x426C00(map; x)` returns `x` unchanged when bit 0 of the flags is clear, else
`x + W` for `x < 0`, `x - W` for `x >= W`, else `x`; `0x426C40(map; y)` is the same with bit 1 and `H`.
`0x426BD0(map; x, y)` is the plain bounds test `0 <= x < W and 0 <= y < H`. Every neighbour scan in this
document wraps, then bounds-tests, then skips an out-of-range tile.

### 1.4 The tribe-name table **V**

`used[0 .. 74]` is a 75-byte array at `0xA526C8` (`0x59FD80` clears it with `rep stosd` 18 dwords + word +
byte at `0x5A002A..0x5A003A`). Index `t` is a tribe id; the groups of 15 are selected by the culture group of
the civilization that is nearest to the camp (`RACE +0x90C`, `0..4`): tribe `15*g + k` for `k = 0 .. 14`.
Tribe `75` ("Barbarian") is the fallback and is never tracked (the code writes `used[75]`, one byte past the
cleared region; harmless).

Writers: `0x55F990` (set), `0x55F9F0` (set), `0x565A00` (clear), `0x5B4070` (clear, nuclear blast, same
sequence as `0x565A00`), `0x59FD80` (clear all, game reset). No loader rebuilds the table, so **after a saved
game is loaded every tribe name is free again** (**I**, from the writer census). The shipped name list
(conquests.biq, RACE row 0, `civilopedia_entry RACE_BARBARIANS`):

| group | tribes `15g .. 15g+14` |
|---|---|
| 0 | Chanca, Lupaca, Cherokee, Anasazi, Teoihuacan, Olmec, Zapotec, Chehalis, Chinook, Apache, Illinois, Inuit, Navajo, Carib, Saxon |
| 1 | Vandal, Goth, Angle, Magyar, Khazak, Iberian, Bulgar, Alemanni, Burgundian, Gepid, Hun, Jute, Marcomanni, Seljuk, Phoenician |
| 2 | Estruscan, Illuryian, Thracian, Phrygian, Gaul, Minoan, Mycenian, Cimmerian, Ligurian, Numidian, Patzinal, Sarmatian, Scythian, Suren, Assyrian |
| 3 | Harappan, Mauryan, Parthian, Harappan, Nubian, Sarbadar, Bactrian, Circassian, Cuman, Hurrian, Kassite, Bantu, Khoisan, Libyan, Shangian |
| 4 | Yayoi, Zhou, Ainu, Polynesian, Aryan, Avar, Ghuzz, Hsung-Nu, Kushans, Yue-Chi, Sakae, Uzbek, Tartar, Toltec, Kushite |
| 75 | Barbarian |

(The list is data; a rules file may replace it. The code only needs the 15-per-group convention.)

### 1.5 The nearest-city search `0x56D040` **V**

`0x56D040(x, y, ownerFilter, continent, excludeOwner, visibleTo, excludeCity)` (cdecl, 7 arguments, returns a
city object or `0`). Over every city `c` (pool `[0xA52E6C]`, last index `[0xA52E78]`) it skips `c` when
`ownerFilter >= 0 and c.owner != ownerFilter`; the continent filter (`continent >= 0`): when
`continent < [0x9C74BC]` or `[0x9C74BC] == -1` the city tile's continent word must equal `continent`,
otherwise `0x4AE170(c; continent)` must hold (**I**: ids from `[0x9C74BC]` up are water bodies, and that
routine is true when one of the eight ring-1 neighbours of the city is a water tile of that body);
`excludeOwner >= 0 and c.owner == excludeOwner`; `visibleTo >= 0` and the city tile's seen-by set
`[cell+0x58]` lacks that player's slot bit (in a network game bits `0xC` of `[0xA52680]` are cleared first; the test is skipped when bit `0x8` is then still set, which only happens on a single machine, meaning of the bit **O**, **H** a reveal-the-map switch); or
`c == excludeCity` (a pointer, `0` for none). For the others it forms `dx = 0x441ED0(map; x, c.x)` (wrapped
`|x - c.x|`, wrap when bit 0 and `dx > W/2`: `W - dx`) and `dy = 0x437970(map; y, c.y)` (same for y), then the
key

```
hi = max(dx, dy);  lo = min(dx, dy)
key = hi - ((((dx + dy) / 2) - lo + 1) / 2)          every division truncates toward zero
```

and keeps the city with the **strictly smallest** key (ties: the earlier pool index). It also sets the
globals `[0x9C34EC] = key of the winner` and `[0x9C34E8] = (dx' + dy') / 2` of the winner (the tile distance
with wrap, `dx'`, `dy'` recomputed). With no city both globals keep the value written at entry, `0x7FFFFFFF` (`0x56D04E`, `0x56D053`, **V**), and the
result is `0`. (Spot checks: `(dx,dy) = (2,0) -> 1`, `(1,1) -> 1`, `(2,2) -> 2`, `(3,1) -> 2`, `(4,0) -> 3`,
`(6,0) -> 4`; the key is an ordering key, not the tile distance.)

## 2. The slot-0 branch of `Player::turn` **V**

`Player::turn` (`0x5604B0`, once per player per round, `turn.md` 3) tests `P.+0x1C == 0` at `0x5604D7`; the
barbarian record takes this branch and returns at `0x5611FD` (no economy, no research, no cities).

### 2.1 Gates and flags

```
if S <= 0:                          return                       0x5604E0
N  = popcount([0xA526C0])
Cw = [0xA52690]
if Cw < 2*N - 2:                    return                       0x5604EC..0x560506
many = (Cw > 4*N - 4)               signed compare               0x560517..0x56052D
camps = 0
```

(The gate `Cw < 2N - 2` also suppresses camp creation: a world with fewer than two cities per real civ
produces no barbarian activity at all. `many` selects the stronger land unit and enables sea spawning.)

### 2.2 The tile loop

For `t = 0 .. T-1` (loop index is a dword compared with `T and 0xFFFF`), with `(x, y)` derived from `t` by
1.2:

```
cell = getCell(t)
if not cell.camp:                   continue                     0x560594
camps += 1

-- land spawn
landCount = (int16) sum over PRTO rows k with domain 0 of P.units[k]        0x5605AD..0x5605DD
cap_land  = 2 * (N - 1) * S
if landCount < cap_land and rand(8) == 0:                                    0x560604..0x560612
    type = many ? [0x9C7250] : [0x9C7254]                                    0x560617..0x560625
    u = unitFactory(P; type, x, y, tribe = cell.tribe, -1, 0, 0, -1)         0x560674
    if u: u.experience(+0x44) = 0                                            0x56067D

-- sea spawn
if not many:                        continue                                 0x56068A
seaCount = (int16) sum over PRTO rows k with domain 1 of P.units[k]          0x560690..0x5606C4
cap_sea  = ((N - 1) * S) / 2        truncating toward zero                   0x5606C6..0x5606E6
if seaCount < cap_sea and rand(8) == 0:                                      0x5606E8..0x5606FD
    ok = map.0x5D8520(x, y, P.slot, &out, 0)                                 section 6.1
    if ok and out != 0:
        (dx, dy) = spiral(out)
        X = wrapX(x + dx);  Y = wrapY(y + dy)                                0x56075E..0x56077F
        c2 = getCell(x', y') for (X, Y)  (by 0x437A70, cdecl 2 arguments)
        if continentRecord(c2.continent).tileCount > 20:                     0x5607A1  (record +0x24 of the
            u = unitFactory(P; [0x9C7258], X, Y, cell.tribe, -1, 0, 0, -1)    array at [0x9C7580], stride 40)
            if u: u.experience = 0                                           0x5607F2
```

Both sums are recomputed for every camp, and the unit factory increments the counters immediately, so a spawn
at one camp counts against the cap at the next camp of the same pass.

**Draw order** (gameplay generator `0x60BAB0(0xA526B4; n)`): per camp, in tile order: `rand(8)` if
`landCount < cap_land`; then, if `many`, `rand(8)` if `seaCount < cap_sea`. The draws are made even when the
spawn later fails.

### 2.3 Camp top-up

After the loop (`0x560816`):

```
if camps < N - 1:   0x55F9F0(P)                                          one attempt per round
```

(`N - 1`, not `camps < something per S`.) One call creates at most one camp (section 4).

### 2.4 Worked caps

| `N` | `S` | cities gate `2N-2` | `many` above | land cap `2(N-1)S` | sea cap `(N-1)S/2` | camps wanted |
|--:|--:|--:|--:|--:|--:|--:|
| 4 | 1 | 6 | 12 | 6 | 1 | 3 |
| 5 | 1 | 8 | 16 | 8 | 2 | 4 |
| 5 | 2 | 8 | 16 | 16 | 4 | 4 |
| 9 | 1 | 16 | 32 | 16 | 4 | 8 |
| 9 | 2 | 16 | 32 | 32 | 8 | 8 |
| 9 | 3 | 16 | 32 | 48 | 12 | 8 |
| 3 | 3 | 4 | 8 | 12 | 3 | 2 |

Expected spawn rate with the cap not reached: one unit per camp per eight rounds on average, per kind.

## 3. Unit creation as the barbarians use it **V**

`0x5694D0(this = Player; type, x, y, tribe, unitId, flag6, flag7, extra)` (cdecl-style, 8 arguments, returns
the unit or `0`). Barbarian calls always pass `(type, x, y, tribe, -1, 0, 0, -1)`. Preconditions that make it
return `0`: the unit pool already holds `>= 0x2000` live units (`[0xA52E90] - [0xA52E8C] + 1`, `0x5694FA`);
`type == -1` (`0x569510`); the tile is occupied by a unit of another civ (`0x56D630(x, y, -1, 1)` returns an
owner that is neither `-1` nor the creator, `0x569532..0x569544`, bypassed only when the sixth argument,
`flag6`, is non-zero). Effects relevant here: a fresh unit gets experience level 1 (`0x5BBA40`), the
creator's counter `P.units[type]` is incremented (`0x569942`), and the fourth argument is stored as unit
field `+0x3C`; when the owner is slot 0 and that argument is `-1` it is replaced by `0x4B` (75), the default
tribe (`0x5BB9F5..0x5BBA00`). Barbarian callers then force experience level `0` (`unit +0x44 = 0`), the
Conscript level.

So `unit +0x3C` is the unit's **tribe id** (**I**: read for display; the readers were not examined).

## 4. Founding a camp `0x55F9F0` **V**

`0x55F9F0(this = P)`; returns at once unless `P.slot == 0` and `(T and 0xFFF0) != 0` (at least 16 tiles).

```
for attempt = 0 .. (T >> 4) - 1:                                          0x55FB9E: attempts = T / 16
    r  = rand(T)                                                          0x55FA24
    y  = r div (W/2);  x = 2*(r mod (W/2)) + (y and 1)                    0x55FA2F..0x55FA53
    if y == 0 or y >= H - 1:                      next attempt           (top and bottom rows excluded)
    if not map.0x5F2DF0(x, y):                    next attempt           section 4.1
    ok = true
    for k = 0 .. 24:                                                      the 5x5 raw block, 0x55FA94..0x55FB31
        (dx, dy) = spiral(k);  nx = wrapX(x + dx);  ny = wrapY(y + dy)
        if (nx, ny) in bounds and 0x56D340(nx, ny, -1, 1) > 0:  ok = false           section 4.2
    if not ok:                                    next attempt
    cont = getCell(r).continent
    city = 0x56D040(x, y, -1, cont, -1, -1, 0)                            0x55FB7A  (section 1.5)
    if city == 0:                                 next attempt           the continent needs a city
    -- success
    getCell(r).setOverlay(0, 0x80)                                        0x55FBE4  (slot +0xE0)
    owner = city.owner;  g = RACE[ Player[owner].race ].+0x90C            culture group, 0..4
    s = rand(15)                                                           0x55FBF8  (always drawn)
    tribe = 75
    for j = 0 .. 14:
        v = (s + j) mod 15
        if used[15*g + v] == 0:  tribe = 15*g + v;  break                  0x55FC49..0x55FC6D
    getCell(r).tribe = tribe                                              0x55FC99  (slot +0xDC)
    used[tribe] = 1                                                       0x55FCB4
    repeat 2:  u = unitFactory(P; [0x9C7254], x, y, tribe, -1, 0, 0, -1);  u.experience = 0     0x55FCC3..0x55FCDF
    return                                                                one camp per call
return                                                                    no camp this round
```

`(x, y)` passed to `getCell(r)` is the tile index `r` itself. `Player[owner].race` is the player record
`+0x20`; the RACE array is `[0x9C71D0]` with a row stride of `0x974` bytes (`605 * 4`), culture group at row
offset `+0x90C`.

Draw order per call: `rand(T)` once per attempt; `rand(15)` once after the first successful attempt
(and never after a failed one).

### 4.1 Site predicate `0x5F2DF0(map; x, y)` **V**

False when any of these holds, in this order: the tile is water (slot `+0x8C`); it has a goody hut (`+0x3C(0)`);
it is a start location (`+0x80`); it has a city (`0x5EA6C0`); it hosts a plain colony (`0x5EA6E0`); it has any
unit (`0x5EA9F0`); its continent record (Map slot `+0x84(continent)`) has fewer than `0x4B` = 75 tiles (record
`+0x24`). Otherwise let `m = (W + H) / 2` (positive, plain truncation) and

```
A = min(m / 100, 4)             N1 = (2*A + 3)^2          inner square, owner test
B = min(m / 25,  4)             N2 = (2*B + 5)^2          outer square, camp test
```

then for `k = 0 .. N2 - 1` (stopping at the first failure): `(nx, ny) = wrap(x + dx_k, y + dy_k)`; skip the
offset when it is out of bounds; if `k < N1` and the tile's owner byte is non-zero the site is rejected; if
the tile has a camp the site is rejected. The test includes `k = 0`, the tile itself.

| `m = (W+H)/2` | `N1` (owned tiles forbidden within) | `N2` (other camps forbidden within) |
|--:|--:|--:|
| 0 .. 24 | 9 (3x3) | 25 (5x5) |
| 25 .. 49 | 9 | 49 (7x7) |
| 50 .. 74 | 9 | 81 (9x9) |
| 75 .. 99 | 9 | 121 (11x11) |
| 100 .. 199 | 25 (5x5) | 169 (13x13) |
| 200 .. 299 | 49 (7x7) | 169 |
| 300 .. 399 | 81 (9x9) | 169 |
| >= 400 | 121 (11x11) | 169 |

(`m / 25` and `m / 100` are the compiler's multiply-and-shift sequences with magic number `0x51EB851F`,
`0x5F2EF1..0x5F2F3A`; exact for `m >= 0`.)

### 4.2 Unit test `0x56D340(x, y, filter, flag)` **V**

Returns `-1` when `(x, y)` is out of bounds or the tile has no military unit, otherwise the owner (`unit +0x34`)
of the first unit in the tile's list for which `0x5BE6E0` (attack) or `0x5BE820` (defense) is positive and
whose `+0x34` is non-zero, and `0` when only barbarian military units are there. With `filter == -1` (as in
the camp test) the `flag` and ability branches are inert, so `> 0` means **a non-barbarian military unit is
within the 5x5 block**. (With a real `filter`, units with the Hidden Nationality ability, ability 17, whose
owner differs from `filter` and from `0` are skipped, and with `flag` set a unit must be visible to `filter`
by `0x5BB650(unit; filter, 1)`.) `0x56D630(x, y, filter, flag)` is the same walk over **all** units, not only
military ones.

## 5. The uprising `0x55FD00` **V**

`0x55FD00(this = P)` returns at once unless `P.slot == 0` and `S > 0`. It is called from the era-change code
`0x55E40F`: after a civilization's era changes, if `[0xA526AC] > 0` and exactly two slots `1 .. 31` that are
in play have the same era (`Player +0xF4`) as the one that just changed (`research.md` 7.2), the call is made
with the barbarian record.

```
E = number of cells with a camp                                           0x55FD28..0x55FD57
repeat  max(0, N - E - 1)  times:  0x55F9F0(P)                            0x55FD59..0x55FD8B  (E is not refreshed)
camps = 0
for t = 0 .. T-1:                                                          0x55FD97..0x55FFDF
    if not getCell(t).camp:  continue
    camps += 1
    repeat 8*S times:                                                      0x55FE10..0x55FE9C
        u = unitFactory(P; [0x9C7250], x, y, tribe, -1, 0, 0, -1);  u.experience = 0
    ok = map.0x5D8520(x, y, P.slot, &out, 0)                              section 6.1
    if ok and out != 0:
        (X, Y) = wrap(x, y) + spiral(out)
        if continentRecord(getCell(X, Y).continent).tileCount > 20:       0x55FF72
            u = unitFactory(P; [0x9C7258], X, Y, tribe, ...);  u.experience = 0
if camps > 0 and [0xA360A7] == 0:                                          UI only
    city = 0x501760(Player[local human]; -1, 0)                           the human's city nearest to a camp
    if city:  message SUMMARY_BARBARIAN_EXPLOSION_CITY (city name)
```

So the uprising founds up to `N - 1 - E` new camps (each attempt may fail), then **every** camp, old and new,
receives `8*S` advanced barbarian units on its own tile and one sea unit on the best adjacent water tile.
The second pass draws no random numbers (the first pass draws as in section 4). A civ that is the only one in
its era, or the third or later, does not trigger it.

`0x501760(P; continent, &tribe)` returns the city of `P` for which `0x56D040(campX, campY, P.slot, ...)` gives
the smallest key among cities whose key is `< 10` (`[0x9C34EC]`), over all camps (restricted to one continent
when `continent >= 0`); `*tribe` receives that camp's tribe id when the pointer is non-null.

## 6. Sea landing and other helpers

### 6.1 `0x5D8520(map; x, y, slotFilter, &best, &bestContinent)` **V**

`ret 0x14`. Returns `false` at once when `(x, y)` is water. Otherwise initialises `*best = 0` (when the
pointer is non-null) and `*bestContinent = -1`, `found = false`, and for `n = 1 .. 8` (the first spiral ring):

```
(nx, ny) = wrap(x + dx_n, y + dy_n);  skip when out of bounds or not water
if slotFilter != -1:
    v = 0x56D630(nx, ny, -1, 1)                       owner of the first unit on that water tile
    if v != -1 and v != slotFilter:   found = true;  continue        occupied by another civ: counted, not used
if best != null:
    if *best == 0:                                   *best = n
    else if size(continent of (nx,ny)) > size(continent of the tile at offset *best):   *best = n
if bestContinent != null: same rule on continent ids (size = continent record +0x24)
found = true
return found
```

`size` is the continent record tile count (`[0x9C7580] + 40 * id + 0x24`); a strictly larger water body wins,
the first ring tile wins ties. The barbarians pass `slotFilter = 0` and `bestContinent = null`; they accept the
result when `found` and `*best != 0`. A water tile with a foreign unit on it never becomes the landing tile.

### 6.2 Other routines

`0x5DF900(mask)` popcount; `0x5D16A0` cell by index; `0x437A70(x, y)` cell by coordinates (cdecl);
`0x426C80(0xA52DD4; head)` unit-list successor; `0x5BC8B0(unit; ability)` ability test.

## 7. Game-start camps `0x5D21E0` **V**

The "Setting up tile objects..." pass (debug text) runs once when a game or scenario is set up. For every
cell with a camp it sets the tribe to `75` if it is `0xFFFF`, then calls `0x55F990(Player[0]; tribe, x, y)`:
`used[tribe] = 1`, then two `[0x9C7254]` (basic barbarian) units are created at `(x, y)` with that tribe and
experience level `0`. A debug line `\tPlaced barbarian camp %d` is printed. (The same pass places victory-point
locations; `victory.md`.) It does **not** test `S`: a scenario with camps and `S <= 0` still starts with 2
warriors at each camp, but never spawns more. Loading a saved game does not run this pass (the units are in
the file).

## 8. Destroying a camp `0x565A00(this = Player; x, y)` **V**

`ret 8`. The player is the one that takes the camp. Steps:

1. **Gold.** `T = P.+0x44 + P.+0x48` (treasury pair, `economy.md` 3.2); `T' = T + 25` (`lea esi,[eax+ecx+0x19]`).
   The treasury is stored as a new pair with total `T'`; if `T' <= 0` the total is `0`. (Pair split:
   `a = timeGetTime() mod R - 0x3039; b = R - a` with `R = T'` for `T' > 0`, else `R = 0xD431` and the total
   comes out `0`.) **The reward is a flat 25 gold, independent of difficulty, era and the camp.**
2. If `P` is the local human (`P.slot == [0x9FD4BC]`): the text `CAPTURE_BARBARIAN` is posted at `(x, y)` with the
   tribe's name as parameter (name `tribe` of RACE row 0, 24-byte entries, `0x61C5A0`), and the UI sound hook
   `0x537700(6)` runs.
3. The camp bit is cleared (slot `+0xCC(0, 0x80, -1, -1)`).
4. `used[tribe] = 0`, then the tribe id is set to `-1` (slot `+0xDC(-1)`).
5. UI: `0x4E69F0` redraws the tile; if it is on screen and in the local human's seen set the dirty flag
   `[0xA281C4] = 1` is raised.

Triggers (all **V** unless noted):

| Trigger | Where | Condition |
|---|---|---|
| A unit arrives on the tile | `Unit::setPosition` `0x5BD220`, `0x5BDD6F..0x5BDDCE` | the destination cell has a camp and the unit's owner (`unit +0x34`) is not `0`; the player record of that owner collects. Then, if that player's difficulty index `Player +0x30` is `<= 4`, the achievement record bit `0x4000000` is set with `0x57CCD0(0xB71288 + 0xB0*owner; 0x4000000)` (`research.md` 7.4; pure bookkeeping), and for the local human's unit with byte `+0x38D` set the unit's activity record (`+0x280`) is reset by `0x403CC0(9)` (UI/automation, **O**). Any kind of unit counts, military or not, and `setPosition` is also the placement routine for newly created units. |
| The tile's owner changes to a civ | `0x5D3AB0(map; x, y, newOwner)` at `0x5D3DED` | `newOwner > 0` and the tile has a camp: `0x565A00(Player[newOwner]; x, y)`. The same routine destroys a plain colony on the tile (`0x5DAA90`) and pops a goody hut (`0x55C6B0(Player[newOwner]; x, y, 0)`, which calls the hut resolver `0x55B8B0`); the arguments `(x, y, newOwner)` are **I** from the register use. `0x5D3AB0` is called only from `0x5D4830` (three sites: `0x5D4ADD`, `0x5D5194`, `0x5D5743`), which in turn is called from `0x4AE2A0`, `0x4AECC0` (the city destroyer), `0x4B0C60` and `0x5D25F0`; both routines, the order of the side effects and the `newOwner > 0` gate are specified in `borders-culture.md` 5 and 6. |
| A nuclear blast | `0x5B4070` at `0x5B4482..0x5B4505` | every camp inside the blast: bit cleared, `used[tribe] = 0`, tribe `-1`, **no gold**; the same blast also removes outposts (`0x5D6430`). |

The same arrival code in `0x5BD220` also handles the other colony kinds (**V**, `0x5BDB90..0x5BDD6F`): an enemy
unit stepping on an **airfield** of another owner triggers `0x5631B0(owner; colonyOwner, 2)`, and the airfield
is captured (`0x5DB0D0(airfield; newOwner)`) when the entering player knows the technology at
`[0x9C7324] + 0x45C`, otherwise destroyed (`0x5DAEC0(airfield; 1)`); on a **radar tower** or **outpost** of
another owner it triggers `0x5631B0(owner; colonyOwner, 2)` and destroys them (`0x5D6360`, `0x5D6430`). A **plain
Colony** of another owner is destroyed on entry as well (`0x5BDB41`, with the `COLONY_BARBS` / `COLONY_CIV` popup for the
local human). All of these are specified in `colonies.md` section 7.

## 9. Interactions specified elsewhere

* Combat: a barbarian attacker or defender adds the other side's difficulty bonus
  `DIFF[difficulty].+0x64` (plus 100 with the barbarian-bonus wonder) to its opponent's side only; barbarian
  units never retreat; a barbarian winner of a fight that would make a Leader is doubled, etc.
  (`combat.md` 4.5, 11).
* A barbarian "capture" of a city is a raid (plunder, no change of owner): `combat.md` 14.2, `capture.md`.
* Barbarians own no cities and have no economy; their `Player::turn` is section 2 only.
* Goody huts can spawn barbarians (outcome 7 of the hut resolver, `goody-huts.md` 5.8): up to three
  basic barbarian units on free land tiles of the ring around the hut, with the tribe chosen by the same
  `15*group + (rand(15) + j) mod 15` scan of the tribe table `0xA526C8`. The other seven scans of the table
  in `0x55B8B0` only pick the tribe name shown in the popup (`goody-huts.md` 6.3). The scan never marks the
  entry as used.

## 10. Not decoded

1. **The behaviour of barbarian units.** The per-unit turn `0x5C7700` (`unit-turn.md`) only restores movement,
   heals, and applies the sea hazard and the jungle disease; it has no AI. Barbarian units are given their
   orders by the AI planners that loop A of the round runs for slot 0 as for every civ (`0x446840`,
   `0x445EA0`, `0x449B20`; `turn.md` 2.1 steps 4 and 7). Which orders the planners give them (attack the nearest
   city, pillage, return to camp, board the galley, land) is not decoded. `BARBARIAN_ATTACK` (`0x5B628A`) and the
   raid flow (`combat.md` 14.2) are the only barbarian-specific consumers found outside this document.
2. (Settled: both globals are `0x7FFFFFFF` when no city matches, section 1.5.) The water-body role of
   `[0x9C74BC]` stays **I**.
3. The readers of `unit +0x3C` (tribe id).
4. Whether any code path other than the four listed sets `S` (`0x596EF0` sets all world options to `1`; the
   resolved value comes from the setup screens `0x585BC0`, `0x48DDE0`, `0x494630`, `0x54EA40`, not re-read).
5. The meaning of bit `0x8` of `[0xA52680]` (it disables the seen-by-player filter of `0x56D040` outside network games).

## 11. Corrections to other files

| File | Statement | Correction |
|---|---|---|
| `NOTES.md` 11.10 | "`placeBarbarianCamps` (`0x5F2090`) ... a **barbarian camp**" | The routine writes feature id 2 / `0x10000` (the *bonus grassland* bit, `world-events.md` 7). Camps are overlay bit 7 and are created only by `0x55F9F0` (during play) and by scenario data. |
| `capture.md` row 4 of the capture table | "`+0x15EC[cont]` and `+0x15F0[cont]` ... two per-continent city-id arrays" | The code at `0x564A9A` runs with `eax = player + 0x1608`, so the arrays are the heap pointers at `Player +0x1608` and `+0x160C`; `Player +0x15F0` is the per-PRTO live-unit counter (section 1.1). Fixed in `capture.md`. |
| `research.md` 7.2 | "`0x55FD00(Player[0])` barbarian landing" | Correct, and the routine is the full uprising of section 5. |
| `biq-format.md` owner type 1 | "76 names, `75` is the constructor default" | Confirmed: 5 groups of 15 plus the default (section 1.4). |

## 12. Verification status

Verified by reading disassembly in this pass: every address cited with **V**. Not run: no dynamic trace of a
barbarian spawn was made; the first checks to make are (a) the draw order of section 2.2 against a trace of
`0x60BAB0` calls during a round with one camp and `S = 2`, (b) the camp-site tables of 4.1 on a 80x80 map, and
(c) the 25-gold reward by capturing a camp with a scout and reading `Player +0x44/+0x48`.
