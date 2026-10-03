# City disease (terrain disease)

Clean-room specification of the city-level **disease** of `Civ3Conquests.exe` (PE32, MSVC 6, image base `0x400000`):
the per-turn step `City::diseaseStep` `0x4B45A0`, its terrain inputs (*Causes Disease*, *Cured by Sanitation*, *Disease
Strength*), the infection and cure rolls and their draw order, the citizen loss, and the notifications. Not covered here
(different mechanisms with their own documents): the scenario **plague** (`world-events.md` 6, city fields `+0x3AC..+0x3B4`)
and the **jungle disease of units** (`unit-turn.md` 3.3). Tags: **V** read from the instructions with the body opened,
**E** executed in an emulator over the unmodified exe (scratch harness, not committed), **H** hypothesis, **O** open.
No reference code is given on purpose.

Conventions (`primitives.md`): `__thiscall` (`ecx` = this), `ret N`, the last pushed argument is the first parameter;
`rand(n)` is `0x60BAB0(0xA526B4; n)` = `(k * (n & 0xFFFF)) >> 15` with a 15-bit draw `k`; it always consumes exactly one
draw (`primitives.md` 1.6). Player at `0xA52E98 + 0x20E4 * slot`. Map object `0x9C736C`.

## 1. Data

| datum | where | meaning |
|---|---|---|
| `City +0x20` | id | compared with each tile's worked-by word |
| `City +0x24`, `+0x26` | x, y (words) | |
| `City +0x28` | owner slot (byte) | |
| `City +0x138` | size (dword) | read again after every change |
| **`City +0x68`** | **diseased flag** (byte, 0 or 1) | zeroed at founding (`city-founding.md` 3.5); read and written only by this routine, never by any yield, growth or happiness code (a scan of every `byte [reg + 0x68]` access found no other city reader) **V** |
| **`City +0xD8`** | **cause code** (dword) | 0 at founding, 3 when the city is infected, 0 when cured; read only by the notification builder `0x4B40D0` (section 6) |
| `cell +0x6C` | worked-by city id (int16, `0xFFFF` none) | `yields.md` 5; the city's own centre tile carries the city id |
| cell vtable `+0xC8` | terrain id | `13` = Ocean, `11` = Coast, `12` = Sea in the shipped rules |
| `TERR` table `[0x9C7328]`, stride `0xF0` | `+0xE8` flag word; `+0xEC` disease strength (percent, dword) | terrain count `[0x9C3DC4]` (14 in the shipped file) |
| `TERR +0xE8` bit 2 (`0x5E8D10`) | **Causes Disease** | |
| `TERR +0xE8` bit 3 (`0x5E8D70`) | **Cured by Sanitation** | the other bits of the word are uninitialised memory in the shipped file (`0xCCCCCCCx`): test single bits only |
| `TERR +0xEC` (`0x5E8D50`) | **Disease Strength** | default 50 |
| `Player +0x40` bit `0x10` | notification style flag (summary line instead of a dialog) | **H** for the name; used only to pick between the two message forms of section 5 |
| `[0x9FD4BC]` | slot of the local (human) player | messages are posted only when the city owner is this slot |
| `0x47B530()` | multiplayer predicate | true in a multiplayer game |

Shipped `conquests.biq` (rows by id): *Causes Disease* is set on Flood Plain (4), Jungle (8) and Marsh (9) only; *Cured by
Sanitation* only on Flood Plain; every row has strength 50. (The editor stores bit 1 on Mountains, Marsh and Volcano; nothing reads it.)

## 2. Where it runs

Once per city turn in the sequencer `0x4BE970`, step 6 (`city-turn.md` 2): the calls are, in order, `0x4B2E10`
(resistance quelling), `0x4B2F80` (pollution event), **`0x4B45A0` (this routine, call at `0x4BEB00`)**, `0x4B4970` (meltdown),
`0x4BDFF0` (disorder turnover), `0x4BE440` (celebration test). It is unconditional: no game-rule flag, difficulty level, government
or building is consulted anywhere in the routine (an Aqueduct, Harbour or Hospital has **no** effect on it). The step runs
for every civ including the AI, and the barbarian civ has no cities.

