# Governments, revolution, war weariness and mobilization

Owns: the GOVT record as the engine reads it; how a government is adopted (the setter, the
revolution, the anarchy countdown, the end of anarchy, the AI's choice and the gate that makes an AI
consider a revolution); war weariness (the counters, what feeds them, per-turn decay, the
Democracy collapse, the AI's refusal); mobilization (the AI trigger, the extra shield, the build
gate, how peace clears it); peace and the declaration of war as far as they touch those counters
(the pair array, the kicker, the calls to arms of allies);
the player initializer's starting government. Reference: `rust/src/government.rs` (the decisions
and the arithmetic), `rust/src/yields.rs` (what a government does to a tile), `rust/src/buildable.rs`
(the mobilization build gate). Image base `0x400000`, static VA = runtime VA. Static analysis only
(radare2 disassembly, byte scans, the shipped `conquests.biq`, `Civilopedia.txt`). Verified unless
marked **HYPOTHESIS**. Message dialogs, the Foreign Advisor and everything that only draws are out
of scope.

## At a glance

* A government is a 488-byte GOVT row (table `[0x9C71D8]`, count `[0x9C3DA8]`); a player keeps the
  index in `Player +0xA0`. Two rows are special and the GOVT section loader `0x595AE0` marks them:
  the **transition** row (GOVT `+0x10`, Anarchy, global `[0x9C3DD0]`, `0x595B61`) and the
  **default** row (GOVT `+0x0C`, Despotism, global `[0x9C3D70]`, `0x595B53`).
* Every adoption goes through `Player::setGovernment` (`0x55CD60`): it stores the index, shrinks the
  cities of a forced-resettlement government, rebuilds the free-building set, recomputes the
  building upkeep of every city and then every city's totals.
* A revolution (`0x55CE50`) switches to the transition government at once and draws the length of
  anarchy (`0x53A860`: `2 + next(3) + next(3) + min(3, 3 * cities / OCN)`, a Religious civ always 2,
  an AI capped by the difficulty row). A length of 1 or less ends it on the spot. The countdown
  `Player +0x9C` runs in the turn routine `0x5604B0`; at 0 `0x55CBB0` ends the revolution: an AI
  asks its chooser, a human is shown the `CHANGE_GOVERNMENT` list.
* The AI scores every government (`0x4446C0`: 8 per city without the tile penalty, 8 more with the
  trade bonus, a corruption table, military police, minus unit support, upkeep and a war-weariness
  term, then 9/8 for the favorite and 7/8 for the shunned) and takes the best (`0x4448F0`). Each
  turn an AI player rolls a 1-in-`r` die (`0x444A10`, `r` 64 unless war, peace or religion changes
  it) and on a 0 starts a revolution if its best government is not the one it has (`0x444970`).
* **War weariness** is one counter per ordered pair, `Player +0xCB4[civ - 1]`. At war it gains the
  *whole* sum of the two pairs' second incident accumulators every turn, 1 for a unit abroad, 1 for
  mobilization, and falls by 1 (to a floor of 30) only when nobody invades. At peace it loses
  5 percent a turn. In every city of a player with a weariness government, a counter above 30
  makes citizens unhappy, in proportion to the city's size (`0x4BD780`, 5.4.1). A High-weariness
  government (Democracy) falls into anarchy when the average exceeds 90, and an AI will not even
  score it at 90.
* **Mobilization** (`Player +0xA4`) adds a shield to every tile that already yields one in a city
  building a military unit (`0x4BFEE0`), blocks every peacetime improvement (`0x56A2A0`), adds one
  weariness a turn, and is cleared only by peace (`0x5025B0`), which also halves one incident
  accumulator and zeroes the other.
* Declaring war on a civ the declarer's attitude likes, with no reason and no hostile act of ours
  against it, adds 30 or 60 weariness and removes 30 from the victim (`0x501F20`; the attitude class
  is the Player vtable `+0x88` method `0x440AD0`).
* The weariness routine `0x500AD0` also **calls the allies to arms**: each turn every enemy military
  unit standing on a tile of a player's territory makes that player's allies and mutual-protection
  partners declare war on the enemy (section 5.2).

## 1. The GOVT record

Memory offsets (record base = `[0x9C71D8] + 0x1E8 * index`). The `conquests.biq` body offset of the
leading fields is the memory offset minus `0x0C` up to the ruler titles, the rest follow the row
reader `0x5E3E80`; `biq/src/sections/govt.rs` has the full file layout (the relation table of
`n * 12` bytes sits at body `+0x18C`, so a body offset `V + k` is `0x18C + 12 * n + k`, `n = 8` in
the shipped file). Every number below was read through the reader and matched to the Civilopedia
page of the government.

