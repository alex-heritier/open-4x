# Diplomacy: contact, war and peace, attitude, deals

Owns: the per-pair relation state of the `Player` object, first contact and embassies, the war and peace
transitions as far as the diplomatic state goes, the AI's attitude toward another civ and its decision to
declare war, the deal item lists and their execution, the deal scorer (the AI's answer to a proposal), the
timed packages, the tech-trade counter and the diplomacy and espionage dialogs.

Not owned here: the war-weariness arithmetic, the call to arms in the turn update, the pair record's
incident accumulators and the kicker (`government.md` 5), the violation scan at the head of a declaration
(`combat.md` 14.3), the price of an advance (`research-ai.md`), the first-contact scan (`turn.md` 3.3) and
the AI diplomacy rotation (`turn.md` 4.1). Reference implementation: `rust/src/diplomacy.rs`
(`Relations`, `Clause`, `Env`, `weigh`); the playable game wraps it in `src/diplomacy.rs` (rules) and
`src/advisors.rs` (screens).

Notation as in `research.md` section 0. `P` is a `Player` record (array `0xA52E98`, stride `0x20E4`,
slot 0 the barbarians), `q` another civ's slot, `rec(P, q)` the 19-dword pair record at
`P + 0x1B0 + 0x4C*q` (offsets below are from `+0x1B0`, as `government.md` 5.1 numbers them). Evidence tags:
**A** decoded from the disassembly (addresses given); **E** executed and compared; **H** hypothesis;
**C** a choice of this repository's clone, not the binary.

The decoding is **A** throughout and nothing here is **E**: the routines need a whole game around them
(rules, a player array, the pool of cities and units), and no run of the exact bytes was made. The unit
tests of `rust/src/diplomacy.rs` check the arithmetic of the reading.

## 1. The per-pair state of a `Player` (A)

Every table is indexed by the other civ's slot `q` (a dword or byte array of 32 entries).

| offset | size | name | meaning | writers |
|---|---|---|---|---|
| `+0xBB0 + 4q` | dword | war memory | the AI's war and peace timer against `q`; non-zero blocks the AI's own peace offer (7.1) | `0x501F20` (AI sides), `0x5025B0` |
| `+0xCB0 + 4q` | dword | war counter | war weariness carried against `q`; negative: winning (`government.md` 5) | `0x501F20`, `0x500AD0` |
| `+0xD30 + q` | byte | **at war** | the only war flag; the table `0xA53BC8` of `combat.md` 14.3 | set by `0x501F20`; cleared by `0x5025B0` |
| `+0xD50 + q` | byte | embassy | `communications` channel; needed to talk | `0x56B7D0` |
| `+0xB30 + 4q` | dword | greeting timer | `0x20` on an AI side that has just met a human (2) | `0x501CD0` |
| `+0xD70 + q` | byte | espionage channel | opens talks regardless of embassy and war (`0x501910`); zeroed by `0x502CC0` | not decoded |
| `+0xDB0 + 4q` | dword | tension | border pressure from `q`'s units; `>= 0x200` is the war track | `0x446C1B`; zeroed by `0x501F20` |
| `+0xEB0 + 4q` | dword | relation word | bits in section 1.1 | `0x501CD0`, `0x446C18`, `0x501F20` |
| `+0xF30 + 4q` | dword | treaty word | `1` mutual protection, `2` right of passage, `4` alliance | `0x502D90` |
| `+0xFB0 + 4q` | dword | allies against | bit `b`: civ `b` is allied with this player against `q` | `0x502D90` |
| `+0x1030 + 4q` | dword | embargo | bit `b`: civ `b` embargoes `q` together with this player | `0x502D90` |
| `+0x1B0 + 0x4C*q` | 19 dwords | pair record | `government.md` 5.1 | many |

The tables are reachable through the player array: `Player +0xD30` is `0xA53BC8`, `+0xBB0` is `0xA53A48`,
`+0xCB0` is `0xA53B48`, `+0xEB0` is `0xA53D48`, and the per-turn gold table `Player +0xE30 + 4q` is
`0xA53CC8` (`turn.md` 3.1). The pair records of all players form one matrix: `rec +0` of `(row, col)` is
at `0xA53048 + 4*(row*2105 + col*19)` (rows of 2105 dwords = the 8420-byte player stride, 19 dwords per
record), so `0xA5304C` is `rec +0x04` and `0xA53070` is `rec +0x28`.

**There is no separate "contact" byte.** Contact is bit 0 of the relation word. `victory.md` labels
`+0xD30` "contact / at-war byte"; the writers say it is the war byte only: a scan of every store with that
displacement finds `0x501F20` (sets) and `0x5025B0` (clears).

### 1.1 The relation word `+0xEB0 + 4q`

| bit | meaning | writer |
|---|---|---|
| `0x01` | **contact**, never cleared | `0x501CD0` (both directions) |
| `0x02` | "just met", set on the word of every **AI** side by `establishContact`, never on a human's; what reads it is not decoded | `0x501CD0` (`0x501E1D`, `0x501E9C`) |
| `0x04` | not decoded | |
| `0x08` | border warning: the other civ's units have stood in our borders | `0x446C18` |
| `0x10` | border war track: the pressure reached `0x200` | `0x446C3B` |
| `0x20` | not decoded | |
| `0x40` | cleared at every turn update for each civ met; the byte `+0xD90[q]` is cleared with probability 1/3 there (`government.md` 7 step 4) | `0x560D80` |

A declaration of war clears `0x3E` (bits 1 to 5) of both words (`and 0xFFFFFFC1`, `0x502111`).

## 2. Contact and embassies (A)

`establishContact(P; q, flag)` `0x501CD0`, `ret 8`:

1. Returns at once for `q <= 0`, `q == P.slot`, or when bit 0 of `P.+0xEB0 + 4q` is already set.
2. In a networked game (`0x47B530`) with `flag == 0` and a human on either side it sends message `0x475670`
   (to each party) instead of acting; the steps below then happen when the message arrives. In a single
   process it goes on.
3. Sets bit 0 of `P.+0xEB0 + 4q` and bit 0 of `Player[q].+0xEB0 + 4*P.slot` (`0x501D90`, `0x501DA6`).
4. For each side that is **not** a human (`[0xA526BC]`), sets bit 1 of its relation word (`0x501E1D`,
   `0x501E9C`). When one side is human and the other an AI, the AI's greeting timer about the human,
   `AI.+0xB30[human]`, is set to `0x20` (`0x501ECF`, `0x501F0D`). The dialog itself (`0x5049D0`,
   section 10.2) is not started here.

Who calls it: the first-contact scan `0x55B540`, run once per turn in `0x5604B0` for every civ not yet met
(`turn.md` 3.3: a foreign unit within sight, or a unit on or next to foreign territory), and `declareWar`
(section 3), the deal executor (contact clause, 8.2) and the espionage missions. City tiles alone create
no contact.

**Contact is not an embassy and is not a treaty.** Talking needs the embassy byte on the asker's side:
`canTalk(P; q)` `0x501910` is `(P.+0xD50[q] != 0 && P.+0xD30[q] == 0) || P.+0xD70[q] != 0`. The embassy
byte is stored by `0x56B7D0(P; q)` (which also refreshes the city pool); its callers (the first-contact
conversation, the espionage mission that establishes an embassy, the deal screens) are not enumerated here.
Contact never makes a war: nothing writes the at-war byte but `declareWar`.

## 3. Declaring war `declareWar(P; q, reason)` `0x501F20` (A)

The head (refusals and the violation scan) is in `combat.md` 14.3, the tail in `government.md` 5.5. The
whole in order, with what this file adds:

1. Refuses `q <= 0`, `q == P.slot`, the pair already at war.
2. `establishContact(P; q)` (section 2).
3. The unit scan (`0x501F5A..0x502077`): if `P` has a right of passage with `q` (`treaty & 2`), or
   `Player[q].+0xDB0[P] >= 0x200`, and one of `P`'s units visible to `q` stands on a tile `q` owns, one
   treaty violation is counted: `rec(q, P) +0x0C += 1`.
4. `rec(q, P) +0x00 += 1` (the victim counts the declaration, `0x502079`); both at-war bytes set
   (`0x5020AC`, `0x5020B4`).
5. Bits `0x3E` of both relation words cleared, both `+0xDB0` zeroed (`0x502111`, `0x502153`, `0x502160`).
6. **Plain, provoked or called**: if `reason == 0` **and** `rec(P, q) +0x10` (hostile acts) is 0
   (`0x502167`, `0x50216F`): `P` asks `attitudeClass(q, 0)` (section 5.2) and adds `0x3C` to
   `P.+0xCB0[q]` for class 0, `0x1E` for class 1, nothing otherwise (`0x502188`, `0x502199`), **and** the
   victim's counter against `P` loses 30 (`0x5021BD`, `add -30`). Any other declaration (a reason word, or
   after a hostile act) does neither. (An earlier reading of this file put the 30 outside the gate; the
   jump at `0x502167`, which skips to `0x5021C2`, says otherwise.)
