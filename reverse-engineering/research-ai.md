# Research, the AI side: valuing and picking advances

Standalone specification of how the game scores one advance for one player (`0x448BF0`), and what is
built on that score: the automatic research pick (`0x449530`), the human's suggestion, the thief's pick
(`0x44A5B0`), the goody hut's tie-break (`research.md` 10.5) and the diplomatic price of an advance
(`diplomacy.md` 8). Companion of `research.md`, which owns everything else about research (beakers,
cost, the step, `acquire`, eras). Reference implementation: `rust/src/research_ai.rs` (`Valuer`,
`Profile`, `Tables`, `flavor_overlap`, `category_mask_from_flags`); the playable game feeds it from
`src/rules_data.rs` (`src/research.rs`).

Notation as in `research.md` section 0: `f(this; a, b)` is a `__thiscall`, `P` a Player record, `T` the
number of advances, `tdiv` C division, `ftol` truncation toward zero, `rand(n)` the gameplay die
`0x60BAB0(0xA526B4; n)`. Evidence tags: **A** decoded from the disassembly (addresses given); **E**
executed and compared; **H** hypothesis; **C** a choice of this repository's clone, not the binary.

Nothing in this file is **E**. The valuation was read, not executed: it needs the whole rule set (units,
buildings, governments, citizens, the flavor matrix) and a player record, and no run of the exact bytes
was made. The unit tests of `rust/src/research_ai.rs` check the arithmetic of the reading, not the
executable. An earlier reading of this routine swapped the two flags (section 1); the figures in this file
replace it and no number from the earlier reading is used.

## 1. The entry `value(P; t, a, mode)` `0x448BF0`, `ret 0xC` (A)

Player vtable slot `+0x54` (vtable `0x66CB38`, the same table for humans and computer players).
Arguments after `this`:

* `t`: the advance, `0 <= t < T`. Anything else returns `10` at once.
* `a`: the **research-pick flag**. It selects the heavier weights (`+6` on treaty advances, the unit
  build weights `64`/`128` against `4`, `+16` against `+4` on valued buildings, `+64`/`+32` for a missing
  defender or transport against `+4`), replaces the base-cost term by `256 / turnsLeft`, and switches the
  tail (section 5) from the "who else knows it" tail to the "who else is researching it" tail.
* `mode`: only gates one random draw (section 4). It matters only when `a` is set.

Callers and the pair they pass (`a`, `mode`):

| caller | `a` | `mode` | purpose |
|---|---|---|---|
| default pick `0x449530`, automatic (`research.md` 9.1, 9.2) | 1 | 1 | the computer's target; the non-interactive chooser; the research step's fallback; the Philosophy gift |
| default pick, interactive suggestion | 1 | 0 | the advance the dialog pre-selects |
| thief's pick `0x44A5B0` | 0 | 0 | what to steal |
| goody hut `0x55B8B0` outcome 6 | 0 | 0 | the tie-break of the hut (`research.md` 10.5) |
| deal scorer `0x440EE0`, item type 8 | 0 | 0 | the price of an advance in a trade (`diplomacy.md` 8) |

The result is a plain integer: larger is more wanted. It is never below `10`.

## 2. The score, in order (A)

`score` starts at `40`. `fl` is `TECH[t].flags` (bit names in `research.md` 1.1), `extra = a ? 6 : 0`.
The sections are applied in this order; the addresses are those of the first instruction of each part.

### 2.1 The advance's own flags (`0x448C3F .. 0x448DF0`)