| memory | field | what the exe does with it |
|---|---|---|
| `+0x0C` | default type | `0x595B53`: the last row with it set becomes `[0x9C3D70]` (Despotism); read once, by the player initializer `0x567E5D` |
| `+0x10` | transition type | `0x595B61`: becomes `[0x9C3DD0]` (Anarchy); 14 readers (below) |
| `+0x14` | requires maintenance | `0x55CFB0` returns 0 without it: Anarchy pays no building upkeep |
| `+0x1C` | standard tile penalty | one less of a food, shield or commerce above 2 (`yields.md`); the AI scores a government without it |
| `+0x20` | standard trade bonus | +1 commerce on a tile that already has some (`yields.md`); the AI scores it |
| `+0x24` | xenophobic (Conquests tail) | consumer not located |
| `+0x28` | forced resettlement | `0x55CD60` and the capture path `0x56515B` shrink cities (section 3) |
| `+0x2C` | name (64 bytes) | popup lists, the `REVOLUTION` text (`0x55CEDE`) |
| `+0x18C` | corruption and waste class | AI score table (section 4); the corruption math is `economy.md` |
| `+0x190` | immune to (an espionage mission row) | per the `biq` crate, compared at `0x445229`, `0x44529C`, `0x44530E` (inside the AI method `0x444CC0`) |
| `+0x194`, `+0x198` | experience level of new diplomats / spies | spies: `0x52A062` indexes `EXPR` (per the `biq` crate) |
| `+0x19C` | pointer to the `n * 12` byte relation table (unused dword, propaganda modifier, resistance modifier) | `0x5280D4` and `0x4ABFB9` (per the `biq` crate) |
| `+0x1A0` | hurry method | 0 none (Anarchy), 1 forced labor, 2 pay; `0x433EE0`, `0x436A10`, `0x4B5290` |
| `+0x1A4` | assimilation chance, percent | `0x4AC23A` compares it with `next(100)` (per the `biq` crate) |
| `+0x1A8` | draft limit | consumer not located |
| `+0x1AC` | military police limit | AI score (section 4) |
| `+0x1B4` | ruler-title count | `0x53AA68` is the modulus that picks a title (per the `biq` crate) |
| `+0x1B8` | prerequisite tech | `-1` none; `0x4448F0` and the human list `0x55CC74` ask `0x561440` |
| `+0x1BC` | rate cap | cap on each of the three rates, 10 = 100% (`economy.md`) |
| `+0x1C0` | worker rate | 1, 2, 3, 4 in steps of 50% (`workers.md`) |
| `+0x1D0`, `+0x1D4..+0x1DC`, `+0x1E0` | free units, per town / city / metropolis, gold per unit | `0x53A960` (`economy.md`); the AI recomputes it for a candidate (`0x55D310`) |
| `+0x1E4` | war weariness class | 0 none, 1 low, 2 high; `0x4446C0`, `0x444A10`, `0x560D18`, `0x43E65C`, `0x440B9C`, `0x4474EA` |
| `+0x1C4..+0x1CC` | three dwords the editor never writes | stock `(0,0,0)`, `(-1,0,0)`, `(1,1,0)` or debug-heap fill; consumer not located |

### 1.1 The eight shipped rows

Row index equals the GOVT file order. Corruption classes: 0 minimal, 1 nuisance, 2 problematic,
3 rampant, 4 catastrophic, 5 communal. `rust/src/government.rs` `SHIPPED` holds the same table.

| row | government | upkeep | tile penalty | trade bonus | resettle | corruption | hurry | draft | police | prerequisite | rate cap | worker steps | free units (base; town / city / metropolis) | gold per unit | weariness |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | Anarchy | no | yes | no | no | 4 | none | 0 | 0 | none | 100% | 1 | none pay (`-1`) | 1 | none |
| 1 | Despotism | yes | yes | no | no | 3 | forced labor | 2 | 2 | none | 100% | 2 | 0; 4 / 4 / 4 | 1 | none |
| 2 | Monarchy | yes | no | no | no | 2 | pay | 2 | 3 | Monarchy (19) | 100% | 2 | 0; 2 / 4 / 8 | 1 | none |
| 3 | Communism | yes | no | no | no | 5 | forced labor | 2 | 4 | Communism (46) | 100% | 2 | 0; 6 / 6 / 6 | 1 | none |
| 4 | Republic | yes | no | yes | no | 1 | pay | 1 | 0 | The Republic (18) | 100% | 2 | 0; 1 / 3 / 4 | 2 | low |
| 5 | Democracy | yes | no | yes | no | 0 | pay | 1 | 0 | Democracy (34) | 100% | 3 | 0; 0 / 0 / 0 | 1 | high |
| 6 | Fascism | yes | no | no | yes | 1 | forced labor | 2 | 4 | Fascism (82) | 100% | 4 | 0; 4 / 7 / 10 | 1 | none |
| 7 | Feudalism | yes | no | no | no | 2 | forced labor | 2 | 3 | Feudalism (22) | 100% | 2 | 0; 5 / 2 / 1 | 3 | low |

The tile penalty belongs to exactly two governments, Anarchy and Despotism, and the trade bonus to
Republic and Democracy (`yields.md`).

### 1.2 RACE and DIFF fields read

| record | memory | field | read by |
|---|---|---|---|
| RACE (stride 2420, `[0x9C71D0]`) | `+0x920` | shunned government | `0x4446C0` (7/8 of a positive score) |
| RACE | `+0x924` | favorite government | `0x4446C0` (9/8 of a positive score) |
| RACE | `+0x948` | trait mask; vtable `[0]` is `hasTrait(bit)` | Religious (bit 4): `0x53A860`, `0x444970`, `0x444A10` (`capture.md` section 11) |
| DIFF (stride 124, `[0x9C40C0]`, row `[0xA52684]`) | `+0x48` | body `+0x44`: `0 0 0 4 3 2 2 1` Chieftain to Sid | cap on the AI's anarchy turns (`0x53A860`, `0x53A92D`); the AI only |
| DIFF | `+0x5C` / `+0x60` | body `+0x58` / `+0x5C`: flat `0 0 0 4 8 12 16 24`, per city `0 0 0 1 2 3 4 8` | extra free unit support of an AI (`0x55D310`, `0x55D3F4`) |
| DIFF | `+0x68` | body `+0x64`: `20 12 10 9 8 7 6 4` | the AI's box cost factor X (`0x5660E0`, `economy.md`) |

The DIFF row is selected by the *human's* level `[0xA52684]` for every non-human player. The `biq`
crate labels body `+0x44` "citizens quelled by military" and `+0x58` / `+0x5C` "extra starting
units" (after the editor dialog controls); the consumers above use them as the AI's anarchy cap and
free unit support, and the numbers (Sid 1 turn, Deity 16 and 4) fit those uses. **HYPOTHESIS:** the
crate's labels are attached to the wrong dwords.

## 2. Player fields used