7. **War memory** (`0x5021D8..0x502245`): an AI victim stores `Player[q].+0xBB0[P] = 8 * ((a + b + 1) / 2)`
   (`a`, `b` the two `rec +0x00` declaration counters in either direction, the new one included, C
   division). An AI declarer stores `P.+0xBB0[q] = Player.vtable[+0x98](q)`, and `0x539CE0` is the same
   formula with the same two counters: the memory is symmetric. A human side keeps no memory.
8. `0x500830(P; q, 1)` settles the packages in force between the two (8.6): each is cancelled and
   `rec(q, P) +0x04` is incremented for it; the treaty words of both sides are cleared.
9. The alliance cascade (`0x5024DB` and after, messages `SUMMARY_DECLARE_WAR`, `MILITARYALLIANCE*`,
   `MUTUALPROTECTION*`): for every other civ in play `b`:
   * `b` is in `allies_vs(P, q)` (allied with `P` against `q`): `b` declares war on `q` with reason
     `0x22 + P.slot`;
   * else `b` is in `allies_vs(q, P)`: `b` declares war on `P`, reason `0x22 + q`;
   * else `b` holds a mutual protection pact with `q` (`Player[q].+0xF30[b] & 1`): `b` declares war on
     `P`, reason `2 + q`.

   Each of those is a full declaration (so a chain of pacts runs to its end; the "already at war" refusal
   stops a loop). Every war started is returned by the reference as a `WarCall`. The reasons `2 + c` are the
   same encoding the call to arms of `government.md` 5.2 uses.

