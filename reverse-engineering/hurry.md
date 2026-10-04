# Hurrying production (gold and forced labor) and citizen removal

Clean-room specification of what happens when a city's production is hurried (bought with gold, or
completed by sacrificing citizens), of the AI price handicap, and of the routine that removes citizens
from a city (`0x4BA230`), which the hurry, starvation, plague, meltdown, bombardment, capture and
unit-completion code all share. Every claim cites the address it was read at (raw disassembly; `r2`).
Tags: **V** read from the instructions, **H** hypothesis (the data flow is read but the meaning is
inferred), **O** open (not decoded). No reference implementation is attached on purpose: the algorithms
and the golden vectors in section 9 are the contract.

Neighbours (not repeated here): `city-turn.md` (the shield box, item costs `0x569FE0` / `0x56A210`, the
completion tick that follows a hurry), `happiness.md` (the hurry-sacrifice unhappiness that reads
`city +0x1C4`), `government.md` (`GOVT +0x1A0` hurry method, the per-pair record that `0x4BA230`
increments), `economy.md` (growth, the food box, Granary, treasury cells), `yields.md` (`0x4B0470`,
`0x4B0540`, `0x4B05D0`, `0x4B07C0`).

## 1. Entry points

| address | role | called from |
|---|---|---|
| `0x4B6010(this; t)` (`ret 4`) | **the hurry command**: validate, then execute or send | the city screen (`0x41F270` at `0x41FD2B`, `0x41B4C0` at `0x41B835`, both with `t = 0`), the AI purchase code `0x433CD0` (11 call sites) and `0x435F80` (`0x436B9F`), the command dispatcher `0x4E8850` (`0x4E9D0F`) |
| `0x4B5290(this; t)` (`ret 4`) | **validator + confirmation**: returns a bool (`al`), does *not* change the city | `0x4B6010` only |
| `0x4B5CA0(this; t)` (`ret 4`) | **executor**: recomputes the price, applies it | `0x4B6010`, and the multiplayer message handler `0x46F8B0` (`0x46FAF5`) |
| `0x4B5190(this; t)` (`ret 4`) | "how many citizens would a forced-labor hurry cost" (no side effects), for the AI | `0x435F80` (`0x436B02`) |
| `0x4B50A0(this; v, mode, t)` (`ret 0xC`) | AI price/people handicap (section 6) | `0x4B5190`, `0x4B5290` (two sites), `0x4B5CA0` (two sites) |
| `0x4BA230(this; n, race, flag)` (`ret 0xC`) | remove `n` citizens (section 8) | see 8.1 |

`t` is a small "purchase category" integer. The city screen passes `0`; the AI passes `0..8` (the table
of call sites is in 6.2). It has no effect on a human owner.

`0x4B6010` (**V**, `0x4B6010..0x4B6048`):

```
if not 0x4B5290(this; t):  return
if multiplayer (0x47B530):   send the request: 0x472270(0x74AF60; city id (+0x20), t)     # nothing is executed locally
else:                        0x4B5CA0(this; t)
```

