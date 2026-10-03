# Happiness: moods, civil disorder, riots, We Love the King Day

Owns: how a city turns its numbers into citizen moods (the recompute routine `0x4BCFF0` and its
helpers), what the per-turn sequencer does with those moods (civil disorder and the riot roll,
`0x4BDFF0`; the celebration test, `0x4BE440`), the nine "reason" bytes the UI reads, and where the
two mood flags `[city+0x30]` bit 0 and bit 1 are read. Reference: `rust/src/happiness.rs`. Image base
`0x400000`, static VA = runtime VA. Static analysis (radare2 disassembly, byte scans, the shipped
`conquests.biq`, the Civilopedia) plus a **differential test against the real code** (section 11).
Verified unless marked **HYPOTHESIS**.

## At a glance

* A city stores **one mood per citizen** (`[citizen+0x128]`: 0 happy, 1 content, 2 unhappy,
  3 resisting, 4 specialist). One routine, `0x4BCFF0`, **rebuilds every mood from scratch**; it is
  called from 22 call sites plus one tail jump (section 2). Nothing else writes a mood.
* The rebuild is: give every ordinary citizen a **base mood** (the first `citizens_born_content` of
  the difficulty are content, the rest unhappy), add up two signed **face accumulators**, then walk the
  citizens with the shift primitive `0x4BDBE0`. The **happy** accumulator takes luxury commerce and
  luxury resources, minus the draft, propaganda, the hurry sacrifice, war weariness and foreign
  nationals of an enemy, plus war enthusiasm. The **content** accumulator takes buildings and
  martial law (sections 3 to 5).
* **Disorder** is `happy < unhappy`, strictly, counted over the mood field. Content citizens,
  resisters and specialists do not count (`0x4BDFF0`, section 7). It is decided once per turn from the
  moods the last recompute left, **not** by `0x4BCFF0`.
* **Celebration** (`WLTKD`, flag bit 1) needs size at least RULE `wltk_min_population` (6), no unhappy
  citizen, strictly more happy than content, no resister, and a non-negative food surplus
  (`0x4BE440`, section 8).
* A city in disorder loses its food surplus (the sequencer sets food eaten to gross food), its shields
  and its commerce (`yields.md`); each turn it keeps in disorder it may riot and lose a building.
* Disorder **doubles** and celebration **halves** an intermediate score of the culture-flip test
  `0x4B28D0` (section 9).
* Checked against the real code: 30 000 random cities through `0x4BCFF0` and 30 000 through the two
  decisions give the same moods, reason bytes and outcomes as `rust/src/happiness.rs` (section 11).

## 1. Data

City object (`this` = ecx; the pool is `[0xA52E6C]`, a list node minus `0x1C` is the object):

| field | meaning | seen at |
|---|---|---|
| `+0x20` | city id | `0x4BE0E3` |
| `+0x24`, `+0x26` | x, y (words) | `0x4BE266` |
| `+0x28` | owner (byte) | everywhere |
| `+0x30` | flags: bit 0 disorder, bit 1 celebration, bit 2 cleared every turn (`0x4BE996`), bit 5 capitalization (`yields.md`) | sections 7 to 10 |
| `+0x40` | stored food | `economy.md` |
| `+0x70` | draft timer (turns), decremented each turn | `0x4BE9B2`, `0x4BD019` |
| `+0xCC..+0xD4` | nine reason bytes (section 6) | `0x4BDAD0`, `0x4BCFF0` |
| `+0xDC` | citizen list: array pointer `+0xE0`, last index `+0xEC`, entry `i` at `[array + 8i + 4]` = citizen `+0x1C` | `0x4BE46F` |
| `+0x138` | size | `0x4BE44D` |
| `+0x1C0` | propaganda unhappiness (faces), decremented each turn | `0x4BE9D1`, `0x4BCFF0` |
| `+0x1C4` | hurry-sacrifice timer, decremented each turn | `0x4BE9C0`, `0x4BD174` |
| `+0x1C8` | gross food | `0x4BEB26` |
| `+0x244` | food eaten | `0x4BEB2C`, `0x4BEB4C` |
| `+0x250` | food surplus (`+0x1C8 - +0x244`) | `0x4BEB62` |
| `+0x274` | the city's own building set | `0x4ACB50` |
| `+0x37C` | per-building counters (section 10) | `0x4BE7CA` |

