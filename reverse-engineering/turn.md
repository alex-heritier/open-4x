# The round and the player turn

Owns: the order in which one game round (a "turn" of the whole world) runs: the main loop that alternates
the human turn driver and the round processor, the round processor `0x4F5EF0` (two slot loops, then the
world phases), the AI unit phases of the first loop, the per-player economy/administration routine
`Player::turn` (`0x5604B0`, "the player turn"), and the small routines it calls that no other file owns
(first contact `0x55B540`, interest, the income pass `0x560160`, the rate rebalance `0x560320`, the AI
diplomacy rotation `0x441F10`/`0x43F370`, the cultural-city marks `0x4F5040`, AI idle-unit handling
`0x4F4F70`/`0x4F4EA0`). The world phases that have their own subject are in `world-events.md` (resource
depletion, volcanoes, plague) and `barbarians.md` (the slot-0 branch of `Player::turn`). Image base
`0x400000`; static VA = runtime VA. This is a **clean-room specification**: it contains no reference code and
every rule carries the address that shows it.

Evidence tags: **V** = the whole routine body was read in the disassembly and the statement is what the
instructions do; **I** = inferred from how a value is used; **H** = **HYPOTHESIS** (do not build on it
without a test); **O** = open (not decoded). Nothing here was checked by running the game.

## At a glance

* A round has **two player loops with different jobs**. The first loop (slots **31 down to 0**) runs the
  *units* and the AI planners; the second loop (slots **0 up to 31**) runs `Player::turn`, which is the
  *economy* (income, upkeep, research, **cities**, government timers). Cities are therefore produced,
  grown and starved **after** every player's units have moved in that round.
* Human players' own units are moved by the human turn driver `0x4F5550`, *before* the round processor
  runs; the round processor moves units only for the AI players (and for humans only in the cases listed in
  section 2).
* The world phases (AI diplomacy rotation, plague, volcano and flood style events, resource depletion every
  5th turn, cultural-city marks, trade network, score means) run **once per round after both loops**, and the
  turn counter `[0xA526AC]` is incremented **after** them (section 4).
* `Player::turn` is the same routine for humans and AI; the AI differences are three virtual calls (`+0x2C`
  before the money, `+0x30`, `+0x24`, `+0x28` after the government timers) and a handful of `human` tests.
* Barbarians are slot 0 and take the other branch of `Player::turn` (`P.+0x1C == 0`): no economy at all,
  only spawning (`barbarians.md`).

## 1. Globals used

| address | meaning | tag |
|---|---|---|
| `[0xA526AC]` | turn counter (0 during setup); incremented at the end of the round processor | V |
| `[0xA526BC]` | bit mask of human slots | V |
| `[0xA526C0]` | bit mask of slots in play | V |
| `[0xA5279C]` | N = number of civilizations (slots 1..N are the rotation range of section 4.1) | V |
| `[0xA52991]` | *sequential* byte: hot-seat / play-by-mail style turn order (every human takes a full turn in sequence) | V (use), I (name) |
| `[0xA527B4]` | byte set to 1 together with `[0xA52991]` by `0x48DDE0` and cleared with it by `0x48B7E0` / `0x54F5D0` (game setup); **H**: "human slots are driven by the round processor too" | V (writers), H (meaning) |
| `[0xA527B5 + slot]` | per-slot byte array written by the main loop `0x4F61B0` (`0x4F6575`, `0x4F658A`) | O |
| `[0xCC37BC]` | quit / abort flag; checked between players and before every world phase | V |
| `[0xA52690]` | number of cities in the world (`+1` in the city founder `0x5663C0`, `-1` in the city destroyer `0x4AECC0`, zeroed by `0x59FD80`; `[0xA52B48][continent]` is the per-continent count) | V |
| `0x47B530` | multiplayer-mode gate (true in a network game) | V |
| `0x6268C0` | message pump / abort check, called after each player in both loops | V |
| `0x9F8700`, `0x9FDB50`, `0xB38C60` | UI singletons (screen, advisor, message log); everything done through them is presentation only | I |
| `Player` array | base `0xA52E98`, stride `0x20E4` (8420), 32 slots; slot field at `+0x1C`; slot 0 = barbarians | V |

