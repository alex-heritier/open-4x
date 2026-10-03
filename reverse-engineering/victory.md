# Victory, defeat, score and civilization elimination

Owns: everything that decides when a game ends and who wins: the per-turn score and its running means
(`0x5382B0`, `0x538480`), the victory-point (VP) counters and every place that adds to them, the master
victory test `CheckVictory` (`0x4F1B60`) with its eight victory types, the diplomatic (United Nations)
vote protocol and the AI's side of it (`0x441B40`, `0x441BB0`, `0x440AD0`), the time limit and Retire
(`0x4E0130`), the final-score announcement (`0x4F0E60`), and the elimination / respawn routine
`Player::eliminate` (`0x568950`). Also owns the mapping from the scenario's victory settings to the runtime
words. Image base `0x400000`; static VA = runtime VA. This is a **clean-room specification**: it contains no
reference code; every rule is stated with its address so it can be re-checked against the binary.

Evidence tags used below: **V** = the whole routine body was read in the disassembly and the statement is what
the instructions do; **I** = inferred from how the value is used (no instruction states it); **H** =
**HYPOTHESIS** (plausible, not proven; do not build on it without a test); **O** = open (not decoded).
Nothing here was checked by running the game; the only dynamic evidence is the golden vectors in section 13,
which are **hand-computed from the decoded formulas**, not captured from the original (the section says so).

## At a glance

* A game ends when `[0xA526A0]` (victory type) leaves `-1`. It is set by exactly four things: `CheckVictory`
  (types 0 to 5, 7, 8), Retire (type 6), the elimination of the local human (type 1, Conquest), and the
  multiplayer receive path. `CheckVictory` is called **once per round, after the whole round has been processed
  and after the turn counter has been incremented** (section 11), and after each human's turn in sequential
  games.
* The tests run in a fixed order: **Victory Points -> Conquest -> Space Race -> Wonder -> Domination ->
  Cultural -> Diplomatic -> Time limit**. Each block is skipped once a victory is set, so the first block that
  fires wins. Within a block, ties go to the lowest slot unless stated.
* The type numbers are: 0 Domination, 1 Conquest, 2 Cultural, 3 Diplomatic, 4 Space Race, 5 Time limit,
  6 Retire, 7 Wonder, 8 Victory Points.
* **Space Race ends the moment every spaceship part count reaches its required number**; there is no launch
  and no arrival test (section 7.3).
* **Diplomatic victory needs strictly more than half of the civilizations' votes** (barbarians excluded), not
  two thirds (section 8.5).
* The score shown to the player is a **running mean over all turns** of a per-turn quantity, stored as float32
  (section 4); at the end it can be topped up with an "early victory" bonus (section 10).
* VP come from five sources (wonders, kills, advances, city captures, held victory locations) plus the flag
  capture modes. All VP additions are gated on `flags & 0x26000` (VP scoring, Capture the Unit or Reverse
  Capture the Flag).
* Eliminating a civilization (`0x568950`) can **respawn** it on an empty continent when the Respawn AI flag is
  set; otherwise it is removed from the in-play mask and, if it was the local human, the game ends with a
  Conquest victory for the killer or the top scorer.

## 1. Globals and data

### 1.1 The runtime object and where the scenario settings go

`RT` is the object at `0xA52658` (`this` of `0x538480`, `0x4F1B60` reads its fields through absolute
addresses). Absolute address = `0xA52658 + offset`.

| address | `RT+` | meaning | evidence |
|---|---|---|---|
| `0xA5267C` | `0x24` | game-flag word (below) | V |
| `0xA52680` | `0x28` | debug / cheat bits. Bit 2 (`0x4`) zeroes the local player's score at the final announcement (section 10); multiplayer clears bits `0x0C` there | V (effect), O (other bits) |
| `0xA52684` | `0x2C` | **difficulty level of the game** (copied from setup word `0x9906CC`, which comes from the LEAD row `+0x40`; `-2` means no override). Multiplies the per-turn score and the early-victory bonus as `(level + 1)` | V (uses), I (identity) |
| `0xA526A0` | `0x48` | victory type; `-1` none | V |
| `0xA526A4` | `0x4C` | winner slot | V |
| `0xA526A8` | `0x50` | diplomatic-vote cooldown (turns) | V |
| `0xA526AC` | `0x54` | turn counter (starts 0; incremented once per round) | V |
| `0xA526B4` | `0x5C` | gameplay RNG state (`primitives.md` section 1.6) | V |
| `0xA526BC` | `0x64` | human-player mask (bit = slot) | V |
| `0xA526C0` | `0x68` | in-play mask. Bit 0 is the barbarian slot; the formulas below use `popcount(mask) - 1` as "number of civilizations", which only works if bit 0 is always set | V (use), H (bit 0 always set) |
| `0xA526C4` | `0x6C` | race (civ type) mask of civilizations in play | V |
| `0xA5279C` | `0x144` | highest slot in use | V |
| `0xA52990` | `0x338` | byte: a diplomatic vote has been called | V |
| `0xA52991` | `0x339` | byte: sequential / play-by-mail style turn order | V |
| `0xA528C8` | `0x270` | elapsed real time (ms), shown in the final announcement | V |
| `0xA528CC` | `0x274` | vote sponsor slot | V |
| `0xA528D0 + q` | | byte: civ `q` has voted (sequential votes) | V |
| `0xA528F0 + 4q` | | **the choice cast by voter `q`** (candidate slot; 0 = abstain). It is indexed by voter, not by candidate | V |
| `0xA52970 + q` | | byte: civ `q` is a diplomatic-victory candidate | V |
| `0xA52B40` | `0x4E8` | GAME `theme` (G+0xB0); only copied | I |
| `0xA52B44` | | last tick for the elapsed-time accumulator | V |
| `0xA283DC`, `0xCC37BC` | | quit flags: set together to leave the game loop | V |
| `0xA283DD` | | the "retire warning" has been shown | V |
| `0x9FD4BC` | | slot of the local player | V |

Game-flag word `[0xA5267C]` bits used here (the full list is `biq-format.md`, GAME `flags`):

| bit | meaning |
|---|---|
| `0x1` | Domination victory enabled |
| `0x2` | Space Race victory enabled |
| `0x4` | Diplomatic victory enabled |
| `0x8` | Conquest victory enabled |
| `0x10` | Cultural victory enabled |
| `0x80` | Respawn AI players |
| `0x200` | Accelerated production (economy.md) |
| `0x400` | City Elimination (capture.md) |
| `0x800`, `0x1000` | Regicide, Mass Regicide |
| `0x2000` | Victory-point scoring |
| `0x4000` | Capture the Unit |
| `0x10000` | Wonder victory enabled |
| `0x20000` | Reverse Capture the Flag |
| `0x40000` | (conquests 12.08) scientific-leader roll gate, not used here |

