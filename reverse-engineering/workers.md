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
* `0x5BA1D0` is **not** a pick stepper: it is the unit sight refresh (`vision.md` 6); the "sentinel `0x31`" is the 49-tile
  (7 x 7 diamond) spiral bound.
* `0x4DC2A0` order-mode dispatcher (child-reported): range pre-check,
  `jmp [ecx*4+0x4DC448]` over mode table.
* Meter gates funnel to `ALREADYMOVED` (`0x5C651E`/`0x5C66A6`) on
  `meter <= 0` (9999-scale `0x5BE470` compare).

Meter gate, instruction-verified (2026-09-29). Both `ALREADYMOVED` sites share
one shape (`0x5C64EB` and `0x5C667F` are the two instances):

    mov edi, [unit + 0x50]       ; consumed this turn
    mov ecx, esi ; call 0x5BE470 ; eax = budget
    sub eax, edi
    js  <ALREADYMOVED>           ; negative
    cmp eax, 0x270F ; jg <can-move>
    test eax, eax ; jne <can-move>   ; zero -> ALREADYMOVED

So the gate is `0x5BE470(unit) - [unit+0x50] <= 0`, 9999 is the full-turn
scale, and `[unit+0x50]` holds what has been *consumed*, not the budget.

`0x5BA010` **body now walked (2026-09-29)** — it is a **step-legality
test, not a cost table**:

* `+0x40` = the unit's *type* index → the PRTO row via
  `[[0x9C71E0] + type*312 + 0x9C] == 1` (`lea edx,[eax+eax*4]; shl edx,3;
  sub edx,eax; …[eax+edx*8+0x9C]` = stride `0x138` from `biq.md`), i.e. a
  unit-domain test; `+0x64` ∈ {1, 15} is the order/action id; `0x9C736C`
  is the world object.
* cell index uses the halved convention: `cell = (W>>1)*y + (x>>1)` with
  `W = [0x9C74D4]` (`sar eax,1` then `imul eax,y`, `shr di,1`).
* target cell = `0x5D16A0(cell)`, then `vfunc(0xB8)` (the region id at
  `cell+0x1E`, see `NOTES.md` §18.7) of target **and** of the unit's own
  cell: equal ⇒ return 1 (same region ⇒ move allowed).
* otherwise the map query result must be `<= 2` (water/coast class) or the
  unit-type lookup at `0x5BC6D0` decides.

So per-terrain *costs* are not read here; the cost source is still open
(candidates: `0x5CD960`, `0x55AD90`, the TERR tail, or a cell-vfunc). ZOC
site, road/rail fractions and the waypoint store shape are unchanged.

The two remaining candidates were opened on 2026-09-29 and are *not* cost
tables either:

* `0x5BC6D0` (called by `0x5BA010`) = bounds + occupancy legality: order
  gate `0x5BC8B0(0x12)`, `0 <= x < [0x9C74D4]`, `0 <= y < [0x9C74C0]`,
  cell = `(W>>1)*y + (x>>1)`, `cell->vfunc(0xA0)(&out)`, then the result
  through a table lookup `0x426C80(0xA52DD4, …)`.
