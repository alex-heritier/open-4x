# Unit orders, worker automation, goto (thin)

Owns: order enums/dispatch, auto-worker brain, goto-target enumeration.
Pathfinding itself is still unlocated; this file owns its consumers.
No Rust module yet.

## Orders are numeric: string-negative (verified: byte grep)

Zero hits for `AUTOWORK`/`AUTO_`/`CanBuild`/`BUILD_`/`WORKER`, and the
15 `ORDER` hits are all `DISORDER` (civil disorder), not unit orders.
`SETTLE*` = city-founding/goody strings only. Order buttons are
icon-only; help text lives in data files. The automation brain must be
found structurally (state machine over numeric enums), not via strings.

## Goto-target enumerator `0x4EBC50` (verified: `r2`)

`push GOTO_CITY (0x7295B0); push 0xCADC18; call 0x47A430` — the 5th
`0xCADC18` dialog site (founding/diplo/espionage/culture/goto). Then it
walks the unit array (`[0xA52E6C]`, `ebp*8+4` stride, unit at
`ecx-0x1C`), filters by owner byte `[unit+0x28]` vs `[edx+0x4DBC]`,
and reads tile words `[unit+0x24]`/`[unit+0x26]` (HYPOTHESIS: valid
goto-target list for the city chooser).

Struct-prefix corroboration: the same `+0x24/+0x26` tile words and
`+0x28` owner byte appear in the culture-border site (`economy.md`),
so this is the shared unit/city struct head — the pathfinder's input
shape when found.

## Movement chain: greedy single-step, no A* (sweep verdict)

The pathfinding sweep found **no** open/closed arrays, heap, or
priority queue anywhere: movement is greedy single-step
(validate → classify → step, re-validated per step), with spiral
scans for best-neighbor. Chain (heads verified):

* `0x5C59B0` goto validator (parent-verified head): in-bounds
  `0x426BD0`, same-coord early-out on dword `[esi+0x24]`/`[esi+0x28]`,
  halved cell-index + `0x5D16A0` fetch. 15 callers incl. the `0x5B`
  combat cluster.
* `0x5F3F50` direction classifier (child-reported): dims `+0x168`/
  `+0x154`, **unconditional** toroidal delta fold (no `+0x1F0`
  wrap-flag test). 19 callers.
* `0x5E6E50` spiral-offset oracle (child-reported, 325 callers):
  ring via odd squares, index 0 → (0,0).
* `0x5BA1D0` neighbor best-pick stepper (child-reported): sentinel
  `0x31` (7x7 spiral), spiral loop + wrap + bounds.
* `0x4DC2A0` order-mode dispatcher (child-reported): range pre-check,
  `jmp [ecx*4+0x4DC448]` over mode table.
* Meter gates funnel to `ALREADYMOVED` (`0x5C651E`/`0x5C66A6`) on
  `meter <= 0` (9999-scale `0x5BE470` compare).

Open: per-terrain move-cost reads (`0x5BA010` head opened: pushes
unit coords + args into map query `0x44A8D0(this=0x9C736C)`,
action-record `+0x9C==1` gate, `[esi+0x64]` in {1,15} gate, cost
vs 3 — body past `0x5BA070` unopened; `0x5CD960`/`0x55AD90`
unopened), ZOC rule site, road/rail fractions (no fixed-point
constants found — possibly small-int `idiv` or cell-vfunc
data-driven), waypoint store shape (`game[0x2E158…]` list
HYPOTHESIS).

## Action dispatch `0x5C2400` (parent-verified head)

128-entry order dispatch: `cmp edi,0x40000100` (magic order id) →
`add edi,0xBFFFFFFF` (bias to 0-based) → `cmp 0x7F` (else
`0x5C1C60`) → byte-index `0x5C2CD4` → `jmp [eax*4+0x5C2CB0]`.
First arm calls `0x5C1AD0` (founding gate — founding is order 0);
second arm calls `0x5BC6D0` + reads `[esi+0x40]`.

Jump table `0x5C2CB0` (9 entries, bytes verified; order ids sparse —
0/1/3/7/15/31/63/127 hit arms 0–7, order 2 hits the default):

| arm | order | entry | head shape |
|---|---|---|---|
| 0 | 0 | `0x5C2430` | `0x5C1AD0` founding gate |
| 1 | 1 | `0x5C2443` | `0x5BC6D0` + `[esi+0x40]` record |
| 2 | 3 | `0x5C2535` | `call 0x561440` (treasury rollup) |
| 3 | 7 | `0x5C25B1` | `call 0x561440` (treasury rollup) |
| 4 | 15 | `0x5C260C` | falls into arm-5 tail (`jmp 0x5C26A5`) |
| 5 | 31 | `0x5C261D` | straight-line (deeper, unopened) |
| 6 | 63 | `0x5C26DB` | cell fetch `0x5D16A0` + text slots |
| 7 | 127 | `0x5C27BA` | `push 0x20000800; call 0x5C1AD0` |
| 8 | 2 (default) | `0x5C1C60` | `0x5C4F60` + `0x5BC8B0` validator |

Sibling magic-id compares (`0x40002000`, `0x40000800`) follow at
`0x5C27FE+`. Arm bodies past heads + order-id identities: open.

## Build actions (child-reported, byte-evidenced)

* Table `0x680758`: 19 build-action dwords, consumed by cost
  evaluator `0x5C66D0` (solo caller of the table reader).
* Cancel handler `0x5C6290`: walks the build queue, refunds via
  `0x5C1AD0`-adjacent accounting (bodies sampled, not fully opened).
* Per-action workers past the dispatch arms: open.

## Leads (open)

* Build costs/times come from the `TERR` data path (`NOTES.md` §13);
  the auto-worker *job scorer* and *route-to* planner are unlocated.
* `IRRIGATE` (`0x6807BC`)/`FORTIFY` (`0x680804`) tags have zero
  push-imm refs (data-file tags, not code markers).
* Terrain improvement art refs cluster `0x4C6127–0x4C69B6` (renderer).