| `Player +` | meaning | notes |
|---|---|---|
| `0x1C` | civ id | 0 is the barbarians |
| `0x20` | race index | |
| `0x2C` | capital city id | `-1` none |
| `0x30` | difficulty level of a human, the RULE `[0x9C7288]` for an AI (`0x567E79..0x567E85`) | **HYPOTHESIS** for the name; set by the initializer |
| `0x34` | AI revolution cooldown, turns | counted down by `0x444A10` |
| `0x3C` | turn the golden age ends | `[0xA526AC]` is the current turn |
| `0x9C` | anarchy countdown | `0x55CE93`, `0x560CEE` |
| `0xA0` | government index | |
| `0xA4` | mobilization (0 or 1) | message 48 stores the new value, `0x5025B0` clears it |
| `0x194` | number of cities | |
| `0x1B0 + 0x4C * civ` | the pair array: 19 dwords about `civ` | section 5.1 (other documents number the same dwords from `0x1C4`) |
| `0xBB0[civ]` | the AI's war and peace timer against `civ` | written by `0x501F20` and `0x5025B0` (section 5.5, 6.4) |
| `0xCB4[civ - 1]` | war weariness against `civ` | section 5 |
| `0xD30[civ]` | at war with `civ` (byte) | the table `0xA53BC8` of `combat.md` 14.3 |
| `0xDB0[civ]` | a counter about `civ`, writer not located; the victim's value of 0x200 or more starts the violation scan in `0x501F20` (`combat.md` 14.3) | zeroed in both directions by a declaration of war (`0x502153`, `0x502160`) |
| `0xEB4[civ - 1]` | relation bits | bit 0 met, bit 6 cleared each turn, bit `0x20` counted by `0x500F50` |
| `0xF30 + 4 * civ` | the treaty word about `civ`: `0x01` mutual protection, `0x02` right of passage, `0x04` alliance | section 5.2 |
| `0xFB0[civ]` | alliance masks | set by treaty clause type 1 in `0x502D40`; read by the tail of `0x5025B0` |
| `0x181C` | sub-object | `0x4F8E20(this + 0x181C)` runs once a turn (not decoded) |

## 3. Adopting a government

### 3.1 `Player::setGovernment` (`0x55CD60`, `ret 4`)

```
old = P.+0xA0 ; P.+0xA0 = g
if g != old and GOVT[g].+0x28 (forced resettlement):
    for every city owned by P: 0x55CD00(city)         ; section 3.2
0x55A560(P)                                            ; rebuild the free-building set (buildable.md)
0x55CF10(P)                                            ; city.+0x2C = building upkeep, for every city of P
for every city owned by P: 0x4B0E80(city)              ; recompute the totals (yields.md section 5)
```

The setter never touches mobilization (`+0xA4`) or the anarchy countdown. Callers: the message
handler `0x46F8B0` (type 43, `Player[msg.+0x30].setGovernment(msg.+0x34)`, no validation), the
revolution's own end `0x55CBB0` and start `0x55CE50`, and the player initializer `0x567C80` (three
sites).

### 3.2 Forced resettlement (`0x55CD00`)

A city of size 1 loses nothing. Otherwise `0x4BA230(city, n, -1, 0)` removes `n` citizens: 3 above
`[0x9C72E8]` (city maximum, 12) and above 3, else 2 above `[0x9C72E4]` (town maximum, 6) and above
2, else 1. Applied to every city of a player adopting a Fascism-type government and to every city
that player captures (`0x56515B`). `government::resettlement_loss`.

### 3.3 The revolution (`0x55CE50`, `this` = the player)

```
if [0x9C3DD0] == -1 or P.government == [0x9C3DD0]:  return        ; no anarchy row, or already in it
setGovernment(P, [0x9C3DD0])                                        ; Anarchy at once
turns = 0x53A860(P.civ)                                             ; section 3.4
if turns <= 1:  jmp 0x55CBB0                                        ; no anarchy at all
P.+0x9C = turns
if P.civ == [0x9FD4BC] (the local player):  popup "REVOLUTION" with the Anarchy name
```

Callers: the AI (`0x444970`), the message handler (type 42, `0x46FC70`: `startRevolution(
Player[msg.+0x30])`), two Domestic Advisor handlers (`0x4DC5C0` at `0x4DC785`, `0x4DC8E0` at
`0x4DCAB6`) and the turn routine (the Democracy collapse, `0x560D2F`). The government is never
asked for here: the new one is chosen when anarchy ends.

### 3.4 How long anarchy lasts (`0x53A860(civ)`, `ret 4`)

* A Religious civ (`hasTrait(4)`) always gets 2 and rolls nothing.
* Else `2 + next(3) + next(3) + min(3, 3 * cities / OCN)`; two draws of the gameplay `Random`
  (`0xA526B4`), the city term uses the optimal city number `0x5676C0` (`economy.md`).
* A civ outside the human mask is capped at `DIFF[[0xA52684]] +0x48` when that is non-zero and
  below the roll: 0 0 0 4 3 2 2 1 from Chieftain to Sid. A Sid AI never suffers more than one
  turn, which `0x55CE91` treats as no anarchy at all.

`government::anarchy_turns`. The longest anarchy a human can draw is `2 + 2 + 2 + 3 = 9` turns.

### 3.5 Ending the revolution

The turn routine `0x5604B0` (`0x560CD4..0x560D06`) runs for every player: while the government is
the transition one it decrements `+0x9C` (if positive) and at 0 calls `0x55CBB0`.

`0x55CBB0` (`this` = the player):

* **non-human** (bit not in `[0xA526BC]`): `g = Player.vtable[+0x1C]()` (the chooser `0x4448F0`);
  `-1` does nothing, else `setGovernment(g)`;
