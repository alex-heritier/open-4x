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
(state>>16)&0x7FFF`. The seed entry is `srand` at `0x64A201`, immediately
before `rand` (classic MSVC pairing, re-verified with r2 2026-09-29):

```asm
0x64a201  call 0x64dc93              ; owner object in eax
0x64a206  mov ecx, [esp+4]            ; seed argument
0x64a20a  mov [eax+0x14], ecx         ; same +0x14 word rand() reads
0x64a20d  ret
```

Seeded from `timeGetTime()` in the `0x56C1EA` startup path — which draws
twice: the first draw is saved to `[0xA526B4]`, the second is pushed as
the `srand` seed — and at game start (`0x6389DD`: `push eax; call
0x64A201`, then `[esi+0xAF8] = 1` start flag). The same startup path reads
prefs through `0x585B00("Video Mode"/"KeepRes"/"QuickStart"/"NoSound")`.
**Map generation never touches it** — every map stage uses the local LCG
`0x60BA80` (`*s = *s*1103515245 + 12345`, high 16 bits out). Consequence
for clones: AI/combat randomness and map randomness are independent
streams; sharing one RNG is *not* faithful.

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

## Caller census (verified this session, recounted 2026-09-29)

Raw `E8`-scan for `0x64A20E` finds **65 direct call sites** (reproducible:
every `E8 rel32` in `.text` resolving to `0x64A20E`; an earlier count of 70
was wrong — recheck the `0x5A`/`0x5C` rows if you repeat it). Game RNG use
is pervasive, not centralised:

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
| `0x5Axxxx` | 3 (`0x5AF703 0x5AF84B 0x5AFA1B` — the candidate scans below) |
| `0x5Cxxxx` (unit-AI helper region incl. `0x5C1AD0`) | 8 — densest cluster |
| `0x5Dxxxx` map/cell | 2 |

## Random-Nth candidate scans, `0x5AF703–0x5AFB1E` (verified this session)

Three loops, one idiom: draw `rand() % (edi & 0xFFFF)` as a countdown,
then scan table rows and return the Nth row that passes all gates (N = the
draw). The AI picks a *random passing candidate*, not a random row. Fully
decoded instance at `0x5AFA1B` (r2, 2026-09-29):

```text
countdown = rand() % (edi & 0xFFFF)     ; [esp+0xC]
count     = [0x9FD4BC]                   ; owner/global unit count
for each row r in 0xA52EB4..0xA94B34 step 0x20E4 (32 rows):
    bit  = dword[row]                    ; bit position for this row
    mask = 1 << bit
    require mask & [0xA526BC] && mask & [0xA526C0]   ; capability words
    require byte[ebx + idx*4 + 0xA53BC8] != 0        ; overlay-table byte
        (idx = owner*2105 idiom; 0xA53BC8 is the rivers.md overlay table:
         the renderer gate and AI selection read the same table)
    for each sub-row (same 32-row walk from 0xA52EB4):
        same capability gates, then pass = 0x501B80(ebx, esi, 0)
        if pass && countdown-- == 0: select (ebx, esi), exit via 0x5AFB1E