* `0x5CD960(terrain_id)` = the **first identified consumer of the TERR
  memory row's unmodelled tail**: `base = [0x9C7328]`, `stride = id * 240`
  (the `0xF0` TERR row stride of `biq.md`), and it tests the **byte at
  `row + 0x7A`** for zero, branching to the legality test otherwise. That
  byte is now mapped to the file: for TERR rows, **memory offset − 4 = file
  body offset** (name: mem `+0x0C` ↔ body `+0x08`, key: `+0x2C` ↔ `+0x28`,
  confirmed by the reader's `fread 0x20` into `[ebp+0xC]` at `0x5E93D1`), so
  `row+0x7A` = **body `+0x76`**, which is `1` for every land terrain except
  Volcano and `0` for Volcano/Coast/Sea/Ocean — an **is-land/enterable
  flag**, the value `0x5CD960` branches on. Neighbours in the tail:
  body `+0x74`/`+0x75`/`+0x77` are `1` for the nine land terrains *excluding*
  Marsh, and `+0x78` is `3` for every row. The movement *cost value* itself
  is still unfound.
* `0x55AD90` is the unit-sight set primitive (`cell +0x5C |= 1 << slot`, then `discover`), `vision.md` 1.2; it only
  looks like a cell-index helper because it computes the same index inline.

**Per-terrain movement cost and defense bonus: located (2026-09-29).** The
TERR row carries both as `u32` fields, and the decoded `conquests.biq`
matches Civ3's published table exactly:

| field | memory | file body | Dst | Pln | Grs | Tun | Fld | Hil | Mtn | For | Jng | Msh | Vol | Cst | Sea | Ocn |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **movement cost** (`u32`) | `+0x5C` | `+0x58` | 1 | 1 | 1 | 1 | 1 | 2 | 3 | 2 | 3 | 2 | 3 | 1 | 1 | 1 |
| **defense bonus** (`u32`) | `+0x58` | `+0x54` | 10 | 10 | 10 | 10 | 10 | 50 | 100 | 25 | 25 | 20 | 80 | 10 | 10 | 10 |
| *8.8 copies* (`cost<<8` / `def<<8`) | `+0x98`/`+0x9C` | `+0x94`/`+0x98` | | | | | | | | | | | | | | |

Both are `u32` fields the loader reads with `fread 4` (`0x5E9488` ff, into
`+0x50`/`+0x54`/`+0x58`); a *second*, 8.8 fixed-point copy of each sits
later in the row, which is why a byte-wise read at body `+0x94`/`+0x98`
shows the same numbers shifted one byte. The defense column is a
percentage: mountains +100 %, hills +50 %, forest/jungle +25 %, marsh
+20 %, volcano +80 %, everything else +10 %. Neighbours
in the same block: body `+0x50` = the land flag (1 land / 0 water), the
`u32` block `+0x48`/`+0x4C`/`+0x60`/`+0x64` holds further per-terrain
enums (e.g. `+0x4C` = 2 on hills/mountains, `+0x60` = 3 on volcano), and the
tail carries the two strings. The *reader* functions for the cost and
defense fields were not located
(only `TERR`-row accessors `0x5E8C70`/`0x5E8C80`/`0x5E8C90` → `+0x88`/`+0x8C`/
`+0x90`, `0x5E8CD0`/`0x5E8CE0` → `+0xA0`/`+0xA4`, `0x5E8CF0` → string
`+0xA8`, `0x5E8D00` → name `+0x0C`, `0x5E8D10` → `+0xE8` bit 1); probe: break
on a movement decision and dump the TERR row base as it is read.

**Order-id structure and issuers (2026-09-29).** Every order goes through the
gate `0x5C1AD0(unit, id)` (104 call sites) with a *packed* id:

* `0x1000_0001` … `0x1000_0100` are `0x10000000 | (1 << n)` — one bit per
  unit action. The 57 sites in the map-view button code (`fn 0x4D73F0`) test
  exactly one bit each, which is what makes the family readable; the action
  index `n` follows the `#UNIT_ACTIONS` cell order of `labels.txt` (the same
  mapping the clone's action bar uses).
* `0x2000_0002` = **found city** (issuers `0x46A9A8`, `0x470D02`, and the
  UI's `0x4DF360`), with `0x2000_0800`, `0x2000_1000`, `0x3000_0010` beside
  it.
* `0x4000_0001/2`, `0x4000_1000/2000` = a third family (`fn 0x54EE20`).
* The PRTO row's own order/action bits are now mapped (`editor.md` "Units
  page"): `+0x8c` AI strategies, `+0xa8` standard orders, `+0xac` special
  actions, `+0xb0` worker actions, `+0xb4` air missions. Use those when
  reading why a unit type may issue an order at all.
* single-id AI issuers observed: goto `4` at `0x4225FB` (after
  `[unit+0x64] != 1`), `0x1000_0008` at `0x44B058` (behind the owner
  capability bit), `0x1000_0100` at `0x41C850`/`0x466CC8`/`0x446840`/
  `0x56E420`/`0x5694D0`.

`0x5BE470` is the budget provider, not a plain getter: order gate
`0x5BC8B0(0x12)`, `0x426BD0(0x9C736C, x, y)` on `[unit+0x24]`/`[unit+0x28]`,
then the cell's `vfunc(0xA0)`, a lookup of that key in `0xA52DD4`, an index
into `[0xA52E84]`, an owner test `[obj+0x60] == [unit+0x20]`, and a
**recursive self-call** (`0x5BE51F`, `0x5BE52E`) whose result it *minimises*.
Name the recursion's list before trusting any number read from it.

**Verified negatives (2026-09-29), so the next session does not repeat them:**

* No instruction anywhere in `.text` loads the cost/defense bytes with an
  addressed displacement — the scan covered `mov r8,[reg+disp32]`,
  `movzx`/`movsx r32,r/m8` and `r/m16` forms for disp `0x95`/`0x99`/`0x9d`
  (all ModRM mod=10 paths, SIB included) and found zero sites, and the TERR
  accessor family only covers `+0x88`/`+0x8C`/`+0x90`/`+0xA0`/`+0xA4`/
  `+0xA8`/`+0x0C`/`+0xE8`. So the two bytes are consumed through a
  register-held offset (or copied out at load); a live watch on
  `[row+0x99]`/`[row+0x9D]` is the way to name the readers.
* No ZOC strings exist in the image (`grep -i zoc` / "zone of control" over
  `allstr.txt`: zero hits), so the rule has no log/diagnostic site to anchor
  on.
  Explained 2026-09-29: ZOC is not a movement rule at all — it is a PRTO
  unit ability, labelled `Zone of Control` on the editor's Units page
  (dialog 151, control 1535, see `editor.md`). The label exists only in the
  editor's dialog resource, which is why the game image has no such string.
  The gate to look for is a test of that ability bit, not a rule lookup.
  Update 2026-09-29: the PRTO AI-strategy dword (`row+0x8c`) is now fully
  bit-mapped from the editor on one side and the game on the other
  (`editor.md` "Units page"), and bit 13 there is **`Settle`** — so ZOC is in
  a *different* PRTO flag source, not that dword.
* Road/rail movement fractions are **not** an inline `/3`: the movement
  region `0x5B0000`-`0x5C8000` contains no `imul …,3`, no `idiv 3`, no
  `0x55555556` magic and no `0xD05` (9999/3) immediate; the 9999 meter
  (`0x270F`, ~45 sites) is the scale, and the fraction is presumably applied
  through the cell/unit flag data, not a constant.
  Extended 2026-09-29: `0xD05` (3333) has **no immediate anywhere** in
  `.text` (the four byte hits at `0x64214E`, `0x654014`, `0x654061`,
  `0x65406E` are fragments of longer instructions), `0x1A0A` (6666) and
  `0x752D` (29997) do not occur in the file at all, and the ±9999 sentinel
  idiom (`and r,0x4E1E` + `add r,-9999`) has only three sites — `0x521A5F`,
  `0x55BEA3`, `0x561653`, none in the movement region. The `div`/`idiv`
  sites inside `0x5A0000`-`0x5D0000` are RNG accumulators (e.g. `0x5BE07D`
  `div 0xD431`, `0x5BE084` `sub edx,0x8235`), not cost fractions. So the step
  cost is a **lookup**, not a scaled constant — consistent with the
  "no cost table in `0x5BA010`" verdict above. Resolved 2026-09-29: the
  road multiplier is **RULE data**, named `Movement Rate Along Roads:` on
  the editor's General Settings page (dialog 131, label 1031, edit 1032,
  spin 1033 — see `editor.md`). So there is no fraction reader inside the
  movement code to find; what is still missing is the RULE row offset.

Road/rail movement and ZOC are no longer "unlocated code": the road
multiplier is a RULE field (`Movement Rate Along Roads`) and ZOC is a PRTO
unit-ability flag, both named from the editor's property pages
(`editor.md`). Still open: the RULE row **offset** for the road multiplier,
the PRTO **bit index** for `Zone of Control`, the waypoint store shape, and
the AI auto-worker *job scorer* / *route-to* planner.

Waypoint store, dead end recorded 2026-09-29: the candidate the AI goto
issuer `0x4225CF` calls, `0x619E70` (capacity check `[obj+0x2184] < 0x100`),
allocates `push 0x7c` (124 B) and `repne scasb`-copies a **C string** into an
array slot (`[ebp + edx*4]`) — it is a 256-entry *string* table, not a path
node. The other `0x5C59B0` callers (`0x45AEC0`, `0x45B4BA`, `0x5BC07F`,
`0x5BD5E5`, `0x5C08D5`, `0x5C5821`) pass an explicit `(x,y)` from the stack,
so on those paths the destination is an argument; the store must be found
where a multi-turn destination is *persisted*, not where it is validated.

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
