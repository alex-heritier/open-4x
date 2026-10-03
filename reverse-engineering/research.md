# Research: beakers, tech cost, acquiring a technology, eras, the Great Library, the Science Age

Standalone specification, recovered from the raw disassembly of `Civ3Conquests.exe` (PE32, MSVC 6,
image base `0x400000`). The goal of this file is that a team that never sees the binary can
reimplement the whole research system and get the same game: the same turn on which each advance
arrives, the same side effects in the same order, and the same random draws. Every number, offset
and branch below is read from the code; the evidence tag says how.

Companions: `primitives.md` (containers, `knowsTech`, the reach set, the small tests this file calls),
`research-ai.md` (how the AI values and picks advances: the valuation `0x448BF0`, the default picker,
the category masks), `yields.md` section 5 (where a city's science comes from), `combat.md` section 1
(the shared random generator), `diplomacy.md` (contact, war and peace, deals and tech trading), `turn.md`
(the rest of the round), `goody-huts.md` (the hut resolver).

Reference implementation: `rust/src/research.rs` (`World`: the cost, the step, `acquire`, eras, the queue,
`hut_advance`) and `rust/src/research_ai.rs` (`Valuer`), both with tests; the playable game wraps them in
`src/research.rs` (beakers, the choose-research dialog, the Science Advisor) and prices trades with the
valuation (`src/diplomacy.rs`).

## 0. Conventions

* `f(this; a, b)` is a `__thiscall`: `this` in `ecx`, the stack arguments listed after the semicolon in
  call order; `ret N` means N/4 arguments. `P` is a Player record, `C` a city, `T` the number of
  advances (`[0x9C3DBC]`, 83 in the shipped Conquests rules). "slot" is a player number 0..31.
* `tdiv(a, b)` is C integer division (truncates toward zero). `ftol(x)` is truncation toward zero
  (`0x64A230`). All integers are 32-bit signed unless stated.
* `rand(n)` is the shared generator call `0x60BAB0(0xA526B4; n)`: a value `0 <= r < n` (taken as an
  unsigned 16-bit word). Its algorithm is `combat.md` section 1. Every draw this file makes is listed
  in section 14 in the order it happens, because the draw order is part of the game state.
* Evidence tags: **A** decoded from the raw disassembly (addresses given). **E** additionally executed:
  the exact bytes of the function were run in a CPU emulator against a synthetic world and compared
  with an independent re-implementation (at least 15 000 random cases and 32 curated ones), and the
  golden vectors of section 4.4 are outputs of that run. **H** hypothesis, never relied on.
* A pool lookup `pool.get(i)` is `primitives.md` section 1.2; "the cities of P" is every city in the
  city pool whose owner byte (`+0x28`) is `P.slot`, visited in ascending pool index.
* "UI only" marks a branch that only draws a dialog, plays a sound or posts a message and changes no
  rule state. A port may omit it. The multiplayer branches (`0x47B530` true: send a network message
  and let the message handler perform the action on every machine) are not specified; a single-process
  port calls the single-machine branch.

## 1. Data model

### 1.1 The TECH row (`[0x9C7320]`, stride `0x74`, count `T`)

Memory offsets (file body offset plus 4; `biq/src/sections/tech.rs` lists both).

| offset | field | used here |
|---|---|---|
| `+0x04` | name (32 bytes) | messages |
| `+0x44` | `cost` | base cost, score (4.1, 6) |
| `+0x48` | `era` (`-1` = none) | eligibility, era grants (7) |
| `+0x58 .. +0x64` | four prerequisites, `-1` = none | reachability, research order |
| `+0x68` | `flags` | table below |
| `+0x6C` | flavor mask | `research-ai.md` |

Flag bits read in this file (`biq::sections::tech::flags` names them all):

| mask | name | consumer here |
|---|---|---|
| `0x1` | Enables Diplomats (Writing) | tutorial popup in acquire (6, step 14) |
| `0x2000` | Trade over Sea Tiles | trade network rebuild (6, step 7); `knowsTechWithFlags` |
| `0x4000` | Trade over Ocean Tiles | same |
| `0x20000` | Not Required for Era Advancement | era predicate (7.1) |
| `0x200000` | Bonus Tech (Philosophy) | first-discoverer free tech (6, step 9) |
| `0x400000` | Reveal Map (Satellites) | acquire step 15 |

Era numbers are `0..3` (Ancient, Middle, Industrial, Modern). A row with `era == -1` is not part of
the tree: nobody can research it and it never blocks an era (7.1).

### 1.2 Who knows what

`[0xA52B4C]` points to `T` dwords; **bit `p` of dword `t` is set when the player in slot `p` knows
advance `t`**. `knowsTech(P, t)` is `primitives.md` 2.1: `t == -1` true, `t == T` false, otherwise the
bit. All writes to knowledge in this file are `known[t] |= 1 << P.slot` (acquire step 2). Knowledge is
never removed.

### 1.3 Player fields

| offset | meaning |
|---|---|
| `+0x1C` | own slot |
| `+0x20` | civilization (RACE index) |
| `+0x2C` | capital city pool index, `-1` none |
| `+0x30` | this player's difficulty level: `LEAD[slot-1].+0x40` when that is not `-2`, otherwise `[0xA52684]` for a human slot and `[0x9C7288]` for an AI slot (`Player::init`, 7.5). **The cost code does not read it**; it reads `[0xA52684]` directly (4.1) |
| `+0x3C` | Golden Age end turn, `-1` none |
| `+0x44`, `+0x48` | treasury, stored as two cells whose **sum** is the gold (an anti-tamper split; section 3.2) |
| `+0x9C` | anarchy turns left |
| `+0xA0` | government |
| `+0xAC` | pending-notice flag (byte): a tech arrived from other civs and the player must be told |
| `+0xAD` | name of the first source civ (zero-terminated, 32 bytes) |
| `+0xCD` | name of the second source civ (32 bytes; empty when there is only one source) |
| `+0xEB0 + 4q` | relation word toward civ `q`; **bit 0 = contact** |
| `+0xF0` | the advance the notice is about, `-1` none |
| `+0xF4` | **era**, `0..3` |
| `+0xF8` | **beakers** accumulated toward the current research |
| `+0xFC` | **current research**: an advance, `T` meaning "a future technology", `-1` nothing chosen |
| `+0x100` | **turns researched** on the current target (only turns with a positive rate count) |
| `+0x104` | number of future technologies acquired |
| `+0x11CC`, `+0x15C0` | two score accumulators (advances add to both; 6, step 6) |
| `+0x15D0`, `+0x15D4` | Science Age flag (bit 0) and its last turn (11) |
| `+0x194` | city count |
| `+0x1A0` | number of advances known |
| `+0x18C4` | research queue dirty byte |
| `+0x18C8` | queue head index |
| `+0x18CC` | queue length |
| `+0x18D0` | queue storage, 256 dwords |

The queue is a ring of 256 slots. `head` and `length` are plain integers; a slot index is `(head + i)
& 0xFF` (the code uses the signed-modulo idiom `x & 0x800000FF` with a fix-up for negatives, which
equals `x & 0xFF` for the non-negative values that occur). `queue.pushBack(v)`: `ring[(head+length)
& 255] = v; length += 1; dirty = 1`. `queue.popFront()`: `head = (head + 1) & 255; length -= 1; dirty
= 1`. The queue holds the human player's planned research order; the AI uses it as a one-element
buffer (9.2).

### 1.4 Rule constants

| symbol | address | RULE/GAME/other source | shipped Conquests |
|---|---|---|---|
| `future_tech_cost` | `[0x9C7304]` | RULE body `+0x120` | 400 |
| `max_research_time` | `[0x9C730C]` | RULE body `+0x128` | 50 |
| `min_research_time` | `[0x9C7310]` | RULE body `+0x12C` | 4 |
| `DIFF[level].cost_factor` | `[0x9C40C0] + level*124 + 0x68`, level `[0xA52684]` | DIFF | Chieftain 20, Warlord 12, Regent 10, Monarch 9, Emperor 8, Demigod 7, Deity 6, Sid 4 |
| `WSIZ[size].tech_rate` | `[0x9C7330] + index*84 + 8`, index `[0x9C73A0]` | WSIZ | Tiny 160, Small 200, Standard 240, Large 320, Huge 400 |
| `advancement_weight` | `[0xA529B4]` | GAME `+0x9C` "Advancement * cost" | 5 (editor default) |
| `battle_created_unit` | `[0x9C728C]` | RULE | the Leader unit |

TECH costs in the shipped rules run from 2 to 360 (Bronze Working 3, Masonry 4, Alphabet 5, Integrated
Defense 360). `[0x9C73A0]` is the world-size index chosen at game creation; `[0xA52684]` is the
difficulty level of the game (one level for the whole game, not per player).

### 1.5 Global words

| address | meaning |
|---|---|
| `[0xA526AC]` | current turn number (0 during setup) |
| `[0xA526BC]` | bit mask of human-controlled slots |
| `[0xA526C0]` | bit mask of slots in play (slot 0, the barbarians, is usually a member) |
| `[0xA5267C]` | game flag word: `0x200` Accelerated Production, `0x2000` Victory Point Scoring, `0x4000` Capture the Unit, `0x20000` Reverse Capture the Flag, `0x40000` scientific-leader rule (section 6, step 9) |
| `[0x9FD4BC]` | slot of the local (viewing) player; **UI only** |
| `[0xA360A7]` | headless/batch byte; suppresses dialogs; **UI only** |
| `0x47B530` | multiplayer-mode gate (tail jump to `0x499FE0`) |

## 2. Where research happens

### 2.1 In the round

`Player::turn` (`0x5604B0`, once per player per round; `turn.md`) runs these steps, in this order, that
matter here. (The call addresses are the call instructions inside `0x5604B0`.)

| step | call | what |
|---|---|---|
| 1 | `0x560AE2` | **income pass** `0x560160`: beakers and gold (3.2) |
| 2 | `0x560AEE .. 0x560BEF` | per-civ gold-per-turn payments (`economy.md`/`diplomacy.md`); no research effect |
| 3 | `0x560C06 .. 0x560CAB` | unit-support and building-upkeep payment, in an order chosen by `rand(4) == 0` (`economy.md`) |
| 4 | `0x560CB2` | **research step** `0x562200` (5): completes the current research, runs the Great Library, chooses a target when none |
| 5 | `0x560CCF` | `0x560050(P)`: calls `0x4BE970(C)` for every city of P (pool order; not decoded here), decrements `P.+0x15C8` when positive, **expires the Science Age** (11), then calls `0x5DAEB0` on every entry of the colony-like pool `[0xA52E3C]` owned by P (byte `+0x2C`). It touches no units. Skipped when `0x47B530` is true and `[0x990390] == 2` |
| 6 | `0x560CD4 ..` | anarchy and government timers, culture accumulator `0x4F8E20`, ... (`turn.md`) |

Because the income pass (step 1) precedes the research step (step 4), beakers earned this turn count
toward completion this turn. Because the research step precedes the city sequencer (step 5), an advance
that arrives this turn is available to the production and growth code of the same turn.

### 2.2 Everything that gives a player an advance

All of them end in `acquire` (section 6). The table lists the 19 call sites found by a call-target
census over the linear disassembly; the last three columns are the arguments `(byResearch, silent,
runEras)` the site passes (section 6.1).

| site | inside | event | (a2, a3, a4) |
|---|---|---|---|
| `0x562303` | `0x562200` | research completes (5) | 1, 1, 1 |
| `0x5621C9` | `0x561860` | Philosophy bonus advance (6, step 17) | 1, 1, 1 |
| `0x5625A9` | `0x562380` | Great Library (8) | 0, (target == current), 1 |
| `0x55E2A3`, `0x55E341` | `0x55E190` | free advances of the era just entered (7.2) | 0, E, 0 |
| `0x55E3C0` | `0x55E190` | Scientific-trait random advance (7.2) | 0, E, 0 |
| `0x5683F2`, `0x56847A`, `0x5688D7` | `0x567C80` | start-of-game grants (7.5) | 0, 1, 0 |
| `0x4AD75F` | `0x4ACF40` | Theory of Evolution (10.4) | 0, F, 1 (`F` = the `silent` argument of `0x4ACF40`) |
| `0x55C60F` | `0x55B8B0` | goody hut (10.6) | 0, 1, 1 |
| `0x43FC18`, `0x43FC7E` | `0x43F3F0` | advance obtained in a trade (`diplomacy.md`) | 0, 1, 1 |
| `0x503451` | `0x502D90` | advance given in a diplomatic response; fills the notice fields with the giver's name (`diplomacy.md`) | 0, 1, 1 |
| `0x52703A` | `0x526DA0` | espionage "steal technology" success; the AI thief takes the best valued advance the victim has (10.7) | 0, 1, 1 |
| `0x479CB7` | `0x479C70` | network message handler: arguments come from the message | message |
| `0x5A8DCE`, `0x5A9795`, `0x5A9A42` | `0x5A8410`, `0x5A91F0` | network message handlers | 0, 1, 1 |

`E` is the first argument given to `enterEra`. The call-site census is by instruction scan (every `call 0x561860` in the image, 19 sites); a call through a function pointer would not appear.

## 3. Beakers

### 3.1 The two sources in a city

Every city stores its science in two cells (`yields.md` 5.5): `C.+0x260` (the city's own science
stream: tax-rate split, libraries and so on) and `C.+0x26C` (scientist specialists). The accessors
(each `ret 8`, argument `k` = stream number, 1 = science) are `0x4ACAE0(C; k, flag)` for the city
stream (`C.[0x25C + 4k]`), `0x4AC9E0(C; k, flag)` for the specialist stream (`C.[0x268 + 4k]`) and
`0x4ACA50(C; k, flag)` for their sum. For `k == 1` and `flag != 0`, when the owner's Science Age is
active (11), the returned value is multiplied by the float `1.25` (`0x66905C` = `0x3FA00000`) and
truncated with `ftol`: for `0x4ACAE0` and `0x4AC9E0` that is applied to the single cell, for `0x4ACA50`
**once to the sum** (so `ftol(1.25 * (a + b))`, which can differ by one from `ftol(1.25a) + ftol(1.25b)`).
For `k != 1` the flag is ignored. The research code passes `flag = 0` everywhere it accumulates (below),
so the accessors are plain cells there.

### 3.2 The income pass `0x560160(P)` (A)

For each city `C` of `P`, in pool order:

1. bit `0x20` of `C.+0x30` (the city mood word) is **set** if `C.+0x50 == 1` (the city is building an
   improvement) and `BLDG[C.+0x4C].+0xEC & 0x80000` (Capitalization / Wealth), otherwise **cleared**.
2. `P.+0xF8 += 0x4ACA50(C; 1, 0)` = `C.+0x260 + C.+0x26C`.
3. the city's gold `g = 0x4ACA50(C; 2, 0)` is added to the treasury: `sum = g + P.+0x44 + P.+0x48`;
   if `sum <= 0` the treasury becomes 0, otherwise it becomes `sum`; then the two cells are
   re-split: `a = (timeGetTime() mod n) - 0x3039`, `P.+0x44 = a`, `P.+0x48 = n - a` with `n = sum`
   (for `sum <= 0`: `a = (timeGetTime() mod 0xD431) - 0x8235`, `P.+0x48 = -a`, so the sum is 0). The
   split is anti-tamper noise from the wall clock; only the sum is game state. The clamp at zero is
   applied after **each** city, so a city with negative gold cannot drive the treasury below 0 but
   later cities still add to it.

Beakers are added with **no** Science Age bonus and **no** cap; a negative science total would subtract.
There is no per-turn "excess" mechanism: beakers simply accumulate in `+0xF8` whether or not a target is
chosen (the research step later discards them if there was no target, 5.3).

### 3.3 The research rate `s` (A, E for the use in 4.2)

`s` is the player's total science per turn as the cost clamps and the completion test see it:

```
s0 = sum over cities of P of 0x4AC9E0(C; 1, 0)     // scientists, +0x26C
   + sum over cities of P of C.+0x260              // city streams  (0x55D750(P))
s  = ftol(s0 * 1.25)  when the Science Age is active (11), else s0
```

(`0x55D810(P)` computes exactly this; `0x562200` and `0x569E80` contain the same loops inline.)
The multiplication is `fild s0; fmul dword [0x66905C]; call 0x64A230` and `0x66905C` holds `1.25f`, so
`s = s0 * 5 / 4` rounded toward zero (exact for any realistic `s0`).

**Observed asymmetry (A).** The Science Age raises `s`, which feeds only (a) the test `s > 0` that
decides whether this turn counts as a research turn, and (b) the two clamps of the effective cost
(4.2). It does **not** raise the beakers added by the income pass (3.2), which use `flag = 0`. In effect
the bonus changes how fast a tech is allowed to complete under the minimum/maximum research time, not
the raw beaker income. This is how the code is written; the shipped game text describing +25% research
is not reflected in the accumulation path. (`0x566140`, the turns-left estimate, uses `s` and so shows
the bonus.)

## 4. What a technology costs

### 4.1 Base cost `Player::baseCost(P; t, neutral)` `0x569C10`, `ret 8` (A, E)

`neutral` is a byte flag: non-zero forces the "human" pricing for any player (the AI valuation passes 1
so that all civs are priced alike; every other caller passes 0).

```
human   = ((1 << P.slot) & [0xA526BC]) != 0                      // 0x437D10
level_cf = DIFF[[0xA52684]].cost_factor
a  = 10                      if human or neutral                 // these players pay as on 10
   = level_cf                otherwise                           // AI civs pay by difficulty
if [0xA5267C] & 0x200: a = tdiv(a, 2)                            // Accelerated Production
r  = max(1, min(10, a))                                          // multiplier 1..10

if t == T:                                                       // a future technology
    x = [0x9C7304] * r                                           // future_tech_cost * r
else:
    n     = popcount([0xA526C0])                                 // civs in play, 0x5DF900
    t1    = tdiv(7*n - 7, 4)
    count = number of slots q in 1..31 such that
               ((1<<q) & [0xA526C0]) != 0                        // q is in play
           and (P.+0xEB0 + 4q) bit 0 is set                      // P has contact with q
           and ( t == -1  or  known[t] has bit q )               // q knows t
    x = tdiv( (t1 - count) * (TECH[t].cost * r) , t1 )           // divide by zero when n == 1

size_rate = WSIZ[[0x9C73A0]].tech_rate
base = tdiv( size_rate * x ,  10 * min(level_cf, 10) )
return max(1, base)
```

Notes.

* The divisor uses the game's difficulty `level_cf` (capped at 10) even for human players, whose
  multiplier `r` is fixed at 10: a human on Sid (cf 4) pays `10 / 4` times the base of a human on
  Regent (cf 10). The AI pays `r = level_cf` (clamped to 10), so an AI on Sid (cf 4) pays the same base
  as a human on Regent, and AIs on Chieftain (cf 20) are priced as on 10.
* The contact count reduces the price: `(t1 - count) / t1` of the full price, where `t1 = tdiv(7n -
  7, 4)` is about 1.75 times the number of other civs. Only civs the player **has contact with** (bit
  0 of its relation word to them) and that are in play count; barbarians (slot 0) never count.
* With `n == 1` (one civ in play) `t1 == 0` and the division faults. Real games have `n >= 2`.
  For `n == 2`, `t1 == 1`; a contacted civ that knows the tech makes the numerator 0 and the price 1.
* The future-technology price ignores `count` and the tech row.

### 4.2 Effective cost `Player::effectiveCost(P; t, neutral)` `0x569E80`, `ret 8` (A, E)

The beaker total the research step compares against.

```
if t == -1: return 0
base = baseCost(P; t, neutral)
s    = research rate (3.3, with the Science Age factor)
minT = [0x9C7310]      maxT = [0x9C730C]      b = P.+0xF8      k = P.+0x100

if t == P.+0xFC:                                    // the current research
    if s == 0:
        return b + (maxT - k)                       // not clamped at 0
    hi = max(0, maxT - k) * s
    lo = max(0, minT - k) * s
    need = max(0, base - b)
    if hi < lo:        c = lo
    elif need < lo:    c = lo
    else:              c = min(need, hi)            // need >= lo here
    return b + c
else:                                               // any other advance
    lo = (s == 0) ? minT : minT * s
    hi = (s == 0) ? maxT : maxT * s
    if hi < lo or base < lo: return lo
    if base > hi:            return hi
    return base
```

For the current target the cost is "beakers already held plus the remaining need, but the remainder is
clamped between `lo` and `hi`". `lo` makes a tech take at least `minT` research turns at the present
rate; `hi` makes it take at most `maxT` turns however slow the rate (once `k >= maxT` both clamps are
0 and the tech completes at once). When `s == 0` the formula degrades to the constants `minT`/`maxT`
(not multiplied), which is what the displayed cost of a non-current tech becomes in a no-science empire.

### 4.3 Reading the clamps as a rule

A tech is finished at the end of a research turn when `s > 0` and `beakers >= effectiveCost`, i.e. when
`max(0, base - b)` clamped into `[max(0, minT - k) * s, max(0, maxT - k) * s]` is 0: **`k >= minT` and
(`b >= base` or `k >= maxT`)**, where `k` counts turns with `s > 0` including the current one.

### 4.4 Golden vectors (E)

These come from running the real `0x569C10`/`0x569E80` code. World: shipped Conquests rules, `P` in
slot 1, `n = 8` civs in play (slots 0..7), difficulty Regent, world size Standard, `P` human, no game
flags unless stated, no contacts unless stated. "count" is the contact count of 4.1. Base cost:

| case | tech (index, cost) | result |
|---|---|---|
| Regent, Standard, count 0 | Bronze Working (0, 3) | 72 |
| count 3 (slots 2,3,4 contacted and know it) | Bronze Working | 52 |
| 3 contacted, 2 of them know it | Bronze Working | 60 |
| 3 know it, none contacted | Bronze Working | 72 |
| Regent | Integrated Defense (80, 360) | 8640 |
| AI on Sid (cf 4) | Integrated Defense | 8640 |
| human on Sid | Integrated Defense | 21600 |
| AI on Chieftain (cf 20) | Integrated Defense | 8640 |
| AI on Deity (cf 6) | Integrated Defense | 8640 |
| AI on Deity, `neutral = 1` | Integrated Defense | 14400 |
| Tiny world, Regent | Masonry (1, 4) | 64 |
| Huge world, Regent | Masonry | 160 |
| Regent | future technology (83) | 9600 |
| AI on Deity | future technology | 9600 |
| flag `0x200` set | Bronze Working | 36 |
| `n = 3` (slots 0,1,3) | Bronze Working | 72 |
| `n = 2` | Bronze Working | 72 |
| `n = 5` (slots 1..5) | Alphabet (2, 5) | 120 |

Effective cost (`neutral = 0`; "base" is the value `baseCost` returns for that tech in the same world):

| case | result |
|---|---|
| non-current, `s = 0`, base 72 | 50 |
| non-current, `s = 30`, base 72 | 120 |
| non-current, `s = 30`, base 8640 | 1500 |
| non-current, `s = 5`, base 64 (Tiny Masonry) | 64 |
| same with the Science Age (`s = 5` becomes `trunc(5 * 1.25) = 6`) | 64 |
| non-current, `s = 0`, base 8640 | 50 |
| current, `s = 10`, `b = 20`, `k = 2`, base 72 | 72 |
| current, `s = 100`, `b = 0`, `k = 0`, base 72 | 400 |
| current, `s = 0`, `b = 15`, `k = 10` | 55 |
| current, `s = 10`, `b = 15`, `k = 60` (past `maxT`) | 15 |
| current, `s = 10`, `b = 500`, `k = 9`, base 72 | 500 |
| current, `s = 10`, `b = 0`, `k = 0`, base 8640 | 500 |
| current future tech, `s = 40`, `b = 100`, `k = 3`, base 9600 | 1980 |
| current, `s = 7`, `b = 30`, `k = 1`, Science Age (`s = 8`), base 72 | 72 |

## 5. The research step `0x562200(P)` (A)

Called once per round per player (2.1, step 4).

```
if P.+0x194 == 0: return                                   // no cities: nothing at all
cur = P.+0xFC
if cur != -1:
    s = research rate (3.3)                                // sum of 0x4AC9E0(C;1,0) + 0x55D750(P), x1.25 in a Science Age
    if s > 0:
        P.+0x100 += 1                                      // a research turn
        need = effectiveCost(P; P.+0xFC, 0)                // 0x569E80
        if P.+0xF8 >= need:
            t = P.+0xFC
            if t == -1: t = defaultPick(P; 1)              // P->vtable[+0x58](1): 0x449530; unreachable normally
            if t != -1:  acquire(P; t, 1, 1, 1)            // (multiplayer: send the same arguments)
greatLibrary(P)                                            // 0x562380 (8), always, even when cur == -1 or s <= 0
if P.+0xF8 >= 0 and P.+0xFC == -1:                         // still no target (it was -1, or acquire cleared it)
    nt = chooseResearch(P; 0)                              // 0x5625D0 (9.1)
    if nt != P.+0xFC:  P.+0xFC = nt; P.+0xF8 = 0; P.+0x100 = 0
    (multiplayer, local player: announce P.+0xFC with 0x475390)
```

5.1 `acquire(P; t, 1, 1, 1)` on completion has already re-chosen the next target (6, step 11) and zeroed
the beakers, so **surplus beakers are lost**: a tech that finishes with 120 beakers against a cost of 100
leaves the next target at 0.

5.2 The test `s > 0` guards both the turn counter and completion. A civ whose rate has fallen to 0 or
below makes no progress and cannot finish a tech even if `b >= base`.

5.3 When the player has no target (`+0xFC == -1`, for example at the start of the game or after a
completion that found nothing to research), the beakers that accumulated (3.2) are discarded at the
moment a target is chosen (`+0xF8 = 0`, `+0x100 = 0`). `chooseResearch` returns the front of the
research queue after topping it up (9.1): a researchable advance or `T`. It does not return `-1` on the
paths used here, so `nt != -1 == +0xFC` and the assignment always fires.

5.4 The step does **not** check the minimum research time separately: it is inside the effective cost
(4.3).

## 6. Acquiring an advance: `Player::acquire(P; t, byResearch, silent, runEras)` `0x561860`, `ret 0x10` (A)

### 6.1 Arguments

| arg | name | effect |
|---|---|---|
| `t` | advance | an index, `T` for a future technology, `-1` (no effect) |
| `a2` | `byResearch` | enables the scientific-leader roll (step 9), the tech-acquired dialog, and tells the chooser to pop the queue head (9.1) |
| `a3` | `silent` | suppresses popups; also forwarded to `enterEra` (7.2) so era grants stay silent |
| `a4` | `runEras` | after the advance, advance eras while the era predicate holds (7.1) |

### 6.2 Ordered effects

```
 1. if t == T:                                    // future technology
        P.+0xF8 = 0;  P.+0x100 = 0;  P.+0x104 += 1;   return      // nothing else happens
    if t == -1:  return
    if known(P, t): return                        // already known
 2. known[t] |= 1 << P.slot;   P.+0x1A0 += 1
 3. (UI only) if P is the local player and !a2 and !a3: popup TECH_FREE
 4. 0x55E5E0(P)                                   // post-acquire hook (10.1)
 5. 0x55A560(P)                                   // rebuild the great-wonder reach set (primitives.md 3.2)
 6. if turn > 0 and [0xA5267C] & 0x26000:         // VPS, Capture the Unit or Reverse CTF
        P.+0x11CC += TECH[t].cost * [0xA529B4]
        P.+0x15C0 += TECH[t].cost * [0xA529B4]
 7. if TECH[t].flags & 0x2000 or & 0x4000:        // trade over sea/ocean
        0x57DE90(0xB72888; 0)                     // rebuild the trade network (trade-network.md)
 8. g = first GOOD row (table order) whose +0x4C (prerequisite advance) == t, if any:
        0x57D980(0xB72888; P.slot, 1, 0, -1)
        0x57E450(0xB72888; a3)                    // recompute resource availability; the argument is the silent flag
        for each city C of P: 0x4B0E80(C)         // recompute the city
 9. first-discoverer rules:
        first = no other slot q in 1..31 (q != P.slot) has known[t] bit q set    // t != T here
        if first and TECH[t].flags & 0x200000:  bonus = true                      // Philosophy, evaluated before the next test
        if first and a2 and turn > 0 and [0xA5267C] & 0x40000:                    // scientific leader roll
            chance = 5 if RACE[P.race] has trait 3 (Scientific) else 3
            if rand(100) < chance and P has a capital city:
                create unit [0x9C728C] at the capital with 0x5694D0(P; type, x, y, -1, -1, 1, 2, -1)
                if created: (UI) message NEWSCILEADER; log
10. if a4:  while tryAdvanceEra(P) { enterEra(P; a3, 0) }                          // 7.1, 7.2
11. if P.+0xFC == t:                                                              // we just got the current target
        nt = chooseResearch(P; a2)
        if nt != P.+0xFC:  P.+0xFC = nt;  P.+0xF8 = 0;  P.+0x100 = 0
        (multiplayer, local player: announce nt)
12. if P is human:
        (UI only) if local and !headless and (a2 or !a3) and !multiplayer: tech-acquired dialog 0x4DC8E0(0x9F8700; t)
    else (AI):
        if some GOVT row has +0x1B8 (prerequisite advance) == t:  P->vtable[+0x20]()    // 0x444970, AI considers a revolution
13. (UI only) if local and (a2 or !a3): for each great wonder b (BLDG +0xF0 & 4) with +0xE0 (obsolete_by) == t
        held by P (owner of its city per 0x539030): message SUMMARY_WONDER_OBSOLETE
14. (UI only) if local and (a2 or !a3) and TECH[t].flags & 1 and P has contact with someone (0x500F00(local) > 0):
        the Writing/diplomacy tutorial popup
15. if TECH[t].flags & 0x400000:  0x5D3030(0x9C736C; 1)                            // Reveal Map (10.3)
16. (UI only) if local, [0xC9C45C] option byte set, not multiplayer, (a2 or !a3): log "ACHIEVED_<name>"
17. if bonus:                                                                      // Philosophy
        u = P.+0xFC
        if u == -1: u = P->vtable[+0x58](1)       // 0x449530(P; 1)
        if u != -1:  acquire(P; u, 1, 1, 1)       // (multiplayer: send)
```

Notes on the order.

* Step 11 runs after the era loop (step 10), so a research completion that also crosses an era chooses
  the next target with the new era's techs already unlocked.
* Step 11 is conditional on `P.+0xFC == t`. A tech obtained any other way (trade, Great Library, hut)
  leaves the current target and its beakers untouched unless it *was* the current target, in which case
  the beakers are reset (a loss for the player; the Great Library notice in 8 and 9.1 tells them).
* The scientific-leader roll (step 9) is evaluated for every first discoverer with `a2 != 0`, which
  means research completions and the Philosophy gift; trade, theft and grants never roll. The unit is
  the RULE "battle-created unit" and appears in the **capital**; there is no other placement logic.
* The Philosophy flag is read from the tech row, not from a tech name; a scenario can put it on any
  advance. It requires being the first among slots 1..31 to know the advance; it is the next
  *current research* (or the AI default) that is given free, and the gift itself runs the full
  `acquire` (so it can roll a scientific leader and complete eras).
* A future technology short-circuits at step 1: it adds no knowledge bit, does not touch `+0x1A0`, runs
  no hook and does not choose a new target; `P.+0xFC` stays `T`.

## 7. Eras

### 7.1 `Player::tryAdvanceEra(P)` `0x561690` (A)

Returns true and increments `P.+0xF4` when the player qualifies for the next era; returns false
otherwise. (Despite the shape of the name it **mutates**.)

```
for t in 0 .. T-1, stopping with failure at the first violation:
    row = TECH[t]
    if row.era == -1:                 continue          // not in the tree
    if row.era > P.era:               continue          // belongs to a later era
    if row.flags & 0x20000:           continue          // Not Required for Era Advancement
    if known(P, t):                   continue
    // an unknown required advance of this or an earlier era:
    if reachable(P, t) via all four prerequisites:   fail   // see 7.1.1: it can still be researched
    // otherwise it is unreachable (some chain member has no era): ignored
if no failure:
    higherCivExists = any slot q in 0..31 with ((1<<q) & [0xA526C0]) and Player[q].+0xF4 > P.+0xF4
    if !higherCivExists:  record achievement bit (1 << P.era) for P: 0x57CCD0(PALV[P.slot]; 1<<P.era)  // 7.4
    if P.era >= 3: return false
    P.+0xF4 += 1;  return true
return false
```

The achievement is recorded for the **old** era number (the mask is `1 << P.era` read before the
increment), and only when no civ in play (slot 0 included when its bit is set) is already in a later
era than P's. Several civs can complete the same era, but only those that do so while nobody is ahead
get the record. At era 3 the record is still attempted but the era does not advance and the function
returns false.

7.1.1 `reachable(P, t)` `0x5614E0`, `ret 4`: `t == -1` true; `t != T` and known true; if `TECH[t].era == -1`
false; otherwise true iff all four prerequisites are reachable (recursively). An unknown required advance
blocks the era only when **all four of its own prerequisites are reachable** (the code tests
`reachable(prereq_i)` for i in 0..3, not `reachable(t)` itself, which is the same thing given the
`era != -1` filter above it).

7.1.2 There is no turn or count requirement: an era is reached the moment the last required advance
of the era (all advances whose era is `<=` the current one, minus the "not required" ones) is known.
`acquire` step 10 (and the end of the initial grants) calls the predicate in a loop, so several eras can
be entered by one acquisition if the player already knows the rest.

### 7.2 `enterEra(P; a1, a2)` `0x55E190`, `ret 8` (A)

Called right after `tryAdvanceEra` returned true, so `P.+0xF4` is already the **new** era. `a1` is the
silent flag forwarded to `acquire`; `a2` selects the direct call over the network send in multiplayer.

```
(UI only: era-change announcement for the local player)
leadIdx = P.slot - 1                                       // LEAD row of this slot
free advances:
  if LEAD count > 0 and [0x9C3D74] > -1 and LEAD[leadIdx].+0x04 != 0:   // the scenario defines custom data for this leader
        for i in 0 .. LEAD[leadIdx].+0x50 - 1:
            t = LEAD[leadIdx].+0x3C[i]
            if TECH[t].era == P.era:  acquire(P; t, 0, a1, 0)
  else:
        for k in 0 .. 3:
            t = RACE[P.race].freeTech(k)                   // vtable +0x1C
            if t > -1 and TECH[t].era == P.era:  acquire(P; t, 0, a1, 0)
if RACE[P.race] has trait 3 (Scientific):
        t = randomResearchable(P, onlyCurrentEra = true)   // 7.3
        if t != -1:  acquire(P; t, 0, a1, 0)
if turn > 0:
        n = number of slots q in 1..31 in play whose Player[q].+0xF4 == P.+0xF4     // includes P
        if n == 2:  0x55FD00(Player[0])                    // barbarian landing, barbarians.md
(UI only: for each unit of P, refresh its era-specific art: 0x406B40(0x73DA40; unit))
```

* The three kinds of grant call `acquire` with `byResearch = 0` and `runEras = 0`, so they cannot recurse
  into another era loop (the caller's `while` handles that).
* The free-advance filter is "the tech's era equals the new era", so the 4 free techs of a civilization are
  paid out one era at a time as the player enters the era each belongs to (era 0 ones at the start,
  7.5).
* The barbarian trigger counts civs 1..31 in play at exactly the new era including the entering civ: the
  second civ to reach an era triggers `0x55FD00` (turn > 0 only). The first and the third do not.
* `[0x9C3D74]` is written by no instruction (it is filled by the scenario loader); the code only tests
  it for `> -1`. For a normal game it is presumed `>= 0` whenever LEAD rows exist; **open**.

### 7.3 `randomResearchable(P, onlyCurrentEra)` `0x55E470`, `ret 4` (A)

Builds the list of `t` in ascending order for which: `t` is neither `-1` nor `T`; `P` does not know it;
`TECH[t].era <= P.era` and is not `-1`; every one of the four prerequisites is `-1` or known (a
prerequisite equal to `T` fails); and, when `onlyCurrentEra`, `TECH[t].era == P.era`. If the list is
empty returns `-1` (no draw). Otherwise draws **`rand(count)`** once and returns that element.

### 7.4 The achievement record `0x57CCD0(rec; mask)`, `ret 4` (A)

`rec = 0xB71288 + slot * 0xB0` (the 32 `PALV` records of the savegame, `savegame.md` 5.8). If `rec.+0x1C
& mask == 0` and `rec.+0xA4 + rec.+0xAC < 32` then `rec.+0xAC += 1; rec.+0x1C |= mask`. It is pure
bookkeeping (a bitmask of "completed era k while nobody was ahead" plus a counter capped at 32 entries); `0x57CD10`
(called from `Player::turn` at `0x560E68`) sums the two counters into `rec.+0xA0`. No rule reads it
in the code covered here (**open**: the score/Hall-of-Fame consumer).

### 7.5 Start-of-game grants `Player::init(P; race)` `0x567C80`, `ret 4` (A)

The new-player routine does a great deal besides research (units, fields); the research part, in order:

```
P.+0xF0 = -1;  P.+0xFC = -1 (if it was not, also +0xF8 = 0, +0x100 = 0);  P.+0xF4 = 0;
P.+0x104 = 0;  P.+0x1A0 = 0;  queue head = 0, length = 0, dirty = 1
... (if slot is in play, slot != 0): leadIdx = slot - 1
    if LEAD count > 0 and [0x9C3D74] > -1 and LEAD[leadIdx].+0x04 != 0:
        for each t in the LEAD free-advance list (+0x3C, count +0x50):
            if t == -1: continue
            if t == T or not known(P, t):
                if TECH[t].era == 0 or == -1:  acquire(P; t, 0, 1, 0)
    else:
        for k in 0..3:
            t = RACE[P.race].freeTech(k)
            if t > -1 and (t == T or not known(P, t)) and (TECH[t].era == 0 or == -1):  acquire(P; t, 0, 1, 0)
... (starting units are created here)
while tryAdvanceEra(P): enterEra(P; 1, 1)                  // the free era-0 grants may already complete era 0
if LEAD count > 0 and [0x9C3D74] > -1:
    for t in 0 .. T-1:                                      // the leader's initial era
        if TECH[t].era != -1 and (t == T or not known(P, t)) and TECH[t].era < LEAD[leadIdx].+0x44:
            acquire(P; t, 0, 1, 0)
    while tryAdvanceEra(P): enterEra(P; 1, 1)
if slot != 0 and LEAD count > 0 and [0x9C3D74] > -1 and leadIdx < LEAD count:
    P.+0x9C = 0;  setGovernment(P, LEAD[leadIdx].+0x4C)   // 0x55CD60
```

So a scenario leader's `initial_era` (`LEAD +0x44`) grants **every** advance of every earlier era that the
player does not yet know, then runs the era loop (which grants that era's free techs, the Scientific
bonus and may trigger barbarians only when `turn > 0`; at setup `turn == 0`, so none).

The government assignment `0x55CD60(P; g)` is `government.md`'s setter. The default (non-LEAD) path sets
`0x55CD60(P; [0x9C3D70])` earlier in the routine; `[0x9C3D70]` is a loader-written count/default and
is **open**.

## 8. The Great Library `0x562380(P)` (A)

Called from the research step every turn (5), after any completion.

```
if 0x55A8D0(P; 2, 0) <= 0: return           // P holds no active wonder with wonder_flags bit 1 (GAIN_ADVANCES_OWNED_BY_TWO_CIVS)
for t in 0 .. T-1:
    if known(P, t): continue
    row = TECH[t]
    if row.era > P.era or row.era == -1: continue
    if any of the four prerequisites is not known (-1 counts as known; T does not): continue
    n = 0;  first = -1;  second = -1
    for q in 1 .. 31:
        if q is in play  and  bit 0 of Player[q].+0xEB0 + 4*P.slot is set     // q has contact with P
           and known[t] has bit q:
              n += 1;  if first == -1: first = q  else: second = q           // second ends as the LAST knower after the first
    if n < 2: continue
    if P.+0xFC == t:
        P.+0xAC = 1                                       // notice pending
        P.+0xAD = civName(first);  P.+0xCD = civName(second)
        P.+0xF0 = t
    acquire(P; t, 0, (P.+0xFC == t), 1)                   // multiplayer: send the same
```

`civName(q)` is `Player[q].+0x115C` when that custom name byte string is non-empty, otherwise the civ's
default name from the RACE vtable `+0x18` (`0x53A...`). Details:

* "In play" is the `[0xA526C0]` mask; the contact test is the **other** civ's relation word toward `P`
  (`Player[q].+0xEB0 + 4*P.slot`, bit 0), the mirror of the word the cost count (4.1) reads. Both are set
  together when contact is made, so they agree in normal play.
* The wonder condition is on the *holder*, using the wonder-flag counter `0x55A8D0` (`buildable.md`):
  the wonder must be built, owned by one of P's cities, allowed by its government and not obsolete.
* The loop is in ascending advance index and `acquire` runs inside it, so an advance acquired for free
  can make a later-indexed advance eligible in the same call. The advance is acquired with `silent = 1`
  only when it is the current research; the player is then informed through the notice fields (UI path
  in 9.1: `TECH_ACQUIRED_ONE_SOURCE`, `TECH_ACQUIRED_TWO_SOURCES`).
* Two civs are required. A third or later knower does not change anything; the "first" and "second"
  names recorded are only for the message.

## 9. Choosing the research target

### 9.1 `Player::chooseResearch(P; byResearch)` `0x5625D0`, `ret 4` (A)

Returns the advance `P` researches next: an index `0 .. T-1`, or `T` (a future technology). It edits the
research queue (1.3) but does not touch `P.+0xFC`, `+0xF8` or `+0x100`; its two callers do that (5 and
6.2 step 11). `byResearch` is 1 when the call follows a research completion or the Philosophy gift,
0 otherwise. The function has two modes:

```
interactive = P is human ([0xA526BC] has bit P.slot)  and  [0xA360A7] == 0  and  0x47B530() == false
```

**Queue cleaning `cleanQueue(P)`**, identical code in both modes (`0x562752` and `0x563031`):

```
while P.qlen != 0:
    v = ring[P.qhead & 255]
    keep =  not knowsTech(P, v)  and  v < T
            and TECH[v].era != -1  and  TECH[v].era <= P.era
            and for every i in 0..3: knowsTech(P, TECH[v].prereq[i])      // -1 counts as known, T as unknown
    if keep: break
    P.qhead = (P.qhead + 1) & 255;  P.qlen -= 1;  P.dirty = 1
```

`knowsTech(P, -1)` is true, so `-1` entries are dropped; `T` is dropped by `v < T`. After cleaning, the
front of the queue is an advance `P` can research now, or the queue is empty.

**Non-interactive mode** (every AI player, and every player in a headless or multiplayer session):

```
cleanQueue(P)
if P.qlen == 0:
    d = P.vtable[+0x58](P; 1)                          // default pick, mode 1 (9.2); d may be T
    ring[(P.qhead + P.qlen) & 255] = d;  P.qlen += 1;  P.dirty = 1
return ring[P.qhead & 255]
```

**Interactive mode** (a human player at the keyboard):

```
if byResearch:                                         // the finished front entry is dropped; unguarded:
    P.qhead = (P.qhead + 1) & 255;  P.qlen -= 1;  P.dirty = 1       // with qlen == 0 the length becomes -1
    0x535D80()                                         // UI hook, not decoded
cleanQueue(P)
d = (P.qlen != 0) ? ring[P.qhead & 255] : 0x449530(P; 0)           // the suggestion: default pick, mode 0
if d == T or d == -1:                                  // nothing can be researched
    ring[(P.qhead + P.qlen) & 255] = T;  P.qlen += 1;  P.dirty = 1
    return ring[P.qhead & 255]                         // no dialog
research dialog (UI, modal), repeated while the result is -1:
    choice = the highlighted item mapped to an advance index by 0x4CBBF0
if P.qlen != 0 and choice == ring[P.qhead & 255]:  nothing changes       // the rest of the plan is kept
elif P.qlen != 0:   P.qhead = 0;  ring[0] = choice;  P.qlen = 1;  P.dirty = 1      // the queue becomes [choice]
else:               ring[P.qhead & 255] = choice;  P.qlen = 1;  P.dirty = 1
return ring[P.qhead & 255]
```

The dialog is shown every time, even when the queue already holds a plan; the queue front is the
pre-selected item. Closing it without a selection yields that default. Its content (UI only):

* items: the suggestion `d` first, then every other advance `t` in ascending order for which the
  conditions of `canResearch` (9.3) hold; each item reads `name (turns)` with `turns` from 9.4,
  printed in decimal, or `--` when 9.4 returns 9999;
* header: `TECH_ADVANCED` when `byResearch`; otherwise, when `P.+0xAC != 0`, the Great Library notice
  (8) `TECH_ACQUIRED_ONE_SOURCE` if `P.+0xCD` is empty else `TECH_ACQUIRED_TWO_SOURCES`, built from
  `P.+0xAD`, `P.+0xCD` and the advance `P.+0xF0`, after which `P.+0xAC = 0`, `P.+0xAD = 0`,
  `P.+0xCD = 0`, `P.+0xF0 = -1`; otherwise `TECH_FIRST`.

A port without these dialogs needs one rule: a human chooses among the advances for which `canResearch`
holds; the default is the front of the cleaned queue, else the mode-0 pick; the choice replaces the queue
unless it equals the front. The notice fields are cleared only by this interactive path, so an AI
player's `+0xAC` stays set after a Great Library event (nothing reads it for an AI).

### 9.2 The default pick `0x449530(P; mode)`, `ret 4` (A); vtable `+0x58`

```
best = -1;  bestScore = 0
for t in 0 .. T-1:
    if canResearch(P; t):
        v = P.vtable[+0x54](P; t, 1, mode)             // the valuation 0x448BF0 (research-ai.md)
        if v > bestScore:  bestScore = v;  best = t
return (best != -1) ? best : T
```

Only strictly positive valuations are eligible and ties keep the lower index, so a player for whom
every valuation is `<= 0` is sent to a future technology even though advances remain. `mode` is 1 for
every automatic caller (non-interactive chooser, the research step's fallback, the Philosophy gift) and
0 for the interactive suggestion. With `mode == 1` the valuation draws two `rand(32)` values per
researchable advance (`research-ai.md` 2.3), so the pick consumes generator state; with `mode == 0` it
draws none. The same vtable (`0x66CB38`) serves human and AI players.

### 9.3 Predicates (A)

| function | address | definition |
|---|---|---|
| `knowsTech(P, t)` | `0x561440`, `ret 4` | `t == -1` true; `t == T` false; otherwise bit `P.slot` of `known[t]` |
| `canResearch(P, t)` | `0x561580`, `ret 4` | false for `t == -1`, `t == T`, `t >= T` and known `t`; false when `TECH[t].era == -1` or `> P.era`; each prerequisite must be `-1` or known (`T` fails); otherwise true |
| `reachable(P, t)` | `0x5614E0`, `ret 4` | `-1` true; `t != T` and known: true; `TECH[t].era == -1` false; otherwise true iff all four `reachable(prereq_i)` |
| `techDepth(P, t, d)` | `0x561620`, `ret 8` | `t < 0` or `t >= T`: `d`; otherwise the maximum over the four prerequisites `p` of `techDepth(P, p, d + 1)`. It ignores what is known; for a tech with no prerequisites the result is `d + 1` |
| `knowsTechWithFlags(P, mask)` | `0x561480`, `ret 4` | true iff some `t` has `TECH[t].flags & mask == mask` and `P` knows it |

`reachable` with `t == T` skips the knowledge test and reads the era of a row past the end of the table;
a prerequisite equal to `T` is not valid input.

### 9.4 Turns-left estimate `0x566140(P; t, remainingOnly)`, `ret 8` (A)

Used by the dialogs above and by the AI valuation (`research-ai.md`).

```
if t == -1: return 0
s = research rate (3.3, with the Science Age factor)
if s == 0: return 9999                                  // [0x668F70]; shown as "--"
c = effectiveCost(P; t, 0)                              // 0x569E80
if remainingOnly and t == P.+0xFC:  c -= P.+0xF8
if c <= 0: return 1
n = tdiv(c, s);  if n * s < c: n += 1                   // ceiling division
return max(1, n)
```

## 10. What an acquisition triggers

Details for the steps of 6.2 that only name an effect.

### 10.1 The post-acquire hook `0x55E5E0(P)` (A), step 4

```
rr = int at [[0x9C7324] + 0x218]                          // the Railroad advance of the terraform table (yields.md)
hasRail = (rr == -1) ? true : (rr == T) ? false : knowsTech(P, rr)
for each city C of P (pool order):
    if hasRail:  cell(C.x, C.y)->vtable[+0xE0](0, 3, C.x, C.y)    // 0x5DA2A0 overlay setter, group 0, value 3 (H: road and railroad bits)
    if C.+0x50 == 2:                                              // the city is building a unit
        u = replacementUnit(C; C.+0x4C)
        if u != -1:  0x4AFAB0(C; 2, u, 0)                         // switch production to unit u
```

The cell index is `(W/2) * C.y + (C.x >> 1)` with `W = [0x9C74D4]` the map width (`yields.md`). The hook
runs for every acquisition, not only the Railroad advance, and rewrites the overlay each time.

`replacementUnit(C; u)` (`0x4C0690`, `ret 4`): the unit a city should switch to when `u` has been made
obsolete.

```
s = PRTO[u].upgrade_to                                    // +0x78
while s != -1 and not cityCanBuildUnit(C; s, 1, 0, 1):  s = PRTO[s].upgrade_to
if s == -1: return -1
a = PRTO[s].alt_strategy_of (+0xA0);  if a != -1: s = a
for i in 0 .. PRTO.count - 1:
    if (i == s or PRTO[i].alt_strategy_of == s) and PRTO[i].ai_strategies == PRTO[u].ai_strategies: return i     // +0x8C
return s
```

`cityCanBuildUnit(C; u, obsoleteCheck, a3, a4)` is `0x4C04E0`; its rules and this walk are
`buildable.md` sections 3.1 and 3.2. The walk asks with `obsoleteCheck = 1`, which rejects a unit when
one of its own upgrades is buildable, so it lands on the last buildable unit of the chain.

### 10.2 First discoverer: Philosophy and the scientific leader (A), steps 9 and 17

`first` is true when no slot `q` in `1 .. 31`, `q != P.slot`, has the knowledge bit of `t` (this loop
does not look at the in-play mask; knowledge bits exist only for real players). If another civ knows `t`,
both the bonus flag and the leader roll are skipped.

* **Bonus advance.** When `first` and `TECH[t].flags & 0x200000` the bonus is armed. At step 17 (after
  everything else, including a new target chosen in step 11) `u = P.+0xFC`; if `u == -1`, `u` is the
  mode-1 default pick; if `u != -1`, `acquire(P; u, 1, 1, 1)`. `u` can be `T`, which then counts as a
  free future technology. The nested `acquire` is a full one: it may arm another bonus, roll a
  scientific leader, and run era checks.
* **Scientific leader.** Only when `first`, `a2 != 0`, `turn > 0` and `[0xA5267C] & 0x40000`. The draw
  `rand(100)` happens first and is compared with the chance `5` if `RACE[P.race]` has trait 3
  (Scientific, vtable `+0`) else `3`; only if `r < chance` is the capital looked up
  (`Player[P.slot].+0x2C` must name an existing city), so the draw is made even when there is no
  capital. The unit is created with `0x5694D0(P; [0x9C728C], capital.x, capital.y, -1, -1, 1, 2, -1)`.
  The factory stores its seventh argument (`2`) into the unit flag word `unit.+0x1F4` through the setter
  `0x5B6620`; the Science Age action requires bit 1 of that word (11), so (H) this unit is the scientific
  leader. The local player gets the message `NEWSCILEADER`, and an event-log entry of kind 13 is written
  (`0x58B5D0`); both are UI.

### 10.3 Reveal Map `0x5D3030(0x9C736C; 1)` (A), step 15

Runs when the acquired advance has flag `0x400000`. The function has no player argument:

```
for each cell index i in 0 .. cellCount-1:                // word at cells+0x40
    (flag == 1: every cell)                               // with flag 0 only cells whose vtable[+0x70]() is true
    y = i div (W/2);  x = 2 * (i mod (W/2)) + (y & 1)     // W = word at cells+0x168
    for each slot q in 0 .. 31 with bit q of [0xA526C0] (in play):
        0x55B1A0(Player[q]; x, y)                         // discover the tile for q
```

`discover(P; x, y)` (`0x55B1A0`, `ret 8`): if bit `P.slot` of the tile's mask (`cell+0x58`) is clear,
set it and increment `P.+0xA8` (discovered-tile count); then store `cell->vtable[+0xA8](0)` into the
tile's per-player memory byte `cell+0xAE+P.slot`. For the local player it also raises display flags and,
at 35, 55 and 75 discovered tiles, calls the UI hook `0x535D80`. After the loop the function tail-calls
`0x578A00(0xA0E270; 1)` (display refresh).

**Observed:** the effect applies to **every civ in play**, not to the acquirer. This is read from the
code only; it has not been seen in a running game (**open**: verify live).

### 10.4 Theory of Evolution (A), in the add path of `0x4ACF40(C; b, add, silent)`

`0x4ACF40` adds (`add = 1`) or removes a building `b` in city `C`. When adding a building whose
`BLDG[b].wonder_flags & 0x400` ("two free advances"), after the building's counters have been updated
and the owner's Golden Age check `0x55C9A0(owner)` has run (11):

```
Owner = Player[C.+0x28];  last = -1
repeat 2 times:
    start = Owner.+0xFC                                    // re-read each time
    if 0 <= start < T:
        for k in 0 .. T-1:
            u = (start + k) mod T                          // cyclic scan beginning at the current research
            if canResearch(Owner; u) and (last == -1 or last != u):
                last = u
                Owner.+0xAC = 0;  Owner.+0xAD = 0;  Owner.+0xCD = 0;  Owner.+0xF0 = -1     // clears the Great Library notice
                if 0x47B530():  send (slot; u, 0, silent, 1)  else  acquire(Owner; u, 0, silent, 1)
                break
```

The first pick is the current research itself when it is researchable; because acquiring the current
research chooses a new target (6.2 step 11), the second pass starts from the new target. With
`+0xFC == -1` or `T` the pass does nothing.

### 10.5 Goody hut: outcome 6, an advance (A), `0x55B8B0`

`0x55B8B0(P; x, y, outcome, resultFlag*, unit)` switches on `outcome` (jump table `0x55C684`, 8 entries;
index 6 is `0x55BD94`, the advance). The whole resolver, the outcome roll and the other seven outcomes are
specified in `goody-huts.md`; this section keeps the advance case:

```
if P.era > 0:  fall back to the default outcome 0x55B7D0(P)                     // only Ancient-era civs
cur = -1;  best = -1;  bestScore = 0x7FFFFFFF
for t in 0 .. T-1:
    skip if known(P, t)
    skip if TECH[t].era != 0                                     // era must be 0 (the checks are era > P.era, era == -1, era > 1)
    skip unless every prerequisite is known                      // -1 counts as known
    0x47B530()                                                   // called, result unused
    if t == P.+0xFC:  cur = t;  continue                         // the current research is only a fallback
    d = max over the four prerequisites p of techDepth(P, p, 1)
    skip if d > 4
    r = rand(100)                                                // first
    v = P.vtable[+0x54](P; t, 0, 0)                              // then the valuation (it draws too)
    score = v + (r & 0xFFFF)
    if score < bestScore:  best = t;  bestScore = score          // lowest total wins, first one on ties
if best != -1:       grant best
elif cur != -1:      grant cur
else:                default outcome 0x55B7D0(P)
grant(t):
    (local player: UI hook 0x537700(3))
    r2 = rand(15)                                                // selects a message variant: consumes one draw
    ...message popup 0x4DCAF0 (UI)...
    if 0x47B530():  send  else  acquire(P; t, 0, 1, 1)
```

* A hut therefore gives an advance only to a civ still in era 0, picks among era-0 advances it can
  already research, and takes the one with the **lowest** valuation plus noise (`noise` is 0..99). The
  default pick (9.2) takes the highest valuation, so the hut leans to the opposite end of the ranking;
  how strongly depends on the valuation scale (`research-ai.md`).
* `d` counts levels of the prerequisite chain: `d <= 4` admits advances at most three levels deep.
* `v` is the valuation with `a = 0`, `mode = 0`, which draws nothing (`research-ai.md` 1), so the only
  draws are `rand(100)` per eligible advance and the final `rand(15)`. Reference: `World::hut_advance`
  (`rust/src/research.rs`; `Brain::value` is the valuation).
* The message-variant draw: `r2 = rand(15)`; the loop tries `v = (k + r2) mod 15` for `k = 0 .. 14` and
  takes the first `v` whose "already used" byte `[0xA526C8 + 15 * RACE[P.race].+0x90C + v]` is 0; it is
  UI text selection but the draw is part of the random stream.

### 10.6 Stealing and receiving advances

* **Espionage, steal technology** (`0x526DA0`, success path): for an AI thief the advance is
  `0x44A5B0(Thief; victimSlot)`: over `t = 0 .. T-1`, those with `canResearch(Thief; t)` and
  `knowsTech(Player[victim]; t)`, valued `v = Thief.vtable[+0x54](Thief; t, 0, 0)`; the best strictly
  positive valuation wins (ties: lowest index); `-1` when none, in which case nothing is taken. Then
  `acquire(Thief; t, 0, 1, 1)` (or the network send). For a human thief the advance is chosen in a UI
  list (not specified here).
* **Trading** (`0x43F3F0`, at the "Tech traded" step) and **diplomatic gifts** (`0x502D90`):
  `acquire(side; t, 0, 1, 1)` for each side that receives an advance (the multiplayer branch sends the
  same arguments with `0x475460`). The trade also bumps the counter of `diplomacy.md` and then calls
  `0x55B3A0(side; other, 0)` (not decoded). Which advances are offered or accepted is decided by the
  diplomacy code, not by research.
* The network handlers (`0x479C70`, `0x5A8410`, `0x5A91F0`) call `acquire` with the arguments carried in
  the message; the multiplayer protocol is not specified here.

## 11. The Science Age (A)

State: `P.+0x15D0` bit 0 (flag) and `P.+0x15D4` (last turn). The Player constructor (`0x558C20`) clears
the flag; both words are written and read back by the Player serializer (`0x558FC0`, at `0x5595AD` and
`0x559E00`), so an age survives a save.

* `active(P)` (`0x55C890`): `(P.+0x15D0 & 1) and turn <= P.+0x15D4`.
* `start(P, on)` (`0x55C830`, `ret 4`): `on != 0`: set bit 0, `P.+0x15D4 = turn + 20`, and when `P` is
  the local player post `SUMMARY_SCIENCE_AGE`. `on == 0`: clear bit 0.
* The only caller is the unit action `0x5C03B0(unit)`: if a city stands on the unit's tile
  (`0x56D2C0(x, y)` non-null) it runs `start(Player[unit.+0x34], 1)`, then, when `unit.+0x20` equals the
  local slot, posts `SUMMARY_GOLDEN_AGE` (sic: the Golden Age text, not the Science Age text), and
  consumes the unit with `0x5BBBC0(unit; 0, 1, 0, 0, 0, 0, 0)`. It is reached from the unit-command
  handlers `0x45FED0` and `0x4DAA70`.
* The availability test `0x5C0300(unit)` is true iff: a city is on the unit's tile; the unit's type
  (`PRTO[unit.+0x40]`; when `0x5BC6D0(unit)` returns another type, that type counts too) has ability
  bit 19 (Leader); `unit.+0x1F4 & 2` (H: the scientific-leader marker, 10.2); and the owner's
  Science Age is **not** active. `0x5C1AD0` (a sibling command table) also consults `active`.
* Expiry: `0x560050`, each round after the research step (2.1 step 5): when the flag is set and
  `turn > P.+0x15D4`, post `SUMMARY_END_SCIENCE_AGE` to the local player and clear the flag. The
  predicate `active` is already false from that turn on, whether or not the flag has been cleared yet.
  So an age begun on turn `T0` is active for turns `T0 .. T0 + 20`.
* Effects: only the research rate `s` (3.3: x1.25, truncated) and the "flagged" science accessors
  (3.1), which feed the effective cost (4.2), the research-turn test (5), the turns-left estimate (9.4)
  and the displays. It adds nothing to beakers (3.2).

The Golden Age is a different mechanism and not part of research: `P.+0x3C` is its end turn, set to
`turn + [0x9C7308]` by `0x55C8C0(P)` (which also calls `0x4B0E80` on every city of P and posts
`SUMMARY_GOLDEN_AGE` to the local player). It is started by `0x55C9A0(P)`, which does nothing unless
`P.+0x3C == -1` and otherwise starts it when, for every trait `k = 0 .. 7` that the civ has
(`RACE.vtable[+0](k)`), the civ owns a great wonder (`BLDG.+0xF0 & 4`) whose category bit for that trait
is set in `BLDG.+0xF0` (the eight masks are `2, 0x40, 0x80, 0x20, 0x100, 0x200, 0x400, 0x800` in some
order that this file does not need), and by the leader command handler `0x5BEF00` (call at `0x5BF005`).
A separate document will cover it.

## 12. Verification record

| item | evidence | how |
|---|---|---|
| `baseCost 0x569C10`, `effectiveCost 0x569E80` | **E** | exact bytes executed in an emulator against a synthetic world; at least 15 000 random plus 32 curated cases matched an independent model; the vectors of 4.4 are outputs, regenerated against the shipped exe this session |
| research step `0x562200`, `acquire 0x561860` (all 17 steps), `0x562380`, `0x561690`, `0x5614E0`, `0x55E190`, `0x55E470`, `0x567C80` (research part), `0x560160`, `0x560050`, `0x5625D0`, `0x449530`, `0x561440/480/580/620`, `0x566140`, `0x55E5E0`, `0x4C0690`, `0x44A5B0`, `0x5D3030`, `0x55B1A0`, `0x55C830/890`, `0x5C0300/3B0`, hut case of `0x55B8B0`, Theory of Evolution in `0x4ACF40` | **A** | read instruction by instruction from the raw disassembly; argument offsets re-derived after every stack adjustment; each function body opened, none inferred from a name |
| `[0x9C7324]+0x218` is the Railroad advance, overlay value 3 = road and railroad | **H** | from `yields.md`; the setter's group-0 semantics are not decoded |
| `unit.+0x1F4` bit 1 marks the scientific leader | **H** | the roll passes `2` as the factory's seventh argument and `0x5C0300` tests bit 1 |
| Reveal Map affects all civs | **A** (static) | not observed live |

## 13. Open items

* `[0x9C3D74]` (tested `> -1` before the leader free-advance list is used) and `[0x9C3D70]` (the default
  government given in `Player::init`) are written by no instruction in the image; the scenario loader
  fills them. Their meaning is presumed (leader-record count validity and default government).
* The consumer of the achievement records (`0x57CCD0`, 7.4).
* Internals of the trade-network functions `0x57DE90`, `0x57D980`, `0x57E450` (called in step 7 and 8
  of 6.2) and the per-city call `0x4BE970` in `0x560050`.
* The UI hooks `0x535D80`, `0x537700`, `0x4DC8E0`, `0x4DCAF0`, `0x49FC50`, `0x611530`, `0x523080`.
* The semantics of the cell setter `0x5DA2A0` for (group 0, value 3) (10.1).
* (resolved in `goody-huts.md`: the other outcomes of the hut switch and the outcome roll.)
* The human steal list and the multiplayer handlers (10.6).
* Whether the Reveal Map effect on all civs (10.3) matches the shipped game.
* (resolved in `research-ai.md` 1: the valuation's `a` is the research-pick flag, `b` gates one random draw;
  open there: the rest of the category mask and the writers of the AI's defender and transport tallies.)

## 14. Random draws, in order

Every draw is `rand(n)` on the shared generator (`combat.md` section 1).

| event | draws |
|---|---|
| income pass, research step, cost, Great Library, queue cleaning, `canResearch` family | none |
| `acquire` step 9, scientific leader | one `rand(100)`, only when `first`, `a2`, `turn > 0` and flag `0x40000` (before the capital test); then the draws of the unit factory |
| `acquire` step 11/17, default pick | one valuation call per researchable advance, in ascending index (each may draw; `research-ai.md`) |
| `enterEra`, Scientific trait | `rand(count)` once if the candidate list is non-empty (7.3), then `acquire` |
| Theory of Evolution | none of its own (valuation is not used) |
| goody hut advance | per eligible advance in ascending index: `rand(100)` (the valuation with `a = 0` draws nothing); then one `rand(15)` for the message variant |
| steal technology (AI) | one valuation call per candidate, ascending index; no draws (`a = 0`) |

The order inside a single `acquire` is the step order of 6.2, with nested `acquire` calls (Philosophy
gift, Great Library inside the same turn) running at the point they are called.