* **human, the local player**: if the multiplayer gate `0x47B530` is true it only marks the choice
  pending (`[0x74CFC4] = 2`, `[0x74CFC8..0x74CFD4] = -1`) and returns; otherwise the
  `CHANGE_GOVERNMENT` popup lists every government except the transition one whose prerequisite
  (`+0x1B8`) is `-1` or known (`0x55CC74..0x55CCA9`) and the choice goes to `setGovernment`;
* **any other human**: nothing; the chosen government arrives as message 43.

The anarchy government's own `+0x1E4` is 0, so the Democracy collapse cannot fire during anarchy.

## 4. The AI's choice

### 4.1 The chooser (`0x4448F0`, `Player.vtable[+0x1C]`, `ret 0`)

For every row except the transition one whose prerequisite the player has (`0x561440`: `-1` is
always met, `[0x9C3DBC]` (the tech count) never, else the player's bit in `[0xA52B4C][tech]`) it
calls the scorer and keeps the strictly greater score, starting from `INT_MIN`, so the lowest index
wins a tie and a score of `INT_MIN` never wins. It returns the index or `-1`.
`government::choose_government`.

### 4.2 The score (`0x4446C0`, `Player.vtable`-independent, `ret 4`)

With `cities = Player +0x194`:

1. a High-weariness government (`+0x1E4 == 2`) when the average weariness (`0x5007B0`) is at least
   90: `INT_MIN` (`0x4446EE`);
2. `+ 8 * cities` without the tile penalty, `+ 8 * cities` with the trade bonus;
3. `+ corruption_points(class) * cities`: 12, 8, 4, 2 for classes 0 to 3, 0 for 4, 4 for the
   communal class 5 (jump table `0x4448D4`);
4. `+ military_police * cities - unit support gold (0x55D310) - building upkeep (0x55CFB0)`;
5. for a government with weariness (`+0x1E4 != 0`) and every civ in play that the player is at war
   with: `- penalty * cities`, penalty 8 above 120, 4 above 60, 2 above 30 of that war's counter,
   doubled for High (`0x4447E5..0x444832`);
6. the civ's favorite: `score * 9 / 8` if positive else `* 7 / 8` (`cdq / and 7 / sar 3` rounding
   toward zero); its shunned government the other way round (7/8 positive, 9/8 otherwise).

The unit-support term uses the candidate's own free-unit terms plus, for a non-human player, the
difficulty bonus `DIFF +0x60 * cities + DIFF +0x5C` (section 1.2). `government::score_government`,
`candidate_support_charge`. Per city, before support, upkeep, war and the race's favorite, the
shipped rows score: Democracy 28 (8 + 8 + 12), Republic 24, Fascism 20, Communism 16, Monarchy and
Feudalism 15, Despotism 4; so a peaceful AI moves to Democracy as soon as it may, and the other
terms are what keep it in anything else.

### 4.3 When an AI considers it (`0x444A10` then `0x444970`)

Both are slots of the Player vtable `0x66CB38` (`+0x24` and `+0x20`); the turn routine calls
`+0x24` and then `+0x28` for every player that is *not* in the human mask (`0x560DD3..0x560DE0`),
after `+0x30` (the AI diplomacy routine `0x445490`, not decoded).

`0x444A10`:

1. `Player +0x34 > 0`: decrement it and return.
2. `r = 64` (128 during a golden age: `[0xA526AC] < Player +0x3C`).
3. `r /= 2` with no enemies (`0x500F50(P, 1) == 0`); `r /= 2` for a Religious civ.
4. With a weariness government: for each war, `r /= step * k` with `step` 4, 2, 1 for a counter
   above 120, 60, 30 (no division at 30 or less) and `k` 2 for High, 1 for Low; without weariness:
   `r *= wars + 1` (`0x500F50(P, 0)`).
5. `r = max(r, 1)`; `next(r) != 0` returns, `0` tail-calls `Player.vtable[+0x20]` (`0x444970`).

`0x444970`: nothing while `+0x34 > 0` or in anarchy; `g = chooser()`; nothing when `g == -1` or
`g == current`; else `startRevolution()` and, unless the civ is Religious, `Player +0x34 =
anarchy countdown + 16` (two evaluations of `hasTrait(4)` make the second arm unreachable).
**An AI therefore starts a revolution whenever its best-scoring government is not its current one**,
once it has won the die; there is no hysteresis. `government::revolution_denominator`,
`considers_revolution`, `revolution_cooldown`.

## 5. War weariness

One 32-bit counter per ordered pair: `Player +0xCB4[civ - 1]` is what the player carries against
`civ`. Three routines read or write it.

### 5.1 The pair array and the incident accumulators

`Player +0x1B0 + 0x4C * civ` is a 19-dword record per civ (a pair of players, seen from this one).
`combat.md` 14.4 and `capture.md` number the same dwords from `+0x1C4`, which is 0x14 higher; the
table gives both. `k` is the offset from `+0x1B0`.