## 4. Peace `makePeace(P; q)` `0x5025B0`, `ret 4` (A)

Caller `0x502D90` (the peace clause, 8.2). Refuses `q <= 0`, `q == P.slot` and a pair not at war.

1. In both pair records, `rec +0x34` is halved and `rec +0x38` zeroed (`0x5025F9..0x502645`).
2. Both at-war bytes cleared.
3. A mobilized side is demobilized and recomputed (`government.md` 6.4).
4. An AI side stores 8 in its memory about the other (`0x5026A9`..): `Player[q].+0xBB0[P] = 8` and, for an AI
   `P`, `P.+0xBB0[q] = Player.vtable[+0x9C]()` = `0x539D30`, which returns the constant `8`.
5. The `MAKEPEACE` message (`0x5027B4`).
6. The tail (`0x5027C5..0x502CB3`): for every other civ `b` the bit `b` is removed from
   `allies_vs(P, q)` and `allies_vs(q, P)` and the bits for `P` and `q` from `b`'s own words; the alliance
   bit (`4`) of the treaty words between the two is cleared. The unit pass (units standing in the other's
   territory are moved) is not decoded.

Nothing else changes: contact and embassies stay, the pair records keep every counter that is not
`+0x34`/`+0x38`, and a war memory of 8 is left standing (7.1).

## 5. Attitude (A)

### 5.1 The score `0x440100(P; q, flag)`, Player vtable `+0x84`, `ret 8`

Larger is more hostile. The start is `clamp(aggression(P), -2, 2)` (`RACE.aggression` through the race
table `[0x9C71D0]` vtable `+0x20`, `0x53A0B0`), then, in this order (`min(a, b)` is the signed minimum):

| term | contribution |
|---|---|
| `rec(P,q) +0x00` declarations by `q` | `+4` each |
| `+0x04` deals cancelled | `+min(4n, 8)` |
| `+0x08` (not decoded) | `+min(8n, 16)` |
| `+0x0C` treaty violations | `+min(2n, 4)` |
| `+0x10` hostile acts | `+4` each |
| `+0x18`, `+0x1C`, `+0x20` (diplomacy-log counters) | `+2*n18 - n1C + n20` |
| `+0x14` attacks by `q` | `+min(n, 10)` |
| `+0x24` goodwill | `-min(n / 10, 10)` (the `0x66666667` divide) |
| `+0x30` visible attacks, `+0x2C` hidden attacks | `+16*(visible + 2*hidden)` |
| `+0x28` tech trades | `-min(n, 1)` |
| `+0x34`, `+0x38` incident accumulators | `+min(n34, 10) + min(n38, 10)` |
| `+0x40` razed cities | `+16` each |
| `+0x44`, `+0x48` | `+1` each (`+0x44` also counts the nationals lost, `government.md` 5.1) |
| `+0x3C` | `-min(n, 10)` |
| units of `P` with `q`'s nationality | `+n` |
| anyone allied with `q` against `P` (`allies_vs(q, P) != 0`) | `+10` |
| anyone embargoing `P` with `q` (`embargo(q, P) != 0`) | `+2` at war, `+10` otherwise |
| at war, or the relation bit `0x10` | `+5`; else the bit `0x08`: `+1` |
| governments differ | `+5` when `q`'s is `P`'s shunned one (`RACE.shunned_government`), else `+1` |
| governments alike and `q`'s is `P`'s favorite | `-5` |
| `score(q) < score(P) / 2` | `+1`; `score(q) > 2 * score(P)`: `-1` (`+0x183C`) |
| embassy `P -> q` | `+1` at war, `-2` otherwise |
| same culture group | `-1` |
| cities on one continent (`0x55E8E0`) | `-5` |
| treaty word: mutual protection or alliance | `-10`; right of passage: `-5` |
| civs allied with both (`0x501020`) | `-2` each |
| each third civ `b` at war with `P` **and** `q` | `-min(rec(b,q)+0x38, 1) - 3 - min(rec(b,q)+0x10, 2) - min(rec(b,q)+0x14, 4) - min(rec(b,q)+0x34, 5)` |
| each third civ `b` at war with `P` that `q` embargoes (and `q` not at war with it) | `-2` |
| each third civ `b` in contact with `P` and not at war with it (the "flavor mix") | `+min(d,2) + min(c,2) + min(r08,3) + min(v,1) + min(razed,8) + min(r44,1)` over `rec(b,q)` (declarations, cancelled deals, `+0x08`, violations, razed, `+0x44`) |

