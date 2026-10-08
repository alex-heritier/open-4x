# Espionage: diplomat and spy missions (clean-room specification)

Executable: `Civ3Conquests.exe` (PE32, MSVC 6, image base `0x400000`). This document specifies the **mission engine**
that the Diplomat and Spy units drive: the nine missions of the `ESPN` table, what each costs, how its success
chance is built, the exact order of the random draws, the effect of each mission, the "hostile act" aftermath, and the
computer player's side (when it tries a mission, which mission, and against which city or technology). It is written
for a clean-room port; no source from the reference implementation is needed.
Companion documents: `diplomacy.md` (embassies, the `Player` relation tables, `canTalk`, the war decision
`wantsWar`), `government.md` (the incident book of section 5.1, which `0x502CC0` writes), `research.md` (`acquire`
`0x561860`, `discover` `0x55B1A0`, the AI technology pick 10.6), `happiness.md` / `city-turn.md` (the propaganda
unhappiness counter `city +0x1C0`), `capture.md` (`0x563410`, the city transfer), `economy.md` (the size classes),
`barbarians.md` 1.5 (`0x56D040`, the nearest-city search), `biq-format.md` (the `ESPN`, `GOVT`, `CULT`, `EXPR`
sections).

Tags: **V** read from the instructions with the function body opened; **H** hypothesis; **O** not decoded; **E**
executed. Every address is an absolute virtual address in the original image. `P` is a `Player` record
(`0xA52E98 + slot * 0x20E4`, slot at `P+0x1C`, race `+0x20`, capital city id `+0x2C`, government `+0xA0`, techs
known `+0x1A0`, culture `+0x183C`, gold = `+0x44` plus `+0x48`). `f(this; a, b)` is an MSVC `__thiscall`; `ret N`
means the callee pops `N` bytes. `rand(n)` is the gameplay die `0x60BAB0(0xA526B4; n)` (uniform `0..n-1`, `rand(0)=0`).
All divisions are signed and truncate toward zero. The local human slot is `[0x9FD4BC]`; `[0xA526BC]` is the bit mask
of human slots; `[0xA526C0]` is the bit mask of civs in play.

Everything below that only shows text to the player (`0x61C5A0` argument setters, `vt+0x170` messages, `0x5565B0`
log lines, `0x611530` dialog waits, `0x6027C0` refresh) is **presentation** and never changes state; messages are
named in the tables so a port can reproduce them, but they are not rules.

---

## 1. Data

### 1.1 The `ESPN` table (`[0x9C40C8]`)

Nine rows, stride `0xEC`. In memory the two rule fields of row `k` are

| field | address | meaning |
|---|---|---|
| `performed_by` | `row + 0xE4`, i.e. `0xE4 + 0xEC*k` | bit 0 = a Diplomat may do it, bit 1 = a Spy may |
| `base_cost` | `row + 0xE8`, i.e. `0xE8 + 0xEC*k` | the mission's cost parameter (section 4) |

(**V**: the `base_cost` offsets are the nine load addresses in the jump table `0x5240BC` of `0x523E40`:
`0xE8, 0x1D4, 0x2C0, 0x3AC, 0x498, 0x584, 0x670, 0x75C, 0x848`; the `performed_by` offsets are the AI's tests
`0x2BC` (row 2), `0x3A8` (3), `0x494` (4), `0x580` (5), `0x66C` (6), `0x758` (7), `0x844` (8).)

Shipped `conquests.biq` (decoded dump):

| row | mission | `performed_by` | `base_cost` |
|---|---|---|---|
| 0 | Build an Embassy | 1 | 20 |
| 1 | Investigate City | 3 | 10 |
| 2 | Steal Technology | 3 | 10 |
| 3 | Steal World Map | 2 | 1 |
| 4 | Plant Spy (a "mole") | 2 | 60 |
| 5 | Steal Plans | 2 | 10 |
| 6 | Initiate Propaganda | 2 | 100 |
| 7 | Sabotage Production | 2 | 10 |
| 8 | Expose Enemy Spy | 2 | 80 |

The row number is the mission id everywhere (dispatcher jump tables `0x52684C`, `0x528D58`, `0x5240BC`).

### 1.2 `GOVT` fields (`[0x9C71D8]`, stride `0x1E8`)

| offset | meaning | shipped |
|---|---|---|
| `+0x190` | `immune_to`: a mission row, or `-1`. A civ under that government cannot be the target of that mission | Democracy `6` (Initiate Propaganda); every other government `-1` |
| `+0x194` | `diplomats_are`: experience level index of the government's Diplomats | `1` (Regular) for all eight |
| `+0x198` | `spies_are`: experience level index of its Spies | `2` (Veteran) for Communism and Fascism, `1` for the rest |
| `+0x19C` | pointer to the government's `VsGovernment` array (12-byte records: `+0` unused, `+4` `propaganda_modifier`, `+8` `resistance_modifier`), indexed by the other government | all `0` / `0..5` (resistance only) |

(**V** for the offsets by the loads in `0x524410`, `0x527FC0`, the start functions. The levels index `EXPR` rows
`0 Conscript, 1 Regular, 2 Veteran, 3 Elite`.)

### 1.3 Constant tables in the image

| address | contents | used by |
|---|---|---|
| `0x66C594` | `{100, 150, 200}` (cost percent per safety level) | quote `0x5240E0` |
| `0x66C5A0` | `{-30, 0, 10, 25}` (success modifier per experience level) | setup `0x524410` |
| `0x66C5B0` | `{-20, 0, 10}` (success modifier per safety level) | quote `0x5240E0` |

(**V**, read with `pxw`.)

### 1.4 Player fields written or read