| k | from `+0x1C4` | what it counts | writer |
|---|---|---|---|
| `+0x00` | `-0x14` | declarations of war by `civ` on this player (the record is the victim's, indexed by the declarer) | `0x5020A6` in `0x501F20` |
| `+0x04` | `-0x10` | **HYPOTHESIS** (name): deals that `civ` cancelled by going to war; the code adds 1 when the list of deals with the victim has items and its end turn (`+0xC`) lies in the future | `0x5008C1` in `0x500830` (called from `0x501F20` with flag 1) |
| `+0x0C` | `-0x08` | treaty violations by `civ`: a unit of the declarer, visible to the victim, stands on a tile the victim owns while the declarer has right of passage with the victim or the victim's `+0xDB0` about it is 0x200 or more (at most once per declaration) | `0x502077` in `0x501F20` |
| `+0x10` | `-0x04` | hostile acts this player committed against `civ`; the war weariness kicker of section 5.5 needs it to be 0 | `0x502CC0`, called by the espionage mission code (`0x526DA0`, `0x527470`, `0x527870`, `0x527BB0`, `0x5283E0`, `0x528800`) |
| `+0x14` | `+0x00` | attacks by `civ` on this player | `0x5B577D` in `0x5B5600` (`combat.md` 14.3) |
| `+0x28` | `+0x14` | tech trades | `diplomacy.md` |
| `+0x34`, `+0x38` | `+0x20`, `+0x24` | the two incident accumulators below | `0x5631B0`, `0x5025B0` |
| `+0x40` | `+0x2C` | razed cities | `capture.md` |
| `+0x44` | `+0x30` | nationals lost: 1 is added to `Player[s].pair[o]` when a unit of nationality `s` dies owned by `o` with `Unit::kill` argument `a2` set (`unit-turn.md` 5), and when `0x4BA230` kills a citizen of race `s` in a city owned by `o`; read by the attitude evaluation `0x440100` (`[edi+esi+0x1F4]`, weight 1) | `0x5BBCF8`, `0x4BA326` |

The accumulators start identical: `Player::0x5631B0(victim, amount)` adds `amount` to both in the
**victim's** record about the actor. Writers found: `0x5631B0` (1 for a hit that leaves a unit at 1
HP, a population kill by a city strike, 16 for a captured city plus 1 more in one case,
`capture.md`), and `0x5025B0` at peace: it **halves the first** and **zeroes the second** in both
directions (`0x502600..0x502645`). A scan of every loop that steps by `0x4C` (`0x444A10`, `0x446840`,
`0x500AD0`, `0x561860`, `0x567C80`, `0x59FD70`, `0x5A1B20`, `0x5B5600`, `0x5BBBC0`, `0x5DCF70`) found no
other writer: **nothing decays an accumulator while the war lasts.** The remaining dwords of the
record are read as the AI's attitude inputs by `0x440100` (section 5.5) and are not decoded.

`0x502CC0(civ, clear)` (`ret 8`) is the common tail of the espionage missions: it adds 1 to `k +0x10`
of the actor's record about `civ`; with `clear` it zeroes the byte `Player(civ) +0xD70[actor]`; an AI
actor then asks its method `+0x8C(civ)` and, when that holds, declares war with reason 0.

### 5.2 The turn update (`0x500AD0`, called from `0x5604B0` at `0x560D41`)

For every civ `c` in play (`[0xA526C0]`), other than the player, with `w = Player +0xCB4[c - 1]`:

**at war** (`Player +0xD30[c] != 0`):

```
w += P.rec[c].+0x24 + Player[c].rec[P.civ].+0x24     ; both second accumulators, in full, every turn
for every unit u in the unit pool [0xA52E84] that passes the military filter:
    abroad  |= u.owner == P.civ  and  owner(tile of u) == c
    at_home |= nationality(u) == c  and  0x5BB650(u, P.civ, 1)  and  owner(tile of u) == P.civ
if abroad:                      w += 1
elif not at_home and w > 30:    w -= 1
if mobilized (+0xA4 == 1):      w += 1
```

The **military filter** (`0x500BA4..0x500BE7`) is: `0x5BE6E0(u) > 0`, or `0x5BE820(u) > 0`, or the
prototype's `+0x48` is positive and `0x5BC8B0(u, 3)` is false, or `0x44A950(u)` is true. The helpers
are not decoded (open list). The nationality is the unit's owner except that a unit with the Hidden
Nationality ability (`0x5BC8B0(u, 0x11)`) of a civ other than `P` counts as civ 0 (`combat.md` 14.3).
The tile owner is `cell.vtable[+0x98]` of the cell at `(width / 2) * y + (x >> 1)`.

**not at war**: unchanged while mobilized, otherwise `w = w * 19 / 20` for `w > 0` (the `0x66666667`
multiply is `/ 20`). A counter of 130 reaches 30 after 25 turns of peace.

`government::weariness_at_war`, `weariness_at_peace`. **Consequence (derived, not observed):**
because the second accumulators are added whole each turn and only peace clears them, every incident
at war raises the counter by its weight on *every* following turn; a city captured from a Democracy
adds 16 (or 17) to both sides' counters each turn thereafter, so a Democracy that conquers is past
90 within a few turns.

**The second pass (`0x500D63..0x500EAD`) is not weariness: it is the call to arms.** It walks all
`[0x9C73AC]` cells of the map (the loop rebuilds `(x, y)` from the cell index) and, for each cell
whose owner byte is the player:

```
enemy = 0x56D480(x, y, P.civ, 1)                  ; below
if enemy > 0 and P.atWar[enemy]:
    for every civ p in 1.. with p != P.civ, p != enemy, p in play ([0xA526C0]):
        if P.treaty[p] & (ALLIANCE 4 | MUTUAL_PROTECTION 1)  and  not Player(p).atWar[enemy]:
            Player(p).declareWar(enemy, reason = P.civ + 2)             ; 0x501F20
```

where `P.treaty[p]` is `Player +0xF30 + 4 * p`. So **an enemy military unit that stands on a tile of
the player's territory makes every ally and every mutual-protection partner of the player declare
war on that enemy, at the next turn update, even when they are far away.** The same predicate
and the same reason code `victim + 2` are used when an attack breaks a peace (`combat.md` 14.3,
`0x5B5600`), but that route needs the attack to happen in the victim's land; this one needs only
the presence. The bits: `0x01` is set by treaty clause type 0, value 1 (`0x502DE0`, message
`MUTUALPROTECTIONPACT`) and cleared at `0x50363B`; `0x02` by value 2 (`0x502ED9`, right of passage,
cleared at `0x503660`); `0x04` is the alliance of `combat.md` 14.3, tested in dozens of places;
**no store that sets it was found** among the direct accesses of `+0xF30` (open list). A
military alliance clause (type 1) sets the mask `Player +0xFB0[target] |= 1 << ally` instead
(`0x502F64`, message `MILITARYALLIANCE`).