Citizen: `+0x20` resister byte, `+0x128` mood, `+0x13C` job (a specialist when it is not the default
worker `[0x9C3D64]`, `0x4ABE70`), `+0x140` race.

Player records (base `0xA52E98`, stride `0x20E4`, indexed by the absolute-address idiom): `+0x1C` slot
(civ) number, `+0x20` race, `+0x2C` capital city id, `+0x30` difficulty level, `+0xA0` government,
`+0xCB0 + 4c` war counter against civ `c` (negative: winning), `+0xD30 + c` at-war byte, `+0x15DC`
word table of owned buildings, `+0x20D0` player-wide building set. `[0xA526C0]` is the mask of civs in
play (bit = slot number).

Tables and rules the code reads, with the shipped values (`conquests.biq`; 26 of the 27 other
scenarios that carry a RULE section agree, one has `draft_turn_penalty` 10):

| global | what | shipped |
|---|---|---|
| DIFF `[0x9C40C0]`, stride 124, `+0x44` | `citizens_born_content`, Chieftain to Sid | 4, 3, 2, 2, 1, 1, 1, 1 |
| GOVT `[0x9C71D8]`, stride 488, `+0x1AC` | `military_police_limit`, Anarchy to Feudalism | 0, 2, 3, 4, 0, 0, 4, 3 |
| GOVT `+0x1E4` | war weariness class (`government.md`) | |
| BLDG `[0x9C40AC]`, stride `0x110` | faces `+0xB0` happy-all, `+0xB4` happy-city, `+0xB8` unhappy-all, `+0xBC` unhappy-city; `+0xD4` required government, `+0xE0` obsolete-by tech, `+0xEC` improvement flags, `+0xF0` other characteristics (4 wonder, 8 small wonder, `0x10` continental), `+0x84` doubles-happiness-of | |
| `[0x9C7260]` RULE `+0x7C` | `chance_of_rioting` | 20 |
| `[0x9C7264]` RULE `+0x80` | `draft_turn_penalty` | 20 |
| `[0x9C7270]` RULE `+0x8C` | `citizens_per_happy_face` | 1 |
| `[0x9C72BC]` RULE `+0xD8` | `hurry_sacrifice_turn_penalty` | 20 |
| `[0x9C72D4]` RULE `+0xF0` | `wltk_min_population` | 6 |
| `[0x9C72E4]`, `[0x9C72E8]` RULE `+0x100`, `+0x104` | `town_max_size`, `city_max_size` | 6, 12 |
| `0x665868` | luxury-resource face table (12 entries) | 0, 1, 2, 4, 6, 9, 12, 16, 20, 24, 28, 32 |

RULE offsets in this file are the `biq` crate's in-memory offsets from `0x9C71E4` (so `0x9C72D4` is
`+0xF0`). `combat.md` section 9 and `yields.md` count from the file body, which is `0x1C` more
(`wltk_min_population` is body `+0x10C` there). `economy.md` once read `[0x9C72D4]` as "a turn-like
counter"; it is `wltk_min_population`.

## 2. Callers (census, with method)

Method: every `E8` (call) and `E9` (jump) with a 32-bit displacement in `.text` whose target is the
entry point, plus a scan of `.rdata` and `.data` for the 4-byte address (a vtable entry). Counts are
of instruction sites, not of distinct functions.

| entry | role | `E8` | `E9` | data refs | callers |
|---|---|---|---|---|---|
| `0x4BCFF0` | recompute | 22 | 1 | 0 | 18 enclosing functions (nearest preceding candidate start): `0x4ACF40`, `0x4AFAB0`, `0x4B0E80`, `0x4B10F0` (the tail jump), `0x4B2030`, `0x4B9F60`, `0x4BA230`, `0x4BA850`, `0x4BAAF0`, `0x4BACE0`, `0x4BB6F0`, `0x4BBC80`, `0x4BE970`, `0x4BED80`, `0x4BF080`, `0x527FC0`, `0x57E450`, `0x5BD220` |
| `0x4BDFF0` | disorder and riot | 1 | 0 | 0 | `0x4BE970` at `0x4BEB0E` |
| `0x4BE440` | celebration | 1 | 0 | 0 | `0x4BE970` at `0x4BEB15` |
| `0x4BE970` | the per-turn city sequencer | 2 | 0 | 0 | `0x46F8B0` (`0x46FD23`), `0x560050` (`0x56008F`) |
| `0x4BDBE0` | shift | 13 | 0 | 0 | `0x4BCFF0` |
| `0x4BDAD0`, `0x4BD420`, `0x4BD6D0`, `0x4BD780`, `0x4BD9B0` | the five helpers | 1 each | 0 | 0 | `0x4BCFF0` |
| `0x4BE730` | the produces-units tick (section 10) | 1 | 0 | 0 | `0x4BE970` |

