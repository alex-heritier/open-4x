# City capture and transfer

Owns: what happens when a city changes hands: the capture routine `Player::capture`
(`0x563410`), the gold plunder, the population loss, the keep-or-raze choice, the culture
conversion, the transfer itself (`Player::takeCity`, `0x564800`) and razing (`0x563370`).
The barbarian raid, a branch of the same routine that never transfers the city, is in
`combat.md` section 14.2. Reference: `rust/src/capture.rs`. Image base `0x400000`, static VA =
runtime VA. Static analysis only (radare2 disassembly, byte scans, the shipped `conquests.biq`
and `script.txt`). Verified unless marked **HYPOTHESIS**.

## At a glance

* One routine serves every ownership change. Its two boolean arguments pick the branch:
  **silent** `(0, 0)`, **military capture** `(1, 0)`, **culture conversion** `(x, 1)`.
* A capture takes gold first: the victim's last city yields the whole treasury, any other city
  `treasury / cities` scaled by size (town 1/2, city 3/4, metropolis 1) (section 3).
* The city then loses one citizen, or is destroyed outright (size 1, one citizen of the old
  owner's race, `+0x5C == 1`), unless the capturer already has a citizen of its race there or a
  stake in it (section 4).
* A local human is asked "Install a new governor" or "Raze the city!"; an AI player decides in
  a Player virtual method (`0x443B60`, section 12). It never razes the old owner's last city,
  a city alone on its landmass, or one with a luxury or strategic resource its capital lacks;
  it keeps a city with a wonder it can use or built, or with half its citizens of its own race;
  it razes a city with wonders it cannot use; where the old owner has more culture and at least
  half the citizens it razes with a chance that grows with the old owner's lead in citizens
  (two dice of 0 to 14); otherwise it razes only once it has twice its optimal number of cities.
  Razing releases `size / 2` Workers.
* A culture flip asks the converting player first; an AI refuses only when it is over twice its
  optimal number of cities and has no war, wonder or citizen reason to want the city
  (`0x443A60`, section 12).
* The transfer destroys the Palace (if it was the capital), every ordinary building with culture
  above 0, every small wonder, and on a capture into a city with no citizen of the capturer's
  race one quarter of the remaining ordinary buildings (section 8). Great wonders and the two
  size gates (`Aqueduct`, `Hospital`) always survive.
* The victim's attitude record about the capturer gains 16 incident points per capture
  (`combat.md` 14.4).

## 1. Callers and the two flags

`Player::capture(city, unit, capture, convert)`, `ret 0x10`, `this` = the player taking the
city. The four stack arguments are pushed `convert` first.

| call site | `unit` | `capture` | `convert` | what it is |
|---|---|---|---|---|
| `0x469B3F` | yes | 1 | 0 | a unit enters a city (UI move) |
| `0x476C76` | yes | 1 | 0 | a unit enters a city |
| `0x5B9D81` | yes | 1 | 0 | a unit enters a city |
| `0x5C4D47` | yes | 1 | 0 | a unit enters a city |
| `0x4B2DC0` | no | 1 | 1 | culture flip during turn processing (city code) |
| `0x5281D3` | no | 1 | 1 | culture flip |
| `0x5034B9` | no | 0 | 0 | silent transfer (the only one) |

The return value is 1 only for the barbarian raid (`0x5638DA`); every other exit is 0 (a
refusal, a pending multiplayer prompt, a raze, a finished transfer).

## 2. Control flow of `0x563410`

```
B = city, O = B.+0x28 (old owner), P = this (new owner)
if P.+0x1C == 0:                       raid (combat.md 14.2), return 1
[multiplayer 0x47B530] mark the city dirty for clients, 0x46F7E0(0x74AF60, B.id)
if convert:                            conversion (section 6)          -> 0x5647D8
elif not capture:                      goto transfer                    -> 0x5647E3
else:                                  capture (sections 3 to 5)        -> 0x563E9A
transfer:                              P.takeCity(B, capture, convert)  (0x564800, section 7)
```

`0x563916` tests `convert`; `0x563E8E` tests `capture`. The capture branch ends in the transfer
(keep) or in `0x56467D` (raze, section 9); a destroyed city (section 4) goes to `0x564685`.

## 3. The plunder (verified)

Order inside the capture branch (`0x563E9A` on):

1. **Incident.** If `P.civ != 0`, `O.civ != 0` and `O != P`: the victim's pair record about the
   capturer gains 16 in `+0x20` (a direct add) and 16 in `+0x24` (the setter `0x47A850`, which
   stores `record[a].field[b - 5] = value`, called with `(capturer, 14, old + 16)`).
2. **Score counters.** If `[0xA5267C] & 0x26000` (VP scoring, Capture the Unit or Reverse Capture the
   Flag): `P +0x11CC` (total victory points) and `P +0x15C4` (victory points from captured cities) each
   gain `size * [0xA529B8]` (the "city conquest" multiplier of the scenario, stock 100). Identified in
   `victory.md` sections 1.4 and 5.1.
3. **Loot** (`0x563F54..0x564012`). `T` = victim treasury (`Player +0x44` plus `+0x48`), `n` =
   victim city count (`+0x194`), `s` = the city's size:

   | case | `base` |
   |---|---|
   | `n == 1` | `T` |
   | `n > 1`, `s <= [0x9C72E4]` (6, a town) | `(T / n) / 2` |
   | `n > 1`, `[0x9C72E4] < s <= [0x9C72E8]` (12, a city) | `3 * (T / n) / 4` |
   | `n > 1`, `s > [0x9C72E8]` (a metropolis) | `T / n` |

   All divisions are signed and truncate toward zero. If the capturing unit exists and its
   prototype's special-action word `+0xAC & 0x10080000` is non-zero, `gain = 2 * base`, else
   `gain = base`. The bonus test (`0x563FE7`) lies after the `n > 1` split and is skipped by
   the `n == 1` jump (`0x563F7A`), so a last city never pays a bonus. The shipped rules set
   neither bit on any of the 141 units (the word's union is `0x2503FF`), so the bonus is
   dormant. The same mask gates the collateral hook of `combat.md` 8.5.
4. **Treasuries** (`0x564012..0x564095`). Capturer: new total `T_c + gain`; victim: new total
   `max(0, T - base)`. Both are written with the treasury writer's split (`economy.md`,
   `economy::treasury_cells`): a total above 0 stores `timeGetTime() % total - 0x3039` and the
   rest; a total at or below 0 stores a sum of 0 (`timeGetTime() % 0xD431 - 0x8235` and its
   negation). A capturer in debt therefore ends at exactly 0 gold.

The victim loses `base`, the capturer gains `gain`: the bonus is created from nothing. The
message number `$NUM0` is the sum of two stack locals at `0x56416A` / `0x5644C8`
(**HYPOTHESIS**: `base` plus the bonus).

## 4. Population loss or destruction (verified)

`0x5640A1..0x5642F1`. Let `A` = `0x4BB410(city, capturer race)` (citizens of the capturer's race),
`S` = `city +0x140 + 4 * capturer civ` (a per-civ integer; the transfer copies the old owner's
entry to the new owner clamped at 0, only when the byte `[0x9C5D6C]` is set; **HYPOTHESIS**: the
civ's culture in the city), `R` = `0x4BB410(city, old owner race)`, `Y` = `city +0x5C`.

```
if A != 0 or S != 0:           no loss
elif size == 1:
    if R == 1 and Y == 1:      destroy the city (0x4AECC0(city, capturer civ, 0)), skip the dialog
    else:                      no loss                 (a size-1 city is kept)
else:                          0x4BA230(city, 1, -1, 0): one citizen of any race dies
                               (and, when the usual civ conditions of step 1 hold,
                               both incident accumulators gain 1 more, 0x5642B0..0x5642E7)
```

`city +0x5C` is 1 at founding (`0x4AE40E`) and is reset to 1 by `0x4B0C60(city, 1, 0, 0)` in the
transfer; what else sets it is open (it is passed on to the redraw call `0x578E90`, so it may
be an art-style tier). The destroy path shows `WE_DESTROY_CITY_GOLD` / `THEY_DESTROY_CITY_GOLD`
and ends at `0x564685`, the counter tail shared with razing (section 9).

## 5. Keep or raze

After the loss step the routine builds the list of great wonders in the city (BLDG `+0xF0 & 4`,
`0x4ACB50(city, b, 0)`) for the `$WONDER3` link and picks the `*_WITH_WONDER` message variant if
there is one.

| capturer | what happens |
|---|---|
| the local human, single player (`0x564448`) | dialog `WE_CAPTURE_CITY_GOLD` / `SEIZE_CITY_WITH_WONDER` with the choices "Great! Install a new governor." (index 0) and "We don't want it. Raze the city!" (index 1); `0x611530` returns the index, 1 razes (`0x56467D`), anything else transfers |
| a human that is not the local human, or any human in multiplayer (`0x56453B`, mask `[0xA526BC]`) | no choice; the old owner, if it is the local human, sees `THEY_CAPTURE_CITY_GOLD` / `CITY_LOST_WITH_WONDER`; the city is transferred |
| an AI player (`0x564558`) | `P.vtable[+0x18](city)` = `0x443B60` decides (section 12); true: the old owner (if the local human) sees `THEY_RAZE_CITY_GOLD` / `CITY_RAZED_WITH_WONDER` and the city is razed; false: as for a human in multiplayer |

The plunder and the loss have already been applied when the choice is made.

## 6. The conversion branch (`convert != 0`)

`0x563922..0x563E89`. Scans the BLDG table for great wonders in the city (as above), then:

| converting civ `P` | what happens |
|---|---|
| the local human, single player (`0x563A74`) | dialog `WE_CONVERT_CITY` / `WE_CONVERT_CITY_WITH_WONDER`, choices "Great! Install a new governor." (0) and "We don't want X. Rebuff the rebels." (1). Index 1 refuses: no transfer, the old owner's pair record about `P` gains 1 in `+0x28` (`0x563B4E`), the routine returns 0 |
| the local human, multiplayer (`0x563B88`) | no dialog and no transfer: a pending prompt is stored in `[0x74CFC4..0x74CFD4]` (type `0x800`, or `0x1000` with a wonder; city id; `P.civ`; old owner; -1) and the routine returns 0 |
| a human that is not the local human (`0x563D68`) | in multiplayer returns 0 at once (**HYPOTHESIS**: the remote machine completes it); otherwise accepted: if the old owner is the local human it is told `THEY_CONVERT_CITY` / `THEY_CONVERT_CITY_WITH_WONDER`, then the transfer |
| an AI (`0x563BE0`) | `P.vtable[+0x14](city)` = `0x443A60` decides (section 12). False: the old owner, if the local human, is told `THEY_REFUSED_TO_CONVERT_CITY`, the same `+0x28` counter gains 1 (`0x563D33`), no transfer. True: as the previous row |

An accepted conversion continues into the transfer with `capture = convert = 1`. The pair
record's `+0x28` therefore counts refused conversions.

## 7. The transfer `0x564800`

`Player::takeCity(city, capture, convert)`, `ret 0xC`, `this` = the new owner `P`, old owner
`O = city +0x28`, `cont` = the tile's continent (tile vtable slot `0xB8`). Steps in code order:

| # | VA | step |
|---|---|---|
| 1 | `0x564826..0x5648B5` | copy the sub-record at `city +0x388` aside (`0x4F4C80`); restored at `0x56501E..0x565084` into `+0x38C..+0x3B4` (meaning open) |
| 2 | `0x5648D8` | if `city.id == O.+0x2C` (O's capital): `0x563220(O, P.civ)`, `O +0x15CC = P.civ`, `O +0x15C8 = 1`, `0x4482B0(O, city)`, then pass A of section 8 |
| 3 | `0x564975..0x564A91` | passes B, C, D of section 8 (`0x4ACF40(city, b, 0, 0)` removes building `b`) |
| 4 | `0x564A91..0x564ADF` | for each civ in play (`[0xA526C0]`), `civ >= 1`: the dword at index `cont` (the city's continent) of each of the two heap arrays `Player +0x1608` and `Player +0x160C` that equals the city id becomes -1. These are two of the five per-continent arrays `+0x1600 .. +0x1610` (one dword per continent, `+0x19C` = their length, allocated by `0x567B90`, freed by `0x567BF0`, filled in `Player::init` `0x567C80` at `0x568315..0x568350` with 0 for `+0x1600`, `+0x1604`, `+0x1610` and -1 for `+0x1608`, `+0x160C`); they hold a city id per continent (I) and are AI planner state (open). An earlier revision of this row named the offsets `+0x15EC`/`+0x15F0`, which was a misread base register (`0x564A9A` runs with `eax = player + 0x1608`) |
| 5 | `0x564AE6..0x564B32` | every unit with `+0x1C8 == city.id` (its home city) goes through `0x5BEB10(unit)`; then `0x4AFAB0(city, 0, -1, 0)` (open) |
| 6 | `0x564B4C..0x564BA0` | per-building counts: for each building the city still holds, `O +0x15DC[b]` loses 1 (if above 0) and `P +0x15DC[b]` gains 1 (words) |
| 7 | `0x564BA2..0x564BFA` | `O +0x194` loses 1, `P +0x194` gains 1 (city counts); `O +0x1610[cont]` loses 1, `P +0x1610[cont]` gains 1 (cities per continent) |
| 8 | `0x564C03` | **`city +0x28 = P.civ`**, the ownership write |
| 9 | `0x564C06..0x564CCC` | if `0x5DBF10(cell) != 0`: tile vtable slot `0xE0` on the city's cell, mode 3 when `f = [[0x9C7324] +0x218]` is -1, mode 1 when `f == [0x9C3DBC]`, else mode 3 if the bit of `P.civ` is set in `[0xA52B4C][f]` and mode 1 if not (open) |
| 10 | `0x564CD2..0x564EEE` | each unit on the city tile owned by `O` and **not inside an Army** (`0x5BCA90`: its container has ability 18): if `capture`, killed (`0x5BBBC0(unit, P.civ, 0, 0, 0, 0, 0, 0)`); else relocated: land units to `O`'s capital (`0x5BD220`), sea units to a nearby water tile found by the spiral walk (`0x5D8520`, `0x5E6E50`) or the capital; any that is still on the tile is killed |
| 11 | `0x564F11..0x564F58` | if this is `P`'s only city (`+0x194 == 1`): `0x55CB20(P, city)` (**HYPOTHESIS**: records it as the capital) and the building with Center of Empire is **added** (`0x4ACF40(city, b, 1, 0)`) |
| 12 | `0x564F58..0x564FAA` | `0x55A560` on both players; `city +0x40 = 0`; shield stock `+0x44 = 0` unless the current item's cost is not positive (then the cost); `0x4BCDE0(city, n)` with `n = 1` for a plain capture, else 10 (stores `city +0x58`; **HYPOTHESIS**: a timer) |
| 13 | `0x564FB9..0x564FFF` | when `capture`: unless **both** players are human, `city +0x70` and `city +0x1C4` are halved (`sar 1`; fields unnamed) |
| 14 | `0x564FFF..0x565084` | `city +0x1C0 = 0`, `city +0x30 &= ~0x1F`, snapshot restored, `0x4C0BB0(city, 0)` |
| 15 | `0x565089..0x5650F9` | `0x4BCEA0(city)`, except for a human new owner in multiplayer, which sends `0x475240(0x74AF60, city.id, local civ)` instead |
| 16 | `0x5650F9..0x565137` | if `[0x9C5D6C]`: `city +0x140[P.civ] = max(city +0x140[O.civ], 0)`; else `0x4B0B70(city)` |
| 17 | `0x565137..0x5651C8` | `0x4B0C60(city, 1, 0, 0)` (`+0x5C = 1`); if the new government's record has `+0x28 != 0`, `0x55CD00(city)`; `0x4BB090`, `0x4B0E80` (the full city recompute: `yields.md` section 5), `0x55CF10` on both players, city vtable `+0x38` and `+0x58`; the log event `0x58B5D0(0xC88588, 4, P.civ, x, y, name, 0)` |
| 18 | `0x5651D4..0x565257` | a **new defender** is created (`0x5694D0(P, city.vtable[+0x20](), ...)`, **HYPOTHESIS**: the city's best defender prototype) when `convert`, or when not `capture` and neither player is human; a capture by the local human in single player then calls `0x4ACD20(city, 0)` (open) |
| 19 | `0x565268..0x565356` | redraw: walks the 21 city-radius tiles (`0x5E6E50`); if the local human sees any, `0x578E90(0xA0E270, x, y, city +0x5C)` and `[0xA281C4] = 1` |
| 20 | `0x565361..0x5653F8` | `O +0x15B4` (cities lost) gains 1; `0x568950(O; cont, P.civ, flag)` with `flag = ([0xA5267C] & 0x400) and count >= [0xA52998]` (`this` = `O`'s record; open); `0x4484C0(player, 1)` for every civ in play |

## 8. Which buildings survive (verified)

Predicates (`BLDG` rows at `[0x9C40AC]`, stride `0x110`; the in-memory row is the file body
shifted by 4):

* `0x4ACB50(city, b, 0)`: the city holds `b` (and the building's required government,
  `+0xD4`, if any, is the owner's, `Player +0xA0`); a membership test in the city's building set
  at `city +0x274` (`0x5DF8D0`).
* `0x4B3290(city, b)` ("ordinary"): `0x4ACB50` and not `+0xF0 & 4` (great wonder) and not
  `+0xF0 & 8` (small wonder) and not `+0xEC & 1` (Center of Empire) and not `+0xEC & 0x1800`
  (the city-size gates: shipped `Aqueduct` `0x800`, `Hospital` `0x1000`).

Passes of the transfer, in this order:

| pass | VA | condition | buildings destroyed |
|---|---|---|---|
| A | `0x56492B` | the city is `O`'s capital | `0x4ACB50` and `+0xEC & 1` (the Palace) |
| B | `0x564985` | always | `0x4B3290` and `+0x98 > 0` (culture per turn above 0: shipped Temple, Library, Cathedral, University, Colosseum, Research Lab) |
| C | `0x564A05` | `capture` and not `convert` and `0x4BB410(city, P race) == 0` | each remaining `0x4B3290` building, when `next(4) == 0` on the gameplay `Random` (`0xA526B4`): 25 percent, one draw per building, ascending index |
| D | `0x564A47` | always | `0x4ACB50` and `+0xF0 & 8` (small wonders) |

Great wonders are never destroyed; they are listed in the capture message instead. A peaceful
transfer (`(0, 0)`) still runs passes B and D.

**BLDG row layout (data check).** On the shipped rows the dword the exe tests as `+0xEC` (file
body `+0xE8`) is 1 for the Palace, `0x800` for the Aqueduct, `0x1000` for the Hospital; the dword
tested as `+0xF0` (body `+0xEC`) has bit 2 set on every great wonder and none of the ordinary
buildings and bit 3 on every small wonder (`Forbidden Palace` `0x108`, `Wall Street` `0x8`,
`Apollo Program` `0x28`, `Heroic Epic` `0x908`, `Iron Works`, `Military Academy`, `Pentagon`,
`Strategic Missile Defense`, `Intelligence Agency`, `Battlefield Medicine`, `Secret Police HQ`);
body `+0xF0` holds culture-trait bits. The in-memory row is the body at +4 (the description string
fills `+0x04..+0x43`, the name sits at `+0x44` and the Civilopedia key at `+0x64`, which is where
`0x56437A` / `0x5643C6` read them for the `$LINK<name=key>` list).

## 9. Razing `0x563370`

`Player::razeCity(city)`, `ret 4`, `this` = the capturer:

1. `Player[O] +0x1C4 + 0x4C * P.civ + 0x2C` gains 1 (`0xA53088`): a "cities razed" counter in the
   victim's pair record (`0x563398`).
2. `size / 2` times: `0x5694D0(P; [0x9C72C4], x, y, -1, -1, 0, 0, O race)`: a unit of the RULE
   worker prototype (file RULE body `+0xFC`, shipped 1, `Worker`) for the capturer.
3. `0x4AECC0(city, P.civ, 0)`: the city is removed.

The caller then runs the shared tail (`0x564685`): `O +0x15B4` (cities lost) gains 1,
`0x568950(O; cont, P.civ, flag)` as in step 20 of section 7, and the routine returns 0.

## 10. Messages (`Text/script.txt`)

Capture: `WE_CAPTURE_CITY_GOLD`, `SEIZE_CITY_WITH_WONDER`, `WE_DESTROY_CITY_GOLD`,
`THEY_CAPTURE_CITY_GOLD`, `CITY_LOST_WITH_WONDER`, `THEY_RAZE_CITY_GOLD`,
`CITY_RAZED_WITH_WONDER`, `THEY_DESTROY_CITY_GOLD`. Conversion: `WE_CONVERT_CITY`,
`WE_CONVERT_CITY_WITH_WONDER`, `THEY_CONVERT_CITY`, `THEY_CONVERT_CITY_WITH_WONDER`,
`THEY_REFUSED_TO_CONVERT_CITY`. The `*_WITH_WONDER` variants carry a `$WONDER3` list. Only the
local human sees a message; every effect applies regardless.

## 11. Corrections to earlier notes

| earlier claim | now |
|---|---|
| `combat.md` 14.5 "capture by a non-barbarian civ (open)" | decoded here, including the AI decisions (section 12); the remaining helpers are in section 13 |
| `biq` crate: BLDG body `+0xE8` is `required_resource` and body `+0xEC` `improvement_flags` | the exe reads body `+0xE8` as the improvement flags (Palace, size gates) and body `+0xEC` as the wonder-class word (bit 2 great, bit 3 small). The crate computed the exe's memory offsets without the 4-byte row header |
| `biq` crate: `other_characteristics` bits 2 / 3 are `EXPANSIONIST` / `COMMERCIAL` | those are the trait bits of body `+0xF0`; the wonder bits the exe tests at `+0xF0` are body `+0xEC` |
| `biq` crate: RACE tail dword 13 (`+0x34`, memory `+0x948`) is `unique_unit` ("Romans 3 = Legion") and dword 14 (`+0x38`) `unique_building` | dword 13 is the **trait mask** the exe tests with `RACE.vtable[0]` (`0x53A080`): bit 0 Militaristic, 1 Commercial, 2 Expansionist, 3 Scientific, 4 Religious, 5 Industrious, 6 Agricultural, 7 Seafaring. All 31 playable civs have exactly two bits set and match the Civilopedia (Rome 3, Egypt 48, Greece 10, Babylon 24, Germany 9, Russia 12, China 33, America 36, Japan 17, France 34, India 18, Persia 40, Aztec 65, Zulu 5, Iroquois 66, England 130, Mongols 5, Spain 144, Vikings 129, Ottomans 40, Celts 80, Arabs 20, Carthage 160, Koreans 10, Sumer 72, Hittites 6, Dutch 192, Portugal 132, Byzantines 136, Inca 68, Maya 96); Rome's 3 only looks like the Legion's index. Dword 14 is 17 in every row |
| earlier note (this file, first draft): "the Player vtable has not been located statically" | `0x66CB38`, stored by the Player constructor `0x539990` (section 12) |

## 12. The AI's two decisions (verified)

All players share one vtable, `0x66CB38`; humans simply never reach these calls. Slot `+0x14` is
`0x443A60`, `+0x18` `0x443B60`, `+0x84` the attitude evaluation `0x440100`, `+0x94` the deal
scorer `0x440EE0` (`diplomacy.md`). Slots `+0x1C..+0xA8` are more AI decision methods
(`0x437F80..0x44A800`); only the four above are read.

Both take the city as their one stack argument (`ret 4`), `this` is the deciding player `P`, and
the result is a boolean in `al`. `O` is the city's current owner (`city +0x28`). The AI sees the
city after the plunder and the loss of a citizen (sections 3 and 4), so `size` below is the
reduced size.

Two predicates recur:

* **active for P** (a great wonder, BLDG `+0xF0 & 4`, that `0x4ACB50(city, b, 0)` finds in the
  city): BLDG `+0xD4` (required government, `-1` none) is `-1` or `P +0xA0`, and BLDG `+0xE0`
  (obsoleting tech, `-1` none) is `-1` or `P.hasTech` (`0x561440`) is false.
* **built by P**: `[city +0xC4] + 12 * b`, field `+4` (the builder's civ id) equals `P +0x1C`.

### 12.1 Accept a flipped city, `0x443A60` (vtable `+0x14`)

Called at `0x563BE6` (section 6): true converts, false refuses. First hit wins:

| # | test | result |
|---|---|---|
| 1 | `P +0xD30[O] != 0`: at war with the old owner (`0x443A6C`) | accept |
| 2 | a great wonder in the city that is active for P, or built by P (`0x443AB4..0x443B03`) | accept |
| 3 | `0x4BB410(city; P.race) > 0`: a citizen of P's race lives there | accept |
| 4 | otherwise (`0x443B3E`) | accept iff `P +0x194 < 2 * OCN` |

`OCN` is the optimal city number `0x5676C0` (`economy.md`). There is no random draw. Refusal
needs all of: not at war with `O`, no wonder worth having, no citizen of its own race, and at
least twice the optimal number of cities. `rust/src/capture.rs`: `ai_accepts_city`.

### 12.2 Raze a captured city, `0x443B60` (vtable `+0x18`)

Called at `0x564558` (section 5) for an AI capturer: true razes (section 9). First hit wins:

| # | test | result |
|---|---|---|
| 1 | `Player[O] +0x194 == 1`: the old owner's last city (`0x443BAD`) | keep |
| 2 | `Player[O] +0x1610[c] + P +0x1610[c] == 1`, `c` the continent of the city's cell (cell slot `0xB8`): no other city of either player on its landmass (`0x443BD6`) | keep |
| 3 | `P` has a capital (`P +0x2C` in the city table) and one of the city's 21 radius tiles holds a luxury or strategic resource that the capital cannot use (`0x443C29..0x443CD4`, below) | keep |
| 4 | a great wonder active for P, or built by P (`0x443CE8..0x443D6C`) | keep |
| 5 | `0x4BB410(city; P.race) >= (size + 1) / 2`: at least half the citizens are P's race (`0x443D91..0x443DAE`) | keep |
| 6 | a great wonder is still in the city (`0x443DBC..0x443E40`). Rule 4 has removed every wonder P wants, so each remaining one is inactive for P and built by another civ | **raze** |
| 7 | `city +0x140[O] > city +0x140[P.civ]` (`O` has the larger culture stake) **and** `0x4BB410(city; O.race) >= (size + 1) / 2` (`0x443E69..0x443EB7`): raze if `Player[O] +0x183C > 2 * P +0x183C`; otherwise draw `a = next(15)`, `b = next(15)` and raze if `R - A >= a + b` (`R`, `A` = the citizens of `O`'s and `P`'s race, `0x443EF0..0x443F47`); a lost contest falls through | raze or fall through |
| 8 | `P +0x194 >= 2 * OCN` (`0x443F55`) | result |

The two draws are made only on the path that reaches rule 7's contest. With `m = R - A` the
contest is won with probability `(m + 1)(m + 2) / 450` for `m <= 14` (two uniform values in
`0..14`): 12% for `m = 6`, 40% for `m = 12`. `rust/src/capture.rs`: `ai_razes_city`, `Wonder`,
`wonder_active`, `is_scarce_good`.

**The resource scan (rule 3).** For `n` in `0..20`: `0x5E6E50(n, &dx, &dy)` (the spiral of
`rust/src/spiral.rs`) gives an offset; `x = wrapX(city.x + dx)` (`0x426C00`), `y = wrapY(city.y +
dy)` (`0x426C40`); tiles outside `0 <= x < [0x9C74D4]`, `0 <= y < [0x9C74C0]` are skipped; the
cell is `0x5D16A0((width / 2) * y + (x >> 1))`, its slot `0x9C` returns the GOOD index (`dword
[cell +8]`, `-1` none), and `0x5E3700(GOOD row)` accepts class 1 (luxury) or 2 (strategic). The
first such tile for which `0x4ADE30(capital; good)` is false ends the decision with *keep*.

`0x4ADE30(capital; good)` (`this` is a city; `ret 4`): false for `good == -1` or `>= [0x9C3DA4]`.
If the owner has no capital, or `0xB72888.0x57F0A0(city, capital, owner)` (a connection test
between the city and the capital) fails, the answer is bit `good` of the city's own resource
mask `city +0x9C`. If connected, it is true when `0x55E730(owner; good)` is: some civ in play
`j >= 1` has `owner +0x1614[(good * 32 + j) * 3]` with both of the record's first two bytes
non-zero (the owner's resource-supply table); otherwise false. The tail that first tests
`city +0x9C` and calls `0x55E850(owner; good)` (the count of records with byte 0 set and byte 1
clear) discards that count and still returns false. **HYPOTHESIS**: the table is the per-civ
resource availability the turn code maintains, so "cannot use" means "no connected supplier".

### 12.3 What the AI does not look at

Neither method reads the city's size beyond the half-size thresholds, its buildings other than
wonders, its distance from the capital, the war state (except rule 1 of the accept test), or the
gold already plundered. `Player +0x183C`, which rule 7 compares, is a per-player rating that the
AI attitude evaluation `0x440100` also reads (`other +0x183C < this +0x183C / 2`). **Correction
(`borders-culture.md` section 8):** it is the *total culture* of the civ, field `+0x20` of the
culture sub-object `Player +0x181C`; `Player::reset` (`0x567C80`) zeroes it, and the per-turn
`0x4F8E20` adds the empire's culture production to it (`max(0, total + max(0, produced))`), so
the earlier reading "the only writer stores 0 and the comparison is `0 > 0`" was wrong: the
writes use the sub-object base, not the absolute offset `+0x183C`.

## 13. Open

* The Player vtable slots `+0x1C..+0xA8` other than the four read here; the connection test
  `0x57F0A0` on `0xB72888`; the table `Player +0x1614` (its writers).
* Helpers read only by their call sites: `0x563220` (capital lost), `0x4482B0`, `0x4AFAB0`,
  `0x4AECC0` (remove city), `0x4ACF40` (remove / add building), `0x5BEB10`, `0x4BCDE0` /
  `0x4BCEA0`, `0x4B0B70`, `0x4B0C60`, `0x4BB090`, `0x4B0E80`, `0x55CB20`, `0x55CD00`, `0x55CF10`,
  `0x55A560`, `0x568950`, `0x4484C0`, `0x5694D0` (unit factory), the cell slots `0xE0`.
* City fields `+0x40`, `+0x58`, `+0x70`, `+0x1C0`, `+0x1C4`, `+0x140[civ]` (its writers: only
  the readers are known), the sub-record at `+0x388`; player fields `+0x15B4`, `+0x15C8` (a counter,
  `victory.md` section 1.4), `+0x15CC`, `+0x15EC` (`+0x15F0` is the per-PRTO live-unit array, `world-events.md`; the per-continent arrays at `+0x1600..+0x1610` are described in the table above) (`+0x11CC`, `+0x15C4` and the words
  `[0xA5267C]`, `[0xA529B8]` are identified in `victory.md`); the words `[0xA52998]`, `[0x9C5D6C]`.
* The remaining mid-function ranges of `0x564800` that only write UI state.