**`0x56D480(x, y, viewer, check_visibility)`** (`ret 0x10`) answers "whose military unit is on this
tile". It returns -1 for a tile outside the map or without a qualifying unit; otherwise it walks the
tile's unit chain (`cell.vtable[+0xA0]` gives the first id; the next-unit table is at `[0xA52DD8]`)
and for each unit:

1. skips it unless it is military: `0x5BE6E0(u) > 0`, or `0x5BE820(u) > 0`, or the prototype's
   bombard strength (`+0x48`) is positive, or ability 16 (Nuclear Weapon). The first two helpers are
   not decoded; they are presumably the unit's attack and defense strengths;
2. with `check_visibility` and a viewer other than -1, skips it unless `0x5BB650(u, viewer, 1)`
   holds (**HYPOTHESIS:** the unit is visible to the viewer);
3. a unit with Hidden Nationality (ability 17) seen by a viewer who is neither -1, 0 nor its owner
   sets the result to 0 and goes on to the next unit;
4. otherwise the result is the unit's owner, and a non-zero owner ends the walk at once; an owner of 0
   (a barbarian) goes on to the next unit with the result 0.

`government::tile_occupant`, `joins_the_defence`, `calls_to_arms`. The routine does this for every
player every turn and walks the whole map each time.

### 5.3 The average and the enemy count

`0x5007B0(P)`: over the civs in play that `P` has met (`+0xEB4[civ - 1]` bit 0) it adds the counter of
those at war (0 for the others) and divides by the count of met civs, truncating; 0 when none.
`0x500F50(P, flag)`: the civs in play at war with `P`, plus with `flag` those not at war whose
`+0xEB4` word has bit `0x20`. `government::average_weariness`, `enemy_count`.

### 5.4 What reads the average and the counters

* the AI score and gate (section 4);
* the collapse: `0x560D18..0x560D2F` in the turn routine, for **every** player: a government with
  `+0x1E4 == 2` and an average above 90 calls `startRevolution` (`government::democracy_collapses`).
  The AI refuses to score such a government from 90 up, so an AI leaves Democracy before it falls;
* **the cities**: `City::0x4BD780(a, b)` (`ret 8`, `this` = the city, the only caller is the
  happiness routine `0x4BCFF0` at `0x4BD1B2`) turns the counters into unhappy citizens. It reads
  them through the absolute address `0xA53B48 = 0xA52E98 + 0xCB0`, which is why a scan for the
  displacement `0xCB0` does not see it. Section 5.4.1;
* `0x43E470` (Player vtable `+0x78`, an AI evaluation) adds 2 to a running score unless the player's
  government has weariness (`+0x1E4 != 0`) *and* the counter against that civ is at least 60
  (`0x43E65C..0x43E66D`); the same method reads the class again for the other side (`0x43E683`);
* `0x444CC0` (vtable `+0x2C`) takes the address of the counter array (`0x444F65`) and `0x446840`
  (vtable `+0x40`) reads the class (`0x4474EA`); both are AI methods that are not decoded;
* `0x501F20` (declaration of war, section 5.5) adds to and subtracts from the counters.

A scan for the displacement `0xCB0`/`0xCB4` alone is not enough for any Player array: the compiler
folds `0xA52E98 + disp` into one absolute address whenever the player index is a register.

#### 5.4.1 Unhappy citizens from war weariness (`0x4BD780`)

```
k = GOVT[government].+0x1E4;  if k == 0: return
total = 0
for every civ c in play with Player(owner).atWar[c]:
    n = Player(owner).+0xCB4[c - 1]
    if n > 0:
        base = size  if k == 2 (High)  else  size / 2
        total += 2 * base  if n > 120
                 base      if n > 60
                 base / 2  if n > 30
                 0         otherwise
police    = number of buildings b the city has (0x4ACB50(city, b, 1)) that are not obsolete for the
            owner (BLDG +0xE0 is -1 or the owner lacks that tech) and have BLDG +0xEC & 0x400000
total     = max(total - (size / 4) * police, 0)          ; only when police > 0
total     = max(total - owner.0x55A8D0(0x800, 0), 0)     ; Universal Suffrage: 1 per active wonder with flag 0x800
total     = min(total, size)
*b       -= total
city +0xCE += total                                       ; a byte
```

`size` is `city +0x138`. The flag `0x400000` of the building word is *Reduces War Weariness* (the
Police Station) and `0x800` of the wonder word *Reduces war weariness everywhere* (Universal
Suffrage), both per the `biq` crate and the Civilopedia. So under the High government of the
shipped file (Democracy) one war with a counter above 60 makes the **whole** city unhappy, and
above 120 twice over before the cap at the size; under a Low one (Republic, Feudalism) half the
city above 60 and all of it above 120. A Police Station takes a quarter of the size off,
Universal Suffrage one citizen. A counter of 30 or less costs nothing, which is the floor the
at-war decay of section 5.2 stops at. `government::city_weariness_unhappy`.

`b` is the **happy-face accumulator** of the happiness routine (`[esp + 0x10]` of `0x4BCFF0`): the
weariness is subtracted from it as faces and the shift primitive `0x4BDBE0` turns the accumulator
into mood changes (`happiness.md` section 5). `city +0xCE` is reason byte 2, the per-city tally of the
unhappiness this source caused. The happiness routine is decoded in [`happiness.md`](happiness.md);
this section's formula (`government::city_weariness_unhappy`) is part of the Rust recompute that
the differential test there compares with the real code, 30 000 cities with war counters on both
sides of 30, 60 and 120 and all three weariness classes.

### 5.5 Declaring war (`0x501F20(P, civ, reason)`)