The recompute is called whenever something that feeds a mood changes: tile assignment, a building
sold or lost (`0x4ACF40`), a unit moved on or off the tile (`0x5BD220`, which counts for martial law),
a capture, a turn (`0x4BE970`). `0x4B10F0` (the recompute chain of `yields.md`) ends with a tail jump
into it.

## 3. The recompute `0x4BCFF0`, step by step

`0x4BCFF0` is a `__thiscall` with no stack argument. Two locals hold the accumulators: `[esp+0x10]` the
**happy** accumulator `a`, `[esp+0x14]` the **content** accumulator `b`. A positive number is faces
that lift citizens, a negative one faces that drop them. In order:

| # | step | address | effect |
|---|---|---|---|
| 1 | base mood | `0x4BDAD0` | zero the nine reason bytes; walk the citizens in list order; each one that is neither a resister nor a specialist becomes content while fewer than `citizens_born_content` have, else unhappy. The unhappy ones go to `+0xCD`. Resisters (3) and specialists (4) keep their mood |
| 2 | buildings | `0x4BD420` | `b +=` the net content faces of the buildings (3.1) |
| 3 | draft | `0x4BD019..0x4BD041` | `t = [city+0x70]`; if `t > 0`: `a -= (t - 1) / penalty + 1`, byte `+0xD1` gains the same |
| 4 | martial law | `0x4BD0A0..0x4BD0B6` | only when **no citizen of the city is a resister**: `b += min(units, limit)`, `units = 0x5A6060(x, y, 4, -1, 0, -1)` (mode 4 counts units on the city tile; its filters `0x5BE6E0` and `0x5BE820` are unread, so "land units of any owner" is **HYPOTHESIS**), `limit` = GOVT `+0x1AC` of the owner (a negative limit gives 0) |
| 5 | luxury commerce | `0x4BD0D9` | `a += (0x4AC9E0(city,0,0) + 0x4ACAE0(city,0,0)) / [0x9C7270]`: the specialists' luxury plus the luxury stream of `yields.md` section 5 |
| 6 | luxury resources and propaganda | `0x4BD0E3..0x4BD14C` | `a += face(n) - [city+0x1C0]`; `n` = usable luxury resources, at most 11 (`0x4BD116`), `face(n) = n` without a building of improvement flag `0x400` (the Marketplace), else the table `0x665868`. `+0xD0` gains `[city+0x1C0]` |
| 7 | hurry sacrifice | `0x4BD174..0x4BD195` | as the draft with `[city+0x1C4]` and `[0x9C72BC]`; `a -=`, byte `+0xD2` |
| 8 | war enthusiasm | `0x4BD6D0` | for each civ in play and at war with the owner whose counter `Player+0xCB0+4c` is **negative**: `a += size / 4`; the sum is capped at `size` |
| 9 | war weariness | `0x4BD780` (called at `0x4BD1B2`) | `a -= w`, byte `+0xCE` gains `w`; `w` is `government::city_weariness_unhappy` |
| 10 | foreign nationals | `0x4BD9B0` | `f` = citizens that are not resisters and whose race maps (through `0x539D60`) to a civ in play at war with the owner (specialists count); `a -= f`, byte `+0xCF` gains `f` |
| 11 | distribute | `0x4BD1C8..0x4BD35B` | section 5 |
| 12 | percentages | `0x4BD35B..0x4BD410` | section 6 |

### 3.1 The building pass `0x4BD420`

Per BLDG row in table order, for the city's owner:

1. A **wonder** (`+0xF0 & 4`) whose required government (`+0xD4`) is not `-1` and not the owner's
   government, or that the owner has made obsolete (`+0xE0` names a tech the owner knows,
   `0x561440`), is skipped.
2. Let `k = 2` if a live wonder of the owner doubles this building (`0x55A7E0(owner, i)`, from BLDG
   `+0x84`), else `1`. Let `n` = `Player+0x15DC[i]` (how many the owner has).