```

Siblings: `0x5AF703` (same countdown, random entry of the `0xA52E98`
2105-stride table via `0x55E730`, owner-compare vs `[0x9FD4BC]`) and
`0x5AF84B` (countdown + the same two capability words, bit from
`[0xA54F98]`). All three share the owner global `[0x9FD4BC]`, which is also
the unit-owner compare in resolve `0x4A53A0` — one identity domain for
"whose units the AI may scan".

`rust/src/ai.rs`: `random_nth_passing()` (countdown selection over a
pass/fail iterator) — the mechanism, predicate `0x501B80` open.

Every sampled caller uses one idiom: `call 0x64A20E; cdq; mov ecx, N; idiv
ecx` — i.e. `rand() % N`, exactly the `GameRng::next_bounded` model.

## AI score jitter `0x4D7F16` → accumulator `0x5FACC0` (verified this session)

Re-verified byte-exact with r2 2026-09-29:

```asm
0x4d7f16  call 0x64a20e              ; rand() in 0..32768
0x4d7f1b  cdq
0x4d7f1c  mov ecx, 0x4b0             ; 1200
0x4d7f21  idiv ecx                   ; edx = rand() % 1200
0x4d7f23  mov eax, [esi+0x4ec4]      ; table index from object field
0x4d7f29  lea eax, [eax+eax*2]       ; x3
0x4d7f2c  and edx, 0xffff            ; no-op on [0,1200), kept by MSVC
0x4d7f32  sub edx, 0x258             ; jitter in [-600, +599]
0x4d7f38  push edx
0x4d7f39  lea edx, [eax+eax*8]       ; x27
0x4d7f3c  lea ecx, [edx*4+0xa502b8]  ; entry = 0xA502B8 + index*108
0x4d7f43  call 0x5facc0              ; clamped score store (this=ecx)
```

`0x5FACC0` head: `mov eax,[esp+4]`; clamp to `[-1200, +1200]`
(`cmp 0xFFFFFB50` / `cmp 0x4B0`, saturating both ends); store at
`[ecx+0x5C]`; then a virtual call through `[ecx+0x40]`. So the AI keeps a
per-entry score accumulator with ±1200 saturation, and this call site adds
uniform ±600 noise before storing — the standard "noisy score" trick so
tied evaluations break randomly.

The same function draws again at `0x4D7F60`: `rand() % 30000 + 30000`
(`mov ecx,0x7530; idiv; and 0xFFFF; add edx,ecx`, range `[30000,59999]`),
pushed with `5` into a call using `[esi+0x2E164]` — a second, larger-scale
jitter in the same decision. Its callee is not yet identified.

Other characterised samples: `0x5CEF16` divides by a variable `esi` then
dispatches through a table (`0x60F6A0`) — random list pick; `0x5DA711`
stores `rand() % ebx` into an object field (`+0xFC`).

`rust/src/ai.rs`: `score_jitter()` (exact `draw % 1200 - 600`) and
`clamp_score()` (±1200 saturation), both tested.

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
| `BARBARIAN_ATTACK` | `0x5B628A` (push; `0x5B628B` is the operand byte) | same cluster |
| `** Unit enslaved` | `0x5BFB02` (sole push; `0x5BFAC8` is the nearby call); `NORM_ENSLAVE` at `0x5BFAC7` | beside the action gate region |
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
  Re-verified 2026-09-29: branch at `0x4A53FA` (`je 0x4A5439`);
  step constructors have exactly one caller each (`0x4A49C0 ←
  0x4A542F`, `0x4A47C0 ← 0x4A544C`, exhaustive `E8` scan). Path 2
  continues past the step into `0x4E69D0(this=0x9F8700)`, halved
  cell-index + `0x5D16A0` + cell vfunc `+0xB8`, then `0x56D040`
  (bombard-chain head) — resolve = move-step construction, then
  effect application down the same chain bombard uses.
* `0x5B2820` head is a bounds-checked indexed fetch, not odds math:

```text
idx = this[0x1C]; table = [0xA52E84]
if (!table || idx < 0 || idx > [0xA52E90]) return 0
v = table[idx*8 + 4]; if (!v) return 0; return v - 0x1C
```

(a `container_of` back-pointer off a strided table). CORRECTION
(child-found): `0x5B2820` is backptr **only** (`ret` at `0x5B2844`);
the combat-UI constructor (SEH, vtables `[edi]=0x66FAC4` /
`[esi]=0x66FAE0`, `idls`/`unit` FOURCCs) is the *next* function at
`0x5B2850` — this file conflated them. Modeled in `rust/src/ai.rs`
as `table_backptr`.
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
  then cell fetch and cell `vfunc(0x8C)` (`isWater`). Re-verified
  2026-09-29: boolean return (`xor al,al`, `ret 0xC`), the kind check
  runs twice (second inverted at `0x4A1616`) — a can-enter/can-attack
  tile predicate, NOT the odds core.
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

## Combat-dice exclusion map (verified 2026-09-29)

All randomness flows through `0x64A20E` (`0x64A201` is srand-only, 2
startup callers; no MSVCRT imports). All 70 raw `E8` sites ranked; the
die roll is NOT in: `0x5B` (zero callers), `0x56`/`0x61` (zero),
`0x4A` (`0x4AEC63` = 6-of-8 shuffle only), `0x5C` (`0x5CEAxx` =
advisor scheduler, `0x5CB0xx` = `%21`/`%4` map scatter),
`0x44` (`0x447BA5` = 1/3 advisor trigger), `0x5D` (`0x5DA711` =
list-scatter modulo, `0x5DCE7A` = `%5` re-roll picker).
Remaining candidates: `0x40/0x42/0x43/0x48/0x4C/0x4D/0x4F/0x50/
0x52/0x53/0x54/0x58/0x59/0x5A` sites, or an indirect call from the
combat path (the `0x5B` assassin/vcall path has no direct `E8`).
Also excluded: `0x584BA2` (`rand % count` random-Nth over a
`0x6C`-stride table), `0x506E47` (`rand % 31` + owner-bitmask test),
`0x5B8B50` (assassin callee: validation + `0x5BE5B0` setup, no roll).

## Kill path `0x5BBBC0` (head + body scan, no HP field yet)

MP notify (`0x46BE30`/`0x46BE70`), tile + cell-vfunc `+0xB8`, two
`0x5BC8B0` calls — but `0x5BC8B0` is a boolean validator (`ret 4`,
opcode `0x12` fast path, `0x5BC6D0` + `0x5E4EF0` record checks), not
a mutator. Body adds unit fields `+0x5D` (flag byte, cleared via
`0x4F02C0`) and `+0x1EC` (byte, `0xFF`-able) plus art/ambience calls
(`0x58B5D0`, `0x539D60`). No HP decrement through `0x5BBD86` — the
HP field and damage write are still open, and with them the odds
core's data anchor.

## Stack merge `0x5BCC90` (verified: head read)

Field census of `0x5B` (`mov r32,[r+disp8]` ranking) surfaced hot
offsets `+0x4C` (11×), `+0x44` (9×), `+0x30`/`+0x48`/`+0x2C` — the
`+0x4C` site is a unit-stack merge, not HP: `[ecx+0x50] =
max([ecx+0x50],[edi+0x50])`; `[ecx+0x4C] = max(0, [ecx+0x4C] +
[edi+0x4C])` (`sets/dec/and` clamp); OR bit 4 of `+0x48`; zeroes the
donor's `+0x50`/`+0x4C`. Followed by `0x5BE5B0` value-vs-`+0x4C`
computations clamped to `[0, 0x270F]`. So `+0x4C` = stack-summed
accumulator, `+0x50` = max'ed meter — new unit-record fields.
Disband block `0x5BC394`: sets bit `0x10` in `[esi+0x30]`,
`[esi+0x44]` = shield value via `0x4ACD70` evaluator (max-take),
`DISBANDSHIELDS` text slot — so `+0x44` = shields, `+0x30` bit
`0x10` = disband flag. HP still unidentified.

## Shuffled 6-of-8 picker `0x4AEC48–0x4AECAB` (verified: `r2`)

Uncharacterized `rand()` caller at `0x4AEC63` (found by ranking `E8`
callers near the combat band; siblings `0x4BE1CF`/`0x4BE3B2` are the
riot picks, `0x5AF703…` the random-Nth scans). MSVC `rand() % 8` idiom
(`and 0x80000007` + sign fixup + `and 0xFFFF`), rejection re-draws
(`bl` dup flag over `[esi+0x74]`-adjacent slots, init `-1`) until 6
distinct values fill the array. HYPOTHESIS: randomized
neighbor/move-direction order for unit AI (pathfinder-adjacent) — the
container does halved-coordinate `0x5D16A0` cell queries and
`table_backptr` lookups and returns `0x1C` at `0x4AECB2`; its start is
above `0x4AEA00` (no direct callers — reached by fallthrough or
indirect call). Upward context (`0x4AEA07–0x4AEBBA`) gates on the same
capability words as the random-Nth scans (`[0xA526BC]` bit test,
`[0xA5267C] & 0x25C00`) and calls `0x5694D0` + vcalls `+0x20/+0x58/+
0x38` — AI-candidate-evaluation context, strengthening the
move-order reading.

`rust/src/ai.rs`: `shuffled6()`, tested (seed-0 slot 0 = `38 % 8`,
uniqueness, determinism).

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

## City founding: NEWCITY dialog-validate-commit (verified this session)

City founding runs through script-name calls in the `0x4D9xxx` unit-action
region (r2, 2026-09-29). Action names live in `.data` (`0x729230` ff):
`DISBAND UPGRADE SETTLEMENT_FOUNDED BADCITYNAME NEWCITY ...`.

```text
0x4D9D85  push "NEWCITY", 0x17 (23), name-buffer, 0x44 (68); call 0x495840
          ; dialog server resolves the city name