"VP mode" below means `flags & 0x26000 != 0`.

### 1.2 Setup words copied into `RT` (`0x494630`, defaults in `0x4848A0`, GAME copy `0x493F40`)

The scenario's GAME block is copied to the setup words `0x9906D0..0x990718` and then into `RT` fields.
A stored value of 0 is replaced by the default shown. All are signed 32-bit.

| setup word | GAME field (biq `VictoryLimits`) | `RT` word | value used (0 means) |
|---|---|---|---|
| `0x9906D0` | end turn | `0xA528C4` | `0` -> 540; then clamped to `[1, 1000]` by `0x426710` |
| `0x9906D4` | victory-point limit (G+0x7C) | `0xA528C0` | `0` -> 50 000 |
| `0x9906D8` | multiplayer time limit, minutes | `0xA528BC` | stored x 60 000 (ms); 0 = none |
| `0x9906EC` | city-elimination count | `0xA52998` | `0` -> 1 |
| `0x9906F0` | culture needed in one city | `0xA5299C` | `0` -> 20 000 |
| `0x9906F4` | culture needed by a civilization | `0xA529A0` | `0` -> 100 000 |
| `0x9906F8` | domination: percent of land+coast tiles | `0xA529A4` | `0` -> 66 |
| `0x9906FC` | domination: percent of population | `0xA529A8` | `0` -> 66 |
| `0x990700` | VP multiplier per wonder, x cost | `0xA529AC` | (stock 10) |
| `0x990704` | VP multiplier per defeated unit, x cost/10 | `0xA529B0` | (stock 10) |
| `0x990708` | VP multiplier per advance, x cost | `0xA529B4` | (stock 5) |
| `0x99070C` | VP multiplier per captured city, x size | `0xA529B8` | (stock 100) |
| `0x990710` | VP per turn per held victory location | `0xA529BC` (= `RT+0x364`) | (stock 25) |
| `0x990714` | VP for delivering a captured flag unit | `0xA529C0` | (stock 1000) |
| `0x990718` | gold for delivering a captured flag unit | `0xA529C4` | (stock 0) |
| `0x99071C` | theme | `0xA52B40` | copied |
| `0x9905C0` | merged game flags | `0xA5267C` | copied |
| `0x9906CC` | difficulty | `0xA52684` | copied |
| `0x9903B8` | seed | `0xA526B4` | copied |

Evidence: V for the copy and the zero replacements of the first eight rows; the stock numbers in the
parentheses are the `conquests.biq` values (`biq/src/sections/game.rs` defaults), not code defaults.

### 1.3 Other global objects used

* **Scenario GAME object** `0x9C40D0`. Alliances: `0x5E2420(G)` returns true iff any alliance number in the
  array `G+0xC` (length `G+0x10`) is non-zero (V). `0x5E23C0(G; race)` returns the alliance number of civ type
  `race` (0 if the race is not listed; V). `[0x9C5B38]` (`G+0x1A68`) is the alliance victory type: 0
  individual, non-zero coalition (V for the test, I for the names). `[0x9C5B42]` is a runtime flag: culture
  threshold override (section 7.6). Calendar arrays: turns per segment at `0x9C4114` (7 dwords) and years per
  turn at `0x9C4130` (7 dwords) (V, section 3).
* **RULE object** (`0x9C71E4` + m): `[0x9C72A8]` number of spaceship part types (default 10); `[0x9C724C]` a
  **pointer** to the int32 array of required counts per part type (stock `[1,...,10]`); `[0x9C72AC]` starting
  treasury; `[0x9C72CC]`, `[0x9C72D0]` start units 1 and 2; `[0x9C7314]` flag unit type. `[0x9C40E8]`,
  `[0x9C40EC]` are the "auto-place capture units / king units" words. All V.
* **Tables.** BLDG `[0x9C40AC]` (stride `0x110`, count `[0x9C3D80]`): memory `+0x94` cost, `+0xF0` bit 2
  (`& 4`) great wonder, bit 3 (`& 8`) small wonder. PRTO `[0x9C71E0]` (stride `0x138`): `+0x54` shield cost
  (in shields in memory), `+0x9C` unit class (0 land, 1 sea, 2 air), `+0xB0` worker actions (bit 1 = Build
  City), abilities through `0x5BC8B0(unit, k)` with k = `0x1D` King, `0x0D` Flag unit, `0x12` Army, `0x11`
  Hidden nationality. TECH `[0x9C7320]` (stride `0x74`): `+0x44` cost. Cell table `0x9C736C`: virtual slots
  `+0x8C` is-water, `+0x98` owner byte, `+0xB8` continent id, `+0xC8` terrain (11 = Coast). Map width
  `[0x9C74D4]` (the x extent; tile index = `(W/2)*y + (x>>1)`), height `[0x9C74C0]`, cell count
  `[0x9C73AC] & 0xFFFF`.
* **Lists** (node pointer minus `0x1C` = object; entry *i* of a list is read at `[array + 8i + 4]`, last index
  in the second word): players `0xA52E98` + 8420 (`0x20E4`) per slot, 32 slots, slot 0 = barbarians;
  cities `[0xA52E6C]`/`[0xA52E78]`; units `[0xA52E84]`/`[0xA52E90]`; **victory locations**
  `[0xA52DF4]`/`[0xA52E00]` (object = entry pointer `- 0x1C`; fields: x word `+0x24`, y word `+0x28`,
  holder word `+0x2C`, `0xFFFF` = none). All V (`0x538480` and `0x5BD220` both add `-0x1C` first).
* **Wonder-built array** `[RT+0x4FC]`: one byte per BLDG index. Written only by `0x4ACF40` (when a building is
  added, value 1; `0x539010` also records it) through the setter `0x538FF0`; read by `0x538FE0`. It is **never
  cleared** (one call site), so a wonder destroyed with its city still counts as built (V for the write sites).

### 1.4 Player fields used (Player = `0xA52E98 + 8420*slot`; offsets from the Player base)

