# Diplomacy: attitudes, tech trade

Owns: relation matrix, trade flows. Reference: `rust/src/diplomacy.rs`.

## Tech trade counter (verified: `0x43FB90` ff)

On a completed tech trade the engine bumps a heap relation matrix:

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

Matrix at `0xA53070`: rows of 2105 dwords (8420 bytes — the engine's
standard table stride), 19 dwords per column. Then:

```text
OutputDebugStringA("Tech traded!!!...\n")   ; 0x43FBD1 / 0x43FC26
0x47B530 global check; 0x47B550(ecx=0x7C7C28) sub-check
notify: 0x475460(this=0x74AF60, eax, edi, 0,1,1)
   else 0x561860(this=ebp,      eax, edi, 0,1,1)
```

Two identical blocks (the `edi`/`[esp+0x34]` variants): both trade sides
get their counter bumped and notified. The `0x561860` vs `0x475460`
split (local vs remote/observer update) is unproven.

`rust/src/diplomacy.rs`: `relation_index()` (exact stride math) and the
bump, tested.

## Government/vote dialogs `0x46CF5F` (verified: region sweep)

Gated on `[edi+0x2128]` + `0x46BBF0`; all exits converge on the `0x46DE6C`
epilogue (`call 0x601F20`, the founding-path return helper):

| site | string | dialog shape |
|---|---|---|
| `0x46D03D` | `CHANGE_GOVERNMENT` | push str + `0xCADC18`; `call 0x47A430`; count `[0x9C3DA8]`; owner `[0x9FD4BC]` |
| `0x46D56D` | `NEW_GOVERNMENT_AVAILABLE` | slot-`0x170` dispatch (founding shape) |
| `0x46DA51` | `DIPLOVICTORYVOTEOPTION` | slot-`0x170` + `0x611530` commit; fail → `0x46E3C0` |
| `0x46DB19` | `CONFIRMDIPLOMACY` | slot-`0x170` + `0x611530`; pre: `[0x9FD4BC]`, `0x61C570`, `0x61C5A0` via `[0xA52E98]` |

Second push sites (uniqueness scan): `CHANGE_GOVERNMENT` also `0x55CC41`,
`NEW_GOVERNMENT_AVAILABLE` also `0x4DCA20`, `VOTEOPTION` also `0x4F28E4`,
`CONFIRMDIPLOMACY` also `0x506301` (the R7 diplo dialog below). `0xCADC18`
dialog-owner identity: open.

## Diplomacy/espionage dialogs (verified: region sweep)

One idiom everywhere — `push STR; push 0xCADC18; call [reg+0x170]; call
0x611530` (the NEWCITY founding shape):

* diplo art-init `0x5049D0`: `consider/counter/uparrow.pcx`,
  `diplomacy.txt` via `push 1; push path; mov ecx,0x9C3508; call 0x598580`
  (+`0x64CCC0`, strcpy, `0x5FC820`, vcalls `+0xDC`/`+0xD8`).
* diplo dialog `0x505F40`: `CONFIRMDIPLOMACY` (`0x506301`),
  `NODIPLOMACY` + direct `0x47A430` variant (`0x50652F`).
* espionage dialog `0x5249B0`: `INVESTIGATE_CITY_IMMUNE` (`0x524D8F`),
  `SAFETY_LEVEL` (`0x524261`), `STEAL_TECH_IMMUNE` (`0x525071`); pre:
  `0x55A210`/`0x61C5A0` + `table_backptr` idiom at `0x524A7E`.
* `0x5354B0`: second `NODIPLO` + `0x611530`, `0x4000`-flag prelude.
* `0x504013`: `MISSION_UNAVAILABLE` + slot-`0x170`; `0x52B505` pushes
  `GCON_Espionage_Missions`.

## Deal-response selector `0x517B70` (verified: `r2`)

First anchor in the dark `0x51` bucket. The function calls `0x440EE0`
(HYPOTHESIS: the deal scorer — note the `0x44` bucket also holds the
`Flavor*` strings at `0x4406E5…`) and switches the return value:

| score | site | script key pushed |
|---|---|---|
| 36 (`0x24`) | `0x517BEC` | `DIPLOADVICETRADE_DEAL_ACCEPT` (`0x72A3D4`) |
| 37 (`0x25`) | `0x517C82` | `DIPLOADVICETRADE_DEAL_WEAKREJECT` (`0x72A3B0`) |
| 38 (`0x26`) | `0x517D18` | `DIPLOADVICETRADE_DEAL_NEUTRALREJECT` (`0x72A38C`) |
| 39 (`0x27`) | `0x517DAE` | `DIPLOADVICETRADE_DEAL_STRONGREJECT` (`0x72A368`) |

Any other value falls through (no dialog). Each arm shares one idiom:
`push key; push 1; push [0x72BC58]` (`text\script.txt`); `mov
ecx,0x9C3508; call 0x598580`; `call 0x60E6B0`; then a
`0x60E6D0`/`0x49FC70`/`0x49DCA0(this=0x9AFD98)` scan loop over a
`[0xCAD74C]` cursor (skips `#`, `-1` = end) writing `[0x9B22B0]`.
Open: what computes 36..39 inside/above `0x440EE0`, and the embargo
half of the deal text.

## Deal scorer `0x440EE0` (verified: `r2` + jump-table bytes)

Two-sided item-list evaluator (thiscall, out-params at `[esp+0x48]`,
`[esp+0x5C]`, `[esp+0x60]` zeroed on entry). Each side walks a list
(`[node+4]` = item type 0..10, `[node+8]` = payload, next at
`[head+0x10]`) through an 11-arm jump table; each arm values one item
into `ebp` via a per-type call, then a `call 0x502D40` gate decides a
second accumulator:

* side A (`0x440F7B–0x44134C`, table `0x441AE4`): accumulators
  `[esp+0x28]` (always) and `[esp+0x24]` (gated).
* side B (`0x44136F…`, table `0x441B10`): accumulators `[esp+0x44]` /
  `[esp+0x20]` (same shape, arms unread).

Table-1 arms (verified bytes at `0x441AE4`):

| type | arm | valuation call(s) |
|---|---|---|
| 0 | `0x440F96` | payload sub-dispatch: 0 → `0x4390A0`; 1 → gate `0x501950` + bit-4 test `[edi+esi*4+0xF30]`, value `0x438650`; 2 → `0x5019F0`, value `0x438740` |
| 1 | `0x44102D` | gate `0x501A60`; bit-test `1<<esi` in `[edi+payload*4+0xFB0]`; value `0x4389A0` (HYPOTHESIS: tech — per-civ known-bitmask) |
| 2 | `0x441071` | gate `0x501B80`; bit-test `[edi+payload*4+0x1030]`; value `0x438CB0` (HYPOTHESIS: second bitmask item — resource/map?) |
| 3 | `0x4410B0` | sub-dispatch on payload: 0 → vcall `[edi]+0x5C(esi,0)`; 1 → `+0x5C(esi,1)` minus `+0x5C(esi,0)` (diff shape; one-shot flags `[esp+0x18]`/`[esp+0x19]`) |
| 4 | `0x441121` | `0x4385A0(payload)` — no civ index (HYPOTHESIS: civ-independent item) |
| 5 | `0x441133` | `0x43B540(payload,1,[esp+0x50])` |
| 6 | `0x44114C` | `0x43B730(payload,1,[esp+0x50])` — sibling of type 5 (adjacent code, same shape) |
| 7 | `0x441165` | sub-dispatch: 0 → long computed path (`0x441192…`, flag `[esp+0x1C]`); 1 → literal `ebp=[ebx+0xC` (flag `[esp+0x1E]`) — the only fixed-price arm (HYPOTHESIS: gold amount vs computed Gold-PT) |
| 8 | `0x441282` | vcall `[edi]+0xA4` gate, then `[edi]+0x54` |
| 9 | `0x4412AC` | `0x438DB0` twice (both-sides `0xA52E98` index math) |
| 10 | `0x4412DF` | `0x438F90` twice, `add ebp, eax` |