3. If the city has the building (`0x4ACB50(city, i, 1)`) and it is not obsolete: `n -= 1`; the
   in-city faces count once: `b += happy_city * k` and `b -= unhappy_city * k`; the latter also goes to
   byte `+0xD3`.
4. With `n != 0` (and `n` replaced by the owner's other cities on this continent that have it,
   `0x4BDDE0`, when `+0xF0 & 0x10` is set), the all-cities faces count `n` times:
   `b += happy_all * n * k`, `b -= unhappy_all * n * k`, the latter into byte `+0xD4`.

So the "all cities" faces of a building are paid by every city of the owner except the one that
holds the building (`owned - 1`), which is how a wonder gives an empire-wide effect while the city
that has it takes its own in-city value.

## 4. The shift primitive `0x4BDBE0(count, from, to)`

Moves up to `count` citizens from mood `from` to mood `to` and returns how many moved. It does
nothing when either mood is 3 or 4. Only ordinary citizens (no resister, no specialist) whose mood is
`from` move. Two passes over the list:

* Pass 1 runs while fewer than `[city+0xCF]` (the foreign-nationals byte of step 10) have moved. It
  takes first a **foreign-race** citizen when making someone unhappy (`to == 2`) and first an
  **own-race** citizen when making someone less unhappy (`from == 2`): the unrest falls on foreigners
  and the relief on natives.
* Pass 2 takes whoever is left, in list order.

## 5. Distributing the faces

The two accumulators become mood changes in one of four regions, tested in this order (`sh` is
`0x4BDBE0`; `up(n)` and `down(n)` below):

| region | what happens |
|---|---|
| `a >= 0, b <= 0` | the smaller of `a` and `-b` cancels the other (`a > -b`: `a += b, b = 0`; else `a = 0, b += a`); `up(a)`; then `sh(-b, happy, content)` |
| `a <= 0, b >= 0` (not both 0) | let `m = -a`; if `b > m`: `a = 0, b -= m`; else `a = m - b, b = 0`; `down(a)`; then `sh(b, unhappy, content)` |
| `a > 0, b > 0` | `sh(b, unhappy, content)`, then `up(a)` |
| `a < 0, b < 0` | `sh(-b, happy, content)`, then `down(-a)` |

`up(n)`: `m = sh(n, content, happy)`; `m2 = sh((n - m) / 2, unhappy, happy)` (two faces lift an unhappy
citizen all the way); if `n > m + 2*m2`, `sh(1, unhappy, content)`. `down(n)` is the mirror image
(`content` to `unhappy`, then `happy` to `unhappy` at half the count, then `sh(1, happy, content)`).

This matches the Civilopedia: "Each content face makes one unhappy citizen content. Each happy face
makes one content citizen happy. If there aren't enough people of the appropriate sort in a city, the
effects of any extra faces are lost". The Civilopedia says nothing of the rule that two leftover
happy faces lift an unhappy citizen straight to happy (`sh((n - m) / 2, unhappy, happy)`); that
comes from the code alone.

## 6. The reason bytes `[city+0xCC..0xD4]`

| byte | index | after step 1 to 10 | source |
|---|---|---|---|
| `+0xCC` | 0 | always 0 (**no writer found**, `0x4BDAD0` zeroes it) | |
| `+0xCD` | 1 | citizens born unhappy | `0x4BDAD0` |
| `+0xCE` | 2 | war weariness | `0x4BD780` |
| `+0xCF` | 3 | foreign nationals of an enemy | `0x4BD9B0` |
| `+0xD0` | 4 | propaganda | `[city+0x1C0]` |
| `+0xD1` | 5 | the draft | `[city+0x70]` |
| `+0xD2` | 6 | the hurry sacrifice | `[city+0x1C4]` |
| `+0xD3` | 7 | unhappy faces of buildings in the city | BLDG `+0xBC` |
| `+0xD4` | 8 | unhappy faces of buildings in all cities | BLDG `+0xB8` |

