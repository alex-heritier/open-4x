# AI play

Status: foundations verified, turn logic not yet recovered. This file records
the confirmed RNG split, the string inventory that bounds the AI surface, and
the concrete next disassembly targets. Anything beyond that is marked open.

## Two RNGs (verified this session)

Disassembly at `0x64A20E` (game RNG):

```asm
0x64a20e  call 0x64dc93              ; tls/self pointer
0x64a213  mov ecx, [eax+0x14]        ; state lives in object+0x14
0x64a216  imul ecx, ecx, 0x343fd     ; a = 214013
0x64a21c  add ecx, 0x269ec3          ; c = 2531011
0x64a227  shr eax, 0x10
0x64a22a  and eax, 0x7fff
```

MSVC-compatible `rand()`: `state = state*214013 + 2531011; return
(state>>16)&0x7FFF`. Seeded from `timeGetTime()` in `main` (`0x56C1F9`) and
at game start (`0x6389DD`). **Map generation never touches it** — every map
stage uses the local LCG `0x60BA80` (`*s = *s*1103515245 + 12345`, high 16
bits out). Consequence for clones: AI/combat randomness and map randomness
are independent streams; sharing one RNG is *not* faithful.

`rust/src/ai.rs` implements `GameRng` exactly (constants above); first output
for seed 0 is 38 (`2531011>>16 & 0x7FFF`), asserted in tests.

## AI surface inventory (strings, unverified mechanics)

* Governor: `Governor`, `GOVERNOR_WIN`, `art\City Screen\governorBack.pcx` —
  city production automation UI exists; logic unmapped.
* Difficulty: `Difficulty` (`0x732D41C`), `AICOUNTERRESPONSE`
  (`0x7329DB8`) — AI bonuses/response tables keyed by level; values unmapped.
* Upkeep/turn: `Faction Upkeep -- m_iTurnSlice == %d ...` (`0x684C70`) —
  per-faction turn slices drive upkeep; the inter-turn sequencer is unmapped.
* Combat (strings only): `AirBombardMove 1..4`, `BOMBARDFAILED`,
  `UNITBOMBARDSUCEEDED`, `*** ASSASSIN TARGET ** : chosen during combat`,
  `** Unit enslaved by %d!`, `NORM_ENSLAVE`, `UNIT_PRODUCED_LEADER`,
  `BARBARIAN_ATTACK`, promotion `UNITPROMOTIONVET/ELITE`.
* Advisors: `MILITARYADVICE...`, `DIPLOADVICEBESTUNIT`, `FA_BEST_UNIT` —
  the advisor layer that surfaces AI evaluations; evaluation functions
  unmapped.
* Limits: `MP_NO_AI_IN_TURNLESS`, `Turn Limit`, `Turn Based`.

## Caller census (verified this session)

Raw `E8`-scan for `0x64A20E` finds **70 direct callers**, spread over the
whole image — game RNG use is pervasive, not centralised:

| region | callers |
|---|---|
| `0x40xxxx` init | 3 |
| `0x42/43/44xxxx` setup/city | 15 |
| `0x48/4A/4Bxxxx` | 4 |
| `0x4Cxxxx` map view/art | 4 |
| `0x4Dxxxx` units/combat/advisor | 4 |
| `0x4F/50xxxx` game systems | 7 |
| `0x52/53/54xxxx` | 12 |
| `0x58/59xxxx` | 3 |
| `0x5Axxxx` | 7 |
| `0x5Cxxxx` (unit-AI helper region incl. `0x5C1AD0`) | 9 — densest cluster |
| `0x5Dxxxx` map/cell | 2 |

Every sampled caller uses one idiom: `call 0x64A20E; cdq; mov ecx, N; idiv
ecx` — i.e. `rand() % N`, exactly the `GameRng::next_bounded` model.
Characterised samples: `0x4D7F16` computes `rand() % 1200`, recenters
`-600`, and pushes the jitter into an AI table call (`0x5FACC0`) — an AI
score with ±600 noise; `0x5CEF16` divides by a variable `esi` then
dispatches through a table (`0x60F6A0`) — random list pick; `0x5DA711`
stores `rand() % ebx` into an object field (`+0xFC`).