0x4D9D9A  call 0x611530 (this=esi)          ; commit gate, nonzero = ok
          on ok: copy default name from [0xCB8B38], then scan the
          existing-city table (base [0xA52E6C], bound [0xA52E78]) with a
          byte-compare loop (+0x1E0 = 480 stride) for a duplicate name
0x4D9E6A  on duplicate: push "BADCITYNAME"; slot-0x170 dialog; 0x611530
          result byte [esp+0x13] picks continuation (0x4D9E9E / 0x4D9ED1
          via 0x601F20 + 0x425A60)
0x4D9F54 / 0x4DA040  "SETTLEMENT_FOUNDED" completion markers
```

So founding = dialog (`NEWCITY`) → validate (name-uniqueness scan, failure
surfaces `BADCITYNAME`) → commit (`0x611530`). The AI site *scorer* (which
tile the settler picks) remains open; this is the commit path it must go
through.

`rust/src/ai.rs`: `city_name_taken()` (exact duplicate-scan semantics),
tested.

## Unit effects: report-text slots `0x61C5A0` (CORRECTED 2026-09-29)

CORRECTION (child-found, parent-verified with r2): `0x61C5A0` is a
combat-report **text-slot setter**, not a damage applicator. Head:
`cmp slot,9` (`0x61C5AD`, range-gated both ends), stores to
`[slot*4+0xCB8B10]` and `[slot*4+0xCADFF8]` (defaults `[0xCC2BB0]`/
`[0xCC2BB4]` on negative), strcpy of the string into
`0xCB8B38+slot*0x1000` (`shl eax,0xC` at `0x61C5E3`). Likewise
`0x56D040` is a **nearest-enemy scan** (inits `[0x9C34EC]`/
`[0x9C34E8]=0x7FFFFFFF`, loops the city table `[0xA52E6C/78]` with
backptr `-0x1C`), and `0x55A240`/`0x55A210` are virtual-dispatch
thunks — so the "bombard chain" `0x56D040 → 0x55A240 → 0x61C5A0`
holds as *call order* (scan → thunk → report-text), but damage/state
effects happen in siblings (`0x4F00F0`, `0x5BBBC0` kill path).

The disease call-site sequence at `0x5C7C7B` stands as an anchor
(pusher of `UNITJUNGLEDISEASE` at `0x5C7CAD`); only the callee role
was wrong:

```text
if (byte[unit+0x74] == 0)
    desc = table[[0x9C71E0] + unit[0x40]*312 + 8]  ; action-record effect