Step 12 then: if **no citizen is unhappy**, all nine are zeroed; otherwise, with `sum` their total
(when it is not 0), each byte becomes `trunc(b / sum * 100)`. The division and the multiply are x87
(`fild sum; fild b; fdiv; fmul 100.0f; _ftol`), so each is rounded to the FPU mantissa. With the
default 64-bit mantissa this differs from the integer `b * 100 / sum` at eight inputs with
`b <= 255, sum <= 2295` (53 or 59 percent, and the multiples of 100: `0.53` and `0.59` round just
below the exact value and come out one lower); at 53 bits twenty inputs would differ.
`happiness::x87_percent` does the rounding exactly in integers. **HYPOTHESIS:** the running game uses
the default 64-bit control word (the CRT default). The bytes feed the UI string
`CITIZEN_UNHAPPY_REASONS`.

## 7. Civil disorder `0x4BDFF0`

Called from the sequencer (`0x4BEB0E`). Counts the citizens whose mood is 0 (happy) and 2 (unhappy)
over the whole list; `cond = happy < unhappy` (`setl bl` at `0x4BE06B`; resisters, specialists and
content citizens are not counted, which is the Civilopedia's "content citizens and specialists are
ignored"). When `cond` is true, the multiplayer gate `0x47B530` is clear and the byte `[0xC9C45C]`
is set, it also calls `0x5B1830(0xC9C440, city)` (unread). Then four ways on `(flag bit 0, cond)`:

| flag | `cond` | what | address |
|---|---|---|---|
| clear | false | nothing | `0x4BE350` |
| clear | true | **disorder begins**: drop the `[city+0xA4]` queue entry, `0x4C0AC0(city, 1, 1, 5)`, set bit 0; for the local player an effect, the random riot sound and the message `CIVIL_DISORDER`; UI refresh | `0x4BE356..0x4BE41C` |
| set | true | **disorder continues**: the riot roll below | `0x4BE0B0` |
| set | false | **disorder ends**: clear bit 0, `CIVIL_DISORDER_OVER` for the local player, UI refresh | `0x4BE2CD` |

**The riot roll** (every turn the city stays in disorder), with `rng(n)` = `0x60BAB0` on the gameplay
`Random` at `0xA526B4`, `n` masked to 16 bits:

1. `rng(100) >= chance_of_rioting` (20): nothing.
2. The owner's **capital** (`city id == Player+0x2C`) is spared.
3. A city of at most `min(town_max_size, city_max_size)` citizens (6 in the shipped rules) needs a
   second roll `rng(100) < 2 * chance_of_rioting` (`0x4BE101`, `0x4BE109`: either limit exceeded
   skips it).
4. Up to `size + 21` draws of `row = rng(building_count)` (`[0x9C3D80]`); the first row for which the
   city has the building (`0x4ACB50(city, row, 0)`) and which is **not** a wonder (`+0xF0 & 4`), not a
   small wonder (`& 8`) and has none of improvement flags bit 0, 11, 12 (`+0xEC & 0x1801`: the
   palace and the two size gates) is destroyed by `0x4ACF40(city, row, 0, 0)` (the routine that
   also recomputes the moods). For the local player: two `0x61C5A0` effects, sound `0x535D20(0)` and
   the message `CIVIL_DISORDER_INTENSIFIES`.
5. No building found (or a roll failed, or the city is the capital): for the local player one
   `0x61C5A0` effect, the riot sound `0x535D20(1 + rand() % 3)` and the message `CIVIL_DISORDER`
   (`0x4BE1A8..0x4BE2B2`).

The sound choice uses the **MSVC `rand`** (`0x64A20E`), not the gameplay `Random`
(`ai.md`); the riot rolls use the gameplay instance. The "disorder begins" path draws the same
random sound (`0x4BE3B2`). The Civilopedia's "prolonged periods of civil
disorder can lead to destruction of existing city improvements ... defection ... an overthrow of
your government": the building destruction is this routine; the culture defection is the flip test
(section 9); where a long disorder overthrows the government is **not located** (section 12).

## 8. Celebration `0x4BE440`

Called from the sequencer (`0x4BEB15`). It never changes a mood. `qualifies` is true when all of:

1. `[city+0x138] >= [0x9C72D4]` (`wltk_min_population`; a signed compare, `0x4BE453`);
2. no citizen has mood 2 (unhappy) (`0x4BE490`);
3. the number with mood 0 (happy) is strictly greater than the number with mood 1 (content)
   (`0x4BE508`);
4. no citizen has the resister byte `+0x20` set (`0x4BE547`);
5. `[city+0x250] >= 0`, the food surplus (`0x4BE553`). The sequencer refreshes that field **after**
   this test, so it is the value of the previous pass.

Then four ways on `(flag bit 1, qualifies)`: clear and false: nothing; clear and true: **WLTKD
begins** (drop the `[city+0xA4]` queue entry; `0x4C0AC0(city, 2, 1, 5)` when the local player's slot
bit is set in the tile's `[cell+0x58]` mask, meaning of the mask unread; set bit 1; for the local
player, when `0x47B530` is clear: the message `WELOVEKING` and sound `0x537700(0x10)`); set and
true: nothing (`0x4BE564`); set and false: **WLTKD ends** (clear bit 1, `WELOVEKINGOVER` under the
same conditions). The Civilopedia's celebration rules (population, happy over content, no unhappy)
match; the resister and food conditions are not in its text.

## 9. Order inside the city turn `0x4BE970`

(Callers `0x46F8B0`, `0x560050`.) In the order the code runs:

1. `[0x9C34E4] = 0`; the city's vtable `+0x38(0)` (unread); `0x4B28D0` (the culture-flip test,
   below; true ends the sequencer, the city has changed hands);