## 3. The algorithm `0x4B45A0` (plain `ret`) **V**, whole routine **E**

Two branches on the flag `+0x68`.

### 3.1 City already diseased

```
if size > 1:
    0x4BA230(city; 1, -1, 0)                               # remove one citizen of any race (hurry.md 8); consumes its own draws
    if owner == local:                                      # [0x9FD4BC]
        if Player[local].+0x40 & 0x10:  post summary SUMMARY_DISEASE at (x, y) with the city name
        else:                           0x4B40D0(city; 0, -1)       # dialog, section 6
size' = city.size                                          # read AFTER the removal
if rand(128) <= size':   return                            # stays diseased (flag and cause code unchanged)
city.+0x68 = 0;  city.+0xD8 = 0                            # cured
```

Facts: a diseased city of size 2 or more loses **one citizen every turn** it stays diseased, including the turn on which it is
cured. A city of size 1 is never reduced, but it can still stay diseased. `rand(128)` is uniform on `0..127` (`128 | 32768`),
so the probability of staying diseased is `(size' + 1) / 128` (capped at 1 for `size' >= 127`) and the cure probability
`(127 - size') / 128`; larger cities are cured **less** often.

### 3.2 City not diseased

Step A: count the disease-causing tiles in the city radius.

```
counts[0 .. 13] = 0;  total = 0                            # 14 dwords on the stack
for k = 0 .. 20:                                           # the 21 tiles of the city radius, ring order of borders-culture.md 4 (k = 0 is the centre)
    (dx, dy) = off(k)                                      # 0x5E6E50
    x = foldX(city.x + dx);  y = foldY(city.y + dy)       # 0x426C00 / 0x426C40 (borders-culture.md, "neighbour step")
    skip unless 0 <= x < Wx and 0 <= y < Wy                # Wx = [0x9C74D4], Wy = [0x9C74C0]
    cell = map.cellAt(((Wx >> 1) * y + (x >> 1)) & 0xFFFF)   # 0x5D16A0 (dummy cell 0xCAA330 when out of range)
    skip unless cell.word[+0x6C] == city.id                # the tile is WORKED by this city (the centre counts)
    t = cell.vt+0xC8()                                     # terrain id
    skip unless t >= 0 and t <= [0x9C3DC4]
    skip unless TERR[t].+0xE8 bit 2                        # Causes Disease
    counts[t] += 1;  total += 1
```

Only tiles actually worked by the city are counted (a tile inside the radius that is unworked, or worked by another city,
contributes nothing), including water tiles whose terrain has the flag. The test is `t <= count` (not `<`): a terrain id equal
to the terrain count would index one row past the table (unreachable with a well-formed map). A ruleset with more than 14
terrain rows would write past the 14-dword `counts` buffer (**H**: undefined behaviour, not reachable in the shipped data).

Step B: the sanitation cure.

```
if Player[owner].knowsTech(8):                              # 0x561440 with the literal tech id 8, see below
    for i = 0 .. min([0x9C3DC4], 14) - 1:
        if TERR[i].+0xE8 bit 3:   total -= counts[i];  counts[i] = 0
if total <= 0:  return                                      # no draw at all
```

The technology is the **literal row id 8** (`push 8` at `0x4B47A4`), not a lookup by name. In the shipped `conquests.biq` row 8
is *Writing* (rows 0 to 8: Bronze Working, Masonry, Alphabet, Pottery, The Wheel, Warrior Code, Ceremonial Burial, Iron Working,
Writing; *Sanitation* is row 50, *Medicine* row 45). The editor's label says "Cured by Sanitation", so the intent was probably another
technology (**H**), but the executed behaviour is: **knowing the technology in row 8 removes the Flood Plain tiles from the count**.
The plague uses the same literal (`world-events.md` 6.3). A ruleset that reorders the technologies changes which technology cures.