| condition | effect |
|---|---|
| `fl & 0x1` Enables Diplomats | `score = 42 + extra` (**assigned**, not added; the base of 40 is thrown away) |
| `fl & 0x2` Irrigation Without Fresh Water | `+4` |
| `fl & 0x4` Bridges | `+2` |
| `fl & 0x8` Disables Flood Plain Disease | `+4` |
| `fl & 0x10` Conscription, `fl & 0x20` Mobilization | `+1` each |
| `fl & 0x40` Recycling, `fl & 0x80` Precision Bombing | `+1` each |
| `fl & 0x100` MPP, `0x200` Right of Passage, `0x400` Military Alliance, `0x800` Trade Embargo | `+2 + extra` each |
| `fl & 0x1000` Double Wealth | `+2` |
| `fl & 0x2000` Trade over Sea, `0x4000` Trade over Ocean | `+4`, `+8` |
| `fl & 0x8000` Map Trading, `0x10000` Communication Trading | `+2` each |
| `fl & 0x20000` Not Required for Era Advancement | remembered (`notRequired`); otherwise `+1` |
| `fl & 0x40000` Double Worker Rate | `+8` |

### 2.2 The rule tables (`0x448DF0 .. 0x4491EF`)

Each is a walk over a whole section of the loaded rules.

| source | effect |
|---|---|
| `TFRM` (13 worker jobs) whose `required_tech == t` | `+1` each |
| `GOOD` (resources) whose `prerequisite == t` | `+16` each |
| every `TECH` row having `t` among its four prerequisites | `+1` per occurrence |
| Space Race on (`[0xA5267C]` bit 1): each advance `o` that has `t` as a prerequisite and each `BLDG` whose `required_advance == o` and that is not an already built great wonder (`0x538FE0`) and makes spaceship parts | `+1`, and `spaceBoost` |
| `PRTO` whose `available_to_civs` has the player's race bit (`0x56AAB0`) and `required_tech == t` | `+2`; if its `ai_strategies` has bit 0 (offense) or bit 1 (defense): `+4`, and when it needs no resource (`required_resource_1..3` all `-1`): `+4` if `a == 0`, else `+128` for a Militaristic race (trait 0, `RACE` vtable `0x53A080`) and `+64` otherwise |
| the same `PRTO`, defense bit, and the player has **no** defenders (`int16 P.+0x10A + int16 P.+0x14A == 0`) | `+64` if `a`, else `+4` |
| the same `PRTO`, naval-transport bit 10, and no transports (`int16 P.+0x11C == 0`) | `+32` if `a`, else `+4` |
| `GOVT` whose `prerequisite_tech == t` | `+1` each, and `govtBoost` |
| `BLDG` whose `required_advance == t`, skipping a great wonder (`other_characteristics & 4`, `0x4490E9`) that is already built | `+2`; `+4` more for a great wonder; `+16` if `a` else `+4` when `improvement_flags & 0x1800` (`test ch, 0x18` at `0x449122`); `+1` and `spaceBoost` for a spaceship part under the Space Race; and `+ (overlap - 50)` where `overlap = flavorOverlap(BLDG.flavors, RACE.flavors)` (section 3), so a building the civ's flavors dislike lowers the score by up to 50 |
| `CTZN` whose `prerequisite == t` | `+2` each |

### 2.3 The cost or urgency term (`0x4491EF`)

Only when `score > 0` so far:

* `a == 1`: `score += 256 / max(turnsLeft(P; t, remainingOnly = 1), 1)` (`research.md` 9.4). With
  `mode == 1` add two more terms, `rand(32) & 0xFFFF` and a second `rand(32) & 0xFFFF`, drawn in this
  order (the multiplayer test `0x47B530` is false in a single process).
* `a == 0`: `score += baseCost(P; t, neutral = 1)` (`research.md` 4.1): an advance is worth what it
  would cost a human, so a trade price scales with the era.

Then `score *= 2` if `govtBoost`, and `score *= 2` again if `spaceBoost`.

## 3. The flavor overlap `0x52D770(flavors; techMask, raceMask)` (A)

A flavor mask has up to 32 bits. `FLAV` is a square matrix of percentages, `rel[i][j]`, the "relation
between flavor `i` and flavor `j`".

```
if the matrix is empty:            return 50
count = 0;  sum = 0
for i in bits of techMask (ascending):
    for j in bits of raceMask (ascending, j < matrix size):
        w = rel[i][j]                                    // 0 when the row is short
        if w >= 100:  return 100                         // the first perfect pair ends it
        count += 1;  sum += w
return count == 0 ? 50 : tdiv(sum, count)
```