The flavor mix is a blend: with the default race flavor type (4) it is the identity (`0x440A4F`), so the term
is the plain sum. The race's flavor mix is not decoded for other types.

After the sum: at war and negative: `0`; `flag == 0`, negative and `rank(q) > rank(P)` (`+0x24`, 1 for the
leader): the score is halved toward zero; finally clamped to `+-(width + height) / 2` (`[0x9C74C0]`,
`[0x9C74D4]`).

### 5.2 The class `0x440AD0(P; q, flag)`, vtable `+0x88` (A)

With `m = ((width + height) / 2) / 10` (C division) and `s` the score:

| class | when | name |
|---|---|---|
| 0 | `s < -m` | friendliest |
| 1 | `-m <= s < 0` | |
| 2 | `s == 0` | neutral |
| 3 | `0 < s <= m` | |
| 4 | `s > m` | hostile |

(`government.md` 5.5 has the same table; its caveat that the score "rises with the incidents" is confirmed:
declarations, hostile acts, attacks and razing all add.) The table `rec` is a *book*: the counters that
say what `q` did to `P` live in `P`'s record about `q`.

## 6. The AI's decision to go to war `wantsWar(P; q)` `0x440B60`, vtable `+0x8C` (A)

Called only from the provoke tail `0x502CC0` (the espionage missions: stealing a tech, poisoning, sabotage:
`government.md` 5.1), with `P` the victim. When it returns true the victim declares war with reason 0.

```
n = number of civs in play
b = clamp(aggression(P), -2, 2)
b -= GOVT[government(P)].war_weariness_class      // 2 -> -2, 1 -> -1
b += by attitudeClass(P; q, 0):   0: 1 - n,  1: -1,  2: 0,  3: +1,  4: n - 1
d = rec(P,q).+0x34 - rec(q,P).+0x34                // the accumulators, ours minus theirs
if d > 0:  b -= d / max(cities(P), 1)
b += rand(max(32 - rank(q), 1))                    // the gameplay die 0x60BAB0
return b >= 32 - rank(P)
```