| field | meaning | writers / readers |
|---|---|---|
| `P +0x44`, `+0x48` | the two shares of the treasury (gold = sum) | `0x5266A0` pays here (section 7) |
| `P +0xD30 + q` (byte) | at war with `q` | read everywhere |
| `P +0xD50 + q` (byte) | embassy with `q` | embassy executor via `0x56B7D0` (`diplomacy.md`) |
| `P +0xD70 + q` (byte) | **a mole of `P` sits in civ `q`** (an "espionage channel": `canTalk` ignores war and embassy when it is set, `diplomacy.md` 1 / `0x501910`) | set by Plant Spy `0x527914`; cleared by `0x502CC0` (with `clear`), by Expose Enemy Spy `0x528A05`, at elimination (`victory.md`) |
| `P +0xD90 + q` (byte) | Plant Spy **cooldown** against `q`: set by a failed attempt, cleared with probability 1/3 per turn (`diplomacy.md` section on bit `0x40`, `government.md` 7) | `0x527A1B`, read `0x5278D0`, `0x44565E` |
| `P +0xEB0 + 4q` (dword) | relation word with `q`; bit 0 = contact (every start function requires it) | `diplomacy.md` |
| `P +0x1848`, `+0x1884` | the two embedded **mission agents** of the player (section 2) | constructor `0x558C20` |

---

## 2. Mission agents

A player owns two embedded objects built by the player constructor (`0x558C20`: `0x56BBC0(P+0x1848; 1)`,
`0x56BBC0(P+0x1884; 2)`, **V**):

| object | address in `P` | `performedBy` |
|---|---|---|
| Diplomat agent | `+0x1848` | `1` |
| Spy agent | `+0x1884` | `2` |

Every executor and start function is a `__thiscall` on an agent (`ecx` = agent). Layout (**V**, `0x56BBC0`, `0x524410`):

| agent offset | player offset (diplomat / spy) | meaning |
|---|---|---|
| `+0x00` | `+0x1848` / `+0x1884` | vtable (`0x66CBF8`, then the derived tables `0x66DC54` / `0x66DC38`); slot `+0x14` is a predicate used by the AI, section 10.2 |
| `+0x08` | | the four-character chunk tag `"Espn"` (`mmioStringToFOURCC`): the agent state is a serialised record in the save |
| `+0x1C` | `+0x1864` / `+0x18A0` | the actor's slot; initialised to `-1` by the constructor, so it is stored by the player initialisation (**O**: the writer) |
| `+0x20` | | byte: the success percent of the mission in preparation (section 6) |
| `+0x24` | | mission row (0..8) |
| `+0x28` | `+0x1870` / `+0x18AC` | `performedBy` (`1` or `2`), fixed by the constructor |
| `+0x2C` | | target city id |
| `+0x30` | | target city owner slot (0 when the city does not exist) |
| `+0x34` | | the third setup argument (always `-1` in every caller) |
| `+0x38` | | the cost already computed for this mission (paid in section 7) |

The serialised record is the dword range `+0x1C .. +0x3C` (`0x4FCAD0(agent; +0x1C, +0x38 ...)`, **V** the call,
**O** the exact byte count).

A mission is therefore: choose a row and a target, `setup` the agent (fills `+0x20..+0x38`), `dispatch` (pays and
runs the executor).

---

## 3. The pipeline

```
start(row; a, b)       0x528CA0 jumps to one of nine start functions         (human menu or AI choice, section 9)
   |  target = city (capital of a civ, or a chosen city)
   |  quote or fixed chance, cost = 0x523E40
   v
setup(chance, city, -1, cost, row)         0x524410      (fills the agent, adds the experience modifier)
   v
dispatch()                                  0x5266A0      (pays the cost, then jumps to the executor of the row)
   v
executor                                    0x526870 .. 0x528800   (rolls, effect, incident)
```