Pools (`node pointer - 0x1C = object`, entry `i` at `[array + 8*i + 4]`, last index in the second word):
cities `[0xA52E6C]`/`[0xA52E78]` (owner byte `+0x28`), units `[0xA52E84]`/`[0xA52E90]`, a per-owner pool
`[0xA52E3C]`/`[0xA52E48]` (owner byte `+0x2C`, byte flag `+0x30`; section 3.2 step 14).

## 2. The round processor `0x4F5EF0`

It is entered from the main loop `0x4F61B0` after the human-turn driver `0x4F5550` returned (section 5);
`CheckVictory` (`victory.md` section 11) runs right after it returns.

Entry: `0x6027C0(0x9FDB50)` (UI), then the cursor is switched to the busy cursor (UI only).

### 2.1 Loop A: slots 31 down to 0 (units and AI planners) **V**

For `slot` from 31 down to 0, with `P` = the player record, skipping everything when `[0xCC37BC]` is set:

1. **Human gate.** If `slot` is a human slot, the slot is processed only when `[0xA527B4] != 0` **or**
   `0x47B530` is true; otherwise it is skipped completely (its units were moved by `0x4F5550`).
2. Skip the slot unless it is in play (`[0xA526C0]`).
3. **Wake call.** If `[0xA52991]` (sequential) is set, **or** the slot is not human: `0x4F4F70(slot)`
   (section 2.3). A human slot in a non-sequential game does not get it.
4. `0x446840(P)` and `0x445EA0(P)` (the Player virtual `+0x3C`): the AI planning routines (`ai.md` open
   items: 6 KB and 3 KB of strategy code, **O**).
5. If `[0xA52991]` is clear **and** the slot is human: stop here for this slot (a non-sequential human only
   gets steps 1 to 4 when `[0xA527B4]` or multiplayer made it through the gate).
6. `0x561220(P)`: for every unit in the unit pool (index order, last index re-read each step) whose owner
   (`unit +0x34`) is `slot`: in a multiplayer game a unit with `unit +0x22C != 0` is skipped; otherwise
   `0x5C7700(unit)` is called. That per-unit turn (sea hazard, jungle disease, healing, auto-wake, movement
   reset) is specified in `unit-turn.md`; it contains no AI.
7. UI refresh `0x4DFB10(0x9F8700; slot)` (and `0x4DFAB0` when it returns true), then `0x449B20(P; 0)`, the
   UI refresh again, then `0x449B20(P; 1)`. `0x449B20` is the unit command pump (`unit-ai.md` 2): phase 1 runs the standing orders 2..14 of every unit that still has movement, phase 0 gives every other unit up to 15 turns at the unit AI.
8. `0x4F4EA0(slot)` (section 2.3), then the message pump `0x6268C0`.

### 2.2 Loop B: slots 0 up to 31 (`Player::turn`) **V**

For `slot` from 0 to 31 (again aborting on `[0xCC37BC]`): skip when not in play; when `[0xA52991]` is set
skip human slots; otherwise call `Player::turn` `0x5604B0(P)` (section 3), then the message pump. In a
non-sequential game **humans are included** (the sequential case runs a human's `Player::turn` from the
human-turn driver, `0x4F5550`, instead).

### 2.3 The two AI unit helpers **V**