The other half of the decision (the declaration by the AI's own planning, 7.2) is not decoded.

## 7. The AI's initiative

### 7.1 Peace (A for the gates, H for the rest)

The AI-to-AI initiative `0x43E470` (Player vtable `+0x78`) is entered from the rotation `0x43F370`
(`turn.md` 4.1) once per met AI pair per turn. Its peace move is built in `0x43D610(P; q, ..)`
(`ret 0xC`, 3.7 KB, which builds the two item lists and hands them to `0x503830`), and that method returns
at once unless the pair is at war (`0x43D61D`: `P.+0xD30[q]`) and `P.+0xBB0[q] <= 0` (`0x43D638`).
`0x43E470` tests the memories of both sides (`0x43E4E4`, `0x43E50A`), and the attitude class and embassy
(through `0x440AD0`, `0x501910`); the reference's `may_offer_peace` is that reading: both war memories
zero, class not 4, an embassy, no right of passage, and then a die `rand(4) == 0`.

**The memory never decays in the code read.** A scan of every reference to the table (the displacement
`0xBB0` and the absolute base `0xA53A48`) finds its writers: the declaration (`0x50221F`, `0x502245`), the
peace (`0x5026C0`, `0x5026E6`), the player initializer (`0x568080`) and one store of zero at `0x43E3A3`
inside `0x43D610`, after the proposal is built. Its readers are `0x439418`, `0x43D63B`, `0x43E4E4`,
`0x43E50A`, `0x43F0D0` and `0x440E23`. No decrement, in the turn routine or anywhere else, so an AI whose
war memory was set to a positive value by a declaration or a peace (both set 8) would never reach the
state in which it offers peace. This is the reading of a negative scan, not an observed run: **H**, and the
playable game relies on it (the AI never opens peace talks). A human can still propose peace (the scorer, 8.3).

### 7.2 Outside war (A for the gate, rest not decoded)

`0x43E87E`: the AI acts on a met civ only when `rand(T) == 0`, `T = max(1, S - 1) - f`, where `S` is a
stored score word and `f` is `1` when a game flag in mask `0x16` is set (`rust` `initiative_gate`). What
follows (proposing trades, requesting aid, planning wars) is the body of `0x43E470` and is not decoded.

### 7.3 Border pressure and tension (A for the thresholds, H for the growth)

A foreign unit standing inside a civ's borders adds its score to `Player.+0xDB0[q]` each turn
(`0x446C1B`); `>= 0x200` sets relation bit `0x10` (`0x446C3B`), anything lower sets bit `0x08`
(`0x446C18`). A right of passage exempts the unit. The writer that decays the tension, and what the AI does
when the bit is set, are not decoded; a declaration zeroes both words (section 3).

## 8. Deals

### 8.1 Item lists (A)

A proposal is two linked lists of nodes (`[node+4]` type 0..10, `[node+8]` payload, `[node+0xC]` the
literal of a gold item, next at `[head+0x10]`): what the first party gives, and what the second gives.
Types, as the executor `0x502D90` (jump table `0x50357C`, `ret 0x10`, arguments `other, type, sub,
payload`) and the scorer read them:

| type | sub | item | executor arm | A/H |
|---|---|---|---|---|
| 0 | 0 | **peace** | `0x502DB1`: `makePeace(this; other)` | A |
| 0 | 1 | **mutual protection** | `0x502DCC`: `treaty |= 1` on both sides, then a message | A |
| 0 | 2 | **right of passage** | same arm, `treaty |= 2` | A |
| 1 | | **military alliance** against the civ in `payload` | `0x502F64`: both added to each other's `allies_vs`, the joint war declared (reason `0x22 + partner`) | A |
| 2 | | **embargo** of the civ in `payload` | `0x503143`: both added to the embargo masks against it | A |
| 3 | 0/1 | **world map** | `0x503240` | H (the scorer's map arm calls vtable `+0x5C`) |
| 4 | | **establish contact** with `payload` | `0x5032A8`: `establishContact(other; payload)` | A |
| 5, 6 | | resources (strategic and luxury, **H**): `0x55EC80(Player[other]; this.civ, payload)` | `0x5032BD` | H |
| 7 | 0 | **gold per turn**: adds the amount to the per-turn table `0xA53CC8` (row `other`, column `this`) | `0x5033C3` | A |
| 7 | 1 | **gold, once**: the treasury cells of `other` lose it (a deficit resets them with the `timeGetTime` split, as `turn.md` 3.4) and `this` gains it | `0x503300` | A |
| 8 | | **an advance** `payload`: `acquire(this; t, 0, 1, 1)`, and the notice fields `+0xAC/+0xAD/+0xCD/+0xF0` carry the giver | `0x5033F8` | A |
| 9 | | **a city** `payload`: `0x563410` (the transfer) | `0x50345D` | A |
| 10 | | a unit by pool id and the giver's capital, `0x5694D0` | `0x5034C5` | H (unidentified) |

The item gates (`canOffer*`): peace needs a war; treaties, alliances and embargoes need an embassy
(`0x501950`, `0x5019F0`, `0x501A60`, `0x501B80`) and the advance flag of `TECH` (`flags` bits `0x100`
mutual protection, `0x200` right of passage, `0x400` alliance, `0x800` embargo: `research.md` 1.1); an
alliance names a third civ that is neither party and not yet allied; a contact item must be a civ the
giver knows and the receiver does not.

### 8.2 Execution (A)

Each item runs through the executor in list order, for both lists. Timed items (the peace treaty and gold
per turn) additionally open a **package** (8.6). A military alliance declares the joint war at once, which
runs the cascade of section 3. Everything the executor changes is in the tables of section 1 plus the
treasuries, the known-advance sets and the transfer of cities.

### 8.3 The scorer `0x440EE0` (A)

Player vtable `+0x94` (vtable `0x66CB38`, the same table as the attitude methods and the capture decisions of
`capture.md` 12), `thiscall`, `ret 0x28`. Two-sided item-list evaluator; out-params at `[esp+0x48]`,
`[esp+0x5C]`, `[esp+0x60]` are zeroed on entry. Each side walks its list through an 11-arm jump table (side
A `0x440F7B..0x44134C`, table `0x441AE4`; side B from `0x44136F`, table `0x441B10`); each arm values one
item into `ebp` through a per-type call, and a gate `0x502D40` (treaties, alliances, embargoes, contact, gold
and advances are *gated*; maps and cities are not) decides whether the value also goes into a second
accumulator: side A `[esp+0x28]` always and `[esp+0x24]` gated, side B `[esp+0x44]` and `[esp+0x20]`.

Arms of table 1 (bytes at `0x441AE4`):

| type | arm | valuation |
|---|---|---|
| 0 | `0x440F96` | sub 0: `0x4390A0`; sub 1: gate `0x501950`, bit 4 test of `[edi+esi*4+0xF30]`, `0x438650`; sub 2: `0x5019F0`, `0x438740` |
| 1 | `0x44102D` | gate `0x501A60`; bit test of `[edi+payload*4+0xFB0]`; `0x4389A0` |
| 2 | `0x441071` | gate `0x501B80`; bit test of `[edi+payload*4+0x1030]`; `0x438CB0` |
| 3 | `0x4410B0` | sub 0: vtable `+0x5C(esi, 0)`; sub 1: `+0x5C(esi, 1) - +0x5C(esi, 0)` |
| 4 | `0x441121` | `0x4385A0(payload)` (no civ index) |
| 5, 6 | `0x441133`, `0x44114C` | `0x43B540`, `0x43B730` `(payload, 1, [esp+0x50])` |
| 7 | `0x441165` | sub 0 (per turn): computed path `0x441192..`; sub 1 (once): the literal `[node+0xC]` — the only fixed price |
| 8 | `0x441282` | vtable `+0xA4` gate, then `+0x54` = the valuation `0x448BF0(t, 0, 0)` of `research-ai.md` |
| 9 | `0x4412AC` | `0x438DB0` twice (both players) |
| 10 | `0x4412DF` | `0x438F90` twice, summed |

The bodies of the per-type valuations (`0x438650` .. `0x438F90`, `0x43B540`, `0x43B730`, the per-turn gold
path) are not decoded; **only the price of an advance (type 8) is known**.

### 8.4 The verdict ladder and the attitude scaling (A)

Before the ladder, side B (what the AI is asked to give) is scaled by the pair record
(`0x441901..0x441931`): `B *= 4*T + 1` with `T = rec(P, q) +0x04`, the deals the other civ cancelled.
Then, with `A` (side A, what the AI is offered) and `B` signed (`0x44198B..0x4419D6`):

| condition | score | dialog key (`0x517B70`) |
|---|---|---|
| `A >= B` | 36 | `DIPLOADVICETRADE_DEAL_ACCEPT` |
| `A > trunc(7B / 8)` | 37 | `..._WEAKREJECT` |
| `A > trunc(B / 2)` | 38 | `..._NEUTRALREJECT` |
| else | 39 | `..._STRONGREJECT` |

(`trunc(7B/8)`: `lea; cdq; and edx, 7; add; sar 3`; `trunc(B/2)`: `cdq; sub eax, edx; sar 1`.) Other
returns: **40** invalid (flag `[esp+0x13]`, multiplayer and owner gates `0x4418C1`, item gates
`0x44196B`, `0x441981`, all `jmp 0x441AA3`), the counter-offer trio **44** via `0x5011A0`, **43** via
`0x501260`, **42** via `0x5010E0` (the last also needs `[esp+0x24] > 0` and `ebp - [esp+0x20] > 0`), and
a null list answers **38** (`0x441AD3`). The epilogue `0x441AA3` writes `[esp+0x58] = offer - ask`,
`[esp+0x5C] = gated offer - gated ask` and returns the score. Five direct callers: `0x43BD66` (vetoes on
`0x28` first), `0x507C55` (accept-only, then `inc [ebp+0xE9C]`), `0x517666`, `0x517BCD` (the dispatcher
`0x517B70`), `0x51B586`.

The dispatcher maps 36..39 to the script keys above (each arm: `push key; push 1; push [0x72BC58]` =
`text\script.txt`; `mov ecx, 0x9C3508; call 0x598580`, then a `0x60E6B0` scan); any other value shows
nothing.

### 8.5 What a price is (A for the function, H for whose viewpoint, C for the rest)

* **Advance** (type 8): the valuation `value(t, 0, 0)` of `research-ai.md` 1 (vtable `+0x54`, behind the gate
  `+0xA4`); it carries the "nobody else has it" doubling, the current-research discount and `x1.5` for
  untradable advances. Which player's viewpoint prices each side (the giver's or the receiver's) is not
  decoded (**H**); the playable game uses the receiver's. The `4T + 1` scaling applies to the AI's side only.
* **Everything else**: the arms above are called but their bodies are not decoded. The playable game
  prices the other items itself (section 12).

### 8.6 Packages (A for the lifetime, H for the contents)

A deal with a timed item (peace, gold per turn) is recorded in a list, each entry with an end turn
`[0xA526AC] + 0x14` (20 turns). `0x500830(P; q, 1)` (declaration, section 3) cancels every entry between
the pair and adds one to `rec(victim, declarer) +0x04` for each that was still running (`end > turn`).
That counter is the `T` of 8.4: the AI that was betrayed asks five times the price, nine times after two
broken deals. The list is not otherwise consulted by the code read here; the per-turn payments run from the
table `0xA53CC8` in the turn routine (`turn.md` 3.1 step 6, `economy.md`).

## 9. The tech-trade counter (A)

On a completed tech trade the engine bumps a cell of the matrix at `0xA53070` (`0x43FB90` ff):

```asm
0x43fb9d  mov ecx, esi        ; esi = row key (faction side)
0x43fb9d  ...x33...x264...    ; ecx = esi*263
0x43fbb0  lea ecx, [esi+ecx*8]; ecx = esi*2105 (dwords)
0x43fbb3  shl ecx, 2          ; ecx = esi*8420 (bytes)
0x43fba2  lea edx, [eax+eax*8]; edx = eax*9
0x43fbaa  lea edx, [eax+edx*2]; edx = eax*19 (eax = [ebp+0x1c], column key)
0x43fbb9  mov edx, [ecx+edx*4+0xA53070]
0x43fbc0  inc edx
0x43fbc1  mov [ecx+eax*4+0xA53070], edx
```

Rows of 2105 dwords (8420 bytes, the player stride), 19 dwords per column: the **pair record** of
section 1 (`0xA53070` is `rec +0x28`, the tech-trade count: `0xA53048` is `rec +0`). The attitude reads
it (`-min(n, 1)`, section 5.1). Then:

```text
OutputDebugStringA("Tech traded!!!...\n")   ; 0x43FBD1 / 0x43FC26
0x47B530 global check; 0x47B550(ecx=0x7C7C28) sub-check
notify: 0x475460(this=0x74AF60, eax, edi, 0,1,1)
   else 0x561860(this=ebp,      eax, edi, 0,1,1)
```

Two identical blocks (the `edi`/`[esp+0x34]` variants): both sides get their counter bumped and notified.
The `0x561860` vs `0x475460` split (local vs remote/observer update) is unproven. `0x561860` is `acquire`
(`research.md` 7); the trade calls it with `(0, 1, 1)`: no discovery credit, a notice, no free-tech
cascade.

## 10. Dialogs

### 10.1 Government and vote dialogs `0x46CF5F` (A)

Gated on `[edi+0x2128]` + `0x46BBF0`; all exits converge on the `0x46DE6C` epilogue (`call 0x601F20`, the
founding-path return helper):

| site | string | dialog shape |
|---|---|---|
| `0x46D03D` | `CHANGE_GOVERNMENT` | push str + `0xCADC18`; `call 0x47A430`; count `[0x9C3DA8]`; owner `[0x9FD4BC]` |
| `0x46D56D` | `NEW_GOVERNMENT_AVAILABLE` | slot-`0x170` dispatch (founding shape) |
| `0x46DA51` | `DIPLOVICTORYVOTEOPTION` | slot-`0x170` + `0x611530` commit; fail: `0x46E3C0` |
| `0x46DB19` | `CONFIRMDIPLOMACY` | slot-`0x170` + `0x611530`; pre: `[0x9FD4BC]`, `0x61C570`, `0x61C5A0` via `[0xA52E98]` |

Second push sites: `CHANGE_GOVERNMENT` also `0x55CC41`, `NEW_GOVERNMENT_AVAILABLE` also `0x4DCA20`,
`VOTEOPTION` also `0x4F28E4`, `CONFIRMDIPLOMACY` also `0x506301`. The dialog-owner identity `0xCADC18`
is open.

### 10.2 Diplomacy and espionage dialogs (A)

One idiom everywhere: `push STR; push 0xCADC18; call [reg+0x170]; call 0x611530` (the NEWCITY founding
shape).

* diplomacy art-init `0x5049D0`: `consider/counter/uparrow.pcx`, `diplomacy.txt` via
  `push 1; push path; mov ecx, 0x9C3508; call 0x598580` (+ `0x64CCC0`, strcpy, `0x5FC820`, vcalls
  `+0xDC`/`+0xD8`).
* diplomacy dialog `0x505F40`: `CONFIRMDIPLOMACY` (`0x506301`), `NODIPLOMACY` and a direct `0x47A430` variant
  (`0x50652F`).
* espionage dialog `0x5249B0`: `INVESTIGATE_CITY_IMMUNE` (`0x524D8F`), `SAFETY_LEVEL` (`0x524261`),
  `STEAL_TECH_IMMUNE` (`0x525071`); pre: `0x55A210`/`0x61C5A0` and the `table_backptr` idiom at `0x524A7E`.
* `0x5354B0`: second `NODIPLO` + `0x611530`, `0x4000`-flag prelude.
* `0x504013`: `MISSION_UNAVAILABLE` + slot-`0x170`; `0x52B505` pushes `GCON_Espionage_Missions`.

### 10.3 Response switches `0x510730` and `0x5157E0` (A for the heads)

* Master (`0x510730`): `cmp eax, 0x34; ja 0x515443; jmp [eax*4+0x5156FC]`: 53 cases (codes 0..52). Case 0
  opens with `0x55A270` + `0x61C5A0` (combat-report text slot).
* USER (`0x5157E0`): `add eax, -53; cmp eax, 0x18; ja 0x515E75; jmp [eax*4+0x51612C]`: 25 cases (codes
  53..77). Code 53 inlines the `USERACCEPT` string copy (`0x51580A..0x51583A`).

Per-row case-to-string map (table dumps and sampled bodies only): master cases 12, 13, 30, 37..40, 48 and
USER codes 53, 54, 55, 57 verified, the rest rest on dumps. The embargo advisor `0x517EE0` (sole caller
`0x5099E0`, deal-screen modes 5 and 6) with the LAND/WATER route gate `0x501390` is unopened here.

### 10.4 Random-Nth city pick `0x52121B` (A: not the combat die)

`rand() % si` gives a word index into `[edi+edx*2+0x4080]`, validated against the city count `[0xA52E78]`
(**H**: a random city or unit pick helper, the family of the `0x5AF703` scans).

## 11. The starting state (A for the writers, H for the zero start)

The at-war byte has exactly two writers (section 1), so **no pair starts at war and none enters war
without a declaration**. The player records are a static array (`0xA52E98`) and `Player::init`
(`0x567C80`, `government.md` 8; it loops over the per-pair tables at `+0xBB0` and `+0xD50`, `0x56806A..0x568084`)
zeroes the counters, so a new game starts with no contact, no embassy, an
empty pair record, no treaty, no allies and no war memory. Contact comes from the first-contact scan
(section 2), the embassy from the first-contact conversation or a deal. The relation matrix therefore
carries no initial state beyond zeros, which is why the barbarians (slot 0) are never a partner: every
transition refuses `q <= 0`. This was read, not run (**H** for the zero start).

## 12. The playable game (C)

`src/diplomacy.rs` is the rules, `src/advisors.rs` the screens (Foreign Advisor `F4`, Science Advisor `F6`,
the Talk dialog, the proposal and war-confirmation modals). It follows the reference where the binary is
decoded and is marked *clone* where it is not:

* **State** is `civ3mapgen::diplomacy::Relations`, one slot per civ (civ `i` is slot `i + 1`,
  `research::slot`). No war until declared; a war ends only with a peace treaty; first contact (a unit within
  2 tiles of a foreign unit or 3 of a foreign city) establishes contact and swaps embassies at once (the
  binary leaves the embassy to its conversation and deals, section 2). The "just met" bit and the greeting
  timer are kept but unused.
* **Declaring war** and **peace** call the reference's `declare_war` and `make_peace`, so the alliance
  cascade, the memory, the weariness, and the broken-package counter are the binary's. A human attack on a
  civ it is not at war with asks for confirmation (`CONFIRM`) and then declares.
* **Attitude and the war decision** are `Relations::attitude` / `attitude_class` / `wants_war`. The facts they
  read: aggression, government, culture group and favorite/shunned government come from the BIQ rows;
  score is `10*cities + 3*techs + soldiers` and rank one plus the civs scoring more (**C**: the binary keeps
  both in the record).
* **Border pressure**: 64 per foreign soldier inside the borders per turn, the binary's thresholds (warning
  `0x80`, war track `0x200`), decay `0x20` on a quiet turn, a right of passage exempts (**C** for the
  rates). When the pressure reaches `0x200` the owner books one hostile act against the intruder's civ,
  drops the pressure to `0x100`, and an AI owner rolls `wants_war` (a declaration with reason 0 if it wins;
  otherwise a demand to withdraw is posted).
* **AI war planning**: from turn 15, once every 6 turns per civ (staggered by civ), each AI takes the
  highest-scoring civ it has met, is not at war with and has no package with, and rolls `wants_war` against
  it (**C**: the binary's own planner is `0x43E470`, not decoded).
* **Deals**: treaty items (peace, passage, pact, alliance, embargo), contact, gold, gold per turn (20 turns)
  and advances, weighed by `weigh` with the binary's ladder and `4T + 1`. Prices are stand-ins on a x20
  scale: gold at face, an advance at `20 * worth` (`research-ai.md`), passage `20 * (4 - class) * 10`
  (refused beyond class 3), a pact the same (refused beyond class 1), peace `500 * max(0, class - 1 -
  warCounter/20)`, alliance `20 * 150` (needs class `<= 2` and a shared enemy), embargo `20 * 60`. World
  maps and cities cannot be traded.
* **The AI makes offers**: a tech-swap pitch to each human (1 in 10 per turn, at most one pending per AI and
  one every 3 turns, only when its attitude is class 2 or better and it would accept the swap itself). Nothing
  else is proposed.
* **The AI never offers peace** (7.1), which matches the binary under the reading there.

## 13. Open

* The body of `0x43E470` (the AI's own initiative, what it offers and when) and of `0x445490`.
* The AI-to-AI war start (the planning that runs without a provocation).
* Relation bits `0x04`, `0x20` and the high bits, and the writer of `+0xDB0` growth and decay.
* The writers of `rec +0x08`, `+0x18`, `+0x1C`, `+0x20`, `+0x24`, `+0x3C`, `+0x48`.
* The per-type bodies of the scorer (`0x438650` .. `0x438F90`, `0x43B540`, `0x43B730`, the gold-per-turn
  path) and the identity of types 5, 6, 10; the counter-offer outcomes 42..44.
* Any decrement of the war memory `+0xBB0`.
* The ally loop of `makePeace` (units pass) and the rest of `declareWar` after `0x5024DB`.
* An execution of `0x501F20` / `0x5025B0` / `0x440100` against a synthetic game to upgrade this file to
  **E**.