| offset | meaning | evidence |
|---|---|---|
| `+0x1C` | slot | V |
| `+0x20` | race (civ type) | V |
| `+0x2C` | capital city index | V |
| `+0x40` | flags; bit 1 = already respawned once | V |
| `+0x44`, `+0x48` | treasury pair (gold = sum; economy.md section 3.2) | V |
| `+0x4C` | **float: running mean of the per-turn score** (section 4) | V |
| `+0xA4` | mobilized flag 0/1 (government.md) | V |
| `+0xD30[c]` | contact / at-war byte against civ `c` | V |
| `+0xD50`, `+0xD70` | per-civ relation bytes (cleared on elimination) | V |
| `+0x104` | **number of future technologies acquired** (research.md); the `+0x11C4` mean tracks it | V |
| `+0x11B5` | byte: set at elimination to the killer / top-scoring civ | V (write), H (readers) |
| `+0x11B8` | float mean: happy citizens | V |
| `+0x11BC` | float mean: content + specialist citizens | V |
| `+0x11C0` | float mean: owned land + coast tiles | V |
| `+0x11C4` | float mean: future technologies | V |
| `+0x11CC` | **total victory points** | V |
| `+0x11D4` | VP gained from held victory locations | V |
| `+0x11D8` | VP gained from delivered flag units | V |
| `+0x15B4` | cities lost (displayed; writer not decoded here) | O |
| `+0x15B8` | VP from wonders | V |
| `+0x15BC` | VP from defeated units | V |
| `+0x15C0` | VP from advances | V |
| `+0x15C4` | VP from captured cities | V |
| `+0x15C8` | **not** a VP counter: a counter set to 1 when the civ loses its capital (capture.md section 7 step 2), decremented once per round while positive by `0x560050` (research.md section 2.1 step 5), cleared at game start (`0x567C80`); its effect is **O** | V (writes), O (meaning) |
| `+0x15FC` | pointer to int16 array: spaceship parts built, per part type | V |
| `+0x183C` | total culture | V |
| `+0x194` | city count | V |
| `+0x1A0` | techs known | V |

The score screen (`0x546000`) shows the six VP rows `+0x11CC`, `+0x11D4`, `+0x11D8`, `+0x15B8`, `+0x15BC`,
`+0x15C0`, `+0x15C4` (V for the reads). `0x567C80` zeroes `+0x11CC`, `+0x11D4`, `+0x15B8..+0x15C8` at game start
(V).

City fields used: owner byte `+0x28`; x word `+0x24`, y word `+0x26`; population `+0x138`; citizen list
`+0xE0` (array) / `+0xEC` (last index), entry *i* at `[array + 8i + 4] - 0x1C`; citizen mood byte `+0x128`
(0 happy, 1 content, 2 unhappy, 3 resisting, 4 specialist; happiness.md); per-owner culture `+0x140 + 4*owner`.

## 2. Helper functions decoded for this document

| address | signature | meaning | evidence |
|---|---|---|---|
| `0x4BDD90` | `(city; k)` | number of citizens of the city whose mood byte equals `k` | V |
| `0x5DF900` | `(mask)` | population count (number of set bits) | V |
| `0x5DF030` | `(this; t)` | turn to calendar-units, section 3 | V |
| `0x538230` | `(RT)` | culture threshold of a civilization, section 7.6 | V |
| `0x538FE0` | `(RT; i)` | `byte [RT+0x4FC][i]`, wonder `i` built | V |
| `0x5A6060` | `(x, y, 6, a, b)` | mode 6: number of units on the tile whose PRTO unit class equals `b` (here `b` = the tile is water), owner filter `a` (`-1` any); only units with defense (`0x5BE820(u) > 0`) | V |
| `0x56D630` | `(x, y, civ, flag)` | owner of the first non-barbarian unit on the tile (0 if only barbarians, `-1` if empty); honours hidden nationality | V |
| `0x440AD0` | `(P; q, flag)` | attitude class of `P` toward `q`, 0..4, section 8.3 | V |
| `0x441B40` | `(P)` | AI decision: call a diplomatic vote, section 8.4 | V |
| `0x441BB0` | `(P; candidates, votes)` | AI casts its vote, section 8.4 | V |

## 3. Turn to calendar units: `U(t)` (`0x5DF030`)

`U` converts a turn number into accumulated calendar units (years or months), using seven segments with
`turns[i]` (array `0x9C4114`) and `units[i]` per turn (array `0x9C4130`). V.

```
acc = 0
for i in 0..6:
    if t < turns[i]: return acc + units[i] * t
    t   -= turns[i]
    acc += units[i] * turns[i]
return acc + t            -- one unit per turn after the seventh segment
```

Defaults (also the shipped scenario): `turns = 25, 25, 40, 50, 100, 100, 100`, `units = 50, 40, 25, 20, 10, 5, 2`.
Cumulative values at the segment ends: `U(25)=1250`, `U(50)=2250`, `U(90)=3250`, `U(140)=4250`, `U(240)=5250`,
`U(340)=5750`, `U(440)=5950`; beyond that `U(t) = 5950 + (t - 440)`. The function is used only by the
final-score bonus (section 10) and by date strings.

## 4. The per-turn score and its running means

### 4.1 `0x5382B0(RT; q, outA, outB, outC, outD)` (`ret 0x14`) - V

For civilization `q`, over every city whose owner byte equals `q`, with `n0..n4` the mood counts of the city
(`0x4BDD90`) and `pop = city.+0x138`:

```
cityTerm  = pop - n3 - n2 + n0          -- = 2*happy + content + specialists
Σcity     = sum of cityTerm over the cities
A (outA)  = sum of n0                   -- happy citizens
B (outB)  = sum of (pop - n3 - n2 - n0) -- content + specialist citizens
C (outC)  = number of cells with owner byte == q that are land or Coast (terrain 11)
D (outD)  = Player[q].+0x104            -- future technologies
return      (RT.+0x2C + 1) * (Σcity + C + D)
```

An out-pointer may be null; the first one (A) is explicitly zeroed first when non-null. `RT.+0x2C` is the
difficulty level (section 1.1), so the per-turn score is multiplied by 1 (level 0) up to 6 (level 5).

### 4.2 `0x538480(RT)`: the running means - V

For `q = 1 .. [0xA5279C]` (no in-play test; slots that are not in play simply decay toward zero) it calls
`0x5382B0` and updates five float32 fields, where `t = [0xA526AC]` is the turn counter **before** the
increment of this round:

```
if t == 0:    field = (float) value
else:         field = (float) ((field * t + value) / (t + 1))      -- field read as float32, arithmetic in x87
```

| field | value |
|---|---|
| `+0x11B8` | A |
| `+0x11BC` | B |
| `+0x11C0` | C |
| `+0x11C4` | D |
| `+0x4C` | the returned total |

Intermediate results are kept in x87 registers and rounded to float32 only on the store (`fstp dword`).
Whether the x87 precision-control word is 53 or 64 bits was not verified (**H**: 53, set by the C runtime
start-up); this affects the last bit of a mean only.