A mask with no bits (an advance without flavors, a civ without flavors) therefore gives `50`: neutral.

## 4. The two tails (A)

Both are applied after the doubling of 2.3.

### 4.1 `a == 1`, the research pick (`0x4492C8`)

Skipped for a human player (the dialog suggestion is not shaded by what the rivals do). For a computer
player:

1. If the advance's category mask (`Game+0x510[t]`, `[0xA52B68]`, section 6) has a bit in common with
   `RACE.build_often` (mem `+0x954`): `score = tdiv(3 * score, 2)`.
2. Count the contacted rivals `q` (in play, `q != P`, slot `q >= 1`, contact = bit 0 of the relation word `P.+0xEB0 + 4q`) whose
   current research `+0xFC` is `t`; `score = tdiv(score, clamp(rivals, 1, 4))`. A rival without contact
   does not count. Rival research is divided in, not subtracted: two rivals halve the want.

### 4.2 `a == 0`, the trade tail

1. Count the contacted civs that **know** `t` (`knowsTech(q, t)`). With fewer than two, `score *= 2`: an
   advance nobody else has is dear.
2. If `t` is the player's current research (`+0xFC == t`): `base = baseCost(P; t, 1)`;
   `left = max(base - P.+0xF8, 0)` (the beakers already stored are in `+0xF8`); when `base != 0`,
   `score = tdiv(left * score, base)`. An advance already half researched is worth half.
3. `score = tdiv(2 * score, 3)`.

## 5. The flavor stage and the clamp (`0x449464 .. 0x449530`) (A)

```
matched = flavorOverlap(TECH[t].flavors, RACE.flavors)
f = (matched >= 100) ? 1.5f
  : (TECH[t].flavors == 0) ? 1.0
  : matched * matched * 1e-4f                               // single-precision constants
score = ftol((0.3f + f) * score)                            // 0.3f and 1e-4f are floats widened to double
notRequiredHalve = notRequired && matched < 90
if score > 0 and (fl & 0x80000 Cannot Be Traded):  score = ftol(1.5 * score)
if notRequiredHalve:  score = tdiv(2 * score, 3)
return max(score, 10)
```

The multiplier is therefore `0.3 + 1.5 = 1.8` for a perfect match, `1.3` for an advance with no flavors,
`0.3 + 0.64 = 0.94` at 80 and `0.3 + 0.25 = 0.55` at 50. The constants are the 32-bit floats of the image
(`1e-4f` is `0.0000999999974737875`, `0.3f` is `0.300000011920929`); computing in `f64` from the decimal
literals gives results one lower at some inputs, which is why the code keeps the float constants. An
advance that is not required for era advancement and sits outside the civ's flavors is worth a third
less.

## 6. The category mask `0x443730` (`Game+0x510[t]`) (A for the advance-flag part, H for the rest)

A per-advance mask of what the advance enables, built once at load. It feeds only step 1 of 4.1, through
`RACE.build_often`. The part contributed by the advance's own flags is read in full (the eighteen
`(flag, bits)` pairs at `0x44375A .. 0x44387C`):

| advance flag | category bits | advance flag | category bits |
|---|---|---|---|
| `0x1` | `0x3` | `0x400` | `0x1` |
| `0x2` | `0x80` | `0x800` | `0x1000` |
| `0x4` | `0x2` | `0x1000` | `0x800` |
| `0x8` | `0x80` | `0x2000` | `0x1000` |
| `0x10` | `0x2` | `0x4000` | `0x1000` |
| `0x20` | `0x101` | `0x8000` | `0x2000` |
| `0x40` | `0x100` | `0x10000` | `0x1000` |
| `0x80` | `0x40` | `0x40000` | `0x1d90` |
| `0x100` | `0x2` | | |
| `0x200` | `0x2001` | | |

The other contributions of `0x443730` (the worker jobs, resources, units via `0x443600`, buildings via
`0x443300`, governments and citizens) are not decoded. A port that omits them gets a category mask that
is too small, so `build_often` matches less often and the `3/2` factor applies to fewer advances.
`rust/src/research_ai.rs` and the playable game use the advance-flag part only (**C**).

