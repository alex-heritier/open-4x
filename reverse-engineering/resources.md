# Resource placement (`0x5f22a0`)

Stage 10 of `generateMap` (`0x5eb580`). Companion to `NOTES.md` §11.8, which
specifies the algorithm; this file adds the data path, the `.biq` record
layout it consumes, and what a clone needs to reimplement it.

## Data path (verified this session)

A raw scan of `.text` for the two 4-byte tags finds only:

* `GOOD` (`0x444F4F47`): 19 refs — `0x4E54C4`, `0x5943BC`, `0x59746C`,
  `0x599632`, `0x5EEF64`, `0x5EEF85`, `0x5EEFF2`, `0x5F22B0`, `0x5F23BB`,
  `0x5F28DD`, +9 more.
* `TERR` (`0x52524554`): 16 refs — `0x594960`, `0x597B48`, `0x599A57`,
  `0x5E9A23`, `0x5E9A83`, `0x5ED76E`, `0x5ED92E`, `0x5F242B`, `0x5F2450`,
  `0x5F28DD`, +6 more.

No `RIVR`/`RIVE` tag exists anywhere in the generator range, confirming
`NOTES.md` §14: the only list data map generation reads is `GOOD` and `TERR`.
Disassembly at `0x5F22A0` opens with `push 0x444F4F47; push -1; call
[eax+0x8C]` — the count query — so the stage enumerates the `GOOD` list first
and the `TERR` list per resource.

`0x599600` is the 4-char-code dispatcher both stages share:
`getListEntry(tag, idx, &out)`, `idx = -1` returns the count.

## Records consumed

| list | field | meaning |
|---|---|---|
| `GOOD` +`0x40` | `int` frequency | per-resource abundance; `0` means roll `rand_int(26)+rand_int(26)+50` |
| `TERR` +`0x08` | pointer to byte array | bit `g&7` of byte `g>>3` = resource `g` may appear on this terrain (`0x5F2470`) |
| both | class predicates | `0x5E3700` / `0x5E3730` (`== 2`) / `0x5E3720` gate the three blocks |

Scenario-loader strings confirm the records come from the `.biq`:
`[Scenario::loadPRTO()]` / `[Scenario::loadTERR()]` version-setup logs and
`BIC_ERROR_TERR` (`0x732E08C` region). The `.biq` supplies definitions, never
placement rules.

## GOOD section layout (decoded `conquests.biq`, `biq.md`)

The `GOOD` section sits at file offset 29 052 of the inflated stream:

| fact | value | source |
|---|---|---|
| row stride | `0x5C` (92 B) | loader loop `add ebx,0x5C` at `0x5974B4` |
| row count source | global at loader `+0x89C` | loop bound `0x59749F` |
| per-row reader | call `0x5E3740` per row | `0x5974A8` — it is the row *reader*, not a validator (corrected: it issues the `0x64AF35` field reads below) |
| freq field | row `+0x40` | read by `0x5E3740` (`lea ecx,[edi+0x40]`, `0x5E37B6`) and by placement (`NOTES.md` §11.8) — doubly confirmed |
| u16 at section `+4` | `0x1A` (26) | consistent with a row count; the loader loop bound is authoritative, not this field |

`0x5E3740` memory layout per row (all `0x64AF35` reads into `edi`):

| memory offset | size | role |
|---|---|---|
| `+0x04` | 24 B | string/blob (unmapped: name or binary) |
| `+0x1C` | 32 B | string/blob (unmapped) |
| `+0x3C` | u32 | unmapped |
| `+0x40` | u32 | **frequency** (confirmed) |
| `+0x44` | u32 | unmapped |
| `+0x48` | u32 | unmapped |
| `+0x4C` | u32 | unmapped |
| `+0x50` | 12 B | unmapped |
| total | `0x5C` | ends exactly at the stride — layout closed |

`0x64AF35` is a flat `fread(dst, count*size)` (via `0x64AF64`: `imul`
count×size, clamped read) — no length prefixes there. But fixed-stride
slicing still fails: a 3×3 grid search over stride {88, 92, 96} × header
{4, 6, 8} peaks at stride-92/header-8 with only 11/26 sane freqs and
names drifting across rows (`H`, `Ir`, `Saltp`, `Oi`, `Rubb…`, `Aluminum…`,
`Gold…`, `Gems`, `Whea…`, `Incens…` — unmistakably the resource list, at
wandering offsets). Drifting offsets with flat field reads means
variable-length data sits *between* the fixed reads: the interleaved
`0x64C2F8` / `0x64C47B` calls (whose worker `0x64C4A7` dispatches on
`[esi+0xC] & 0x83` and arg ∈ {0, 1, 2}) read the variable parts — likely
Pascal strings. So file rows are variable-length; memory rows are the
fixed `0x5C`. `0x64C31A` is a dual-mode cursor skip: if `[edi+0xC] & 0x108 == 0` it
returns a binary byte delta (`bytes_read − [edi+4]`); else it counts `\n`
newlines through a ctype-like table (`0xCCF9A0`, bit `0x80` test per
byte) — a text mode serving the game's text/ini files through the same
reader. `0x64C4A7`'s arg-1 path adds that count to the read length.
Which mode the BIQ stream uses (the flag word is per-open state) is the
remaining unknown — one flag bit decides whether GOOD rows contain
variable-length text fields at all. Until then the GOOD×TERR matrix for
clones stays hardcoded (as the clone does today).