Then, if `flags & 0x2000` (VP scoring), the victory locations are scored (section 5.2).

The score screen shows rows scaled `(D' + 1) * value` where `D'` is the difficulty level, and doubles the happy
row (V for the reads in `0x546000`; the exact per-row text is UI).

### 4.3 When it runs

`0x538480` is called from the end of a round (`0x4F5EF0` at `0x4F6162`) and from `0x4F5160` (`0x4F5215`),
**before** the turn counter is incremented (`0x4F6167..0x4F6172`). Therefore at turn 0 the means are initialised
and at every later round `t` is the number of rounds already averaged. `CheckVictory` of the same round sees
the incremented counter but the means that were updated with the old one (V).

## 5. Victory points

All additions below are gated on `[0xA5267C] & 0x26000` (VP mode). Every addition goes to the owner's
`+0x11CC` (total) and to one breakdown counter. All integer arithmetic is 32-bit signed.

### 5.1 Sources

| source | where | amount | counters |
|---|---|---|---|
| great wonder completed | `0x4ACF40` building-add path, at `0x4AD51A..0x4AD5DA`, only if BLDG `+0xF0 & 4` | `BLDG.cost (+0x94) * [0xA529AC]` to the city's owner | `+0x11CC`, `+0x15B8` |
| enemy unit destroyed | `0x5BBBC0` (unit death), at `0x5BBF05..0x5BBF9F`; only if killer slot != 0 and killer != the unit's owner (`unit+0x34`) | `trunc(PRTO[type].+0x54 / 10) * [0xA529B0]` to the killer's civ; `unit+0x40` is the type | `+0x11CC`, `+0x15BC` |
| advance learned | `0x561860` (gain technology), at `0x561A61..0x561AC6`; only if turn counter `> 0` | `TECH[t].+0x44 * [0xA529B4]` | `+0x11CC`, `+0x15C0` |
| city captured | `0x563410` (capture branch), at `0x563EFC..0x563F38` | `city.+0x138 * [0xA529B8]` to the capturer (the population field at that instruction; whether an earlier step already reduced it is the capture order in capture.md section 3 step 2, which lists the counters just before the loot) | `+0x11CC`, `+0x15C4` |
| flag unit delivered | `0x5BD220`, section 5.3 | `[0xA529C0]` VP and `[0xA529C4]` gold | `+0x11CC`, `+0x11D8` |
| victory location held | `0x538480`, section 5.2 | `[0xA529BC]` per location per turn | `+0x11CC`, `+0x11D4` |

The wonder and advance factors multiply the **cost** (shields or beakers), so expensive wonders and advances
are worth more. The kill value divides the shield cost by 10 first (truncating toward zero via the
`0x66666667` magic), so a unit costing less than 10 shields is worth 0.

### 5.2 Victory locations (`0x538480`, after the means; only when `flags & 0x2000`) - V

For each location (list `[0xA52DF4]`, tile index `(W/2)*y + (x>>1)` from its x, y):

1. `held = 0x5A6060(x, y, 6, -1, isWater) > 0`, where `isWater` is the cell's water bit. If not held, skip
   (the location keeps its previous holder).
2. `owner = 0x56D630(x, y, -1, 1)`.
3. If `owner == 0` (only barbarians): release - post message 12 (old holder) and set holder to `-1`.
4. If `owner != holder`: post message 12 (old holder, if any) and message 11 (new owner) and set
   `holder = owner`.
5. Every turn the location is held (including the turn it changes hands): `Player[owner].+0x11CC += RT.0x364`
   and `Player[owner].+0x11D4 += RT.0x364` (stock 25).

(The text of messages 11/12 is **O**.)

### 5.3 Flag modes: `0x5BD220` (the routine that handles a flag-carrying unit after it moves) - V for the paths read, O for the rest

A unit carries a captured flag when `unit+0x1EC != 0xFF` (`+0x1EC` = civ slot of the flag's owner, `+0x1EA` =
the flag unit type word).

* **Capture the Unit** (`flags & 0x4000`): if the unit stands in a city (`0x56D2C0(x, y)`) whose id
  (`city+0x20`) equals the carrier owner's capital index (`Player.+0x2C`): the delivery fires (messages
  `WE_CAPTURE_FLAG` for the local player, the `THEY_` variant otherwise).
* **Reverse Capture the Flag** (`flags & 0x20000`): the delivery fires when the carrier stands on the x, y of a
  victory location (list `[0xA52DF4]`); the message is `WE_RETURNED_FLAG` for the local player.