2. clear flag bit 2; `[+0x36C] = 0`, `[+0x6C] = 0`; decrement the timers `+0x70` (draft), `+0x1C4`
   (hurry sacrifice) and `+0x1C0` (propaganda), each only while positive;
3. `0x4BE730`, the produces-units tick (section 10);
4. the small-wonder notice loop (unread: BLDG rows with `+0xF0 & 8`, a per-owner byte table at
   `0xA54484`, the message `SUMMARY_NEW_SMALL_WONDER` for the local player);
5. `0x4AC140` for every citizen (the nationality drift, [`city-turn.md`](city-turn.md) 8.1);
6. `0x4B2E10` (resistance quelling, `city-turn.md` 8.2), `0x4B2F80`, `0x4B45A0` (disease), `0x4B4970`
   (the last two unread; the last reads flag bit 0);
7. **`0x4BDFF0` disorder, then `0x4BE440` celebration**, on the moods the previous recompute left;
8. food: if flag bit 0 (disorder) is set, `[+0x244] = [+0x1C8]` (it eats all it makes), else
   `[+0x244] = (size - resisters) * [0x9C72B4]` (`0x4BB2A0(-1)` counts resisters, `yields.md`
   section 4.1); `[+0x250] = [+0x1C8] - [+0x244]`;
9. `0x4B05D0` shields, `0x4B07C0` commerce, **`0x4BCFF0` recompute**, `0x4B2030` growth;
10. a city of size 0 ends here; otherwise `0x4B9950` production: specified in
    [`city-turn.md`](city-turn.md) sections 5 to 7 (shield box, completion, queue). The full 15-step
    table of the sequencer is `city-turn.md` section 2.

So a city's disorder status lags: the moods are those of the last recompute, which the previous turn's
step 9 or a later player action produced; the new tiles and the new totals are applied in the same
turn but their mood effect is decided one turn later.

**The flip test `0x4B28D0`** reads both flags: with a score in `edx`, a city in disorder doubles it
(`add edx, edx`, `0x4B2AFF`) and a celebrating city halves it, truncating toward zero
(`sar eax, 1`, `0x4B2B6A`), before the score is compared (the rest of the flip test is in
`economy.md`). This is what `economy.md` called "WLTKD-halves-distance".

## 10. Flag readers

Bit 0 (disorder): the scan "load `[reg+0x30]`, then `and`/`test` with `1`, `0xFE` or `0xFFFFFFFE`
within 8 instructions", over the linear disassembly, finds 24 functions: `0x42A9B0`, `0x4ACF40`,
`0x4B0540`, `0x4B0E80`, `0x4B10F0`, `0x4B1190`, `0x4B4970`, `0x4B4DD0`, `0x4B5290`, `0x4B9F60`,
`0x4BA850`, `0x4BAAF0`, `0x4BACE0`, `0x4BB6F0`, `0x4BBC80`, `0x4BE970`, `0x4BED80`, `0x4BF080`,
`0x4DD240`, `0x4E2400`, `0x4E8850`, `0x51CC50`, `0x527FC0`, `0x5FACC0`. It misses a test through a
register that holds the constant (`0x4BDFF0` itself, `0x4B28D0`), and not every hit is a city (the
scan cannot tell the object). Known roles: `0x4B0540` eats all gross food and `0x4B1190` (kind 1)
loses all shields (`yields.md`); `0x4B5290` refuses a hurry (`HURRY_CIVIL_DISORDER`, `ai.md`);
`0x4BE970` sets food eaten to gross food; `0x4B28D0` doubles the flip score; `0x527FC0` adds 10 to a
score (below).