## Names live outside the BIQ (verified)

Full resource names appear nowhere in the decoded BIQ (only `Aluminum`,
`Gold`, `Fish` surface as fragments; the rest — Saltpeter, Horses,
Cattle, Wheat, Coal, Uranium, Rubber — zero hits). The canonical 26
names come from `Conquests/Text/PediaIcons.txt` (`#ICON_GOOD_*`, 26
entries, alphabetical: Aluminum … Tobacco), each pointing at per-resource
civilopedia icons — not at map-sheet cells. GOOD rows carry short keys
only; matching row `g` to a name needs the variable-length parse above.

## TERR file rows: same variable-length story (verified)

The `TERR` section at inflated offset 195 819 opens with a plausible
`0x0E` (14 terrains?) but its rows are fragmented short keys and binary
(`…se W_se…`, `WetH`, repeated `Ba.g` / `nnHH` filler-like quads) — same
variable-length framing as `GOOD`, same unmapped field boundaries. The
memory-side contract is what placement actually reads and is verified
(`[row+8]` pointer + `g&7`/`g>>3` bit test, `0x5F2470`); file-side TERR
parse needs the same `0x64C4A7`-path analysis as GOOD.

## Icon sheet order: unverified

`Art/resources.pcx` is 300x300 px with a 6x6 grid of 50 px cells (36 cells,
magenta gutters every 50 px) — it does **not** trivially match a 26-row
`GOOD` list, so whether `GOOD` index `g` addresses sheet cell `g` in
row-major order is **unverified**. Deciding it needs the `GOOD` row name
field (row layout beyond stride/`+0x40` is unmapped) or the renderer that
blits the sheet. Do not assume `g` = sheet cell.

## Quantity math (from `NOTES.md` §11.8, restated for implementers)

```text
pct = good.freq != 0 ? good.freq : rand_int(26) + rand_int(26) + 50
n1  = (map.area_factor * pct) / 32          # area_factor = this->[0x15C]
score = count of TERR rows allowing g (+4 each for t >= 11)
n = score<2 ? n1*0.5 : score<4 ? n1*0.75 : n1
n = max(n, score>=4 ? 2 : 1)
```

Implementation notes from the block-2 disassembly (`0x5F2464`):

* The bit array is a **pointer**, not inline bytes: `eax = [TERR_row + 8]`,
  then `test byte [eax + (g sar 3)], 1 << (g & 7)` (`and ecx,7` /
  `sar edx,3` / `shl ebx,cl`, `0x5F2479`). A `TERR` list query
  (`vfunc 0x8C` with the tag) precedes each row fetch.
* Score rule confirmed instruction-level: `inc ebx` per allowing row,
  plus 4 when the row index `t >= 11` (`cmp edi,0xB`, `0x5F2490`).
* The 0.5/0.75 factors are `float` constants at `0x6653B4` / `0x6707C8`
  (`fmul` after `fild`); conversion via `0x64A230` sets x87 RC=truncate,
  so Rust's `as i32` truncation matches exactly.

Block 3 **skips** each candidate when `rand_int(score<2 ? 6 : score<4 ? 4 : 2) > 1`
(`cmp ax,1; ja 0x5F2C06` = loop advance), i.e. it places on `roll <= 1`:
33 % / 50 % / 100 %. It repeats until `numPlayers >> 5` further placements
land — a no-op guard in normal games (see the `>= 32` note in `NOTES.md` §11.9).

Placement invariant: all copies of one resource share one `vfunc(0xB8)`
region id. Write is `tile->vfunc(0xEC)(g)`.

## Reference implementation

`rust/src/resources.rs`: `Good`/`Terr` records, `quantity()` (exact integer
math above), block-3 acceptance probabilities, region-equality check.
Fully specified stages `placeGoodyHuts`/`placeBarbarianCamps` are there too
(no `.biq` data needed).

## Alternate GOOD accessor `0x5D90D0` (verified: region sweep)

`0x4E54C3` pushes `'GOOD'` (`0x444F4F47`) with `ecx=0x9C736C` and calls
`0x5D90D0` directly — not the `0x599600`/vfunc-`0x8C` path documented
above — then reads `GOOD` row `+0x48` (unmapped u32). Preceded by index
math onto table `0xA52E98` gated by `0x561440`. The two accessors'
relationship: open.

## Open

* UI meaning of the three class predicates `0x5E3700/30/20`.
* Faithful `GOOD`/`TERR` rows await the `.biq` framing fix (`NOTES.md` §15.4).