* On a delivery: the owner's treasury gains `[0xA529C4]` (stored through the pair-split of economy.md
  section 3.2, including the `timeGetTime` re-split when the new total is not positive), the owner's `+0x11CC`
  and `+0x11D8` gain `[0xA529C0]`, the carrier's flag fields are reset (`+0x1EC = 0xFF`, `+0x1EA = 0xFFFF`)
  and the flag unit is returned to its original owner through `0x56AFB0` (gated by the setup word "respawn
  flag unit on capture", default 1).
* The early part of the routine (before `0x5BDEA9`) and the multiplayer announcements were not read;
  the conditions under which the routine is invoked are therefore **O** (it is called from the unit
  movement code; the call sites were not enumerated).

## 6. `CheckVictory` (`0x4F1B60`): entry, guards and precomputation

Signature: `CheckVictory(arg)`, `arg` is 0 from every normal call; the network handler passes a non-zero
value to force the multiplayer vote path to run. Return value: 1 = the game is over and the caller must leave
the loop, 0 otherwise. All V.

### 6.1 Entry guards (in order)

1. Multiplayer (`0x47B530`) and `[0x74D09F]` (a vote is pending) and `arg == 0` -> return 0.
2. `[0xCC37BC]` set -> return 1.
3. `[0xA526A0] != -1` (a victory has already been recorded, e.g. the player chose "continue") -> return 0.

### 6.2 Precomputation

* `tiles[o]`: for every cell, if it is land, or water with terrain 11 (Coast), then `tiles[owner byte]++` and
  `totalTiles++`. Owner 0 (unowned) is counted in `totalTiles`. Lakes and deep ocean are ignored.
* `pop[o]`: for every city, `pop[owner byte] += city.+0x138`, `totalPop += city.+0x138` (barbarian cities
  included in the totals).
* **Team mode** (byte flag): `0x5E2420(G)` is true **and** `[0x9C5B38] != 0`. In team mode a civilization's
  team index is `0x5E23C0(G; race)` (1..4; 0 = no team). A team's **representative** is the local human if
  he is in the team, else the lowest in-play slot of the team. Team sums are accumulated per index 1..4.
* "Individual loops" in team mode skip civilizations that belong to a team (index non-zero) and are in play,
  unless stated.

## 7. The victory types, in evaluation order

### 7.1 Type 8: Victory Points - gate `flags & 0x26000` (V)

1. Team mode: for slots 1..31, sum `+0x11CC` per team; any team with a sum `>= [0xA528C0]` wins; the winner is
   its representative.
2. Individual loop over slots 0..31 (in team mode skipping in-play team members): a civilization with
   `+0x11CC >= [0xA528C0]` wins. It **overrides** a team result found in step 1 when it is a non-team
   civilization; among individuals the lowest slot wins.

### 7.2 Type 1: Conquest - `flags & 0x8` (V)

* If `popcount(inPlay) - 1 == 1` (exactly one civilization besides the barbarian bit): that civilization wins.
* Else, if `0x5E2420(G)` (any alliance exists; the coalition setting is **not** consulted here): if every
  in-play civilization (slots 1..31) has the same non-zero alliance number, the game is won; the winner is the
  local human if he is in play, otherwise the highest in-play slot.

### 7.3 Type 4: Space Race - `flags & 0x2` and `[0x9C72A8] > 0` (V)

The first in-play slot, in ascending order, for which `int16 (*Player.+0x15FC)[k] >= (*[0x9C724C])[k]` (signed
compare of the sign-extended word against the dword) holds for **every** part type `k` in
`0 .. [0x9C72A8]-1` wins. There is no launch, no arrival and no travel time: the parts only need to have been
built (the array is the per-type built count that buildable.md uses at `0x56A2A0`). When the winner is the local human in a single-player game the routine first calls
`0x5A3910(0xC90E80; 1)` and `0x4E2820(0x9F8700; 1)` (victory UI/media; **O**).

### 7.4 Type 7: Wonder - `flags & 0x10000` (V)

Fires only when **every** BLDG row with `+0xF0 & 4` has bit 0 of its `[RT+0x4FC]` byte set (`0x538FE0`; the
caller computes `all &= byte`, starting from 1). The winner is then the civilization with the greatest score:

* score = `Player.+0x11CC` (int) in VP mode (`flags & 0x26000`), otherwise `Player.+0x4C` (float);
* teams (team mode): the members' scores are summed in ascending slot order (non-VP: each step is
  `sum = trunc(member.+0x4C + (float) sum)` through `_ftol`, so the fraction of each mean is lost step by step);
  the best team is chosen, then the individuals that are not in teams are compared against the same running
  best;
* an update happens only when the new value is **strictly** greater, scanning slots ascending.

### 7.5 Type 0: Domination - `flags & 0x1` (V)

```
tilesNeeded = ([0xA529A4] * totalTiles) / 100          -- C truncating
popNeeded   = ([0xA529A8] * totalPop)   / 100
```

A team (sums over its members) or an in-play individual wins when `tiles > tilesNeeded` **and**
`pop > popNeeded` (both strictly). Slot order ascending; teams are tested first.

### 7.6 Type 2: Cultural - `flags & 0x10` (V)

* **(a) One city:** the first city in the city list whose `culture[owner] >= [0xA5299C]` (the city's own
  per-owner culture word `city+0x140+4*owner`) makes its owner the winner.
* **(b) Civilization:** let `T = 0x538230(RT)` and `c_q = Player[q].+0x183C`. A civilization (or team: sums)
  wins when `c_q >= T` and **every other** civilization/team in play has `c <= trunc(c_q / 2)`, where the
  half is `(c - (c >> 31)) >> 1` (rounds toward zero). Individuals are tested for slots 1..31; teams first.

Threshold `T` (`0x538230`):

```
if [0x9C5B42] != 0:   T = [0xA529A0]                              -- scenario override, taken as is
else:                 x = trunc( -0.5 * (H + W) * ((float)[0xA529A0] * 0.009999999776482582f) )   -- _ftol, toward zero
                      T = 1000 * trunc((499 - x) / 1000)            -- signed divide, truncating
                      T = clamp(T, 5000, 500000)
```

Here `W = [0x9C74D4]`, `H = [0x9C74C0]`. Because `x` is negative the expression is `499 + trunc(0.5*(H+W)*c')`.
`0.01f` is the float32 constant `0x3C23D70A` (the 24-bit approximation of 0.01). The product is computed in
extended registers and truncated once.

### 7.7 Type 3: Diplomatic - `flags & 0x4` (skipped in multiplayer while `[0x74D09F]`) (V) - section 8

### 7.8 Type 5: Time limit (V) - section 9

## 8. The diplomatic (United Nations) vote

### 8.1 Preconditions and candidates

1. **Sponsor**: scan in-play slots ascending; the sponsor is the last slot found that owns the wonder with
   flag `0x2000` (BLDG "Allows Diplomatic Victory"; test `0x55A8D0(Player; 0x2000, 0)`). No sponsor -> the block
   does nothing (no vote can be called).
2. **Candidates** (`[0xA52970 + q]`): the wonder owner(s), plus any civilization with `tiles > totalTiles/4`
   or `pop > totalPop/4` (C truncating division by 4).
3. Cooldown `[0xA526A8]`: if `> 0` it is decremented and the block is skipped; in multiplayer the
   decrement happens once per turn (guard `[0xA283E0]` = the turn). When a vote is called it is set to 10.

### 8.2 Calling the vote

* The sponsor's decision is: local human sponsor -> the dialog `DIPLOVICTORYVOTEOPTION` (multiplayer: set
  `[0x74CFC4] = 0x200`, `[0x74CFC8..0x74CFD4] = -1`, `[0x74D09F] = 1` and wait for the network); another
  human in multiplayer / sequential play -> the network path; AI sponsor -> `0x441B40` (section 8.4).
* The result is stored in `[0xA52990]`; the sponsor in `[0xA528CC]`.
* Sequential games: `0x4F6810(local)` is invoked (section 8.6) and the in-line vote below is bypassed.

### 8.3 Attitude class `0x440AD0(P; q, flag)` - V

`A` is the attitude value returned by the Player virtual method at vtable `+0x84` (`0x440100`; government.md
describes its inputs; **the sign convention "lower = friendlier" is H**, but both vote routines below use it
consistently). With `t = trunc((W + H) / 2)` and `m = trunc(t / 10)` (signed, toward zero):

| condition | class |
|---|---|
| `A < -m` | 0 |
| `-m <= A < 0` | 1 |
| `A == 0` | 2 |
| `0 < A <= m` | 3 |
| `A > m` | 4 |