## 7. The picks built on the score

```
defaultPick(P; mode)        0x449530      best researchable advance, or T
    best = -1;  bestScore = 0
    for t in 0 .. T-1:  if canResearch(P; t):
        v = value(P; t, 1, mode);  if v > bestScore:  bestScore = v;  best = t     // first wins a tie
    return best == -1 ? T : best

stealPick(Thief; Victim)    0x44A5B0      best advance to steal, or -1
    for t in 0 .. T-1:  if canResearch(Thief; t) and knowsTech(Victim; t):
        v = value(Thief; t, 0, 0);  if v > bestScore:  ...
```

The valuation draws random numbers only with `a == 1`, `mode == 1`, so `defaultPick` consumes two draws
per researchable advance (in ascending index, `rand(32)` twice) and `stealPick` none.

## 8. Worked values (the port, not the executable)

These come from `rust/src/research_ai.rs` (`cargo test`), which sets up three civs and three isolated
advances of row cost `3` (base cost 72):

| case | arithmetic | result |
|---|---|---|
| `a = 0`, no contacts | `40 + 1 + 72 = 113`; nobody knows it, `x2 = 226`; `2*226/3 = 150`; no flavors, `x1.3` | **195** |
| `a = 1`, human, 8 turns left | `41 + 256/8 = 73`; human skips the tail; `x1.3` | **94** |
| `a = 1`, `mode = 1`, both draws `7` | `73 + 14 = 87` | **113** |
| `a = 1`, computer, one rival researching it | divided by 1 | **94** |
| `a = 1`, computer, two rivals | `73 / 2 = 36`; `x1.3` | **46** |
| `a = 0`, two contacted civs know it | base cost drops to 24; `41 + 24 = 65`; no doubling; `2*65/3 = 43` | **55** |
| `a = 0`, current research with 30 of 72 stored | `226 -> 42*226/72 = 131 -> 2*131/3 = 87` | **113** |
| `a = 0`, Cannot Be Traded | the 195 case times 1.5 | **292** |
| `a = 1`, human, an offensive unit enabled, no resource | `41 + 2 + 4 + 64 = 111`; `+32 = 143` | **185** |
| the same, Militaristic | `41 + 2 + 4 + 128 = 175`; `+32 = 207` | **269** |

The playable game feeds the real `conquests.biq` tables. For Japan at the start of a game its first
choices value, in the order of the dialog, `Pottery 158`, `Bronze Working 231`,
`Masonry 257`, `Alphabet 286`, `Warrior Code 215`, `Mysticism 249` (`a = 0`, the trade price basis;
regression values of `src/research.rs`, **C**-dependent because the category mask and the AI tallies of
sections 6 and 9 are the clone's).

## 9. What the clone feeds in (C)

* `Profile.defenders`: the count of the civ's units with both attack and defense above zero (the writers
  of `P.+0x10A` and `P.+0x14A` are not decoded). `Profile.transports`: `0` (the game has no ships).
* The category mask: the advance-flag part only (section 6).
* `space_race`: false. `wonder_built`: false (the game has no wonders).
* `RACE.flavors`, `build_often` and the Militaristic trait come from the BIQ rows of the four
  civilizations; the `FLAV` matrix, `PRTO`, `BLDG`, `GOVT`, `CTZN`, `TFRM`, `GOOD` rows are the shipped
  rules reduced to what the valuation reads (`biq/examples/gen_game_rules.rs`).

## 10. Open items

* The remaining contributions to the category mask (`0x443730`: jobs, resources, units, buildings,
  governments, citizens) and whether `build_often` is tested against exactly that mask.
* The writers of `P.+0x10A`, `P.+0x14A` (defender tallies) and `P.+0x11C` (transport tally). They are
  maintained by the AI's planners (`ai.md`).
* The second argument of the tail count: the loop of 4.1 starts at slot 1 (slot 0 is the barbarians); it
  is read that way, not run.
* An execution of `0x448BF0` against a synthetic world, to upgrade section 1 to **E**.