* `0x4F4F70(slot)` (**V**, read in full this time): not a gameplay routine. For every unit of `slot` (other than the local player's) whose animation-state field `U.+0x2A0` is `7`, it either calls `U+0x280 .0x403CC0(1)` (when the byte `U.+0x24D` is set) or
  resets the state (`U.+0x2A0 = 1`, `U.+0x38C = 0`, `U.+0x378 = 1`, `U+0x280 .0x403CC0(0)`), and redraws (`0x4EEB20(0xA268B8)`) when any unit was touched. All the fields are in the animation block
  (`+0x278..+0x38D`, the same block `setPosition` calls into, `movement.md` 5 step 4), so **H** it only resets the unit sprite pose. It changes no order, movement or position.
* `0x4F4EA0(slot)` (auto-fortify idle AI units): for every unit owned by `slot` that is not the local
  player's, whose order byte `+0x64 == 1`, for which `0x405890(unit + 0x360; 7)` is true and whose word
  `+0x2A0 != 7`: set `+0x29C = 3`, `+0x2A0 = 7`, `+0x378 = 0x4058B0(...) - 1`, then `0x403CC0(unit + 0x280;
  0)`. (These are the activity/"idle for N turns" fields; the unit-record file owns the field names, **O**.)

## 3. `Player::turn` (`0x5604B0`)

One call per player per round (Loop B). `P` is the player record, `slot = P.+0x1C`, `human` = bit `slot` of
`[0xA526BC]`.

### 3.1 Barbarian branch

If `P.+0x1C == 0` the routine takes the barbarian branch and never reaches section 3.2: see `barbarians.md`.

### 3.2 Normal branch, in code order **V** (bodies of the callees as noted)

| # | address | step | detail |
|---|---|---|---|
| 1 | `0x560848..0x560890` | **Golden Age end** | if `P.+0x3C == turn` (`[0xA526AC]`): for every city `C` of `P` call `0x4B0E80(C)` (city yield recompute); the local player additionally gets the message `SUMMARY_END_GOLDEN_AGE`. (The Golden Age start is in the government code, `government.md`; the length constant is not in this routine.) |
| 2 | `0x5608BC..0x560915` | **UI "recent" counters** (humans only) | for `k` in `0 .. [0x9C3DB0]-1` (PRTO count) the int16 array `[P+0x15F8][k]` moves one step toward 0; for `b` in `0 .. [0x9C3D80]-1` (BLDG count) `[P+0x15E4][b]` is decremented when positive. The only reader of both arrays is the 30 KB UI function `0x42C8A0` (accessors `0x437C30`, `0x437C90`), so they are advisor "new item" markers, not game state (**I**). `P+0x15DC[b]` (building counts, `capture.md`) is a different array. |
| 3 | `0x560917..0x56095E` | **first contact scan** | for `q` = 1 .. 31, `q != slot`, `q` in play, and bit 0 of the relation word `P +0xEB0 + 4*q` clear (not yet met): `0x55B540(P; q)` (section 3.3). |
| 4 | `0x560978` | **AI hook** | if not human: virtual `+0x2C` = `0x444CC0` (per-turn AI state; `government.md` section 7). |
| 5 | `0x56097B..0x560A56` | **treasury interest** | `n = 0x55AA10(P; mask 8; onlyCity 0)` (count of small wonders whose flag word has bit 3, "Treasury Earns 5%", `primitives.md` 3.3.1); `g = P.+0x44 + P.+0x48`; `bonus = trunc(n*g / 20)` (`0x66666667` magic, `sar 3`, sign fix), and when `bonus > 50` then `bonus = 50` (a negative bonus is not capped, so a debtor with the wonder loses 5% per wonder); `g' = g + bonus`; when `g' < 0` the treasury becomes 0. The treasury is stored with the pair split of `economy.md` section 3.2. |
| 6 | `0x5609E7..0x560ADB` | **per-civ gold-per-turn deals** | for every other civ the tables `[0xA53CC8 + 4*...]` (stride 2105 dwords) and `Player +0xE34[q]` carry per-turn gold deals; humans and AI differ by the `0x47B530` and human tests at `0x560A11`/`0x560A1D`. The payment rules are in `diplomacy.md`/`economy.md` (**not re-derived here**). |
| 7 | `0x560AE2` | **income pass** `0x560160` | section 3.4. |
| 8 | `0x560AED..0x560BEF` | **per-civ payments** | second walk over the civs (`diplomacy.md`). |
| 9 | `0x560BF7..0x560CAB` | **upkeep payment** | `rand(4) == 0` (via `0x60BAB0(0xA526B4; 4)`) selects the order of the two payments: units/buildings. Payments are `0x55DFD0` (building upkeep) and `0x55CFB0(P; govt)` (unit support); when the treasury cannot cover one of them the treasury is set to 0 (pair split of 0) and, when `P.+0x30 >= 1`, `0x560280(P)` **sells one improvement**; otherwise `0x4C2350(P; treasury - cost)` is called (the shortfall consequence, **O**). |
| 10 | `0x560CB2` | **research** `0x562200` | `research.md` section 2.1. |
| 11 | `0x560CCF` | **cities and pools** `0x560050` | calls `0x4BE970(C)` (city turn, `happiness.md` section "per-turn city sequencer") for **every city of `P`** in pool order, decrements `P.+0x15C8` when positive, expires the Science Age (`research.md` section 11), then `0x5DAEB0(entry)` (writes 0 to byte `+0x30`) on every entry of the pool `[0xA52E3C]` owned by `P`. Skipped when `0x47B530` is true and `[0x990390] == 2`. |
| 12 | `0x560CDA..0x560D3A` | **government timers** | anarchy/revolution countdown and the other timers of `government.md` section 7; `0x55CBB0`, `0x5007B0(P)`, `0x55CE50`, culture accumulator `0x4F8E20(P + 0x181C)`. |
| 13 | `0x560D41` | **war weariness** `0x500AD0` | `government.md` section 6. |
| 14 | `0x560D4B..0x560DAA` | **contact flags** | for each civ in play that `P` has met: clear bit 6 of `+0xEB4[civ-1]`, and when the byte `+0xD90[civ]` is non-zero clear it with probability 1/3 (`rand(3) == 0`). |
| 15 | `0x560DB2..0x560DE0` | **AI hooks** | if not human: virtual `+0x30` = `0x445490`, then `+0x24` = `0x444A10` (revolution gate), then `+0x28` = `0x444B80` (mobilization check). |
| 16 | `0x560DE5` | **rate rebalance** `0x560320` | section 3.5. |
| 17 | `0x560DFD..0x561040` | **palace view** | `0x57CDA0`, `0x57CD30`; for the local human, outside multiplayer, with `[0x9C5B42]` clear: the dialog `PALACE_VIEW_UPGRADE`. Presentation only. |
| 18 | `0x561045..0x561215` | **treasury warning** (local human only) | projected income minus expenses from `0x55D550`, `0x55D6F0`, `0x55D600`, `0x55D310`, `0x55CFB0`; when the net is negative and `treasury + net < 10` the message `SUMMARY_LOW_TREASURY`. Presentation only. |

Consequences that matter for a port: research completes **before** the cities are processed (an advance that
arrives this turn is usable by this turn's production); the treasury is paid for upkeep **after** the income
pass; the interest is computed on the treasury **before** this turn's income.

### 3.3 First contact `0x55B540(P; q)` **V**

1. If `P` is not human **and** `q` is human, the call is forwarded as `Player[q].0x55B540(P)` (contact is always
   evaluated from the human's side when exactly one of the two is human).
2. For every unit `u` in the unit pool: the *apparent owner* is `u.+0x34`, except that when `u` has ability bit
   17 (**Hidden Nationality**, `0x5BC8B0(u; 17)`) and `slot` is not -1, not 0 and not `u.+0x34`, the apparent
   owner is 0.
3. If the apparent owner is `q` and `0x5BB650(u; slot, 0)` is true (visibility test, body not read, **O**): the
   tile `(u.+0x24, u.+0x28)` is "seen" by `P` when bit `slot` is set in the OR of the four per-civ visibility
   words (`cell +0x5C`, `+0x60`, `+0x64` and the tile helper `0x437A70(x, y) +0xD0`). Seen implies contact.
4. Otherwise, if the apparent owner is `P`'s own slot: for the nine tiles of the 3x3 block around the unit
   (spiral offsets 0..8 of `0x5E6E50`, the Y coordinate wrapped when `[0x9C755C] & 2`, X normalized by
   `0x426C00`, bounds checked) whose border owner (`cell vtable +0x98`, byte `cell +5`) equals `q`: contact.
5. After the scan, if contact was found: `0x501CD0(P; q, 0)` establishes it (relation bits, first-contact
   events; its body is in `diplomacy.md`'s territory, **O** here).

City tiles do not by themselves create contact; a unit must see a foreign unit, or stand on or next to foreign
territory.

### 3.4 The income pass `0x560160(P)` **V**

For each city `C` of `P` (owner byte `+0x28`), in pool order:

1. City flag: when `C.+0x50 == 1` (the production item is an improvement) and that BLDG row's flag word
   (`[0x9C40AC] + id*272 + 0xEC`) has bit 19 (`0x80000`, Capitalization) set, set bit 5 (`0x20`) of `C.+0x30`;
   otherwise clear it.
2. `P.+0xF8 += 0x4ACA50(C; 1, 0)`: the city's research stream (`economy.md`).
3. `t = 0x4ACA50(C; 2, 0) + P.+0x44 + P.+0x48`: the city's tax stream added to the treasury, **city by
   city**; when `t < 0` the treasury is set to 0 with the pair split of 0 (so a negative city never drags the
   treasury below zero, and the order of cities can matter), otherwise it is stored with the pair split.

### 3.5 Rate rebalance `0x560320(P)` **V**

Runs for every player each round (after the government timers and the AI hooks). Rates are in tenths:
`P.+0x1A4` luxury, `P.+0x1A8` science, `P.+0x1AC` tax. `cap = GOVT[P.+0xA0].+0x1BC` (row stride 488).

1. For each of the three rates, if it is greater than `cap`, set it to `cap` and run `0x5612F0(P)` (every city
   of `P` is recomputed through `0x4B10F0`).
2. Let `s` = the sum of the three rates. While `s < 10` (at most 10 iterations): if tax `< cap` then tax `+= 1`;
   else if science `< cap` then science `+= 1`; else if luxury `< cap` then luxury `+= 1`; else stop. Each
   increment is followed by a recompute of all of `P`'s cities (`0x4B10F0`); `s += 1`.

Example: government cap 6, rates (lux 0, sci 3, tax 4) sum 7: tax rises to 5, 6 (cap), then science 4 (sum 10).
If the sum is already 10 or more nothing changes. A sum above 10 is **not** reduced here.

## 4. The world phases after the loops **V** (bodies as noted)

After Loop B, if `[0xCC37BC]` is clear:

| # | address | what |
|---|---|---|
| 1 | `0x441F10(RT)` | AI-to-AI diplomacy rotation (4.1) |
| 2 | `0x4F5250` | plague scheduler (`world-events.md`) |
| 3 | `0x4F4380` | volcano events (`world-events.md`) |
| 4 | `0x4F4CB0` | resource depletion, every 5th turn (`world-events.md`) |
| 5 | `0x4F5040` | cultural-city marks (4.2) |
| 6 | (UI) | advisor checks for players with `+0xF4 < 2` (`0x4F73E0`, `0x53E900`); the debug string `begin city net update...` |
| 7 | `0x57DE90(0xB72888; 1)` | for every civ in play `q = 1..31`: `0x57D980(net; q, 0, flag, -1)` then `0x57E450(net; 0)` (trade-network recompute; **O**, `trade-network.md` is pending) |
| 8 | `0x441F80(RT)` | **O** (1280 bytes, not decoded) |
| 9 | `0x538480(RT)` | score running means (`victory.md` section 4) |
| 10 | `[0xA526AC] += 1` | the turn counter |
| 11 | `0x542230(0xB38C60)`, `0x6027C0`, `0x406120`, `0x5B04F0` | UI and housekeeping (**O**) |

`CheckVictory` is called by the main loop after this routine returns, so it sees the incremented turn counter
(`victory.md`).

### 4.1 AI diplomacy rotation `0x441F10` / `0x43F370` **V**

`N = [0xA5279C]`, `start = turn mod N`. For `i` = 0 .. N-1, `slot = ((i + start) mod N) + 1`; if the slot is in
play and not human: `0x43F370(Player[slot])`. `0x43F370(P)` repeats the same rotation (`start = turn mod N`
again) over the other slots: for each slot in play, not human, and with bit 0 of the relation word
`P +0xEB0 + 4*slot` set (contact made): call the Player virtual `+0x7C` with that slot (`0x43E470`, the
AI-to-AI diplomacy initiative; **O**, `diplomacy.md`). The rotation start moves by one slot per turn, so over
`N` turns every AI gets the first move once.

### 4.2 Cultural-city marks `0x4F5040` **V**

First clear bit 3 (`0x8`) of `city +0x30` in **every** city. Then, among cities with `city +0x13C > 2`
(per-turn culture from buildings), find the city with the highest `city +0x140 + 4*owner` (accumulated culture
of its owner in that city) and the best city of a *different* owner, and set bit 3 on those two cities. The
effect is a pair of flagged "cultural showcase" cities per round; the flag's consumers were not traced (**O**).
(The tie-breaking and the exact comparison direction follow the instruction order in `0x4F5040`; re-read the
routine before porting.)

## 5. The main loop and the human turn

`0x4F61B0` is the game's main state machine (30 KB, mostly UI: map loading, save/load, the title flow). The
part that matters: after setup it repeatedly calls `0x4F5550` (the **human turn driver**) until it returns with
the quit flag or with "turn finished", then calls the round processor `0x4F5EF0`, then `CheckVictory`.

`0x4F5550`: in hot-seat/PBEM mode (`[0xA52991]`) it shows the password dialogs (`MP_ENTER_PASSWORD`,
`MP_RETRIEVE_PASSWORD`), then runs the human's turn UI loop, then `CheckVictory(0)` and `0x4F6810(local)`
(`victory.md` section 8, the sequential tail). Everything the human does during his turn (unit orders, city
management, diplomacy screens) happens inside this routine and is driven by player input, so a headless port
only needs the *rules* of those actions, not the loop.

## 6. Golden vectors (hand-computed from the decoded rules)

| case | input | result |
|---|---|---|
| interest | `n = 1`, treasury 400 | `trunc(400/20) = 20` -> 420 |
| interest cap | `n = 1`, treasury 2000 | bonus 100 capped to 50 -> 2050 |
| interest, two wonders | `n = 2`, treasury 300 | `trunc(600/20) = 30` -> 330 |
| interest on debt | `n = 1`, treasury -100 | bonus -5 -> -105 -> treasury 0 |
| rotation | `N = 5`, `turn = 7` | `start = 2`; order of AI slots 3, 4, 5, 1, 2 |
| rotation | `N = 5`, `turn = 10` | `start = 0`; order 1, 2, 3, 4, 5 |
| rebalance | cap 6, (lux 0, sci 3, tax 4) | (0, 4, 6) |
| rebalance | cap 4, (lux 5, sci 5, tax 0) | clamp to (4, 4, 0), then tax 1..2 -> (4, 4, 2) |
| rebalance | cap 10, (0, 0, 0) | tax 10 -> (0, 0, 10) |

The rebalance rows follow the code; the cap-4 row: after clamping the sum is 8, two iterations raise tax to 2.

## 7. Verification status and open items

* Read in full: `0x4F5EF0`, `0x441F10`, `0x43F370`, `0x4F5040`, `0x4F4EA0`, `0x561220`, `0x560320`, `0x5612F0`,
  `0x560160`, `0x560050`, `0x55B540`, `0x57DE90` (call structure), the interest block of `0x5604B0`.
* Not read: bodies of `0x446840`, `0x445EA0`, `0x449B20`, `0x4F4F70`, `0x441F80`,
  the per-civ payment passes (steps 6 and 8) in this file, `0x4C2350`, the Golden Age *start*, `0x501CD0`,
  `0x5BB650`, the consumers of the cultural-city flag, `0x57E450` / `0x57D980`.
* **O**: a second round path exists for network games (`0x476330`, which reaches `0x4F5160`); it was located
  through the call graph but not read, so section 2 describes the single-player and hot-seat order only.
  Whether the multiplayer path reorders any phase is unknown.
* **O**: what barbarian and AI units do on their turn is decided by the planners `0x446840` / `0x445EA0` /
  `0x449B20` in loop A (step 4 and 7 of section 2.1), not by the per-unit turn (`unit-turn.md`).
* **H**: the meaning of `[0xA527B4]` and of the per-slot bytes `[0xA527B5 + slot]`.
* Dynamic confirmation of the loop order (units before economy within one round) was not attempted.