Note the divisor is **10** (`sar edx, 2` after the `0x66666667` multiply at `0x440B06` and `0x440B2B`), not 5.

### 8.4 AI behaviour - V

* **Call the vote** (`0x441B40(P)`): `n = 1 + count of in-play slots q = 1..31, q != P.slot, with
  0x440AD0(Player[q]; P.slot, 1) == 0` (class 0 - those who are most favourable to `P`). Returns true iff
  `n > trunc((popcount(inPlay) - 1) / 2)`.
* **Cast a vote** (`0x441BB0(P; candidates, votes)`): scan candidate slots `c = 1..31` that are in play and
  flagged in `candidates`. If `c == P.slot` the AI votes for itself and stops (`votes[P.slot] = c`). Otherwise
  it takes `A = attitude(P, c, 1)` (vtable `+0x84`) and remembers the candidate with the smallest `A` that is
  **strictly negative** and strictly smaller than the best so far (ties keep the lowest slot). The vote is
  `votes[P.slot] = best` (0 = abstain when no candidate has `A < 0`). Note the early exit: a voter that is
  itself a candidate votes for itself even if a lower-numbered candidate was seen first.

### 8.5 Single-player / simultaneous procedure - V

1. If exactly one candidate exists, the in-play non-candidate with the largest population (`> 0`) is added as
   a second candidate (not done on the sequential path).
2. Each in-play slot 1..31 votes: the local human through the dialog `DIPLOVICTORYVOTE` (the dialog lists the
   candidates' names), AI through `0x441BB0`. `[0xA528F0 + 4*voter]` = the chosen candidate.
3. The dialog `DIPLOVICTORYVOTERESULTS` shows the tally.
4. A candidate `c` wins when the number of voters whose choice is `c` is **greater than**
   `trunc((popcount(inPlay) - 1) / 2)`; the victory is type 3 with winner `c`. Otherwise `NODIPLOVICTORY` is
   shown and the cooldown (section 8.1) applies. One vote counts for each voter regardless of size.

With seven civilizations the threshold is 3, i.e. four votes are needed.

### 8.6 Sequential tail: `0x4F6810(slot)` - V

For a human `slot` with a pending vote (`[0xA52990]` and `[0xA52991]` set and `[0xA528D0 + slot] == 0`) it
shows `DIPLOVICTORYVOTE`, stores that human's choice and sets `[0xA528D0 + slot] = 1`. It returns state 3 when
the slot has already voted. Once the number of humans in play that have voted equals the number of humans in
play, the AI civilizations vote through `0x441BB0`, `DIPLOVICTORYVOTERESULTS` is shown and the same majority
rule as step 4 of section 8.5 is applied.

## 9. Time limit, Retire and quitting

### 9.1 Time limit (type 5) - V

* If `[0xA283DD] == 0` and `turn == endTurn - 20` (`endTurn = [0xA528C4]`): show `RETIREWARNING` (the date
  string of `endTurn`, via `0x5DF100`/`0x5DF2B0`) and set `[0xA283DD] = 1`.
* The game ends when `turn == endTurn` (the turn counter was already incremented for this round) or, in
  multiplayer only, when real elapsed time `[0x74AF84] >= [0xA528BC]` (accumulated with `timeGetTime` and
  `[0x74CFD8]`, `[0x74D09D]`). In multiplayer without `[0x74D0CB]` the routine instead sends `0x475EC0(0x74AF60)`
  and returns 0.
* **Winner**: the leader by VP if VP mode is on: scan slots (teams: summed) for the largest `+0x11CC` greater
  than 0, remembering a bitmask of civilizations tied at that value. No VP leader (`best <= 0`) -> the
  civilization with the highest `Player.+0x4C`. One VP leader -> that civilization. Several tied leaders ->
  among them the one with the strictly greatest `+0x4C`, scanning ascending from an initial `0.0f` (so a
  tie at exactly `0.0` keeps the first scanned; if none is positive the winner slot is 0). The result is
  type 5.

### 9.2 Retire (`0x4E0130`, type 6) - V

After the dialog `REALLYRETIRE` (yes): `[0xA283DC] = [0xCC37BC] = 1`. In a sequential game, if any **other**
human is still in play (scanning slots from 1, comparing against the local slot), both flags are cleared
again (the game continues for the others). The winner is the civilization with the greatest `Player.+0x4C`
among slots 31 down to 1 not above `[0xA5279C]` (initial `-1.0f`, strictly greater updates, so equal scores
keep the **higher** slot; no in-play test). Then `[0xA526A0] = 6` and `[0xA526A4] = winner`; if no victory was
set before: multiplayer notifies `0x499940`, then `0x4F0E60(6, winner)`. If the game was not quit (others still
playing), a byte of the calling UI object (`this+0x2E198`) is cleared, the retiring player is eliminated by
`0x568950(Player[local]; -1, 0, 1)` (no respawn, no killer, force) and `[0xA526A0]` is reset to `-1`.

### 9.3 After a victory is found - V

If type 4 and the winner is the local human in single player, the UI hooks of section 7.3 run; in
multiplayer `0x499940`. Then `0x4F0E60(type, winner)`, `[0xA283DD] = 0`. In multiplayer (or if multiplayer at
entry) `0x4F401B` sets the quit flags and returns 1. Otherwise the dialog `GAMEOVERMAN` offers: end (sets
`[0xA283DC] = [0xCC37BC] = 1`, return 1) or continue (return 0; `[0xA526A0]` stays set, so every later call
returns 0 at guard 3 and the game goes on with the winner recorded).

## 10. Final announcement `0x4F0E60(type, winner)` - V unless marked

1. Multiplayer: if `[0x74D0A0]` is already set, return; else set it. Sequential non-retire victory with a human
   winner other than the local player: the local slot becomes the winner for display.
2. The routine returns immediately when `n = [0xB38C60 + 0x20]` (an object returned by `0x5426B0`; identity
   **O**) satisfies `n & (n - 1) == 0` (**H**: the announcement needs at least two bits set, i.e. a result
   exists for at least two entries of some list).
3. **Early-victory bonus.** If `winner == local` and the type is not 5 or 6:

   ```
   Player[winner].+0x4C = (float)( (U(endTurn) - U(turn)) * (RT.+0x2C + 1) ) + Player[winner].+0x4C
   ```

   with `turn = [0xA526AC]`, `endTurn = [0xA528C4]`, `U` = section 3. The bonus is added **once**, to the
   float mean, so it is large compared with the mean itself.
4. Text: `END_GAME_WIN` / `END_GAME_LOSS` selected by type (win index: 0->0, 1->3, 2->4, 3->2, 4->1, 5->`WIN2`
   index 0, 6->3, 7->5, 8->6; loss index: 0->1, 1->4, 2->5, 3->3, 4->2, 5->7, 6->0, 7->6, 8->8), with a
   tooltip variant `_TT` that appends the real-time clock `[0xA528C8]` (hours:minutes:seconds). Cosmetic
   variation uses C `rand()` parity (not the gameplay RNG).
5. Debug: `[0x9C40F4]` or `[0xA52680]` bit 2 sets the local player's `+0x4C` to 0 (multiplayer clears bits
   `0x0C` of `[0xA52680]`).