Step C: one infection roll per remaining tile.

```
for i = 0 .. 13 (ascending terrain id):
    if counts[i] > 0:
        s = TERR[i].+0xEC
        if s > 0:
            bound = 0x300 - trunc(s * 512 / 100)           # s * 512 is formed first, then divided by 100 (multiply-high by 0x51EB851F, arithmetic shift 5, sign fix): truncation toward zero
            repeat counts[i] times:
                r = rand(bound) & 0xFFFF                   # bound is passed as a 32-bit value; rand masks it to 16 bits
                if r <= city.size:   INFECT, stop all loops
        else:
            counts[i] = 0                                   # no draw for a terrain with strength <= 0
```

The draws are made one tile at a time, in ascending terrain id, and the loop **ends at the first success**; a terrain with
`n` counted tiles can consume up to `n` draws. The comparison uses the city's **current** size (`<=`, so a size-8 city
infects on `r = 0..8`).

INFECT:

```
if city.size <= 1:  return                                  # a size-1 city is never infected (no flag change, no message)
city.+0x68 = 1;  city.+0xD8 = 3
if owner == local:
    if (Player[local].+0x40 & 0x10) and not 0x47B530():
        post summary SUMMARY_DISEASE at (x, y) with the city name
        0x4BA230(city; 1, -1, 0);  return
    0x4B40D0(city; 0, i)                                    # dialog / network event, section 6; i = the terrain id whose roll succeeded
0x4BA230(city; 1, -1, 0)                                    # remove one citizen
```

So the infection itself also costs one citizen (and, as in 3.1, another one on each later turn). For a non-local owner no message
is posted. The removal call happens **after** the message in every path, and nothing else (food box, production, happiness) changes.

### 3.3 Probabilities (closed form)

* Per worked disease tile of strength `s` and city size `z`: `bound = 768 - trunc(5.12 s)`; `P(infect) = min(1, (z + 1) / bound)` for
  `0 < bound <= 32768` and `bound` dividing 32768 (the draw is uniform then); in general `P = #{ k in 0..32767 : (k * bound) >> 15 <= z } / 32768`.
* `s = 50 -> bound 512`; `s = 100 -> 256`; `s = 25 -> 640`; `s = 10 -> 717`; `s = 150 -> 0`; `s > 150 -> bound < 0`.
* **Edge cases.** `s = 150` gives `bound = 0` and `rand(0) = 0`, so the first counted tile infects with certainty. For `s > 150` the
  bound is negative and `rand` masks it to 16 bits (for example `s = 151` gives `65531`), so the roll is almost uniform on
  `0..65530` and an infection is nearly impossible. `s < 0` or `s = 0` draws nothing.
* A city with `m` worked Flood Plain, Jungle or Marsh tiles at strength 50 and size `z`: `P(infected this turn) = 1 - (1 - (z + 1) / 512)^m`
  (if no cure applies). Larger cities are **more** likely to be infected and less likely to be cured.

## 4. Draw order (per call)

* Diseased city: the draws of `0x4BA230` (if `size > 1`), then one `rand(128)`.
* Healthy city: no draw for steps A and B; then one `rand(bound_i)` per remaining counted tile in terrain order until the first
  success; on success the draws of `0x4BA230`. Cities with no worked disease tiles consume nothing, so a map without
  those terrains never perturbs the RNG stream.

## 5. Notification variants (no state change)