else
    desc = unit+0x74                                ; direct effect selector
0x61C5A0(0, desc, 0, 0)                             ; set report-text slot
0x4ED220("UNITJUNGLEDISEASE", unit[0x28], unit[0x24], 1)  ; game log
0x4F00F0(unit, 6, 0) with ecx=0xA268B8, [0xA281D0]=1 ; state change
0x47B530 global check; fail -> 0x5BBBC0(unit, 0,1,0,0,0,0,0), return
```

Unit-record fields confirmed: `+0x74` effect/direct selector,
`+0x40` action-record base (312-byte stride into `[0x9C71E0]`),
`+0x28`/`+0x24` tile coords (reused as the log position).

`rust/src/ai.rs`: `effect_descriptor()` (direct-vs-table selection),
tested.

## Hurry validator `0x4B5290` (verified: region sweep)

2561-byte validator with 10 `HURRY_*` push sites (`0x4B52ED–0x4B5C58`)
and a triple dialog idiom: direct `0x495840` (`0x4B52F7`/`0x4B5372`/
`0x4B545F`), slot-`0x170` dispatch (`0x4B55BD`/`0x4B5698`/`0x4B5BBF`),
deferred tail through `0x47A430` (`0x4B5C64`) — all converging on the
`0x611530` commit gate (`0x4B5C6F`). Gates: city `[esi+0x30]` bit 0 set
→ `HURRY_CIVIL_DISORDER`; owner compare `[0x9FD4BC]`; `0x47B530`
neg/sbb/`&0x4000` prelude. Sibling `0x4B5CA0` holds
`HURRY_NOT_ENOUGH_PEOPLE` (`0x4B5EF1`) + `0x61C570` + the same commit tail.
Hurry gold/people cost math: open.

## Disorder turnover `0x4BDFF0` (verified: region sweep)

Riot selection is `rand() % 3` twice (`0x64A20E` at `0x4BE1CF`/`0x4BE3B2`,
cdq/idiv 3, dec/je 3-way); riot sound via `0x535D20` (arg 3/2/1); turnover
clears the disorder bit (`and al,0xFE → [esi+0x30]`, `0x4BE2CD`),
confirming `[esi+0x30]` bit 0 as the disorder flag the hurry validator
reads. Two `0x61C5A0` riot effects, then game-log calls (`0x4ED220`,
ecx=`0x9F8700`, x/y from words `[esi+0x24]`/`[esi+0x26]`):
`CIVIL_DISORDER_INTENSIFIES` (`0x4BE278`), `CIVIL_DISORDER`
(`0x4BE2A6`/`0x4BE410`), `CIVIL_DISORDER_OVER` (`0x4BE323`);
`WLTKD`/`WELOVEKINGOVER` (`0x4BE658`/`0x4BE70F`) share the idiom.

## Spaceship/production tail `0x4B9270` (verified: region sweep)

`0x55A240` → `0x61C5A0` (`0x4B98F2`/`0x4B98FA`/`0x4B990F`, the bombard/riot
effect chain) → `0x5565B0` with `SUMMARY_THEIR_SPACESHIP_PART`
(`0x4B9923`); `CITYPRODUCE` (`0x4B9153`/`0x4B973A`) via `0x4ED220`;
`WONDERPRODUCE` (`0x4B9881`) + `IMPROVEMENT_COMPLETE` (`0x4B8920`) via the
dialog-slot idiom + `0x611530` tail.

`rust/src/ai.rs`: `hurry_blocked_by_disorder()`, `riot_select()`,
`assassin_branch()` (R9's `0x5B63DB` 3-way `al` dispatch), tested.

## AI flavor weights `0x4406E0` (verified: region sweep)

Personality knobs load through the `0x585B00` config reader:
`FlavorRelationType` (clamped 0..4, stored `[0x74AF30]`),
`FlavorRelationPortion` (default 90), `FlavorTechBasePortion` (default
30). The portions convert percent-to-fraction via `fild` × the 0.01
**double** at `[0x666AC8]` into `[0x6849B0]` (0x3F666666) and `[0x684A40]`
(0x3E99999A) — bit patterns byte-verified, and only the double
intermediate reproduces both. Consumer `0x440750` reads `[0x74AF30]`, dispatches
`[eax*4+0x440A5C]`, and mixes `[0x6849B0]`/`[0x6653B8]`/`[0x666AD4]`
(0.02f) in x87. Jump-table cases: open.

`rust/src/ai.rs`: `flavor_fraction()`, tested.

## Barbarian capture flow `0x5635A8` (verified: region sweep)

Pushes `BARBARIAN_DESTROY_WALLS` (`0x72CC64`) + `0xCADC18` into the
`0x495840` message dispatch, preceded by `push 3; call 0x61C5A0` (the
shared effect applicator) and a `0x47B530` gate, followed by a
`0xA52EB8`/`0xA52EDC`/`0xA52EE0` table scan with 2105-stride math.
Siblings: `BARBARIAN_CAPTURE_CITY_{POPULATION,GOLD,PRODUCTION}`
(`0x563688`/`0x563783`/`0x563898`). Linkage to camp placement
(`0x5F2090`) and `BARBARIAN_ATTACK` (`0x5B628B`): open.

## Game-log queue `0x4ED220` (verified: region sweep)

50-slot loop (`cmp esi,0x32`), stride `0x170` over `[ebx+0x6F8]`,
`timeGetTime`-gated. This is the queue behind every `0x4ED220(string, x,
y, flag)` log call cited above (bombard, disease, disorder, production).

## Next targets (concrete)

1. Combat odds: data flow between `0x5B6820` (target select) and `0x4A53A0`
   (resolve); rank nearby `0x64A20E` callers.
2. Settler site scorer: `TUT_*` is a dead end (all 22 keys, zero
   `.text` refs — tutorial engine is data-driven). Lone `0x5B`
   founding-gate caller `0x5B9F90` is order *execution* (`push
   0x20000002; call 0x5C1AD0`, then `0x4E69D0` commit), not site
   choice. The scorer (where to send the settler) is still open.
3. Turn root `0x4708B0` (parent-verified head): SEH frame (handler
   `0x657E6B` — a real `0x65`-bucket citation), `[0x990390]==2`
   gate, `malloc 0x40`, unit coords `+0x20/+0x24/+0x28`, `call
   0x47B420`. Sequencer past the root + `0x6389DD` reseed outward:
   open.

## Reference implementation

`rust/src/ai.rs`: `GameRng` (exact rand + srand/`reseed`), `score_jitter`
(0x4D7F16) and `clamp_score` (0x5FACC0), `Difficulty` level table shape
(hypothesis: handicap multipliers, values open), turn-slice counter model
from the upkeep format string, three-stage `action_available` gate,
`table_backptr`, `cell_index`, `wrap_coord`, `in_bounds`. Combat odds math
proper and the full turn-loop root: open, no stubs.