Side-B loop (`0x44136F…`, table `0x441B10`) mirrors these arms
(unread except its head). Item-type identities: HYPOTHESIS only (see
rows). But the final
sum-compare is solved (`0x44198B–0x4419D6`, `ecx` = side-A total,
`ebp` = side-B total, signed):

| condition | score |
|---|---|
| `A >= B` | 36 ACCEPT |
| `A > trunc(B*7/8)` | 37 WEAKREJECT |
| `A > trunc(B/2)` | 38 NEUTRALREJECT |
| else | 39 STRONGREJECT |

(`0x44199D`: `lea ebp*7; cdq; and edx,7; add; sar 3` = truncating
`/8`; `0x4419BD`: `cdq; sub eax,edx; sar 1` = truncating `/2`.)
Before the ladder, side B is attitude-scaled (`0x441901–0x441931`):
`B *= 4*T+1` where `T = [esi*76 + idx*4 + 0xA5304C]`, `idx` from
`call 0x5010E0` — a table 30 dwords below the `0xA53070` relation
matrix (HYPOTHESIS: attitude/relation table; lookup open).

Two more outcomes bypass the dialog: 40 = invalid/impossible (flag
`[esp+0x13]` at `0x44187C`, MP/owner gates at `0x4418C1`, item gates
at `0x44196B`/`0x441981` — all `jmp 0x441AA3` past the switch), and a
counter-offer trio sharing one shape (flag byte set + callee returns
`!=-1` → write `[esp+0x60]` out-param → `mov ebx,N`): 44 via
`0x5011A0` (`0x4419D2–0x441A12`), 43 via `0x501260`
(`0x441A1D–0x441A52`), 42 via `0x5010E0`
(`0x441A54–0x441A9E`, additionally gated on `[esp+0x24]>0` and
`ebp-[esp+0x20]>0`). Null input lists short-circuit to 38 at
`0x441AD3–0x441ADF` (`mov eax,0x26`, the `0x440F0F`/`0x440F19` exits).

Epilogue `0x441AA3–0x441AD0` (verified): `[esp+0x58] =
[esp+0x28]-ebp` (offer-minus-ask delta), `[esp+0x5C] =
[esp+0x24]-[esp+0x20]` (gated delta), returns `ebx`, `ret 0x28`.
Five `E8` callers of the scorer (child-verified): `0x43BD66` (vetoes
on `0x28` first), `0x507C55` (accept-only → `inc [ebp+0xE9C]`),
`0x517666`, `0x517BCD` (the `0x517B70` dispatcher), `0x51B586`.

## Response switches `0x510730` + `0x5157E0` (verified: heads r2)

* Master (`0x510730`): `cmp eax,0x34; ja 0x515443; jmp
  [eax*4+0x5156FC]` — 53 cases (codes 0–52). Case 0 opens with
  `0x55A270` + `0x61C5A0` (combat-report text slot — consistent with
  the corrected callee role).
* USER (`0x5157E0`): `add eax,-53; cmp eax,0x18; ja 0x515E75; jmp
  [eax*4+0x51612C]` — 25 cases (codes 53–77). Code 53 inlines the
  `USERACCEPT` string copy (`0x51580A–0x51583A`).

Per-row case→string map (child-reported, table dumps + sampled bodies
only): master cases 12/13/30/37–40/48, USER codes 53/54/55/57
verified; the rest rest on dumps. Embargo advisor `0x517EE0` (sole
caller `0x5099E0`, deal-screen modes 5/6) with LAND/WATER route gate
`0x501390`: child-reported, unopened here.

## Random-Nth city pick `0x52121B` (verified: not the combat die)

`rand() % si` → word index into `[edi+edx*2+0x4080]`, validated
against city count `[0xA52E78]` (HYPOTHESIS: random city/unit pick
helper — same family as the `0x5AF703` scans, not combat dice).

`rust/src/diplomacy.rs`: `deal_threshold_verdict()`,
`attitude_scaled()`, `scorer_deltas()`, tested.