`combat.md` 14.3 gives the head (the refusals, the violation scan). The reasons seen are 0 (a plain
declaration: `0x5B5600`, `0x5C0E20`, `0x502CC0`) and `victim + 2` (a call to arms, section 5.2).
The tail, in order:

1. `0x5020A6` counts the declaration in the victim's pair array (`k +0x00`); both at-war bytes are
   set (`0x5020AC`, `0x5020B4`);
2. in a networked game (`0x47B530`, with a human on either side) message `0x3E` goes to both parties;
   otherwise bits 1 to 5 (`& 0xFFFFFFC1`) of both relation-bit words, `P +0xEB0[civ]` and
   `Player[civ] +0xEB0[P.civ]` (the `+0xEB4[civ - 1]` of section 2), are cleared
   (`0x502111..0x502139`);
3. `Player +0xDB0[civ]` and `Player[civ] +0xDB0[P.civ]` are zeroed (`0x502153`, `0x502160`);
4. **the weariness kicker**, only when `reason == 0` **and** the dword `k +0x10` of `P`'s record about
   `civ` (hostile acts, section 5.1) is 0: `P` asks `Player.vtable[+0x88](civ, 0)`, the attitude class
   below, and adds to `Player +0xCB4[civ - 1]` 60 for class 0, 30 for class 1, nothing for 2, 3, 4
   (`0x502188`, `0x502199`, `0x5021A3`); then, whatever the class, it lowers the **victim's** counter
   against the declarer by 30 (`Player[civ] +0xCB4[P.civ - 1]`, `0x5021AF..0x5021C0`);
5. an AI **victim** (its civ is not in the human mask `[0xA526BC]`) stores
   `Player[civ] +0xBB0[P.civ] = 8 * ((a + b + 1) / 2)`, where `a` and `b` are the two `k +0x00`
   counters, the declarations in either direction, the new one included (`0x5021D8..0x50221F`);
6. an AI **declarer** stores `Player +0xBB0[civ] = Player.vtable[+0x98](civ)` (`0x50223A..0x502245`);
7. `0x500830(civ, 1)` settles the deals between the two (it counts a cancelled running deal in
   `k +0x04`); `0x57D980`, `0x57E450` and the `SUMMARY_DECLARE_WAR`, `MILITARYALLIANCE*` and
   `MUTUALPROTECTION*` messages follow (`0x502287..0x5024D1`); not decoded.

**The attitude class** is `Player.vtable[+0x88]` = `0x440AD0(civ, flag)` (`ret 8`). It takes the
score `s` of `Player.vtable[+0x84]` = `0x440100(civ, flag)` (a capped sum of the pair array's
dwords and a race term; not decoded), `n = (width + height) / 2` from `[0x9C74D4]` and
`[0x9C74C0]` (truncating), `m = n / 10` (truncating toward zero; the divide is the `0x66666667`
multiply followed by `sar edx, 2` at `0x440B06` and `0x440B2B`; an earlier revision of this file said
`n / 5`, which was wrong, see `victory.md` section 8.3), and returns

| class | when |
|---|---|
| 0 | `s < -m` |
| 1 | `-m <= s < 0` |
| 2 | `s == 0` |
| 3 | `0 < s <= m` |
| 4 | `s > m` |

**HYPOTHESIS:** the score rises with the incidents the civ caused (they are added, `0x4401A6`), so
classes 0 and 1 are the friendly ones and the kicker is the cost of an *unprovoked* declaration
(no reason, no hostile act of ours) on a civ the declarer likes. The kicker is not limited to AI
declarers: the vtable is shared. `government::attitude_class`, `declaration_weariness`,
`victim_war_memory`.

## 6. Mobilization

### 6.1 The state

`Player +0xA4` is 0 or 1. Message 48 (`0x46FD7B`) stores `msg.+0x34` there and calls `0x561290(P)`
(recompute every city of `P`); the initializer clears it; **only peace clears it** (below).

### 6.2 The AI sets it (`0x444B80`, `Player.vtable[+0x28]`, `ret 0`)

Early exits: government is the transition one; `+0xA4 == 1`; the player knows no tech whose TECH
flags contain `0x20` MOBILIZATION (`0x561480(0x20)`, Nationalism); the capital `+0x2C` does not exist.
Then `balance = sum over civs c in play, at war, with at least one city on the capital's continent
(Player[c] +0x1610[continent] > 0)` of `P.rec[c].+0x20 - Player[c].rec[P.civ].+0x20`. If the balance
exceeds **32** the player stores `+0xA4 = 1` and runs `0x561290`. `government::ai_mobilizes`.

### 6.3 What it does

* **Production** (`0x4BFEE0`, the only caller is the shield function at `0x5D7A6D`): a city whose
  owner is mobilized and whose build item is a *military unit* gets +1 shield on every tile that
  already yields one (`yields.md`). Military means: not a transport (cargo capacity above 0) unless
  it carries only aircraft or only tactical missiles (so the Carrier and the Submarine count), and
  attack, defense or bombard strength above 0, or the nuclear ability. Sixteen of the 141 shipped
  prototypes are not military (Settler, Worker, Scout, Explorer, Leader, Princess, Army,
  Helicopter, Galley, Caravel, Galleon, Transport, Carrack, Dromon and the AI copies of the last
  two). `government::is_military_unit`, `mobilization_bonus`.
* **Building** (`0x56A2A0`): a mobilized player cannot build an improvement unless it has the
  Militaristic characteristic, is Wealth, carries the United Nations flag or is a spaceship part
  (`buildable.md`).
* **Weariness**: +1 a turn per war and no peaceful decay (section 5.2).

### 6.4 Peace (`0x5025B0(P, other)`, `ret 4`, caller `0x502DC0`)

Returns at once for `other <= 0`, `other == P` or not at war. Otherwise, for the two parties:

1. the other's pair record about `P` and `P`'s about the other: `+0x20` halved (`sar 1`), `+0x24`
   set to 0;
2. both at-war bytes (`Player +0xD30`) cleared;
3. **each party with `+0xA4 == 1` is demobilized and recomputed** (`0x5025B0` `0x50265D..0x502690`,
   unconditionally: it does not look for another war);
4. an AI `other` stores 8 in `Player[other] +0xBB0[P.civ]`; an AI `P` stores `Player.vtable[+0x9C]()`
   in `P +0xBB0[other]`;
5. the `MAKEPEACE` message (`0x5027B4`).

The rest (`0x5027C5..0x502CB3`) walks the other civs through the alliance masks `+0xFB0` and the
unit pool; it is not decoded.

## 7. The turn routine's order, as far as governments go (`0x5604B0`)

1. (`0x560CD4`) in anarchy: countdown, end of revolution; otherwise the High-weariness collapse test.
2. `0x4F8E20(Player + 0x181C)`.
3. `0x500AD0`: war weariness.
4. a pass over the civs in play that the player has met: clear `+0xEB4` bit 6 and, for a non-zero
   `+0xD90[civ]` byte, clear it with probability 1/3 (`next(3) == 0`, `0x560D80`).
5. non-human only: `vtable[+0x30]`, then `[+0x24]` (the revolution gate), then `[+0x28]` (the
   mobilization check).

The Player vtable at `0x66CB38`: `+0x14` `0x443A60` (accept a captured city), `+0x18` `0x443B60`
(keep or raze), `+0x1C` `0x4448F0`, `+0x20` `0x444970`, `+0x24` `0x444A10`, `+0x28` `0x444B80`, `+0x2C`
`0x444CC0`, `+0x30` `0x445490`, `+0x34` `0x445730`, `+0x38` `0x4398B0`, `+0x3C` `0x445EA0`, `+0x78`
`0x43E470`, `+0x84` `0x440100`, `+0x88` `0x440AD0`, `+0x8C` `0x440B60`, `+0x94` `0x440EE0` (the deal
scorer of `diplomacy.md`), `+0x9C` `0x539D30`.

## 8. The player initializer (`0x567C80`)

`Player::init(race)` zeroes the player's counters (`+0xA4`, `+0x9C`, `+0x188`, ...), seeds `+0x44` and
`+0x48` from `timeGetTime()` (`t % 0xD431 - 0x8235` and its negative), then sets the starting
government: when the civ has a scenario leader row (table `[0x9C71DC]`, row `+0x4C`; **HYPOTHESIS:**
the LEAD section) that row's government (`0x567DDA`); else the default row `[0x9C3D70]`
(`0x567E5D`..`0x567E64`). `Player +0x30` is set from `[0xA52684]` for a human and `[0x9C7288]` for an
AI (`0x567E85`).

## 9. Corrections

* `[0x9C3DD0]`: its initializer is the GOVT section loader `0x595AE0` (`0x595B61`), not a RULE field;
  `[0x9C3D70]` is the default government.
* `0x567C80` was an unlabelled caller of `setGovernment`: it is the player initializer.
* The pair record's second accumulator **is** reset: `0x5025B0` zeroes it at peace.
* `0x444A10` / `0x444B80` have no direct callers: they are Player vtable slots `+0x24` / `+0x28`,
  called from the turn routine for non-human players.
* The second pass of `0x500AD0` is not part of the weariness arithmetic: it is the call to arms
  (section 5.2). The earlier remark that it "tests the owner of a tile against the player" was right
  and incomplete.
* The dword `Player +0x1C0 + 0x4C * civ` that `declareWar` tests is not "the dword before the
  record": it is the hostile-acts counter of the pair array, which starts at `+0x1B0` (section 5.1).
  The victim's relief of 30 is not unconditional: it needs the same two conditions as the kicker
  (plain declaration, no hostile act).
* The RULE dword `[0x9C72A8]` is the count of the spaceship-part limit array, not "a count that is not
  a body field" (`combat.md` 9): it is body `+0x60`.
* `0x5CFFB5` and the other `lea reg, [esi + 0xCB4]` hits outside the Player methods are destructors
  of unrelated dialog objects, not war-weariness readers.
* "No city code reads the counters" (an earlier draft of 5.4) was wrong: `0x4BD780` does, through an
  absolute address that a displacement scan cannot see.

## 10. Open

* the tail of `0x5025B0` (alliance cascade, unit pass), and the readers of the pair array's dwords
  in `0x440100` (the attitude score);
* the helpers of the military filters: `0x5BE6E0`, `0x5BE820`, `0x44A950`, `0x5BB650` and the ability
  tests `0x5BC8B0` with 3 (Cruise Missile) and 16 (Nuclear Weapon);
* the writer of treaty bit `0x04` (alliance) and of `Player +0xDB0[civ]`; the timer semantics of
  `Player +0xBB0[civ]` (its readers `0x439410`, `0x43D610`, `0x43E470`, `0x440E10` are AI methods);
* GOVT `+0x18`, `+0x24`, `+0x1A8`, `+0x1C4..+0x1CC` (no reader located);
* the AI methods `0x444CC0`, `0x445490`, `0x445730`, `0x43E470`, and the sub-object `0x4F8E20`;
* `0x500830` (deal settlement at a declaration) and the messages that follow it in `0x501F20`;
* the human path that offers mobilization (message 48's sender);
* (resolved in [`happiness.md`](happiness.md): the happiness routine `0x4BCFF0` and its family, the
  base content count, luxuries, military police (GOVT `+0x1AC`), the mood shift and `city +0xCE`;
  still open there: the overthrow of a government by prolonged disorder);
* the RACE fields `+0x920` / `+0x924` and the scenario leader government are read as documented here
  but never exercised by a live run.