`0x528CA0(row; a, b)` (`ret 0xC`, `ecx` = agent, **V**) is the start dispatcher: `row > 8` returns `false`; otherwise it
calls the start function of the row with the two stack arguments `(a, b)`. The start functions (all `ret 8`, return a
bool in `al`; start addresses): `0x524500` (0), `0x5249B0` (1), `0x524FB0` (2), `0x525150` (3), `0x5252F0` (4),
`0x525780` (5), `0x525920` (6), `0x525EA0` (7), `0x5263A0` (8). `a` is the target civ index (the AI passes the
victim, the unit's action menu passes the city owner, `0x5A5D72`) and `b` is `0` in every caller found.

When the game is networked (`0x47B530` true) and the actor's slot is a human slot, the start function does not run
the mission itself: it sends message `0x472C90(0x74AF60; chance, city, -1, cost, row, performedBy)` and the receiver
`Player::0x56AA40` (`ret 0x18`, **V**) picks the agent by `performedBy` (`1` -> `+0x1848`, `2` -> `+0x1884`) and
runs `setup(chance, city, a3, cost, row)` then `dispatch()` on every machine. In a single process the start function
calls `setup` and `dispatch` directly. The rules below do not depend on this.

---

## 4. The cost `0x523E40(this = agent; row, cityId)` (`ret 8`)

Returns `0` when `cityId` is not a live city of the pool (`[0xA52E6C]`, last `[0xA52E78]`). Otherwise, with `C` the
target city (`size = C.+0x138`, `owner = C.+0x28`, `x = C.+0x24`, `y = C.+0x26`, both signed words), `actor = agent.+0x1C`
and `E[k] = ESPN[k].base_cost`, the **core** `s` is, by row (jump table `0x5240BC`, **V**, every constant re-read):

| row | core `s` |
|---|---|
| 0 Embassy | `E[0] + size` |
| 1 Investigate | `E[1] * size` |
| 2 Steal Technology | `(Player[owner].+0x1A0 * WSIZ[worldSize].tech_rate) * E[2] / 100` |
| 3 Steal World Map | `((W + H) / 2) * E[3]` (map width `[0x9C74D4]`, height `[0x9C74C0]`) |
| 4 Plant Spy | `E[4]` |
| 5 Steal Plans | `Player[owner].+0x18C * E[5]` |
| 6 Initiate Propaganda | with `B = E[6] * size`: `s = B + goldO * (B/2) / goldA + cultO * B / cultA` |
| 7 Sabotage | `E[7] * C.+0x44` (the city's shield box) |
| 8 Expose Enemy Spy | `E[8]` |

`WSIZ` is `[0x9C7330]`, stride 84, indexed by the world size `[0x9C73A0]`; `tech_rate` is the dword at row `+8` in
memory. `Player +0x18C` is the owner's unit count. In row 6, `goldO`, `goldA` are the gold of owner and actor
(sums of `+0x44 + +0x48`), `cultO`, `cultA` their culture (`+0x183C`); `B/2` truncates toward zero, and the products
are 32-bit signed. **Division by zero**: a civ with no gold or no culture as the *actor* makes `goldA` or `cultA`
zero and the `idiv` faults (**O**: nothing in the image guards it; the AI never gets here with zero gold because of the
threshold of section 10.2, a human with zero gold would crash).

Then the **distance term**. `0x56D040(x, y, actor, -1, -1, -1, 0)` (`barbarians.md` 1.5) is called, which sets the global
`K = [0x9C34EC]` to the ordering key of the actor's nearest city to `(x, y)` (`0x7FFFFFFF` when the actor has no
city). The **size class** is

```
cls = 1                      if size <= [0x9C72E4]          (6 shipped)
cls = 2                      if [0x9C72E4] < size <= [0x9C72E8]   (12 shipped)
cls = 3                      if size > [0x9C72E8]
```

(`0x524059..0x524071`: `setg` against `[0x9C72E4]`, forced to 2 when above `[0x9C72E8]`, then `+1`; **V**.) The
sum is `t = s + cls * K` (32-bit wrap). Finally

```
N    = 0x4BB410(C; Player[actor].+0x20)        // citizens of the city whose race is the actor's race
cost = ((2*size - N + 1) * t) / (2*size + 1)
```

(**V** `0x524096..0x5240B5`; `0x4BB410` is `ret 4`.) So a city full of the actor's own people costs about `1/(2*size+1)` of
the base, and a city with none costs `(2*size+1)/(2*size+1) = 1` times `t`.

---

## 5. The quote `0x5240E0(this = agent; row, cityId, &cost, &chance, &levelHint)` (`ret 0x14`)

Used by rows 2, 3, 5, 7, 8 (rows 0, 1, 4, 6 use fixed chances, section 9). Returns the safety level `L` (0..2) or `-1`
when the player cancelled.

```
base = 0x523E40(agent; row, cityId)
c0   = 60 if agent.+0x28 == 1 else 75                 // Diplomat 60, Spy 75     (0x524142)
L    = 1
if humanMask has bit (actor slot):
    if levelHint != 0:  L = levelHint[+4]; if L < 0 return L        // preselected level
    elif actor == [0x9FD4BC]:                                          // local human: ask
        dialog "ESPIONAGE_SAFETY_LEVEL" with three buttons whose costs are base, base*3/2, base*2
        L = answer (0..2) or -1 (cancel -> return -1)
    else: return -1                                                    // a remote human: no quote
*cost   = (A[L] * base) / 100                      A = {100,150,200}      (0x66C594, 0x524175 magic /100)
chance  = c0 + B[L];  chance = clamp(chance, 0, 95)  B = {-20,0,10}      (0x66C5B0, clamp at 0x5243D7)
*chance = chance
return L
```

For every non-human actor `L = 1`, so the AI pays `1.5 * base` and starts from `c0` unmodified. The dialog texts
quote `base * 25 * {4, 6, 8} / 100`, i.e. `base`, `1.5 base`, `2 base` (**V** `0x5241B4..0x524216`).
Spy chances before the experience modifier: `55 / 75 / 85`; Diplomat: `40 / 60 / 70`.

---

## 6. The setup `0x524410(this = agent; chance, cityId, a3, cost, row)` (`ret 0x14`)

```
m = 0
if agent.+0x28 == 1:  m = M[ GOVT[ P(actor).government ].+0x194 ]       // M = {-30,0,10,25} at 0x66C5A0
if agent.+0x28 == 2:  m = M[ GOVT[ P(actor).government ].+0x198 ]
else-branch (any other performedBy): m = chance                          // 0x52448B; unreachable in practice
agent.+0x20 = clamp(chance + m, 0, 100)                                   // byte
agent.+0x2C = cityId
agent.+0x30 = owner of city cityId, or 0 if the city does not exist     // the first byte of the city at +0x28
agent.+0x34 = a3
agent.+0x38 = cost
agent.+0x24 = row
```

(**V**, whole body re-read with the light filter; the `performedBy == 2` arm is the first one in the code and uses
`+0x198`.) The experience modifier is applied **after** the quote's 95 cap, so the final percent can reach 100.
Shipped: a Veteran spy (Communism, Fascism) adds `10`; every other government adds `0`.

---

## 7. The dispatcher `0x5266A0(this = agent)` (no stack arguments)

```
city = pool[agent.+0x2C]                                   // must exist, else return (0x5266B2..0x5266D2)
gold = P(actor).+0x44 + P(actor).+0x48
if gold < agent.+0x38:                                     // cannot afford
    if actor == [0x9FD4BC]: message NOTENOUGHGOLD
    return
gold -= agent.+0x38                                        // the cost is paid BEFORE any roll, success or not
if gold < 0: gold = 0                                      // cannot happen after the test; the code clamps anyway
r = (gold == 0) ? timeGetTime() % 0xD431 - 0x8235 : timeGetTime() % gold - 0x3039
P.+0x44 = r ;  P.+0x48 = gold - r                          // the treasury is re-split with a wall-clock mask
jump table 0x52684C on agent.+0x24 (0..8 -> executors; > 8 returns):
    0 -> 0x526870   1 -> 0x526C00   2 -> 0x526DA0   3 -> 0x527470   4 -> 0x527870
    5 -> 0x527BB0   6 -> 0x527FC0   7 -> 0x5283E0   8 -> 0x528800
then refresh the UI (0x6027C0(0x9FDB50))
```

(**V**.) The split does not change the gold total (`+0x44 + +0x48 = gold` always), only the representation, which makes
a saved game or a memory edit harder to forge. A port that stores gold as one integer may drop the `timeGetTime()`
mask; **the total deducted is exactly `agent.+0x38`**. The unit that carried out the mission is **not** consumed
here (section 12, open item 1).

---

## 8. Conventions shared by the executors

All executors begin by resolving the target city `C = pool[agent.+0x2C]` and return without effect when it does not
exist. `actor = agent.+0x1C`, `victim = C.+0x28`, `chance = agent.+0x20`, `isSpy = (agent.+0x28 == 2)`.

**The incident tail** `Player[victim].0x502CC0(actor, isSpy)` (`ret 8`; `government.md` 5.1): adds 1 to the victim's incident
counter about `actor` (record offset `+0x10`), and **when `isSpy`** zeroes the actor's mole byte `Player[actor].+0xD70[victim]`
(a captured Spy loses the mole it had planted in the victim's country, a captured Diplomat does not). When the
victim is a computer player it then asks `wantsWar(victim; actor)` (`diplomacy.md` 6, `0x440B60`) and declares war with
reason 0 if that holds. The call is made in the executors `0x526DA0`, `0x527470`, `0x527870`, `0x527BB0`, `0x5283E0`,
`0x528800` only, never in Embassy, Investigate or Propaganda.

Three templates occur:

| template | missions | draws |
|---|---|---|
| **T1, one roll, silent failure** | Embassy (0), Investigate (1) | `r1 = rand(100)`; success iff `r1 < chance`; failure does nothing |
| **T2, theft** | Steal Technology (2), Steal Map (3) | `r1 = rand(100)`; if `r1 >= chance`: **incident**. Else `r2 = rand(100)`; if `r2 >= 80`: failure message, **no incident**; else the effect |
| **T3, exposure** | Steal Plans (5), Sabotage (7), Expose Enemy Spy (8) | `r1 = rand(100)`; success iff `r1 < chance` (and the mission's precondition); on success the effect, then `r2 = rand(100)`; `exposed = r2 < X` with `X = 10 / 20 / 10`. On failure `exposed = true` without a draw. If `exposed` the **incident** is raised, otherwise the victim (only if human) gets a "missed" message |

Plant Spy (4) is T1-like with a cooldown (section 9.5); Initiate Propaganda (6) has its own per-citizen draws
(section 9.7).

The number of draws is observable (the die is shared), so the order matters: the roll of `r1` precedes everything,
`r2` is drawn only on the path stated, and Plant Spy draws one die, Embassy / Investigate one, Propaganda one per
citizen.

---

## 9. The nine missions

### 9.1 Build an Embassy (row 0)

*Start* `0x524500(a, b)` (**V**). The chance is the constant `100` (no quote). The target city is the capital of the
chosen civ `c` (`Player[c].+0x2C`). `a` is the civ; when `a == actor == [0x9FD4BC]` (a human asking for the menu) the
menu lists every civ `c` with all of: `c != actor`, bit `c` of `[0xA526C0]`, `Player[c].+0x2C != -1`, contact
(`P.+0xEB0 + 4c` bit 0), **not at war** (`P.+0xD30[c] == 0`), **no embassy yet** (`P.+0xD50[c] == 0`) and
`GOVT[Player[c].government].immune_to != 0`; the entry shows `0x523E40(0, capital)`. An empty list shows
`ESPIONAGE_NO_EMBASSIES_AT_THIS_TIME` and the function returns false. Otherwise `cost = 0x523E40(0, capital of a)`,
`setup(100, capital, -1, cost, 0)`, `dispatch()`.

*Executor* `0x526870` (T1): `r1 = rand(100)`; if `r1 < chance` then `Player[actor].0x56B7D0(victim)` **and**
`Player[victim].0x56B7D0(actor)` (`diplomacy.md` 2: stores the embassy byte `+0xD50` and refreshes the city pool): the
embassy is **mutual**. Failure has no effect and no incident. Final chance `= clamp(100 + M, 0, 100)`: a Diplomat of
government level `Regular` (all shipped governments) always succeeds.
Messages: `ESPIONAGE_MISSION_ESTABLISH_EMBASSY_SUCCESS` (actor), `ESPIONAGE_FOREIGN_EMBASSY_ESTABLISHED` (victim).

### 9.2 Investigate City (row 1)

*Start* `0x5249B0`: for a chosen city; a human whose target's government has `immune_to == 1` gets
`ESPIONAGE_MISSION_INVESTIGATE_CITY_IMMUNE` and the function ends. Chance constant `100`, `cost = 0x523E40(1, city)`,
`setup(100, city, -1, cost, 1)`, `dispatch()`.

*Executor* `0x526C00` (T1): `r1 = rand(100) < chance`, then for `k = 0..20` the tile at spiral offset `spiral(k)` around the
city (the 21-tile fat cross, `NOTES.md` 8) is normalised for wrapping (`0x426C00` for x, `0x426C40` for y) and, if it
lies on the map, `Player[actor].0x55B1A0(x, y)` reveals it (`discover`, `research.md` 10.3). A local human then gets
`ESPIONAGE_MISSION_INVESTIGATE_CITY_SUCCESS` and the city screen opens (`0x4ACD20`). Failure: nothing.

### 9.3 Steal Technology (row 2)

*Start* `0x524FB0(a, b)`: the target is civ `a`, the city its capital. A **human** actor first tests
`GOVT[Player[a].government].immune_to == 2` (message `ESPIONAGE_MISSION_STEAL_TECH_IMMUNE`, return false); a
computer actor skips the test (the AI's own gate, section 10.1, applies it). Then the quote of section 5
(`0x5240E0(2, capital, &cost, &chance, b)`), a negative result returns false, `setup(chance, capital, -1, cost, 2)`,
`dispatch()`.

*Executor* `0x526DA0` (T2, **V**):

```
r1 = rand(100);  if r1 >= chance:  incident path (below)
r2 = rand(100);  if r2 >= 80:      failure path "no luck": message ..._STEAL_TECH_FAILURE (local human actor), nothing else
candidate exists iff  some tech t in 0..[0x9C3DBC)-1 has  Player[victim].0x561440(t)  (victim knows t)
                       and not Player[actor].0x561440(t)  and  TECH[t].+0x48 != -1   (era is not "none")
if no candidate:  message ESPIONAGE_MISSION_STEAL_TECH_UNABLE (human actor); end
tech = computer actor : Player[actor].vt+0x64(victim)  (= 0x44A5B0, research.md 10.6; -1 -> end)
       human actor    : the steal-technology screen 0x49D070, with the globals [0xC94494]=actor,
                        [0xC94470]=victim, [0xC92FD0]=city, and the victim's era (Player.+0xF4,
                        research.md 1.3) temporarily replaced by the actor's era around the call (saved in ebx,
                        restored at 0x5270B7)                                    (UI; the pick is the player's)
Player[actor].0x561860(tech, 0, 1, 1)   (acquire; or, in a networked game, message 0x475460)
```

*Incident path* (`0x52727A`): `Player[victim].0x502CC0(actor, isSpy)`; messages `..._STEAL_TECH_FAILURE_CAUGHT` (human
actor) and `ESPIONAGE_CAUGHT_STEALING_TECH` (human victim). There is **no incident on a successful theft**.
The cost parameter `(techs known by the victim) * tech_rate * 10 / 100` grows with the victim's progress and with the
world size (shipped tech rates `160, 200, 240, 320, ...` for tiny, small, standard, large, ...).

### 9.4 Steal World Map (row 3)

*Start* `0x525150`: as 9.3 with `immune_to == 3` and row 3 (capital as the city, quote).

*Executor* `0x527470` (T2): `r1 >= chance` -> incident (`..._STEAL_WORLD_MAP_FAILURE_CAUGHT`, victim
`ESPIONAGE_CAUGHT_STEALING_MAP`); `r2 >= 80` -> `..._STEAL_WORLD_MAP_FAILURE`; else
`Player[victim].0x55B3A0(actor, 0)` and, for a human actor, `..._STEAL_WORLD_MAP_SUCCESS`.

`0x55B3A0(this = giver; taker, flag)` (`ret 8`, **V**) walks every map cell index `i = 0 .. cellCount-1`
(`cellCount = (W/2) * H`), derives `row = i / (W/2)`, `x = 2*(i % (W/2)) + (row & 1)`, `y = row`, and with `flag == 0`
calls `Player[taker].0x55B1A0(x, y)` (discover) for each cell whose seen-by mask (`cell +0x58`) has the giver's slot
bit; with `flag != 0` it uses the cells whose **owner** (`cell.vt+0x98`) is the giver. At the end it sets the redraw flag
`[0xA281C5] = 1`. So the thief learns **every tile the victim has ever seen** (not the tiles the victim has under
vision now) and nothing is taken from the victim.

### 9.5 Plant Spy (row 4)

*Start* `0x5252F0`: the human menu lists civs `c` with: in play, `Player[c].+0x2C != -1`, contact, **no mole yet**
(`P.+0xD70[c] == 0`), `immune_to != 4`; there is no war test. The chance passed to `setup` is the constant **50**
(no quote); `cost = 0x523E40(4, capital of c)` (= `60 + 2*K`-type terms only: the core is `E[4] = 60`).

*Executor* `0x527870` (one die, with the cooldown):

```
if P(actor).+0xD90[victim] != 0:   go to FAIL                                  // 0x5278D0: a recent failure blocks the attempt
r = rand(100);  if r >= chance:    go to FAIL
P(actor).+0xD70[victim] = 1                                                    // 0x527914: the mole exists from now on
Player[actor].0x55B1A0(capital.x, capital.y)    // reveals the victim's capital tile (the code resolves Player[victim].+0x2C)
local human actor: message ESPIONAGE_MISSION_PLANT_MOLE_SUCCESS
FAIL: Player[victim].0x502CC0(actor, isSpy);  P(actor).+0xD90[victim] = 1       // incident AND cooldown latch
      messages ..._PLANT_MOLE_FAILURE (human actor), ESPIONAGE_CAUGHT_MOLE (human victim)
```

(The cooldown byte is cleared with probability 1/3 per turn for each civ met, `government.md` 7 step 4.) Because
`isSpy` is true for the only agent allowed to plant (`performed_by = 2`), a failure also zeroes the mole byte
`+0xD70[victim]`, which cannot be set at that moment anyway (the menu forbids a second mole). A **mole** lets the
actor's computer logic go on to the other spy missions against that civ (section 10.2) and keeps `canTalk` true even
at war (`diplomacy.md` 1).

### 9.6 Steal Plans (row 5)

*Start* `0x525780`: as 9.3 with `immune_to == 5`, row 5, quote, the capital as the city.

*Executor* `0x527BB0` (T3, `X = 10`): `r1 < chance` -> success; the **only game-state effect is a bit in the relation
word**: `Player[actor].+0xEB0 + 4*victim |= 0x40` (`0x527EE3`); in a networked game with the actor or the victim human the
bit is set by message `0x475670(0x74AF60; actor, victim, 0x40, 0)` on every machine instead (`0x527DE1..0x527DF1`). The
meaning of bit `0x40` is the one in `diplomacy.md` (the relation-word bit table: cleared at every turn update for each
civ met, `0x560D80`); what **reads** the bit is open (**O**, listed in section 12). After success `r2 = rand(100) < 10`
-> exposed. On failure: exposed. Exposed: victim message `ESPIONAGE_CAUGHT_STEALING_PLANS[_SUCCESS]` and the incident
tail; not exposed (success path only): victim message `ESPIONAGE_MISSED_STEALING_PLANS`.

### 9.7 Initiate Propaganda (row 6)

*Start* `0x525920`. A human first picks a civ and then a city; the list (`0x525A79..0x525CB0`, **V**) contains each city whose owner
is the chosen civ, is not the actor, whose tile is **discovered** by the actor (the cell's seen-mask `+0x58` has the
actor's bit), and whose government is not `immune_to == 6` (shipped: Democracy is immune). An empty list shows
`ESPIONAGE_MISSION_INITIATE_PROPAGANDA_IMMUNE`. A computer actor takes `Player[actor].vt+0x68(target civ)` = `0x44A630`
(section 10.3). The cost is `0x523E40(6, city)`.

The **chance** passed to `setup` is *not* a constant: with `cultA`, `cultO` the cultures of the actor and the city owner,

```
ratio = (int)( cultA * 100.0f / cultO + 0.5f )                 // x87: fild, fmul 100.0f, fidiv, fadd 0.5f, _ftol  (0x4F8C50)
k     = the CULT row with the greatest culture_ratio_percent <= ratio  (ties: the first); none -> [0x9C3D6C] if >= 0 else 0
chance = CULT[k].propaganda_success_percent                    // table [0x9C40BC], stride 92, field at +0x44
```

`0x4F8C50(this = 0xA2A7E8; cultA, cultO)` is `ret 8` (two stack arguments; **correction**: `city-turn.md` 8.3 says it takes
three, the third being whatever `ebx` held; the function never reads `ebx`). With `cultO = 0` the division yields
infinity, `_ftol` returns `0x80000000`, below every row, so the fallback row is used (**H**: FPU exceptions masked, which
is the CRT default). Shipped rows (`ratio` threshold -> success percent): `300 -> 30`, `200 -> 25`, `100 -> 20`,
`75 -> 10`, `50 -> 5`, `33 -> 3`. Then `setup(chance, city, -1, cost, 6)`, `dispatch()`. Note that `setup` adds the
experience modifier and clamps to `0..100`.

*Executor* `0x527FC0` (no `r1`; **V**, the whole body). With `C` the city:

```
f    = 0x5A6060(C.x, C.y, 4, -1, 0, -1)                 // units on the city tile counted by mode 4 (happiness.md martial law)
base = -5 * f
if C.+0x20 == Player[victim].+0x2C:   base -= 40        // the owner's capital
if C.countBuildingsWithFlag(0x80) != 0: base -= 20      // improvement flag 0x80 = "Resistant to Propaganda" (Courthouse)
if C.+0x30 & 2:  base -= 10                             // celebrating
if C.+0x30 & 1:  base += 10                             // in disorder
base += GOVT[Player[actor].government].vs[ Player[victim].government ].propaganda_modifier
n = 0
for every citizen u of the city (pool at C+0xDC, indices 0..C.+0xEC inclusive, empty slots skipped):
    p = chance + base
    if u.+0x140 == Player[actor].+0x20:  p += 20        // a citizen already of the actor's race
    p = clamp(p, 0, 95)
    if rand(100) < p:  n += 1
pct = (n * 100) / C.+0x138                              // citizens persuaded, in percent of the population
if pct >= 75:
    Player[actor].0x563410(C, 1, 1)                     // the city changes hands (capture.md)
    if C.+0x28 != actor:                                // the transfer was refused
        C.+0x1C0 = max(C.+0x1C0, n);  0x4BCFF0(C)       // propaganda unhappiness, refresh
    local-human actor: ESPIONAGE_MISSION_INITIATE_PROPAGANDA_SUCCESS ; human old owner: ESPIONAGE_MISSED_PROPAGANDA_LOST_CITY
else:
    C.+0x1C0 = max(C.+0x1C0, n);  0x4BCFF0(C)
    local-human actor: ESPIONAGE_MISSION_INITIATE_PROPAGANDA_FAILURE
```

The propaganda unhappiness `+0x1C0` is counted as faces in the happiness pass and decays by 1 per city turn
(`happiness.md`, `city-turn.md` 2). **No incident** is raised by this mission and no second roll exists. The old
`happiness.md` note that `0x527FC0` was "a score of a mission that also writes `[city+0x1C0]`" is thereby resolved.

### 9.8 Sabotage Production (row 7)

*Start* `0x525EA0`: a human gets a list of the target civ's cities that the actor has discovered, skipping governments
with `immune_to == 7`; a computer actor takes `Player[actor].vt+0x6C(target civ)` = `0x44A800` (section 10.4), which returns
`-1` when nothing qualifies. Quote with row 7, `cost` core `10 * (shield box of the city)`.

*Executor* `0x5283E0` (T3, `X = 20`): success iff `rand(100) < chance`. The effect:
`C.+0x44 = min(C.+0x44 / 2, 0x4ACD70(C))` where `0x4ACD70` is the cost of the item in production (the shield box is halved
and never left above the item's cost). Then `rand(100) < 20` -> exposed. Failure is exposed. Messages
`ESPIONAGE_MISSION_SABOTAGE_PRODUCTION_SUCCESS` / `..._FAILURE`, `ESPIONAGE_CAUGHT_SABOTAGING[_SUCCESS]`,
`ESPIONAGE_MISSED_SABOTAGING`.

### 9.9 Expose Enemy Spy (row 8)

*Start* `0x5263A0`: civ `a`, the city its capital, `immune_to == 8` test, quote with row 8, `cost` core `E[8] = 80`.

*Executor* `0x528800` (T3, `X = 10`). Reading the owner `V = C.+0x28`: the precondition is that **V has a mole in the actor's
country**: `Player[V].+0xD70[actor] != 0` (`0x528956`). `success = (rand(100) < chance) and precondition` (the die is
drawn first, so it is drawn even when the precondition fails). On success the mole is removed:
`Player[V].+0xD70[actor] = 0` (`0x528A05`, executed whether or not the exposure roll follows) and `rand(100) < 10`
exposes the actor. Failure is exposed. Exposed -> incident (`Player[V].0x502CC0(actor, isSpy)` also zeroes
`Player[actor].+0xD70[V]`, **the actor's own mole in V's country**), victim message
`ESPIONAGE_CAUGHT_EXPOSING_MOLE[_SUCCESS]`; not exposed: `ESPIONAGE_MISSED_EXPOSING_MOLE`. Messages to the actor:
`ESPIONAGE_MISSION_EXPOSE_MOLE_SUCCESS` / `_FAILURE`.

---

## 10. The computer player

### 10.1 The per-turn driver `0x445490` (Player vtable `+0x30`, `turn.md` step 15)

Called for every non-human player once per turn. With `P = this` and `ctrl(c)` meaning "`c` is a human slot":

```
thr  = P.+0x194 + (X > 0 ? 1000 / X : 0),  X = 0x55AA10(P; 8, 0)     // recomputed before each use; P.+0x194 = city count
gold = P.+0x44 + P.+0x48
for c = 1 .. 31:                       // ebx = c; the loop ends when the cursor Player[c].+0x1C reaches 0xA94B34 = Player[32].+0x1C
    if not (bit c of [0xA526C0] and bit 0 of P.+0xEB0 + 4c):  continue        // in play, contact
    # A + B: the diplomat. Every failed test of A jumps to 0x4455EF, which skips B as well
    if P.+0xD30[c] == 0 and diplomat.vt+0x14() and gold > thr:
        if P.+0xD50[c] == 0 and Player[c].+0x2C != -1 and (ESPN[0].performed_by & diplomat.+0x28):
            if not ctrl(c) or rand(0x40) == 0:  mission(0; c, 0)                  // 0x528CA0; no immunity test
        if ctrl(c) and P.+0xD50[c] != 0 and P.+0xD30[c] == 0 and gold > thr:      // re-read after the embassy attempt
            tryEspionage(diplomat, c)                                              // 0x445160
    # C: the spy works only against humans (the human-mask test is the first instruction at 0x4455EF)
    if ctrl(c) and spy.vt+0x14() and gold > thr:
        if P.+0xD70[c] == 0 and P.+0xD90[c] == 0 and Player[c].+0x2C != -1 and (ESPN[4].performed_by & spy.+0x28):
            if rand(P.+0xD30[c] != 0 ? 4 : 16) == 0:  mission(4; c, 0)            // plant a mole
        if P.+0xD70[c] != 0 and gold > thr:  tryEspionage(spy, c)                  // also right after a successful plant
```

(**V**, re-read raw at `0x4454B1..0x445716` for this revision. The dice of C: `neg al; sbb eax, eax; and al, 0xF4; add eax, 0x10`
yields `rand(4)` at war and `rand(16)` otherwise. `gold` is `+0x44 + +0x48`; `ctrl(c)` is bit c of the human mask `[0xA526BC]`.
The performed-by words are the agent's own `+0x28`: `P.+0x1870` for the diplomat, `P.+0x18AC` for the spy.)
Two quantities are **not decoded**: the predicate `agent.vt+0x14()` (**O**; **H**: the agent has a usable Diplomat or
Spy unit available) and `X = 0x55AA10(P; 8, 0)` (**O**; a generic per-player table sum used by 38 other callers; **H**: a
count of the player's improvements with a flag, so the threshold is `cities + 1000/X` gold). Everything else is
exactly as written. The embassy attempt therefore happens **with certainty** every turn the conditions hold when the
other civ is a computer player, and with probability 1/64 per turn when it is a human.

### 10.2 `tryEspionage(this = P; agent, q)` `0x445160` (`ret 8`)

```
V = {200000, 20000, 2000, 200, 20, 0}[ attitudeClass(P; q, 0) ]   // vt+0x88; class > 4 -> 0   (jump table 0x445448)
if P.+0xD30[q] != 0 or P.rec(q).+0x10 > 0:   V = V / 2           // at war, or q has committed hostile acts against P
r = rand(V)                                                       // rand(0) = 0
if r > 9: return
mission by r  (jump table 0x44545C):
   r in 0..1 : Steal Technology (2):   needs (ESPN[2].performed_by & agent.+0x28), GOVT[q].immune_to != 2,
                                       Player[q].+0x1A0 > P.+0x1A0 (q knows more techs)  -> 0x528CA0(2; q, 0)
   r = 2     : Steal World Map (3):    performed_by(3), immune_to != 3, P.vt+0x5C(q, 0) > 0   -> 0x528CA0(3; q, 0)
   r = 3     : Steal Plans (5):        performed_by(5), immune_to != 5                         -> 0x528CA0(5; q, 0)
   r in 4..5 : Initiate Propaganda (6):performed_by(6), immune_to != 6                         -> 0x528CA0(6; q, 0)
   r in 6..7 : Sabotage (7):           performed_by(7), immune_to != 7                         -> 0x528CA0(7; q, 0)
   r in 8..9 : Expose Enemy Spy (8):   performed_by(8), immune_to != 8, Player[q].+0xD70[P.slot] != 0 (q has a mole in P)  -> 0x528CA0(8; q, 0)
```

(**V**: the base values `0x30D40 = 200000`, `0x4E20`, `0x7D0`, `0xC8`, `0x14` are the five immediates at `0x445180..0x44519C`.
`attitudeClass` is the method of `diplomacy.md` 6 where class `0` is the friendliest and `4` the most hostile; at the most hostile
class the chance of trying a mission in a call is `10/20 = 50%`, at the friendliest `10/200000`. The method `vt+0x5C` is **O**.)
Mission ids 0 (Embassy), 1 (Investigate) and 4 (Plant Spy) are never chosen here; 0 and 4 come from the driver
(10.1), and Investigate City is never started by the computer player.

### 10.3 The propaganda target `0x44A630(this = P; q)` (Player vtable `+0x68`)

Over the city pool: among cities owned by `q`, choose the one with the greatest `key = size + 0x4BB410(city; P.+0x20)` (population
plus the citizens of the actor's race; strictly greater replaces, so the first of equals wins). The function also
computes several other terms (distance key `0x56D040`, the unit count `0x5A6060(x, y, 4, -1, 0, -1)`, the capital halving, the
Courthouse and mood factors) whose results are **never compared**: they are dead code in this routine (**V**: the only
comparison is `cmp edi, [esp+0x10]` with `edi = size + N`). Returns the city (the caller reads its id).

### 10.4 The sabotage target `0x44A800(this; q)` (Player vtable `+0x6C`, `ret 4`)

Over cities owned by `q`: `score = city.+0x44` (shield box); only when the production kind `city.+0x50 == 1` (a building, with id
`city.+0x4C`) the score is multiplied by 16 if the building's `spaceship_part` field (memory `+0xD8`) is not `-1`, and
by 4 more if its `other_characteristics` (memory `+0xF0`) has bit 2 (`Wonder`). A strictly greater score replaces the
best (initial best 0). Returns the best city's **id** (`city.+0x20`) or `-1` when every score is `0` or less.

### 10.5 The technology pick `0x44A5B0(this; q)` (Player vtable `+0x64`)

Specified in `research.md` 10.6.

---

## 11. Golden vectors (derived by hand from the text; inputs are shipped values or stated)

| # | input | result |
|---|---|---|
| G1 | Steal Technology core: victim knows 20 techs, standard world (`tech_rate 240`), `E[2] = 10` | `20*240*10/100 = 480` before the distance term |
| G2 | Embassy to a capital of size 12 (class 2: `12 > 6`, `12 <= 12`), nearest actor city key `K = 4`, `N = 2` | `t = (20+12) + 2*4 = 40`; `cost = (24-2+1)*40/(25) = 920/25 = 36` |
| G3 | Sabotage on a size-8 city (class 2), shield box 40, `K = 5`, `N = 3` | `s = 10*40 = 400`; `t = 410`; `cost = (16-3+1)*410/17 = 5740/17 = 337` |
| G4 | Quote on G3 for a Spy at level 1 | `cost = 150*337/100 = 505`; `chance = clamp(75+0) = 75`; a Veteran-spy government adds 10 in setup: 85 |
| G5 | Quote for a Diplomat, levels 0/1/2 | chance `40 / 60 / 70` |
| G6 | Treasury 1000, mission cost 150 | gold becomes `850`; `P.+0x44 = timeGetTime() % 850 - 0x3039`, `P.+0x48 = 850 - P.+0x44` |
| G7 | Cultures actor 300, owner 100 | `ratio = 300` -> row `300` -> propaganda base chance 30 |
| G8 | Cultures 100 and 300 | `ratio = (int)(33.33 + 0.5) = 33` -> row `33` -> 3 |
| G9 | Cultures 100 and 100 | `ratio = 100` -> row `100` -> 20 |
| G10 | Cultures 60 and 100 | `ratio = 60` -> greatest row `<= 60` is `50` -> 5 |
| G11 | Propaganda: chance 60, two units on the tile, not the capital, no Courthouse, disorder, no government modifier, citizen of the actor's race | `p = 60 - 10 + 10 + 20 = 80`; of another race `60` |
| G12 | Propaganda: city of 8 citizens, 6 persuaded | `pct = 600/8 = 75` -> `>= 75`: the city changes hands |
| G13 | Propaganda: 5 of 8 persuaded | `pct = 62` -> `C.+0x1C0 = max(old, 5)` |
| G14 | AI try: class 4 at peace, no incidents | `V = 20`; a draw `0..9` out of `20` -> mission by the table; `r in 10..19` -> nothing |
| G15 | AI try at war with class 2 | `V = 2000/2 = 1000`; chance of acting `10/1000 = 1%` |
| G16 | Sabotage success on shield box 61 with an item costing 100 | `min(61/2, 100) = 30` |
| G17 | Sabotage success on shield box 61, item costs 20 | `min(30, 20) = 20` |

---

## 12. Quirks and open items

**Quirks (original behaviour a port should reproduce or knowingly diverge from)**

1. The cost is paid before the roll and is never refunded; a failed mission still costs gold (section 7).
2. Embassies are mutual: the target also receives an embassy with the actor (9.1).
3. Steal Plans has **no immediate effect other than setting relation-word bit `0x40`**; it is the only record of the mission (9.6).
4. Steal Technology and Steal Map: a mission that passes the first roll can still fail without an incident (`r2 >= 80`, 20%).
5. Initiate Propaganda raises **no** incident and never reveals the actor; a failed attempt still leaves `n` faces of unhappiness.
6. Initiate Propaganda divides by the actor's gold and culture in the **cost** (zero faults); and by the owner's culture in the
   **chance** (zero gives the fallback row).
7. The immunity test of Steal Technology, Steal Map, Steal Plans etc. is made only for a **human** actor in the start
   functions (and for computer actors in `0x445160`, rows 2, 3, 5, 6, 7, 8); the driver's own Plant Spy and Embassy
   attempts do not test `immune_to` (Plant Spy: `0x5252F0` tests it in the human menu only).
8. `0x44A630` computes a score and then ignores it (10.3).
9. The computer player never starts Investigate City (row 1): the driver starts only rows 0 and 4, and `0x445160` only
   rows 2, 3, 5, 6, 7 and 8 (10.1, 10.2).
10. A captured Spy loses the mole it had planted; a Diplomat does not (8, `0x502CC0` flag).
11. With `K = 0x7FFFFFFF` (the actor owns no city) the distance term overflows (`cls * 0x7FFFFFFF` wraps), so costs become
    negative or huge; reachable only if the actor has no city.
12. `agent.+0x34` (`a3`) is `-1` in every call found and is never read.

**Open items**

1. **What happens to the Diplomat or Spy unit after a mission** (consumed on success, may "escape" with a probability,
   moved back to a city): the executors above never touch the unit. The caller is the unit's action code, reached through
   `0x5A5680` (`0x5A5D72` is the call into `0x528CA0`) and `0x503A10` / `0x52B170` (the menus); not read.
2. The human menus (`0x503A10`, `0x528F20`, the city-list builders) beyond the eligibility predicates listed above.
3. `agent.vt+0x14` and `0x55AA10(P; 8, 0)` (10.1); `P.vt+0x5C` and `P.vt+0x88` internals (the latter is `diplomacy.md` 6).
4. The reader of relation-word bit `0x40` (Steal Plans effect), and the `Espn` record's byte layout in the save.
5. The writer of `agent.+0x1C` (the actor slot) at game start and load.
6. The steal-technology screen `0x49D070` (UI): which technologies it offers is, by the scan above, exactly the
   candidates `{t : victim knows t, actor does not, era(t) != none}`; the temporary swap of the victim's era
   (`Player.+0xF4`, `research.md`) for the actor's around the call (`0x527074..0x5270B7`, **V**) is presumably a display
   device (**H**); not decoded.
7. The network message `0x472C90` / `0x56AA40` wire format (`multiplayer.md` is a lead only).
8. The Intelligence Agency wonder's link to `performed_by` / `allows spy missions` (`wonder_flags` bit 7): the building flag is
   not tested by any function in `0x523E40..0x528FFF` (**V** by absence of a table read of `[0x9C40AC]` there); whether it
   gates the Spy unit's availability is therefore elsewhere (probably the agent predicate, item 3).