## Unit action gate `0x5C1AD0` (verified this session)

104 direct callers (57 in `0x4D` unit/action region, 18 in `0x55`, 10 in
`0x5C`): "may unit `esi` perform action `edi`?" Decided in three stages:

```text
1. meter  = clamp(0x5BE470(9999, 0) - unit[0x50]); require meter > 0
2. entry  = table[0x9C71E0 + f(unit[0x40], arg_hi)]; require entry & action & 0xFFFFFFF != 0
3. bounds = 0x426BD0(unit[0x28], unit[0x24]) via singleton 0x9C736C
```

Stage 3 (`0x426BD0`, 11 bytes) is fully recovered — an in-bounds test:

```text
in_bounds(x, y) = 0 <= x < obj[0x168] && 0 <= y < obj[0x154]
```

The dim offsets match the `0x4C3210` view check (`cmp edx,[ecx+0x168]`,
`cmp ebp,[ecx+0x154]`): same map-dims layout. Adjacent helpers `0x426C00`
/ `0x426C40` test bits 0/1 of `+0x1F0` (wrap X / wrap Y) and wrap the
coordinate — cross-confirming the `Map` wrap flags at `+0x1F0`
(`NOTES.md` §3).

Unit fields used: `+0x28/+0x24` (tile position, also fed to the map query),
`+0x40` (action-record base; `+eax*4 … shl 3 − eax` stride arithmetic),
`+0x50` (spent/remaining counter against a 9999-scale meter). The
`0x20010000` test at `0x4D8F63` is one instance of stage 2. `0x9C736C` is
the same map singleton the `GOOD` count query uses (`0x4E54C8`).

`rust/src/ai.rs` models all three stages as `action_available`.

## Combat flow, `0x5B5xxx` cluster (verified this session)

Each combat log string has exactly one code ref, all in `0x5B`:

| string | ref | context |
|---|---|---|
| `*** ASSASSIN TARGET **` | `0x5B63C1` | after `call 0x5B6820` target selection; then `call 0x4A53A0` (singleton `0x9C7348`), 3-way dispatch on `al` |
| `BARBARIAN_ATTACK` | `0x5B628B` | same cluster |
| `** Unit enslaved` | `0x5BFAC8` | beside the action gate region |
| `BOMBARD*` (4 variants) | `0x5B50CF`… | selected by unit-table field `+0x9C == 2` (`0x5B50A4`) and a counter compare; logged via `0x4ED220`, effects via `0x56D040` → `0x55A240` → `0x61C5A0` |

Shared idioms with the rest of the game (not combat-specific): the
`0x270F` meter scale (`sub eax,[esi+0x4C]` then clamp, `0x5B640E`), cell
index math off map width `[0x9C74D4]` (`sar 1; imul y; add x`, `0x5B641E`),
unit-record stride arithmetic. Combat odds math itself is not yet isolated.

## Target select `0x5B6820` and resolve `0x4A53A0` (verified this session)

* `0x5B6820` gates on a capability bit test of the same family as the
  action gate: `test [unitTable + idx*8 + 0xAC], 0x10010000` (`0x5B684C`;
  bits 16 and 28), then calls `0x56D2C0` (stack scan), `0x4C0E60` (view
  check), global check `0x47B530`, and reuses the in-bounds helper
  `0x426BD0` (`0x5B68DC`) — the stage-3 model in `rust/src/ai.rs` covers
  this call site too.
* `0x4A53A0` (SEH frame, `0x2E4` locals) gates on `0x47B530`, then
  three-ways: `0x5B2820` twice with `0x4A49C0` (one path), or `0x4A47C0`
  (other path), with a unit-owner compare against `[0x9FD4BC]` inline.
* `0x5B2820` head is a bounds-checked indexed fetch, not odds math:

```text
idx = this[0x1C]; table = [0xA52E84]
if (!table || idx < 0 || idx > [0xA52E90]) return 0
v = table[idx*8 + 4]; if (!v) return 0; return v - 0x1C
```

(a `container_of` back-pointer off a strided table). Its tail constructs
the combat UI object (vtables `0x66FAC4`/`0x66FAE0`, strings `0x72EDF8` /
`0x72EE00`). Modeled in `rust/src/ai.rs` as `table_backptr`.
* `0x4A49C0` / `0x4A47C0` are movement-step constructors, not odds:
  unit index → pointer table `0x6705E8`, copy unit `+0x24/+0x28`, offset
  via `0x5E6E50`, then wrap the destination. `0x4A49C0` inlines the wrap
  (gated on `[0x9C755C]` bits 1/2, dims `[0x9C74D4]`/`[0x9C74C0]`);
  `0x4A47C0` calls the shared helpers `0x426C00`/`0x426C40`. Tails reuse
  the gate idioms: `0x5BE470` meter compare with threshold `[0x9C72C8]`
  (`setg` byte at step `+9`), capability test `0x10010000`, then
  `0x4A1910`. Modeled in `rust/src/ai.rs` as `wrap_coord`; the odds core
  moves one level deeper (`0x4A1590` / `0x4A1910`).

## Step guards `0x4A1590` / `0x4A1910` (verified this session)

* `0x4A1590` guard chain: `0x5BCA90` check, then unit-table `+0x9C == 1`
  (compare bombard's `+0x9C == 2` selector — `+0x9C` is a unit-kind field),
  then cell fetch and cell `vfunc(0x8C)` (`isWater`).
* Both use halved-coordinate cell indexing:
  `idx = (x>>1) + (W>>1)*y` with `W = [0x9C74D4]`
  (`sar; imul; shr; add` at `0x4A15D5` and `0x4A1955`), masked `0xFFFF`,
  fetched via `0x5D16A0`. Either cells are half-resolution in X or the
  callers pass doubled coordinates — one convention, two sites.
* `0x4A1910` bounds-checks both axes (`W`, `H = [0x9C74C0]`), starts a
  `0x3E8` (1000) budget counter, queries cell `vfunc(0xA0)`, consults
  `0x426C80` via singleton `0xA52DD4`, then inlines the `table_backptr`
  idiom (`[0xA52E84]` / `[0xA52E90]`, `-0x1C`) and recurses into
  `0x4A1590` — a bounded recursive cell walk. The backptr reuse confirms
  the `table_backptr` model a second time.
* `rust/src/ai.rs` gains `cell_index`; odds math proper remains open.

## City-site factors (verified this session)

Five tutorial strings name the site-scoring axes, and nothing else:

`TUT_GOOD_CITY_SITE_COASTAL` (`0x72ED1C`), `…_FRESHWATER` (`0x72ED38`),
`…_RESOURCES_COMMERCE` (`0x72ED58`), `…_RESOURCES_FOOD` (`0x72ED80`),
`…_RESOURCES` (`0x72EDA4`). The scorer evaluates coastal access,
freshwater access, and resources split into food vs commerce — five
factors, no more named axes in the binary.

All five have **zero** `.text` VA references: the tutorial engine resolves
them by name at runtime, so static xrefs dead-end here. Reaching the
scorer needs a dynamic breakpoint on the tutorial text lookup, or the
settler-side caller (whoever gates city-founding through `0x5C1AD0`).

## Next targets (concrete)

1. Combat odds: data flow between `0x5B6820` (target select) and `0x4A53A0`
   (resolve); rank nearby `0x64A20E` callers.
2. The settler/city-site scorer near the `TUT_GOOD_CITY_SITE_*` strings.
3. `0x6389DD` (game-start reseed) outward — the turn-loop root.

## Reference implementation

`rust/src/ai.rs`: `GameRng` (exact), `Difficulty` level table shape
(hypothesis: handicap multipliers, values open), turn-slice counter model
from the upkeep format string. Strategy/settler/combat AI: open, no stubs.