Bit 1 (celebration): the same scan with `2` and `0xFD` finds `0x44A630` (AI, unread), `0x4B1190`
(one read at `0x4B1329`: when its byte argument `[esp+0x38]` is set and the city celebrates, the
accumulator `esi` gains a quarter of a table product; the quantity is unread), `0x4B28D0` (halving),
`0x4BE440` (the writer), `0x527FC0` (a score: `-5` per unit on the city tile, `-40` for the capital,
`-20` for a city with an improvement of flag `0x80`, `-10` celebrating, `+10` in disorder, plus a
GOVT table value; **resolved:** it is the per-citizen success percentage of the Initiate
Propaganda executor, which also writes `[city+0x1C0]`: `espionage.md` 9.7) and `0x57CDA0` (a bounded counter, unread). The scan also names `0x49A920`, which is
a false hit: the matched load is a store to an unrelated object.

### The produces-units tick `0x4BE730` (side result)

For every BLDG row the city has (`0x4ACB50(city, i, 1)`), not obsolete (`+0xE0` a tech the owner
lacks, or `-1`), with improvement flag `0x40000000` (produces units): the counter `[city+0x37C + 4i]`
counts up to `unit_frequency - 1` (`+0x10C`); when it gets there and both required resources
(`+0xE4`, `+0xE8`; `-1` is no requirement) are usable (`0x4ADE30`), it is reset to 0 and the unit
`+0x108` (`unit_produced`) is created at the city (`0x5694D0`); its `[unit+0x44]` is set to
`clamp(0x4B0160(city, PRTO row), 0, 3)` (**HYPOTHESIS:** an experience level) and the local player
gets the message. If a resource is missing the counter stays full and the check repeats next turn.

## 11. Verification