6. The hall-of-fame hook `0x4A97A0` follows; its layout is **O**.

## 11. Where it runs in the round (integration)

Round driver `0x4F61B0` calls `0x4F5EF0` (the round), then `CheckVictory(0)` at `0x4F677E`. If a victory
appeared during the round (`[0xA526A0]` changed from `-1`), the driver sets `[0xCC37BC] = [0xA283DC] = 1`.

Round end order inside `0x4F5EF0` (V):

1. Slots 31 down to 0: the AI phases for each in-play non-human player: `0x4F4F70`, `0x446840`, `0x445EA0`,
   `0x561220`, `0x449B20(P; 0)`, `0x449B20(P; 1)`, `0x4F4EA0`.
2. Slots ascending: `Player::turn` (`0x5604B0`) for in-play non-human players (all players when the game is
   not sequential); then `0x441F10(RT)`.
3. `0x4F5250`, `0x4F4380`, `0x4F4CB0`, `0x4F5040` (bodies not read here; **O**).
4. `0x57DE90(0xB72888; 1)`, `0x441F80(RT)`, **`0x538480(RT)`** (the score means, section 4),
5. `[0xA526AC]++` (`0x4F6167..0x4F6172`),
6. `0x542230(0xB38C60)`, `0x406120`, `0x5B04F0`.

`CheckVictory` is also called from `0x4F5550` (`0x4F5D0C`, arg 0) after each human's turn in sequential
games, followed by `0x4F6810(local)`; from the network handler `0x4695F0` (`0x469700`) and from `0x47A510`
(`0x47A523`).

## 12. Elimination and respawn: `Player::eliminate` (`0x568950`)

Signature: `0x568950(P; continentId_or_-1, killerSlot, force)`, `ret 0xC`. Returns 1 if the civilization is
eliminated, 0 if it survives or was respawned. `continentId` is the continent of the last city (callers in
the city-destruction code pass it) and enables the respawn search; `-1` disables it. `killerSlot` 0 means
"find one". All V unless marked.

### 12.1 Survival test

Return 0 at once if `P.slot == 0` (barbarians) or `P` is not in play. Unless `force`:

* with neither regicide flag (`flags & 0x1800 == 0`): the civilization survives if `P.+0x194 > 0` (it still
  has a city); otherwise it survives if it owns a unit whose PRTO `+0xB0` has bit 1 (Build City).
* with a regicide flag: it survives only if it owns a unit with the King ability (`0x5BC8B0(u, 0x1D)`).

### 12.2 Effects of elimination

1. For every other in-play civilization the relation bytes `+0xD50` and `+0xD70` toward `P` (and `P`'s toward
   them) are cleared.
2. The slot bit is cleared in `[0xA526C0]` (in-play); the human bit in `[0xA526BC]` is cleared too, remembering
   "was human" for the respawn decision; the race bit is cleared in `[0xA526C4]`.
3. Cities with population are destroyed (`0x4AECC0(city, 1)`); four per-owner object lists (owner byte
   `+0x2C`; routines `0x5DAA90`, `0x5DAEC0`, `0x5DB990`, `0x5DB4E0`; these are the Colony, Airfield, Radar Tower and
Outpost pools, `colonies.md` 7 row `0x568950`) are emptied.
4. Units: units with the Flag ability (`0x0D`) are kept; every other unit that still has hit points (or is an
   Army) is killed through `0x5BBBC0(u, killer, 0, 0, 1, 0, 0, 0)`. If anything changed `0x57E450(0xB72888; 0)`
   (trade network recompute) runs.

### 12.3 Respawn - condition and procedure

Condition: `P` was **not** human, `continentId != -1`, `P.+0x40` bit 1 is clear and `flags & 0x80` (Respawn
AI) is set.

