# Culture, corruption, trade economy

Owns: culture accumulation/border expansion, corruption/waste, commerce
split, victory culture thresholds, city growth (the food box), unit
support, building upkeep and the treasury's turn. Reference:
`rust/src/economy.rs`; the clone's rules live in `src/economy.rs`. The tile
yields and the city's per-turn totals (food eaten, the shield multiplier, the
whole commerce split, Wealth, tourists, specialists) are in `yields.md` with
`rust/src/yields.rs` and `rust/src/city.rs`.

## Culture-border expansion event `0x4B0D70…` (verified: `r2`)

The expansion *event* (dialog + flag + map update), not the
accumulation math. Shape is the founding/diplo dialog idiom:

```asm
0x4b0d9b  call 0x47B530            ; mode check → 0x4000 MP flag (neg/sbb/and)
0x4b0db5  push CULTUREBORDERVERBOSE ; 0x6861AC
0x4b0dba  push 0xCADC18
0x4b0dbf  call [edx+0x170]          ; modal dispatch (4th 0xCADC18/0x170 site)
0x4b0dcb  call 0x611530             ; commit
0x4b0de8  mov ecx, [edx*4+0xA52ED8] ; owner-indexed flag table ([0x9FD4BC] key,
0x4b0def  or ecx, 8                 ;  same lea/shl index math as relation matrix)
0x4b0df2  mov [edx*4+0xA52E98+0x40], ecx
```

Then an owner branch (`[esi+0x28]` vs `[0x9FD4BC]`): own city pushes
`CULTUREBORDER` (`0x4B0E04`), foreign pushes `CULTUREBORDEROVER`
(`0x4B0E1F`), both `call 0x4ED220(this=0x9F8700, x=[esi+0x24],
y=[esi+0x26])` (HYPOTHESIS: border map repaint from tile-coordinate
words). Pre-calls: `0x53AA50(this=0xA9590C)` + `0x61C5A0` twice.

## Culture math (verified: sweep + parent spot-checks)

* Thresholds `0x4B0C60` (parent-verified head): prologue `51 53 55
  56 57 8BF1`; accumulated `[esi+owner*4+0x140]` (`owner =
  [esi+0x28]`); level = count of powers `base^k <= accum` for
  `base=[0x9C7300]`, capped at 6 (`cmp ebx,6`); stored to
  `[esi+0x5C]`. City-screen bar `0x41988F` renders `fild/fidiv`
  progress ratio (child-reported).
* Per-turn `0x4B2680` (sole caller `0x4BEBDB`): zeroes
  `[city+0x13C]`, loops BLDG table (`count [0x9C3D80]`, stride
  `0x110`) with has/obsolete/flag-`0xF0&4` gates; accumulation at
  `0x4B2897` adds into `[city+owner*4+0x140]` with `max(0,·)` clamp,
  then calls the border check (child-reported). **Superseded** by `borders-culture.md` 2 (verified body, including
  the xenophobic gate and the obsolescence test).
* Per-building `0x4F8CE0` (child-reported): base `BLDG+0x98`,
  doubled if age (`0x4C2420`) vs 1000, halved (`(ebp+1)/2`) when
  `[owner*8420+0xA52F3C]==1`. **Verified and extended** in
  [`city-buildings.md`](city-buildings.md) section 6: obsolete non-wonders give 0, a building whose recorded
  builder is not the owner gives 0, the doubling applies only to buildings the city itself has, and
  `+0xA4 == 1` is the halving condition.
* Victory ini keys confirmed: `one city culture to win`
  (`0x72D3CC`, default 100000) / `all cities culture to win`
  (`0x72D3B0`, default 66 — units HYPOTHESIS) at `0x58607C`/
  `0x58611D`.

## Border shape by culture level (verified: save corpus)

A city's border level is `[city+0x5C]`, the culture level of `0x4B0C60` (1 up to
culture 9, one more per power of `RULE.border_factor`, 10 in the shipped rules,
capped at 6). The tile owner is `Cell+0x05` of each save's map. Measured over
the shipped and sample saves (2,856 cities; `biq/examples/border_stats.rs`),
counting only tiles where no other city is as close:

| level | tiles claimed | squared tile distance |
|---|---|---|
| 1 | 3x3 square, 9 | `<= 2` |
| 2 | the 21-tile city radius | `<= 5` |
| 3 | 37 tiles | `<= 10` |