**Static.** The family was decoded from the raw disassembly. The numbers it yields agree with:
the Civilopedia (the face table for luxury resources with a Marketplace, what a face does, "more
unhappy than happy", "content citizens and specialists are ignored"; the Cathedral's three and the
Colosseum's two content faces are BLDG `+0xB4` in the shipped file); the shipped BLDG, GOVT, DIFF and RULE values read through the `biq`
crate; and an earlier, independent decode of `0x4BD780` in `government.rs`, which `happiness.rs`
calls.

**Differential, against the real code.** The test harness is scratch (it lives in `/tmp`, as the
method requires) and is described here so it can be rebuilt:

* An x86 emulator (Unicorn 2.1.4) maps the PE sections at `0x400000` and builds a synthetic city,
  citizen list, Player record (slot, race, level, government, war counters, at-war bytes, owned
  building words), the BLDG, GOVT, DIFF, GOOD and RACE tables and the RULE globals in emulated
  memory, then runs the real `0x4BCFF0` (`ecx` = city) to its return.
* Everything reachable from `0x4BCFF0` is the real code except **12 leaf predicates**, which are
  stubbed with the answer of the case: `0x4ACB50` (has building), `0x561440` (tech known), `0x55A7E0`
  (doubles), `0x4BDDE0` (continent count), `0x5A6060` (unit count), `0x4AC9E0` and `0x4ACAE0` (luxury),
  `0x4B1F90` (called with `0x400` only), `0x4ADE30` (resource usable), `0x5E3720` (luxury resource),
  `0x539D60` (race to civ), `0x55A8D0` (suffrage wonders).
* Compared after the call: every citizen's mood and the nine reason bytes. Cases: 3 seeds of 10 000
  cities with 1 to 14 citizens in all five moods, resisters, specialists, foreign races at war or not,
  all eight difficulty levels, draft, hurry and propaganda values on both sides of every threshold,
  0 to 6 buildings with random faces, ownership, obsolescence, doubling and continental scope, wars
  with counters on either side of 0, 30, 60 and 120. Result: **30 000 of 30 000 identical**.
* Sensitivity: perturbing one Rust input at a time (born content, draft timer, draft penalty, units,
  police limit, luxury points, divisor, luxury resources, propaganda, hurry penalty, police buildings,
  one citizen's foreign flag, one war counter) is detected in 35 to 1083 of 1500 cases each.
* The two decisions: the real `0x4BDFF0` and `0x4BE440` run until the first of their fixed outcome
  addresses (`0x4BE0B0`, `0x4BE2CD`, `0x4BE356`, `0x4BE430`; `0x4BE56A`, `0x4BE6AB`, `0x4BE720`), so no
  message, sound, effect or riot roll executes; the outcome is compared with
  `disorder_step(.., is_disorder(..))` and `celebration_step(.., celebrates(..))`. Cases vary the moods,
  the resister bytes, the size independently of the list length, `wltk_min_population`, the food
  surplus and both flags. Result: **30 000 of 30 000 identical**, with at least 500 cases per 10 000
  in each of the eight outcomes; perturbing `wltk_min_population`, the size, the food surplus or
  either flag is detected.

**What this does not cover.**

* The 12 stubs: what they return for a real city is read from their disassembly and from
  `yields.md`, not exercised here. The case generator also fixes the *meaning* of the inputs
  (`police_buildings` and `luxury_goods` are computed by the harness from my reading of
  `0x4B1F90` and `0x4ADE30`).
* The x87 control word: the emulator starts with the FPU reset value (64-bit mantissa), so the
  percentage path is confirmed under that assumption only (section 6).
* In the `0x4BCFF0` cases `[city+0x138]` equals the list length; only the decision tests vary it.
* The riot roll and everything after it (destruction, effects, messages) is verified by reading, and by
  the unit tests of `happiness::riot`, not executed.
* The 43 unit tests in `happiness.rs` prove only consistency with my reading; the differential test is
  what ties it to the binary.

## 12. Open

* `[city+0xCC]` (reason byte 0): zeroed, no writer found.
* `[city+0xA4]` (when non-zero it is cleared on disorder and on celebration, with a queue entry at
  `+0x3BC` dropped through `0x406200`), `0x4C0AC0(city, a, b, 5)` (called with `(1, 1)` on disorder
  and `(2, 1)` on celebration), `0x4DD240`, `0x5B1830`.
* ~~The magnitude written to `[city+0x1C0]` by `0x527FC0` (propaganda), and what `0x527FC0`'s score
  is a score of.~~ Resolved in `espionage.md` 9.7 (`0x527FC0` is the Initiate Propaganda executor).
* `0x4BDF10`; `0x5BE6E0` and `0x5BE820`, the two predicates `0x5A6060` mode 4 uses to decide which
  units count for martial law (`government.md` section 10); `0x44A630`, the AI evaluator that reads
  the celebration flag; `0x57CDA0`.
* The `rand() % 3` riot sound selection `0x535D20` and the three `0x61C5A0` effect calls.
* Where a long disorder overthrows the government (the Civilopedia says it does).
* The origin of the luxury points: this module takes `0x4AC9E0 + 0x4ACAE0` as input
  (`yields.md` section 5); the Civilopedia's slider text was not tested against a live game.
* Whether the live x87 control word is the 64-bit default (section 6).
* The clone has no happiness model yet (`PLAN.md`); `happiness.rs` is the specification to port.

## 13. Corrections to other documents

* `ai.md` "Disorder turnover": "the happiness stage `0x4BCFF0` ... the flag it sets" was wrong.
  `0x4BCFF0` sets no flag; `0x4BDFF0` sets bit 0 each turn from the moods the recompute left.
* `economy.md` ("`0x4BE440` is a per-city pass, not a free-standing mood solver ... the mood math
  proper is inside that loop"): `0x4BE440` is only the celebration test; the mood math is `0x4BCFF0`.
  "Mood flags ... bit1 WLTKD-halves-distance: child-reported" is verified and narrowed (section 9).
* `government.md` 5.4.1: `[esp+0x10]` of `0x4BCFF0` is the **happy-face accumulator** (it takes
  weariness as faces, not as "citizens it is about to shift"), and the shift is `0x4BDBE0` through the
  distribution of section 5; `city +0xCE` is reason byte 2.
* `yields.md` sections 7 and 8: the happiness family is now decoded; the table `0x665868` is the
  happy-face count for luxury resources with a Marketplace-class building (section 3, step 6), and
  the Civilopedia lists the same numbers.