1. Search the cells of the continent: a cell qualifies if `0x56D7D0(x, y, -1, 1) == -1` (nothing stands on
   the tile: no city, no unit of any civ or barbarian, no colony owner, no camp; `goody-huts.md` 6.1, **V**), none of the 121 neighbourhood cells (offsets `0x5E6E50`, `k < 121`) is
   owned and none holds a non-barbarian foreign unit. The site with the largest `0x442480(RT; x, y, slot, 0,
   0)` strictly greater than 0 is chosen (the valuation function's gates are in `goody-huts.md` 6.5, the scoring body is **O**); no site -> no
   respawn.
2. Create the units on the site (`0x5694D0`): the start-pack units of the civilization record, RULE
   `start_unit_1`, `start_unit_2`; king units when `flags & 0x1800` and `[0x9C40EC]` (1, or 7 with
   `0x1000`); the flag unit `[0x9C7314]` if `flags & 0x24000` and `[0x9C40E8]` and no owned unit with the Flag
   unit AI-strategy bit (PRTO `+0x8C` bit 18) exists.
3. Treasury becomes `Player.+0x44 + Player.+0x48 + 10 * RULE.starting_treasury` (`[0x9C72AC]`), stored through
   the pair split (`n <= 0`: `+0x44 = timeGetTime() mod 0xD431 - 0x8235`, `+0x48 = -(+0x44)`; `n > 0`:
   `+0x44 = timeGetTime() mod n - 0x3039`, `+0x48 = n - (+0x44)`; economy.md section 3.2).
4. `P.+0x40 |= 2`; the slot bit is set again in `[0xA526C0]` and the race bit in `[0xA526C4]`; return 0.

### 12.4 No respawn

For each other in-play civilization `i`: `0x500830(P; i; 0)` (settles deals) and `0x503A10(Player[i]; j, 1/2,
P.slot, 0)` for the other in-play `j` (cleanup of agreements; semantics **O**). For every civilization with a
contact byte `+0xD30[P] != 0` and `+0xA4 == 1` (mobilized): `+0xA4 = 0` and its cities are recomputed
(`0x4B0E80`).

If `P` is the local human:

* `P.+0x11B5` = the killer (`killerSlot` if non-zero), else the top scorer: scan slots 31 down to 1, excluding
  `P`, skipping slots above `[0xA5279C]`, with **no in-play test**, taking the strictly greatest `+0x4C`
  (initial `-1.0f`; ties keep the higher slot).
* Non-sequential game: if `[0xA526A0] == -1` it is set to type 1 (Conquest) with that civilization as the
  winner, `[0xA283DC] = [0xCC37BC] = 1`, multiplayer `0x499940`, then `0x4F0E60(1, winner)`.
* Sequential game: the "civ destroyed" popup `0x4DCE90(0x9F8700; slot, killer)`.
* Multiplayer: `0x46F880` notifies the others.

Returns 1.

## 13. Golden vectors (hand-computed from the formulas above; not captured from the game)

`U(t)` with the default calendar: 

| t | 0 | 25 | 50 | 65 | 90 | 100 | 140 | 240 | 300 | 340 | 440 | 540 | 1000 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `U(t)` | 0 | 1250 | 2250 | 2625 | 3250 | 3450 | 4250 | 5250 | 5550 | 5750 | 5950 | 6050 | 6510 |

Early-victory bonus, end turn 540, victory at turn 300: `U(540) - U(300) = 500`; the bonus is
`500 * (level + 1)` = 500, 1000, 1500, 2000, 2500, 3000 for difficulty levels 0..5.

Culture threshold `0x538230` (scenario override off, `W` and `H` the map extents):

| W | H | `[0xA529A0]` | intermediate `trunc(0.5*(H+W)*c')` | T |
|---|---|---|---|---|
| 40 | 40 | 100000 | 39999 | 40000 |
| 60 | 60 | 100000 | 59999 | 60000 |
| 80 | 80 | 100000 | 79999 | 80000 |
| 100 | 100 | 100000 | 99999 | 100000 |
| 120 | 120 | 100000 | 119999 | 120000 |
| 80 | 80 | 50000 | 39999 | 40000 |
| 80 | 80 | 1000 | 799 | 5000 (clamped up) |
| 500 | 500 | 100000 | 499999 | 500000 |

(The intermediate is one less than the "natural" value because `0.01f` is slightly below 0.01; the `+499`
brings the result back to the thousand.)

Per-turn score term of a city, `pop 8`, moods happy 2, content 4, unhappy 1, resisting 0, specialists 1:
`8 - 0 - 1 + 2 = 9`; A contribution 2; B contribution `8 - 0 - 1 - 2 = 5`. Running mean: `t = 3`, old mean
10.0, value 14 -> `(10*3 + 14) / 4 = 11.0`.

Majority: 7 civilizations in play -> `popcount(inPlay) = 8` -> threshold `trunc(7/2) = 3` -> a candidate needs 4
votes. Domination with 3200 counted tiles and 66 %: `tilesNeeded = (66*3200)/100 = 2112`, the winner needs
2113 or more.

Attitude class with `W = H = 80`: `t = 80`, `m = 8`: `A <= -9` class 0; `-8..-1` class 1; `0` class 2; `1..8`
class 3; `>= 9` class 4.

## 14. Verification status, open items, hypotheses

Verified by reading the complete routine: `0x4F1B60` (all branches listed above), `0x4F0E60`, `0x538480`,
`0x5382B0`, `0x538230`, `0x568950`, `0x4E0130` (first 170 lines of 976 bytes: the confirm / winner / eliminate
path), `0x441B40`, `0x441BB0`, `0x440AD0`, `0x5DF030`, `0x5E23C0`, `0x5E2420`, `0x5DF900`, `0x538FE0`,
`0x538FF0`, `0x4F6810` (vote, tally and AI part; the cleanup tail was skimmed), the VP windows in `0x4ACF40`,
`0x5BBBC0`, `0x561860`, `0x563410` and `0x5BD220`.

Open (**O**) and hypotheses (**H**), none of which changes a rule above unless noted:

* `0x442480` start-site valuation scoring body (decides where a respawned civilization lands; its rejection gates
  are specified in `goody-huts.md` 6.5); `0x503A10`; the four per-owner lists emptied by `0x568950`.
* `0xB38C60` (`0x5426B0`) identity and its `+0x20` gate in `0x4F0E60`; the hall-of-fame hook `0x4A97A0`.
* Messages 11 and 12 of the victory-location handler; the bits of `[0xA52680]` other than bit 2.
* The sign convention of `0x440100` (lower = friendlier is **H**).
* In-play bit 0 always set (**H**).
* x87 precision control (**H**: 53 bits).
* Callers and early part of the flag-delivery routine `0x5BD220`; UI hooks `0x5A3910`, `0x4E2820`,
  `0x499940`, `0x475EC0`, `0x46F880`, `0x540710`.
* Multiplayer-only words `[0x74D09F]`, `[0x74CFC4..0x74CFD4]`, `[0x74D0A0]`, `[0x74D0CB]` (only their uses in
  `CheckVictory` are described).
* `Player.+0x15B4` (cities lost) writer; `Player.+0x15C8` meaning beyond "capital lost".
* Bodies of `0x4F5250`, `0x4F4380`, `0x4F4CB0`, `0x4F5040`.

## 15. Corrections this document makes to others

* `Player.+0x104` is the number of future technologies (research.md already says so); it is **not** a
  "wonder/other points" term (an earlier note of this analysis said so; wrong).
* `Player.+0x4C` is the running mean of the per-turn score (section 4), not a "score so far" snapshot.
* `RT.+0x2C` / `[0xA52684]` is the game difficulty level (a multiplier `level + 1` in the score and the early
  bonus); `economy.md` already indexes the DIFF table with it. `[0xA5267C] & 0x200` is Accelerated Production
  (corrected in `economy.md`, which had called its name open); `Player.+0x30` is (I, from the player setup, not
  re-derived here) the **per-player** difficulty, still unnamed in `economy.md`.
* `yields.md` claimed nothing sets the Science Age; it is set by `0x55C830` from the unit action `0x5C03B0`
  (`research.md` section 11). Corrected in `yields.md`.
* `[0xA52B40]` is the GAME `theme`.
* `government.md` and `rust/src/government.rs::attitude_class` use a margin of `(W/2)/5`; the code divides by
  **10** (`0x440B06`, `0x440B2B`). The Rust module and its test must be fixed or marked non-authoritative.
* The kill / wonder / advance / capture VP counters are `+0x15BC`, `+0x15B8`, `+0x15C0`, `+0x15C4` (section
  1.4); `capture.md` (section 3 step 2) calls `+0x15C4` and `+0x11CC` "unnamed".