The rule that fits every level is `d² <= level² + 1` in tile steps (the diagonal
neighbour counts one step on each axis; in the save's doubled grid `d² =
(dx² + dy²) / 2`). Inside the shape 3,045 of 3,061 sampled tiles belong to the
city's civ, and **none** of the 41,790 tiles outside it do. The 16 misses are
tiles another civ's older claim holds. No save has a city above level 3, so
levels 4 to 6 (`17`, `26`, `37`) were a **HYPOTHESIS** and are **wrong for levels 4 and 6**: the executed
ring enumerator and the table `0x670540` give the squared reach `2, 5, 10, 18, 26, 41` in tile steps (the table holds
twice these, in doubled-grid units; tile counts 9, 21, 37, 61, 89, 137; `borders-culture.md` 4).
The shape is not `level² + 1` and the claiming procedure (cities in order, tie-break, ocean and bracket rules) is
specified in `borders-culture.md` 5.

Where the shapes of two civs overlap (`biq/examples/border_contest.rs`, 1,671
tiles, 263 of them equally near), the nearer city's civ owns the tile in 89% of the 1,408
others, ties go to the city with more culture in 84%, and the oldest city wins only 53%. The rest is
sticky ownership the clone does not model (a tile keeps its owner while that
civ's border still reaches it). Tiles reached by cities of one civ are that
civ's whichever city is nearer, so the borders of its cities merge: 29,655 of
29,708 such tiles belong to the civ.

## Corruption math (verified: the whole routine, executed; see "The whole routine")

`0x4B1190(city, gross, kind)` returns the amount **lost** (`ret 8`; verified
2026-10-01, see `yields.md` section 5): `kind` 0 is corruption of the gross
commerce (`0x4B07C0`), `kind` 1 is waste of the gross shields (`0x4B05D0`).
The head, in order: `gross <= 0` returns 0; `kind != 0` and the city in
disorder (`[city+0x30] & 1`) returns `gross` (`0x4B11BB`); no capital (the
player's `+0x2C` is not a live city id) returns 0; a government whose class
`[GOVT+0x18C]` is 4 (Anarchy) returns `gross` for **both** kinds (`0x4B1239`);
the tail clamps the loss to `gross` (`0x4B1A1A`). The rest is the distance and
Courthouse arithmetic below:

* Courthouse count `0x4B1250` (child-reported): has + !obsolete +
  `BLDG+0xEC` bit 16; capital bonus `ebx+10`.
* OCN, the optimal city number, `0x5676C0` (**verified**, `this` = the
  player; `rust/src/economy.rs` `optimal_city_number`). With `B` = the world
  size's "optimal number of cities" (`WSIZ[[0x9C73A0]]` memory `+4`, table
  `[0x9C7330]` stride 84; shipped 14 / 17 / 20 / 28 / 36 for Tiny to Huge):
  `B + 3 * N * B / d` where `N = 0x55AA10(player, 0x20, 0)` counts the
  player's Forbidden Palace-like buildings (below) and `d` is 1 for GOVT class 5
  and 8 otherwise; plus `B / 4` for a Commercial civ (`RACE.vtable[0](1)`);
  plus a class term from the jump table `0x567818` on `[GOVT+0x18C]`
  (class 0 and 1: `B / 8`; 2: `B / 16`; 3 and 4: nothing; 5: `2 * B`;
  anything else nothing); plus, for a player that is not in the human mask
  `[0xA526BC]` only, `B / 2` above game level `[0xA52684]` 4, `B / 4` at 4
  and `B / 8` at 3; times `DIFF[player+0x30]` memory `+0x6C` (body `+0x68`;
  shipped 100, 95, 90, 85, 80, 70, 60, 50 from Chieftain to Sid) over 100;
  at least 1. Every division truncates. Standard map, class 3, no
  Palace-likes, human: 20. The AI's capture decisions use `2 * OCN` as the
  city limit (`capture.md` section 12).
  * `0x55AA10(player; mask, cityFilter)` (`ret 8`, verified) counts wonders:
    BLDG rows with `+0xF4 & mask` (memory offset; each small wonder has its
    own bits there) whose required government `+0xD4` is -1 or the player's
    `+0xA0`, that are small (`+0xF0 & 8`; city id `[player+0x15E8][row]`) or
    great (`+0xF0 & 4`; city id `0x539030(0xA52658; row)`, unread), and whose
    city is owned by the player and, when `cityFilter` is non-zero, is that
    city. Ordinary buildings never count. With mask `0x20` that is the
    Forbidden Palace and the Secret Police HQ (required government 3,
    Communism), which is why those two raise the optimal city number.
  * GOVT `+0x18C` (the editor's corruption and waste level; row body `+0x178`)
    is 0 minimal, 1 nuisance, 2 problematic, 3 rampant, 4 catastrophic, 5
    communal. Shipped: Democracy 0, Republic 1, Fascism 1, Monarchy 2,
    Feudalism 2, Despotism 3, Anarchy 4, Communism 5. That fixes the
    "communal HYPOTHESIS on 5" below: class 5 is Communism.
* Distance arms (class switch `[ebp*4+0x4B1A2C]`, table bytes
  parent-verified: `[0x4B14E2, 0x4B156F, 0x4B156F, 0x4B1576,
  0x4B14F0, 0x4B14E5]`): arm 0 = `3D/4` (parent-verified `lea` +
  trunc `/4`); arms 1–2 = `D`, 3 = `3D/2`, 4 = `(W+H)/4`, 5 =
  `(W+H)/16` (child-reported; class 5 is the communal class, Communism).
* Rank loop `0x4B1617` over `[0xA52E78]` cities (child-reported):
  same-owner + capital-distance ordering with tiebreaks; finalize
  (parent-verified at `0x4B18D9`): `rank' = rank>=R ? 2*rank-R :
  rank`, then `imul gross`.
* **Not waste: food eaten** (corrected 2026-10-01; the child report
  mislabeled it). The expression `([city+0x138] − resisters) * [0x9C72B4]`
  (disorder: the whole food gross) is `0x4B0540`'s food eaten, not a loss
  routine. `[city+0x138]` is the city's **population** (`0x427540` compares
  it with the town/city caps, `0x4B23D0` adds one on growth), the subtracted
  count is the citizens whose byte `+0x20` is set, "resisting"
  (`0x4BB2A0`), and `[0x9C72B4]` is RULE "Food Consumption per Citizen" (body
  `+0xEC`, 2). See `yields.md` section 5.3 and `rust/src/city.rs`
  (`food_eaten`).

### The whole routine (2026-10-04, **V** + **E**)

Read end to end from the raw body `0x4B1190..0x4B1A27` (jump table `0x4B1A2C`) and **executed**: the real
routine was run in the emulator (`tools/emu/corruption.py`) over the loaded save `yolo.SAV`, with every city
handed to one player and its government class (0..5), Courthouses, disorder/celebration flags and a
Reduces-Corruption small wonder varied; all 3 456 results equal `rust/src/economy.rs` `corruption` (64 kept as
golden vectors). Integer ops truncate toward zero unless said.

```
if gross <= 0:                              return 0
if waste and city in disorder:              return gross
if owner has no capital:                    return 0
if GOVT[owner].class == 4:                  return gross
C = courthouses + (10 if this is the capital)            # 0x4B1250..0x4B12C3; BLDG +0xEC bit 8, present, not obsolete
O = OCN(owner) + B * C / 4                               # 0x5676C0; B = WSIZ +4 (shipped Standard 20)
if waste and celebrating:  O += B / 4
d = dist(capital, city)                                  # 0x4378D0, the game's metric (borders-culture.md 9)
for each BLDG row with +0xF4 & 0x20 whose required government is -1 or the owner's:
    wc = small wonder: city id Player[owner].+0x15E8[row];  great: 0x539030(row), owned by owner
    if wc exists:  if wc == city: C += 7;   d = min(d, dist(wc, city))
m = (mapWidth + mapHeight) / 4                            # [0x9C74D4] + [0x9C74C0]
e = class 0: 3d/4 | 1, 2: d | 3: 3d/2 | 5: m/4 | else: m
if not 0x57F0A0(city, capital, owner):  e = 5e/4          # not trade-connected to the capital
e = (m < 2) ? 2 : clamp(e, 2, m)
if waste and celebrating:  e = (e + 1) / 2
repeat C times:            e = (e + 1) / 2
share = e * gross
rank  = class 5: Player.+0x194 / 2
        else: number of the owner's other cities (the capital included) nearer the capital than this
              city by dist; at equal distance the tie chain below
rank' = rank >= O ? 2*rank - O : rank
lost  = (m * ((rank' * gross + 1) / 2) + share * O + (m * O) / 2) / (m * O)
lost  = (Σ CTZN[job].+0x78 over non-resisting citizens >= lost) ? 0 : lost - that sum      # Policeman
lost  = lost * DIFF[owner difficulty].+0x74 / 100                                          # shipped 100 everywhere
cap   = max(9 - C, 0) * gross / 10
return (cap < 0 or lost < 0) ? 0 : min(lost, cap)
```

Consequences: the capital loses nothing (C ≥ 10, cap 0); a Forbidden Palace city at most 20 %; with no Courthouse
the loss never exceeds 90 %; a Courthouse both halves the distance share and raises the rank threshold by `B/4`;
the Forbidden Palace counts in `OCN` (`3 * N * B / 8`, see above) and replaces the capital as the distance origin
for every city nearer to it. The connection test passes the owner as the third argument (the capital lookup
`0x437870` is `ret 4` and leaves the owner pushed below it).

**Equal-distance ties** (`0x4B16EB..0x4B18B4`): with `o` the other city and `t` this one, `z(X) = X.+0x364 || 1`,
`s(X) = X.+0x35C + 1`, `u(X) = X.+0x360 + 1`, and the mode `m = o.+0x358`: mode 0 ranks `o` ahead when `z(o) < z(t)`,
or `z(o) == z(t)` and `o.id < t.id`; mode 1 when `z(o) < z(t)`, or equal `z` and `s(o) < s(t)`, or both equal and
`o.id < t.id`; mode 2 whenever `z(o) == z(t)` (first stage, `0x4B172C`; its second stage, which would compare `u`
and the ids, is reached only with `z(o) != z(t)` and then skips); any other mode never ranks `o` ahead. **E**: the words are
**not** zero in play: every city of `yolo.SAV` holds mode 1, `+0x364 = 1450`, `+0x35C` 0 or 6. Their writers are still
open (`borders-culture.md` 11 item 1); the clone ranks equal distances by city order only.

**Clone** (`src/citycalc.rs`): the clone's square grid is the native diamond, so a clone offset `(dx, dy)` is the
native `(dx - dy, dx + dy)`; the map size in the clamp is the shipped Standard map, 100 by 100 (**H**);
`src/trade.rs` supplies the connection (roads, harbors over coast, `trade-network.md`).

## Commerce split (verified 2026-10-01: all of `0x4B07C0..0x4B0AA5`)

The full chain, with the tourist income, the multipliers, Wealth and the
specialists, is in `yields.md` section 5.5 and `rust/src/city.rs`
(`commerce_split`). The first part, as first read at `0x4B0802–0x4B0881`:

```asm
call 0x4B1190               ; lost commerce (kind 0)
[esi+0x24C] = lost          ; 0x4B0814
[esi+0x258] = gross - lost  ; 0x4B0821 (net)
[esi+0x25C] = (net*[o*8420+0xA5303C]+5)/10   ; LUXURY, 0x66666667 magic
[esi+0x260] = (net*[o*8420+0xA53040]+5)/10   ; SCIENCE
```

Owner index rebuilds `o*8420` inline (`al*2105*4` lea/shift chain at
`0x4B0827–0x4B0841` — second independent confirmation of the 8420
stride). Rates are tenths in the per-owner table: `Player+0x1A4`
(`0xA5303C`) is the **luxury** rate and `+0x1A8` (`0xA53040`) the **science**
rate.

**Correction (2026-10-01):** this section used to label the two streams the
other way round. `[city+0x25C]` is luxury and `[city+0x260]` is science: the
research routines (`0x562200`, `0x55D810`, `0x569E80`) sum `+0x260`, and the
per-turn income routine `0x560160` adds `0x4ACA50(city, 1, 0)` (stream 1 plus
the specialists' research) to the player's research cell `Player+0xF8` and
`0x4ACA50(city, 2, 0)` (stream 2, tax) to the treasury, city by city. The tax
stream is `[city+0x264] = net − lux − sci` with the *revised* shares: when
`lux + sci > net` the luxury share becomes `net − sci` (`0x4B0887`). The
multipliers, formerly child-reported, are verified: each stream times
`(2 + n) / 2`, `n` counting the present, non-obsolete buildings whose BLDG
`+0xEC` has bit 3 (luxury), bit 2 (research) or bit 4 (tax); research also
`+ 2 *` the Doubles-Research wonders (`0x55A8D0`, BLDG `+0xF8 & 0x10`). The
tourist income of `0x4B0710` is part of the gross, the Wealth gold is added
to tax after the multiplier, each stream is clamped at 0, and
`[city+0x258]` ends as the sum of the three. In `conquests.biq` no building
has bit 3; the Marketplace has bit 4 and bit 10 (`0x400`, "Increases Luxury
Trade").

Mood flags `[city+0x30]` (bit0 disorder, bit1 celebration): verified in `happiness.md`. The
culture-flip test `0x4B28D0` doubles an intermediate score for a city in disorder (`0x4B2AFF`) and
halves it for a celebrating city (`0x4B2B6A`); the "distance" of the earlier note is that score
(full algorithm: `borders-culture.md` 9).

## Growth and the food box (verified: r2 + Civilopedia)

Per-turn food routine `0x4B2030` (this = city). `[city+0x40]` is the
stored food, `[city+0x250]` the net surplus, `[city+0x138]` the
population:

```asm
0x4b2039  eax = [city+0x250]             ; net food surplus
0x4b203f  [city+0x40] += eax             ; stored food; edi = the new total
0x4b2049  jns 0x4b219d                   ; total >= 0: growth test, below
0x4b2051  [city+0x40] = 0                ; total < 0: starvation
0x4b208c  push "STARVE"                  ; "Starvation at $CITY0!" (human owner)
0x4b20a5  call 0x4ba230(city, 1, -1, 1)  ; one citizen goes (signature HYPOTHESIS)
0x4b20b0  [city+0x138] == 0 -> 0x4aecc0  ; none left: the city is removed
                                         ;   (callee's role HYPOTHESIS)
0x4b21c6  call 0x5660e0                  ; X for the owner
0x4b21cf  call 0x427540                  ; size class
0x4b21d5  imul ebp, eax ; add ebp, ebp   ; box = X * (class + 1) * 2
0x4b21da  cmp edi, ebp ; jl keep         ; grow iff stored + surplus >= box
0x4b21e4  call 0x4b1dc0                  ; growth gate, below
```

* **Box** `2 · X · (class + 1)`: 20 / 40 / 60 for a human (X = 10). The
  test is on the *new* total, so a city grows the turn its store plus
  surplus reaches the box, not the turn after.
* `0x5660E0(player, flag)` is X: 10 when the player's bit is set in the
  human mask `[0xA526BC]` (or `flag != 0`); otherwise
  `DIFF[level].+0x68` (`[0x9C40C0]`, stride 124, `level = [0xA52684]`);
  then halved (`cdq/sub/sar`) when `[0xA5267C] & 0x200`, which **is** the GAME rule bit "Accelerated
  Production" (`biq-format.md` rule-bit list: `0x200`; an earlier revision of this file said the
  name was open); the cost factor `X` is the multiplier of every unit and building cost, so
  halving it halves the cost of everything; minimum 1. `0x569FE0` multiplies BLDG costs by the
  same X.
* `0x427540(city)` is the size class: population `> [0x9C72E8]` is 2,
  `> [0x9C72E4]` is 1, else 0 (town / city / metropolis). The two globals
  are RULE body `+0x11C` and `+0x120` (shipped 6 and 12). **Verified
  2026-10-01:** the RULE reader `0x5E78E0` fills the `0x9C71E4` object
  directly, with `body = object + 0x20` for object offsets up to `0xC0` and
  `object + 0x1C` from `0xC8` on (`combat.md` section 9), so `0x9C72E4` and
  `0x9C72E8` are objects `+0x100` and `+0x104`; the decoded `conquests.biq`
  has 6 and 12 there and the Civilopedia agrees (below). There is no later
  "copy" step. `0x53A960`, `0x4B1DC0` and `0x4B2274` repeat the test
  inline.
* **Growth branch** `0x4B2274–0x4B2365`: counts the city's active
  improvements with BLDG `+0xEC` flag `0x200` (present, and not obsolete:
  a BLDG `+0xE0` tech id >= 0 that the player owns, `0x561440`, skips it).
  Any: `[city+0x40] = X · (class + 1)`, **half the box**. None:
  `[city+0x40] = 0`. The class is the *pre-growth* one, and the store is
  overwritten, so nothing above the box carries over. Then population + 1
  (`0x4B23D0`, saturating at `0x7FFFFFFF`; `0x4B236E` leaves it alone from
  255).
* **Growth gate** `0x4B1DC0(city)` returns -1 (may grow) or the BLDG index
  it lacks. Population >= `[0x9C72E4]` (6) needs an active improvement
  with BLDG `+0xEC & 0x800`, the Aqueduct, unless `0x5F39E0(0x9C736C, x,
  y)` passes (freshwater: `worker-jobs.md` 3.2 and `rust/src/lakes.rs`).
  The exact call is at `0x4B1E7F`, the successful branch `0x4B1E86`.
  Population >= `[0x9C72E8]` (12) needs one with `+0xEC & 0x1000`, the
  Hospital or Shakespeare's Theater. This second scan starts at `0x4B1EBF`,
  counts active, nonobsolete level-3 effects at `0x4B1F3A`, and returns
  the first missing requirement at `0x4B1F6E..0x4B1F84`. **No freshwater
  exemption applies to level 3** (read through the return `0x4B1F8A`). Blocked: `[city+0x40] = box` (the store stays
  full; `0x4B2234`), and for the human at the keyboard (`[0x9FD4BC]`,
  `[0xA526AC] & 0x1F == 0`, `[0xA52678] & 0x400000 == 0`) the "needs a
  building" notice `0x4DD530(0x9F8700, city, bldg)`.
* **Despotism's tile cap** (**located and verified 2026-10-01**): it is
  the second-to-last step of each of the three tile functions (`0x5D75B0`
  food, `0x5D7AB5` shields, `0x5D7FCB` commerce; `yields.md` section 4):
  when GOVT `+0x1C` (Anarchy and Despotism) is set and the yield is above 2
  it loses 1. The city's own square **is** capped (the only exemption is an
  Agricultural civ's city centre next to fresh water, for food only,
  `0x5D759F..0x5D75AE`). Civilopedia `GOVT_Despotism`: "any city
  production square which produces more than two food, shields, or
  commerce in a despotic government instead produces one less"; the
  manual's tutorial says the same of "any terrain square producing three or
  more of any resource type".
* **Clone** (`src/economy.rs`, `cities::process_city_turn`): food box,
  size class and Granary retention follow the rules above. The Despotism cap
  applies to worked tiles and the city center. Agricultural city-center food
  is exempt when the map reports freshwater (`yields.md` 4.1); irrigated
  effective Desert also gets the trait's +1 food before the cap. Growth uses
  freshwater / active Aqueduct-class effects at six, and active Hospital-class
  effects at twelve (`rust/src/economy.rs::growth_limit`). Still different:
  accelerated-production food boxes and difficulty-dependent AI box size are
  unimplemented; a city never starves below size one instead of being removed.

## Gold: unit support and upkeep (verified: r2 + game script text)

**Treasury cell.** Gold is the *sum* `[player+0x44] + [player+0x48]`. The
only writer, `0x4C2350(player, n)`, splits it afresh each time: `n <= 0`
stores `[+0x44] = timeGetTime() % 0xD431 − 0x8235`, `[+0x48] = −[+0x44]`
(sum 0); `n > 0` stores `[+0x44] = timeGetTime() % n − 0x3039`,
`[+0x48] = n − [+0x44]`. A tamper guard; the sum is never negative.

**Turn end** (`0x560BF5–0x560CB0`, this = player). A loop before it
(`0x560B04–0x560BEF`, over the player table, stride `0x20E4`) takes a
per-civ amount `[ebp]` off this treasury, clamped at 0 (HYPOTHESIS:
per-turn gold in diplomatic deals; not modelled). Then the two bills,
**in an order the gameplay `Random` decides**:

```asm
0x560bf5  push 4 ; ecx = 0xA526B4 ; call 0x60bab0   ; Random.next(4)
0x560c06  jne 0x560c5d      ; != 0 (3 turns in 4): upkeep, then units
0x560c08  call 0x55dfd0     ; == 0 (1 in 4): units, then upkeep (0x560C16)
0x560cab  call 0x55dfd0     ;   the units call of the 3-in-4 order
```

* **Upkeep** `0x55CFB0(player, govt = [player+0xA0])`: 0 when GOVT `+0x14`
  is 0 (label HYPOTHESIS); else the sum of `[city+0x2C]` over the cities
  whose owner byte `[city+0x28]` is this player (global list `[0xA52E6C]`,
  `[0xA52E78] + 1` entries). The writer of `[city+0x2C]`, the per-city
  total, is not located. Affordable (`cost <= treasury`): `0x4C2350(
  treasury − cost)`. Otherwise the treasury is set to 0 (`0x560C27`) and,
  when `[player+0x30] >= 1` (field unidentified), `0x560280(player)`
  sells **one** improvement; the unpaid rest of the bill is forgiven.
* **Sale** `0x560280`: `start = Random.next(cities + 1)`, then the global
  city list from `start` with wraparound; the first of this player's
  cities for which `0x4C1590(city, playerId)` returns true ends it.
  `0x4C1590` does the same over the BLDG table (`start =
  Random.next([0x9C3D80])`, stride `0x110`): a building qualifies when
  the city has it (`0x4ACB50`), BLDG `+0xF0` has neither bit 4 nor bit 8
  (HYPOTHESIS: the wonder classes), BLDG `+0xEC` has none of bit 0 and
  `0x800` / `0x1000` (so never an Aqueduct, Hospital or Sewer), and
  `0x4ACDF0` agrees (unread). The sale credits `0x4B32F0` gold
  (`0x4C2350(price + treasury)`), removes the building (`0x4ACF40`), and
  tells the human `MAINTSHORT`: "We can no longer support our
  $IMPROVEMENT0 at $CITY1. We must think more about our treasury!"
  `0x4B3340` returns shields for the city when `0x561480(player, 0x40)`
  holds (HYPOTHESIS: the Recycling effect; Civilopedia `TECH_Recycling`,
  "Returns 25% of the shield cost when selling improvements in addition
  to the gold received").
* **Sale price** `0x4B32F0(city, bldg) = 0x569FE0(player, bldg, 0) /
  [0x9C7268]`. `0x569FE0` is BLDG `+0x94` (cost) times X (`0x5660E0`),
  halved once when any of five probes on the civ-type object (`[0x9C71D0]`,
  indexed by `[player+0x20]`; args 0, 4, 6, 7, 3) meets its BLDG `+0xF0`
  bit (`0x2 / 0x100 / 0x400 / 0x800 / 0x20`). **Verified:** the probes are
  `RACE.vtable[0]` = `hasTrait(bit)` on the trait mask `RACE +0x948`
  (bit 0 Militaristic, 4 Religious, 6 Agricultural, 7 Seafaring, 3
  Scientific; `rust/src/economy.rs` `trait_bit`, `TRAIT_DISCOUNTS`); the mask
  equals the two Civilopedia traits of each of the 31 playable civs
  (`capture.md` section 11 lists the values), and the paired BLDG bits sit
  on the matching buildings (`0x2`: Barracks, Walls, SAM Missile Battery,
  Coastal Fortress, Airport; `0x20`: Library, University, Research Lab;
  `0x100`: Temple, Cathedral; `0x400`: Aqueduct, Recycling Center, Solar
  Plant; `0x800`: Harbor, Offshore Platform, Commercial Dock). **Wonders
  are never discounted:** the probe block is skipped when BLDG `+0xF0` has
  bit 4 or bit 8 (`0x56A068`, `0x56A073`). One match halves the cost once
  (truncating); several do not stack. Commercial has no probe here; its bit
  `0x40` marks Marketplace, Bank and Stock Exchange. After the halving, a
  Center-of-Empire building (BLDG `+0xEC & 1`, the Palace) is multiplied by
  `6 * cities / B` clamped to 3..10 (`0x56A1B2..0x56A1F0`, `B` the world
  size's optimal city count; `palace_cost_factor`), and the result is at
  least 1. `[0x9C7268]` is the divisor the Wealth branch of `0x4B0AC0` (BLDG
  `+0xEC & 0x80000`) also reads. **Its value is 4** (resolved 2026-10-01):
  the RULE reader `0x5E78E0` stores the record directly (object offset `+0x84`
  is body `+0xA4`, `combat.md` section 9), and the decoded `conquests.biq`
  has 4 there, the editor's "Shield Cost Per Gold" (the stock civ3mod.bic has
  8). So a sold improvement fetches `cost / 4` in the game's units, and
  Wealth turns 4 net shields into 1 gold (2 with Economics:
  `yields.md` section 5.6). The earlier "linear map from `0x9C72E4`" attempt
  was wrong because the object-to-body map is not linear: the offset steps
  by 4 bytes at object `+0xC8`.
* **Units** `0x55DFD0(player)`: returns when the city count `[player+0x194]`
  is 0 (`0x55DFE7`, so a start party costs nothing before the first
  city). Else
  `charge = ([player+0x18C] − 0x55D030(player) − 0x55D2A0(player, govt)) ·
  perUnit` with `perUnit` from `0x53A960` (`0x55E006`). `charge <= 0`: done.
  `charge <= treasury`: pay it. Otherwise the **whole treasury** goes
  (`edi = treasury`, `0x55E067`) and, when `[player+0x30] >= 1`, the
  player object picks a unit (vtable `+0x34`; the pick is not decoded), the
  human sees `NOSUPPORT` ("We have insufficient gold to continue
  supporting all our units. One $UNIT1 unit will be disbanded. (Someone
  should be looking after our treasury!)") and it is disbanded
  (`0x5BBBC0`; `0x474140` in multiplayer). **One unit per turn**,
  however large the gap.
* `0x53A960(this = 0xA9590C, player, govt, *perUnit, *free)`: GOVT table
  `[0x9C71D8]`, record stride 488 (`61 · 8`). `base = +0x1D0`, `perUnit =
  +0x1E0`. `base == -1` (the shipped row 0): `perUnit = 0`, no city loop,
  `free = 0`. Else `free = max(0, base + Σ` over the player's cities of
  `+0x1D4 + 4 · class(city))`, class as `0x427540`, summed **civ-wide**.
  `0x55D2A0` calls it for `free`; for a player outside the human mask
  `[0xA526BC]` it goes on to the difficulty table (unread).
* `0x55D030(player)` counts the **exempt** units: those whose owner
  `[unit+0x34]` is the player and either `[unit+0x38] != [player+0x20]`
  (HYPOTHESIS: a unit of another civ, e.g. captured) or whose PRTO row
  `+0xD0` is 0. That PRTO field is the 4-byte read at `0x5E585E`, file row
  offset 202 (the reader packs: `+0xC0` is 14 bytes, then `+0xD0`); in
  `conquests.biq` it is 1 for the 110 ordinary unit types (Settler, Worker,
  Scout, Warrior, ...) and 0 for the 31 Leader units: the editor's "Req.
  Support" ability (`editor.md`). So **every unit the clone has needs
  support**; only Leaders (and, by the first test, captured units) are
  free.
* **Shipped numbers** (GOVT row dwords 134 / 135–137 / 138 = `+0x1D0` /
  `+0x1D4..+0x1DC` / `+0x1E0`, matched by value to the Civilopedia):

  | row | base | town / city / metropolis | gold per unit |
  |---|---|---|---|
  | 0 (Anarchy, HYPOTHESIS: first row) | -1 | 0 / 0 / 0 | 1 |
  | 1 (Despotism) | 0 | 4 / 4 / 4 | 1 |
  | 2 | 0 | 2 / 4 / 8 | 1 |
  | 3 | 0 | 6 / 6 / 6 | 1 |
  | 4 | 0 | 1 / 3 / 4 | 2 |
  | 5 | 0 | 0 / 0 / 0 | 1 |
  | 6 | 0 | 4 / 7 / 10 | 1 |
  | 7 | 0 | 5 / 2 / 1 | 3 |

  The Civilopedia's Despotism entry lists "Unit Support per town 4, per
  city 4, per metropolis 4". Rows 2–7 are not matched to government
  names. The manual (ch. "Paying for support") counts all units, "even
  Settlers"; the Civilopedia's `GCON_Unit_Support` says "military unit"
  and the binary agrees with the manual (`Req. Support` is 1 for
  Settlers and Workers).
* **Clone** (`src/economy.rs`, `cities::end_turn_cities`,
  `actionbar::update_gold`): `finance` is the one source for the turn and
  the info box (`{treasury} Gold ({net:+} per turn)`, red in deficit):
  `tax − upkeep − unit_cost` with `unit_cost = max(0, units − 4 ·
  cities)` gold, nothing without cities. `end_turn_cities` pays income
  first, then **upkeep, then units** (the 3-in-4 order; the dice are not
  modelled). A short upkeep zeroes the treasury, sells one improvement
  (`sale_pick`: highest upkeep, then newest city, then newest building)
  and credits its **shield cost in gold** (`sale_price`; the binary's
  divisor is 4, `[0x9C7268]`, so the original pays a quarter of the cost
  in game units: the clone overpays); a short unit bill spends the treasury and
  disbands one unit (`disband_pick`: cheapest to rebuild, then the type
  listed first in the build list, so a Warrior before an equally priced
  Worker; the entity id only separates units of one type).
  The shipped texts `MAINTSHORT` and `NOSUPPORT` are the messages. Not
  modelled: the first-loop per-civ payments, Recycling's shields, the
  "wonder" and growth-building exclusions (the clone has none), AI
  allowances, captured-unit exemption (no capture), corruption and waste
  (`Corruption math` above), and the commerce split's rounding, which
  at the fixed 50 / 50 / 0 rates gives the same tax as the clone's floor
  (`split_share(n, 5)` rounds the science share up, so tax = `n / 2`
  rounded down).

## Leads (unverified)

* `GCON_Corruption` (`0x6840F0`, pushed `0x42007F`/`0x4200AF`),
  `GCON_Culture` (`0x6840B4`, pushed `0x4201B7`/`0x422D6D`/`0x4231B1`/
  `0x4FB8A6`), `GCON_Commerce` (`0x684070`, pushed `0x4202EF`/
  `0x521095`) — all via `call 0x4CBE10(this=0x9E85F0)` in `PtInRect`
  city-screen mouse code (HYPOTHESIS: help-text/constant lookup, not
  the yield math itself). Siblings: `GCON_Treasury`/`GCON_Research`/
  `GCON_Moods` (`0x4200CF…`).
* Culture advisor UI: `CULTURE_ADVISOR*` + `cultureometer.pcx` pushed
  at `0x4FA5BD…` (`0x4F` thin bucket).
* `0x4BE440` is a **per-city pass, not a free-standing mood solver**
  (2026-09-29): `this` is a city (`[this+0x138]` compared against the
  global `[0x9C72D4]` and an array at `+0xDC` with count `+0xEC` is
  iterated). Sole caller `0x4BEB15`. So the mood math proper is inside that
  loop, not in the head; the "mood engine" label should be narrowed to
  "per-city mood/happiness update". **Correction (2026-10-01):** `[0x9C72D4]`
  is not a turn counter: it is RULE body `+0x10C`, which the decoded
  `conquests.biq` stores as 6 (the BIQ crate names it the culture level
  count and notes this population comparison). **Correction (2026-10-02):** the mood math is
  not in `0x4BE440` at all: the loop there only counts happy, content, unhappy and resisting
  citizens to decide the celebration. The moods are written by `0x4BCFF0` (`happiness.md`).
* **Resolved** (`happiness.md`): `0x4BE440` is only the celebration test (it reads the moods and
  never writes one), and the happiness routine `0x4BCFF0` that ends every recompute is decoded.
  Open: slider dialog writer,
  trade-route income, palace-as-capitalIdx only (no separate palace flag
  found). Per-tile gross summation is now documented (`yields.md`
  section 5).
* Open, gold and growth (2026-10-01): the writer of `[city+0x2C]`
  (per-city upkeep total) and what GOVT `+0x14` gates; `[player+0x30]`
  (gates both the unit disband and the sale); the unit pick behind vtable
  `+0x34`; `0x4ACDF0` and the flag argument of `0x4ACB50`; the first per-civ payment loop at `0x560B04`
  (the bit `0x200` of `[0xA5267C]` is Accelerated Production, resolved, see above);
  the AI half of `0x55D2A0`; `0x4BA230` and `0x4AECC0` as citizen removal and
  city destruction. The level-3 half of `0x4B1DC0` is now resolved above. Resolved on 2026-10-01 and
  removed from this list: the `[0x9C7268]` divisor (4), the RULE copy into
  the `0x9C72xx` globals (the reader stores them directly), the Despotism cap
  site and whether it covers the city square (it does).

### Clone growth-gate integration (2026-10-04)

`rust/src/economy.rs::growth_limit` models stock RULE limits, freshwater and
active level-2/3 improvement effects, with golden vectors for the two gates.
`src/citycalc.rs` uses it for natural city growth and unit population joining.
The AI's city-build inputs include local freshwater, and a blocked town seeks
an active level-2 effect while a blocked city seeks level 3. Lake classification
uses the threshold and water-connectivity rules in `worker-jobs.md` 3.2;
river generation remains absent in the playable map. The old clone overview
in section 9 is historical and does not describe these current growth gates.