In multiplayer each peer executes `0x4B5CA0` when the message arrives (`0x46F8B0` calls it directly,
so *the executor repeats none of the validator's checks*; it only clamps, section 5).

## 2. Inputs

**Rules constants** (the RULE object at `0x9C71E4`; memory order, field order of `biq/src/sections/rule.rs`):

| address | RULE field | stock value | used for |
|---|---|---|---|
| `[0x9C7280]` | Shield Value in Gold (hurry production) | 4 | gold per missing shield |
| `[0x9C7284]` | Citizen Value in Shields (hurry production) | 20 | shields one sacrificed citizen is worth |
| `[0x9C72BC]` | Turn Penalty for Each Hurry Sacrifice | 20 | turns of unhappiness per sacrificed citizen |

**Method selector**: `GOVT[P.+0xA0].+0x1A0` (`GOVT` at `[0x9C71D8]`, stride `0x1E8`; `P.+0xA0` is the
owner's government) read at `0x4B5488..0x4B54A6` and `0x4B5CBD..0x4B5CD5`:

| value | method | validator | executor |
|---|---|---|---|
| 0 | none (Anarchy in the shipped rules) | `HURRY_UNAVAILABLE` for the local owner (`0x4B5C34..0x4B5C64`), returns false | not reached; the executor returns without effect (`0x4B6003`) |
| 1 | forced labor (sacrifice citizens) | section 4.2 | section 5.2 |
| 2 | pay gold | section 4.1 | section 5.1 |
| other | treated as 0 in the executor (no effect); the validator returns false silently (`0x4B54B7 -> 0x4B5C74`) | | |

**Helpers** (all `this` = city; **V**):

| address | meaning |
|---|---|
| `0x4ACD70` | the current item's cost: kind 2 `0x56A210(P; id, 0)`, kind 1 `0x569FE0(P; id, 0)`, kind 0 gives 0 (`city-turn.md` section 4). A kind above 2 returns the saved `ecx` (`0x4ACDE6`; unreachable, kinds are 0..2) |
| `0x4B0AB0` | the construction bonus pool: `+0x3B8` when the current kind is 1 (building), else 0 |
| `0x4ACD40` | **available shields** `max(0, +0x44 + 0x4B0AB0())`. Written `S` below |
| `0x569BB0(P; b)` (`ret 4`) | "ordinary improvement": true iff BLDG row `b` has improvement flags (`+0xEC`) bit 0 clear (not the Palace), other characteristics (`+0xF0`) bits 2 and 3 clear (not a great or small wonder), improvement flags bit 19 (`0x80000`) clear (not Capitalization), and spaceship part (`+0xD8`) equal to `-1` (`0x569BB0..0x569C08`). The same predicate gates `canBuildImprovement` (`0x4C025B`) |

`S` includes the bonus pool for a building and the box only for a unit; "empty box" in this document
always means `S == 0`, not `+0x44 == 0`.

## 3. The price and the people

Let `cost = 0x4ACD70()`, `S = 0x4ACD40()` and `rem = cost - S` (the bonus pool and the box are both
subtracted; `S` is never negative).

* If `rem < 1` there is nothing to hurry: the validator reports `HURRY_NOT_NECESSARY`; the executor
  treats the price (or people) as 0.
* **Gold price** (method 2, `0x4B54C2..0x4B5510`, `0x4B5CEA..0x4B5D37`):

  ```
  price = [0x9C7280] * rem               # linear in the missing shields
  if S == 0:  price = price * 2           # nothing invested yet: double
  price = 0x4B50A0(this; price, 0, t)     # AI owners only (section 6); identity for humans
  ```
* **People** (method 1, `0x4B5A03..0x4B5A62`, `0x4B5E5B..0x4B5EA7`, `0x4B5190`):

  ```
  people = ceil(rem / [0x9C7284])         # q = rem / c (truncating idiv); q++ when q*c < rem
  if S == 0:  people = people * 2
  people = 0x4B50A0(this; people, 1, t)   # AI owners only (section 6); identity for humans
  ```

A RULE value of 0 for `[0x9C7284]` would fault (`idiv` by zero at `0x4B5A33`, `0x4B5E7F`, `0x4B524C`); the
shipped rules have 20. The doubling test is `S == 0` (`0x4ACD40() == 0`, `0x4B54F1..0x4B54FA`, `0x4B5A41..0x4B5A4A`), so a
building whose bonus pool is positive is never doubled, and a unit whose box is empty always is.
`0x4B5190(this; t)` is the same people computation with `cost` read through the current kind
(`0x4B5190..0x4B5280`): it returns 0 when `rem < 1`.

## 4. The validator `0x4B5290`

Returns true (`al = 1`) when the hurry may proceed. For an owner other than the local slot
(`[0x9FD4BC]`) no message is ever shown (every `jne 0x4B5C74` before a dialog, `0x4B52D2`, `0x4B534D`,
`0x4B53DD`, `0x4B5521`, `0x4B558D`, `0x4B5A70`, `0x4B5AB1`, `0x4B5C41`) and the function simply returns
false or true. A "message" is a modal notice (`0x495840`, or the dialog host's slot `+0x170`) whose
`0x611530` commit follows; in multiplayer the notice call receives the flag `0x4000` (`0x47B530`
neg/sbb/and idiom; H: non-modal). Checks in this exact order:

| # | check | at | outcome |
|---|---|---|---|
| 1 | city flag `+0x30` bit 0 (disorder) | `0x4B52BC..0x4B52C4` | message `HURRY_CIVIL_DISORDER`; false |
| 2 | any citizen with `+0x20 != 0` among pool slots `0..+0xEC` (a resister) | `0x4B5301..0x4B533F` | message `HURRY_RESISTANCE`; false |
| 3 | current kind 1 **and** not `0x569BB0(P; id)` (Palace, wonder, small wonder, Capitalization, spaceship part) | `0x4B537C..0x4B53C8` | message `HURRY_CANNOT` with the building name; false. Kind 2 and kind 0 skip this check |
| 4 | method (section 2): 0 -> `HURRY_UNAVAILABLE`, false | `0x4B5488` | |

### 4.1 Gold method (`0x4B54BD..0x4B59FE`)

```
price = (section 3)
if rem < 1 or price == 0:                    message HURRY_NOT_NECESSARY; false           # 0x4B5514
if price > P.+0x44 + P.+0x48:                message HURRY_NOT_ENOUGH_GOLD (number arg = price); false    # 0x4B5583
if owner == local slot:
    dialog HURRY_GOLD   text arg 0 = item name (unit PRTO +0x08, building BLDG +0x44), number arg 0 = price
    if the answer (0x611530) == 1:  false                                                  # 0x4B56AB (1 = cancel)
true
```

`P.+0x44 + P.+0x48` is the treasury (two shares; `economy.md` "Treasury cell").

### 4.2 Forced-labor method (`0x4B5A03..0x4B5C32`)

```
people = (section 3)
if rem < 1 or people == 0:                   message HURRY_NOT_NECESSARY; false           # 0x4B5A64
if people > size/2:                          message HURRY_NOT_ENOUGH_PEOPLE (number arg = people); false     # 0x4B5AA0
if owner == local slot:
    dialog HURRY_PEOPLE   text arg 0 = item name, number arg 0 = people
    if the answer == 1:  false                                                             # 0x4B5BD5
true
```

`size/2` is the signed halving `eax = size; eax -= (eax >> 31); eax >>= 1` (`0x4B5A95..0x4B5A9E`), i.e.
`floor(size / 2)` for the sizes that occur. **A city of size 1 can never be hurried by sacrifice**, and
the city is never emptied by it (people at most half the size).

## 5. The executor `0x4B5CA0`

It re-derives the price itself (it does not receive it), so the two ends of a multiplayer game must
agree on all inputs. Method 0 or an unknown method: return at once (`0x4B6003`).

### 5.1 Gold (`0x4B5CEA..0x4B5E45`)

```
price = (section 3; 0 if rem < 1)
+0x30 |= 0x10                                           # 0x4B5D45  "hurried" marker (cleared when an item completes, city-turn.md 7)
+0x44 = cost            # min(cost, 0x4ACD70()) computed from the current kind (0x4B5D53..0x4B5DEC); the box is FILLED to the cost
T = P.+0x44 + P.+0x48 - price                           # 0x4B5E03..0x4B5E13
if T <= 0:  T = 0                                       # the treasury is never negative
write the treasury pair with the treasury writer of economy.md:
    T > 0:  P.+0x44 = timeGetTime() % T - 0x3039;        P.+0x48 = T - P.+0x44         # 0x4B5E49..0x4B5E59, 0x4B5E2E..0x4B5E39
    T == 0: P.+0x44 = timeGetTime() % 0xD431 - 0x8235;   P.+0x48 = 0 - P.+0x44         # 0x4B5E19..0x4B5E39
if owner == local slot:  [0xA281C4] = 1                 # redraw request, 0x4B5FFC
```

The bonus pool `+0x3B8` is **not** touched: `box + bonus >= cost` holds afterwards, so the next
production tick completes the item (`city-turn.md` 5 and 6.2). The executor does not check that the
treasury covers the price; the validator did.

### 5.2 Forced labor (`0x4B5E5B..0x4B5FFC`)

```
people = (section 3; 0 if rem < 1)
if people > size/2:  (local owner: message HURRY_NOT_ENOUGH_PEOPLE with the number)  return          # 0x4B5EB4; nothing happens
+0x30 |= 0x10                                           # 0x4B5F20
+0x44 = cost                                            # 0x4B5F2A..0x4B5FC8
+0x1C4 += [0x9C72BC] * people                           # the hurry-sacrifice timer (happiness.md): each citizen adds 20 turns   0x4B5FCB..0x4B5FE3
0x4BA230(this; people, -1, 1)                           # remove `people` citizens of ANY race; flag 1 counts them as nationals lost (8.4)
if owner == local slot:  [0xA281C4] = 1
```

`+0x1C4` accumulates: a second hurry within the period adds to the remaining turns (the per-turn
decrement is in the city sequencer, `city-turn.md` 2). The unhappiness itself is computed by the
happiness recompute (`happiness.md` section 4, row 7).

The executor's second `people > size/2` check protects the multiplayer receiver; in single player it
cannot fire after the validator.

## 6. The AI handicap `0x4B50A0(this; v, mode, t)`

`v` = price or people, `mode` 0 = gold price, 1 = people, `t` = the purchase category. **V**
(`0x4B50A0..0x4B515F`):

```
if owner is in the human mask [0xA526BC]:           return v
if mode == 1:   if v <= 0: return v                  # need at least one person
elif mode == 0: if v <= 1: return v
else (any other mode value): fall through
d = [0xA52684]                                       # game difficulty level (0 = easiest)
m = table[t]                                         # 6.1
w = (m * v) / (d + 3)                                # signed idiv, truncating toward zero
if w >= v:                         return v          # the handicap never raises a price
if w > 0:                          return w
if mode == 1:                      return 1          # at least one person (v > 0 here)
return w                           # mode 0: w <= 0, i.e. 0: the AI hurries for free
```

### 6.1 The numerator table (jump table at `0x4B5164`, **V**)

| `t` | 0 (and any `t > 8`) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| `m` | 3 | 2 | 1 | 4 | 5 | 5 | 3 | 4 | 3 |

The factor is `m / (d + 3)` with `d` the global difficulty (`[0xA52684]`): at `d = 0` the factor is
`m/3` (1 for `t` 0, 6, 8; lower for `t` 1, 2; no change for `t` 3, 4, 5, 7 since `w >= v`), and it falls as
`d` rises, so **harder levels make AI purchases cheaper**. A price of 1 or 0 is returned unchanged.

### 6.2 Where each `t` is used (O for the meaning)

`0x433CD0` (the AI purchase function, whose body is **O**): `t = 1` at `0x4343C9` and `0x434952`, `2` at
`0x43463F`, `7` at `0x43483D`, `8` at `0x4349C1`, `4` at `0x434A0E` and `0x434AFF`, `3` at `0x434A41`, `5` at
`0x434AB0`, `6` at `0x434B4E`, `0` at `0x434C2A`; `0x435F80`: `8` at `0x436B9F`, `0` for the `0x4B5190`
query at `0x436B02`. Each call is preceded by a test like `0x4BFD20(this; &x, 1) > k`, so the categories
correspond to different "I badly want this" situations; the mapping is not decoded.

## 7. What a hurry does not do

* It does not change the production, the queue, or the item kind; it only fills the box.
* It does not check the 'one hurry per turn' idea: there is no such check (the marker `+0x30 & 0x10` is
  set but the validator never reads it; the readers of that bit are **O**).
* The gold price is linear in the missing shields (`4` gold each) with a 2x surcharge for an empty
  box: there is no quadratic term in the executable.
* A hurry in a city whose item is Capitalization, a wonder, small wonder, Palace or spaceship part is
  refused for buildings (check 3); a **unit** is always allowed, including units with a population cost
  (the population cost is paid at completion, `city-turn.md` 7).

## 8. Citizen removal `0x4BA230(this; n, race, flag)` (`ret 0xC`)

`n` = how many citizens to remove (`n <= 0`: returns 0 at once), `race` = the RACE row to remove or
`-1` for any, `flag` = 1 to count the deaths as nationals lost (8.4). Returns the number of citizens
actually removed. **V** (`0x4BA230..0x4BA6BC`).

### 8.1 Callers (`flag` in brackets)

| call site | purpose |
|---|---|
| `0x4B5FE9` (`n`, `-1`, 1) | forced-labor hurry (section 5.2) |
| `0x4B8E1A`, `0x4B8E5A` (`n`, race, 0) | unit completion with a population cost (`city-turn.md` 7) |
| `0x4B20A5` (1, `-1`, 1) | starvation (`economy.md`) |
| `0x4B45DC`, `0x4B493F`, `0x4B4961` | disease / plague (`world-events.md`), `0x4B4A49` meltdown (`0x4B4970`) |
| `0x4A29F9` | city strike / bombardment population kill (`combat.md`) |
| `0x4AED6F` | inside the city removal `0x4AECC0` |
| `0x563410`: `0x56362A`, `0x5642F1` | capture (`capture.md`) |
| `0x55CD29`, `0x55CD3A`, `0x55CD4B` | a small helper `0x55CD00`, **O** |
| `0x4B37AC`, `0x4B3F60`, `0x4B5020`, `0x5B4070` (`0x5B4236`) | other callers, **O** |

### 8.2 The loop

The whole body runs `n` times. Each pass is independent (the city is fully recomputed at the end of
each pass).

```
removedThisPass = false;  specialistFlag = false;  resisterFlag = false
last  = city.+0xEC                                   # highest pool slot index
start = rand(last + 1)                               # gameplay RNG 0x60BAB0(0xA526B4; last+1), the low 16 bits    0x4BA266
for i in 0 .. last:
    idx = (start + i) mod (last + 1)                 # cdq / idiv                                               0x4BA28F
    node = pool.array[idx].+4                        # 8-byte nodes at city.+0xE0; skip when the array is null, idx out of range, or the node is null
    c = node - 0x1C
    if race != -1 and c.+0x140 != race:  continue
    victim = c;  break
if no victim: next pass                              # an unsuccessful pass still consumes one RNG draw and one count
```

The victim is therefore the first occupied slot at or after a uniformly random slot (cyclically), not a
uniformly random citizen: a citizen after a long run of empty slots is more likely. The RNG draw is made
**once per pass, always**, whether or not a victim exists.

### 8.3 Removing the victim (`0x4BA2D7..0x4BA4EF`)

Let `c` = the victim.

```
1. if flag:                                          # 0x4BA2D7
       S = civSlot(RACE[c.+0x140])                   # 0x539D60
       if S != owner:  Player[S].+0x1F4[19 * owner] += 1       # dword at 0xA5308C + 0x20E4*S + 0x4C*owner  ("nationals lost", government.md +0x44)
2. if c.+0x21 != 0:                                  # the victim works a tile (index 1..20 of the work radius; H for the exact meaning of +0x21)
       (dx, dy) = spiral(c.+0x21)                    # 0x5E6E50
       (x, y)   = city (x, y) + (dx, dy), wrapped (0x426C00 / 0x426C40), checked with 0x426BD0
       if valid:  cell = 0x437A70(x, y);  cell.word[+0x6C] = 0xFFFF            # the tile is released: worked-by city id = -1
                  0x4B0470(this; c.+0x21, 0)                                   # take the tile out of the city's totals
3. specialistFlag = (0x4ABE70(c))                     # = (c.+0x13C != [0x9C3D64]), i.e. the citizen's job is not the ordinary worker
4. if c.+0x20 != 0:  resisterFlag = true
5. release the pool node: 0x5B0380(node) (destructor, O); array[idx].+4 = 0; array[idx].+0 = freeHead (+0xE4); freeHead = idx; freeCount (+0xE8) += 1
6. removed += 1
7. classBefore = size > [0x9C72E8] ? 2 : size > [0x9C72E4] ? 1 : 0
8. size = max(0, size - 1)                            # +0x138
9. if specialistFlag:  +0x134 = max(0, +0x134 - 1)     # +0x134 counts the specialists (H: it is written by 0x4BAAF0, 0x4BA850, 0x4BACE0 which move citizens between tiles and jobs)
10. if resisterFlag and owner == local slot:
       if no citizen in the pool has +0x20 != 0:  log RESISTANCEENDS (text arg 0 = city name) at the city tile       0x4BA47B..0x4BA4EF
```

### 8.4 Size class change (`0x4BA4EF..0x4BA5D4`)

```
classAfter = size > [0x9C72E8] ? 2 : size > [0x9C72E4] ? 1 : 0
if classAfter != classBefore:                         # a city shrank across the town / city / metropolis limit (6 and 12 in the shipped rules)
    g = number of BLDG rows b (0 .. [0x9C3D80]-1) with  0x4ACB50(this; b, 1)       (present, not obsolete)
                                                  and  not 0x4ACCC0(this; b)       (not obsolete: BLDG[b].+0xE0 < 0 or the owner does not know that tech, 0x561440)
                                                  and  BLDG[b].+0xEC & 0x200       (the Granary flag, economy.md)
    if g > 0:   cap = 0x5660E0(P; 0) * (0x427540(this) + 1)                           # X * (new class + 1) = half of the new food box
                +0x40 = min(+0x40, cap)
    else:       +0x40 = 0                                                              # the food store is lost
```

(The compiled form multiplies by 2 and halves again, `0x4BA5B9..0x4BA5C0`; the result is the product.)
`0x5660E0(P; 0)` is the cost factor `X` of `city-turn.md` section 4 (10 for a human, `DIFF +0x68` for an
AI, halved by Accelerated Production). When the class does not change, the food store is untouched.

### 8.5 Recompute (`0x4BA5D4..0x4BA69F`), after every pass

```
+0x1C8 = +0x1CC = +0x1D0 = 0                          # gross food, shields, commerce
centre = city (x, y); if valid:  for i in 0..2:  +0x1C8[i] += 0x4B0330(this; i, x, y)       # the city square itself
for k in 1 .. 20:  if 0x4C2680(this; k):  0x4B0470(this; k, 1)                              # every worked tile is added back
0x4B0540(this); 0x4B05D0(this); 0x4B07C0(this)         # food eaten / surplus, shields, commerce split
0x4BCFF0(this)                                         # happiness recompute
vtable[+0x38](1)                                       # refresh (H: the argument asks for a redraw)
```

Notes:

* The routine never removes the city, never changes the player's city count and never touches the
  production box. Callers that can empty the city (starvation, unit completion) test `size == 0`
  themselves and call `0x4AECC0`.
* No message is shown except `RESISTANCEENDS` (8.3, step 10); the callers print their own.
* The order of RNG consumption is exactly one `rand(last + 1)` per pass, in pass order, before any
  other use of the gameplay RNG in the routine (the recompute draws none).

## 9. Golden vectors

All gold vectors: human owner, stock rules `[0x9C7280] = 4`, `[0x9C7284] = 20`, `[0x9C72BC] = 20`.

**Gold price** (`cost`, `S` = `0x4ACD40`):

| id | cost | box | bonus pool | kind | S | rem | price |
|---|---|---|---|---|---|---|---|
| G1 | 30 | 0 | 0 | unit | 0 | 30 | 240 (`4*30*2`) |
| G2 | 30 | 10 | 0 | unit | 10 | 20 | 80 |
| G3 | 30 | 29 | 0 | unit | 29 | 1 | 4 |
| G4 | 30 | 0 | 12 | building | 12 | 18 | 72 (not doubled: `S > 0`) |
| G5 | 30 | 30 | 0 | unit | 30 | 0 | `HURRY_NOT_NECESSARY` |
| G6 | 30 | 0 | 0 | building, bonus 0 | 0 | 30 | 240 |

**People**:

| id | cost | S | rem | people | size needed (`people <= size/2`) | timer added (`20 * people`) |
|---|---|---|---|---|---|---|
| P1 | 30 | 0 | 30 | `ceil(30/20) = 2`, doubled: 4 | 8 | 80 |
| P2 | 30 | 10 | 20 | 1 | 2 | 20 |
| P3 | 41 | 1 | 40 | 2 | 4 | 40 |
| P4 | 100 | 0 | 100 | `5 * 2 = 10` | 20 | 200 |
| P5 | 30 | 9 | 21 | `ceil(21/20) = 2` | 4 | 40 |

A size-7 city cannot execute P1 (`7/2 = 3 < 4`): `HURRY_NOT_ENOUGH_PEOPLE` with number 4.

**AI handicap** (`0x4B50A0`, non-human owner, `d = [0xA52684]`):

| id | v | mode | t | d | result |
|---|---|---|---|---|---|
| A1 | 240 | 0 | 0 (`m = 3`) | 2 | `3*240/5 = 144` |
| A2 | 240 | 0 | 2 (`m = 1`) | 2 | 48 |
| A3 | 240 | 0 | 4 (`m = 5`) | 2 | `1200/5 = 240 >= v`: 240 |
| A4 | 240 | 0 | 3 (`m = 4`) | 2 | 192 |
| A5 | 240 | 0 | 0 | 7 | `720/10 = 72` |
| A6 | 3 | 0 | 2 | 2 | `3/5 = 0`: returns 0 |
| A7 | 4 | 1 | 2 | 2 | `4/5 = 0`: returns 1 (mode 1 keeps one person) |
| A8 | 1 | 0 | 2 | 2 | 1 (`v <= 1` is returned unchanged) |
| A9 | 240 | 0 | 0 | 0 | `720/3 = 240 >= v`: 240 |
| A10 | 240 | 0 | 12 (out of range) | 2 | table default `m = 3`: 144 |

**Treasury after a purchase**: price 240, treasury 1000: `T = 760`, the pair is `(timeGetTime() % 760 -
0x3039, 760 - that)`; the sum is 760. Price equal to the treasury: `T = 0`, pair `(timeGetTime() % 0xD431 -
0x8235, -that)`, sum 0.

**Food after shrinking** (X = 10, `[0x9C72E4] = 6`, `[0x9C72E8] = 12`):

| id | size before -> after | class before -> after | Granary | store before | store after |
|---|---|---|---|---|---|
| S1 | 7 -> 6 | 1 -> 0 | yes | 25 | `min(25, 10 * 1) = 10` |
| S2 | 7 -> 6 | 1 -> 0 | no | 25 | 0 |
| S3 | 13 -> 12 | 2 -> 1 | yes | 35 | `min(35, 10 * 2) = 20` |
| S4 | 5 -> 4 | 0 -> 0 | no | 25 | 25 (unchanged: same class) |

**Victim selection** (`last = 3`, slots `[A race 1, empty, B race 2, C race 1]`, `race = 1`): `start = 0` -> A;
`start = 1` -> C (slot 2 is B, wrong race); `start = 2` -> C; `start = 3` -> C; with `race = -1`:
`start = 1` -> B (slot 1 is empty).

## 10. Open items

1. The meaning of `t` (6.2) and the AI's decision to purchase (`0x433CD0`, 8.9 KB; `0x435F80` blob).
2. The readers of the marker `city +0x30 & 0x10` (set by every hurry, cleared at completion).
3. `0x5B0380` (the citizen node destructor), the exact meaning of citizen `+0x21` (worked tile index)
   and city `+0x134` (specialist count, **H**).
4. The callers listed as **O** in 8.1 and what the `vtable[+0x38](1)` argument selects.
5. Whether the dialog's button 1 is labelled "No" or "Cancel" (the code only compares with 1).

## 11. Corrections to earlier documents

* `ai.md` "Hurry validator `0x4B5290`": the sentence "Hurry gold/people cost math: open" is resolved by
  sections 3, 4 and 5; the validator does not apply the hurry, `0x4B5CA0` does.
* `city-turn.md` section 12 and `capture.md`: `0x4BA230` is specified here (section 8) rather than left as
  a shrink routine of unknown body.