* Summary line: text-slot 0 is set to the city name (`City +0x1E0`), the popup/log key is `SUMMARY_DISEASE`, position `(x, y)` (`0x4ED220`).
* Dialog `0x4B40D0(city; a, terrain)` (`ret 8`, local owner only): in a **multiplayer** game it does not show a dialog but
  records a pending network event (`[0x74CFC4] = 4`, `[0x74CFC8] = city id`, `[0x74CFCC..D4] = -1`). Otherwise it builds the text
  `VERBOSE_DISEASE_TERRAIN` (text slots: 0 city name, 1 and 2 civ/ruler names, 3 the name of the terrain) or `VERBOSE_DISEASE_UNKNOWN` and
  opens a modal dialog. `City +0xD8` selects the variant through a 4-entry jump table (0 -> UNKNOWN, 1 -> terrain 8 (Jungle), 2 -> the
  argument terrain or 4 (Flood Plain) when the argument is -1, 3 -> the argument terrain, or a generic name from `[0xCADC0C] + 0x5B0` when
  the argument is -1). Because the step always stores 3, the text is the terrain name of the roll that infected the city; the cases 1 and 2
  are dead in the shipped game. The dialog body is presentation (**O** beyond the above).

## 6. Golden vectors

| id | input | result |
|---|---|---|
| D1 | diseased, size 5, `rand(128)` returns 0, 4, 5, 127 | one citizen is removed first (size 4), then: stays, stays, cured, cured |
| D2 | diseased, size 1, `rand(128)` returns 1 / 2 | no removal; stays (`1 <= 1`) / cured |
| D3 | healthy, one worked Jungle tile (strength 50), size 8: `rand(512)` returns 8 / 9 | infected (size 7, flag 1, cause 3) / not infected |
| D4 | as D3, size 1 | the roll may succeed but nothing happens (`size <= 1`) |
| D5 | healthy, two worked Jungle and one worked Marsh, strength 50, size 8 | up to three draws of `rand(512)` in order: Jungle, Jungle, Marsh; the first with `r <= 8` ends the step |
| D6 | one worked Flood Plain tile, the owner knows tech row 8 | `total = 0`, no draw |
| D7 | one worked Flood Plain and one worked Jungle, tech 8 known | only the Jungle tile is rolled |
| D8 | an unworked Jungle tile in the radius | not counted, no draw |
| D9 | Jungle strength 150, size 3, healthy | the argument of the draw is `0` (`bound = 768 - 768`); `rand(0) = 0` (`primitives.md` 1.6), `0 <= 3`: infected |
| D10 | Jungle strength 151 | the argument is `-5` (`768 - 773`), masked by `rand` to `65531`; infection only for `r <= size` |
| D11 | strength 50, size 8, 5 worked Jungle tiles | `1 - (503/512)^5 = 8.5 %` |

**E** (D1 to D10 were executed in the harness below; the results of `rand` are supplied by the harness, D11 and D9's `rand(0)` come from
`primitives.md` 1.6): the real `0x4B45A0` was run in the emulator with `0x60BAB0` (scripted queue that logs
the argument of each call), `0x4BA230` (decrement the size), `0x4B40D0`, `0x4ED220`, `0x61C5A0` and `0x47B530` stubbed, the real
`0x5E6E50`, `0x426C00`, `0x426C40`, `0x5D16A0`, `0x5E8D10`, `0x5E8D50`, `0x5E8D70` and `0x561440` executed, and a fake cell vtable
(`+0xC8` returns a stored terrain id). 1500 random scenarios (owner, local player, notification flag, multiplayer, tech 8, 14
terrain rows with random flags and strengths 0 to 100, size 1 to 12, diseased or not, wrap-x/wrap-y at map edges, worked and unworked
tiles) matched a model written from sections 3.1 and 3.2 exactly on the sequence of `rand` arguments, the sequence of
called notifications and citizen removals, the final size, flag and cause code.

## 7. Open items

1. **O** Meaning of `Player +0x40` bit `0x10` (the routine uses it only to choose between the summary line and the dialog; the
   same bit appears in the plague messages).
2. **O** The body of the dialog `0x4B40D0` beyond the variant selection; the multiplayer event with kind 4.
3. **O** Whether and where the save file stores `City +0x68` and `+0xD8` (`savegame.md` has the city record grammar; the two fields were not located in it).
4. **H** The intended technology of the cure (the code tests row 8; the editor text says Sanitation).
