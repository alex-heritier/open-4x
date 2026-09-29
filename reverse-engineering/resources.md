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
| per-row reader | call `0x5E3860` per row | reader loop `0x594597`; flat `fread`s (`0x64B3E3`) into `+0x04/+0x1C/+0x3C…` (verified destinations). `0x5E3740` is the WRITER's row function (loop `0x59749F`, `fwrite`s via `0x64AF35`) — formerly mislabeled as the reader |
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

**CORRECTION (verified live): `0x64AF35` is `fwrite`, not `fread`.**
`0x64AF64` calls `memmove(streambuf ← userbuf)` (`0x6517d0` =
memmove with overlap check); its twin `0x64B3E3`/`0x64B412` calls
`memmove(userbuf ← streambuf)` = the real `fread`. Direction was proven
by memmove operand order, then confirmed live (writer opens `"wb"`).
Every `0x64AF35` call below is a WRITE (memory → file). The Pascal-string
/ text-mode theory for GOOD is dead: the GOOD row reader (`0x5E3860`)
does flat `fread`s only, and the live GOOD stream used the binary arm
(`[stream+0xC] & 0x108 == 0`, flags read live as `0x0A`). The
`0x64C31A` dual-mode cursor analysis stands as written (it serves text
files elsewhere); it is simply not on the GOOD path.

Slicing history (superseded): a 3×3 grid search over stride {88, 92, 96}
× header {4, 6, 8} on conquests.biq-inflated peaked at stride-92/header-8
with only 11/26 sane freqs and drifting names. That failed because
conquests.biq's GOOD section genuinely contains NO name strings (see
below) — not because of variable-length framing. Memory rows are the
fixed `0x5C`; PTW-format file rows are `[u32 len=88][88B data]` (see
`## Reader/writer split`).

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

## Stream layer identified: `0x64C47B` = fseek, `0x64C2F8` = ftell

* `0x64C2F8(stream)` = lock + `0x64C31A` + unlock. `0x64C31A` calls
  `0x651238([stream+0x10], 0, 1)` — `lseek(fd, 0, SEEK_CUR)` on the MSVC
  `FILE` fd at `+0x10` — minus `[stream+4]` (`_cnt`, buffered-but-unread)
  on the binary path. Text-mode (`[stream+0xC] & 0x108`) newline counting
  is the other arm. BIQ streams open `"rb"` (`0x5942C4`), so the binary
  arm applies.
* `0x64C47B(stream, N, whence)` = fseek. `whence ∈ {0,1,2}` else
  `errno = 0x16` (EINVAL); `SEEK_CUR` adds `ftell` first (`0x64C4D0`);
  tail is `lseek` via `0x651238` → `SetFilePointer` (`0x6512C7`). The
  earlier "dispatches on `[esi+0xC] & 0x83`" note was the readable-stream
  validation, not a format switch.

## GOOD rows: exact writer, paradoxical file (verified: `r2` + probes)

(This section describes the WRITER, `0x597070`/`0x5E3740` — formerly
mislabeled the reader. All transfers are `fwrite`s.)

`0x5E3740` emits exactly 92 flat bytes per row: `ftell`, then fwrites of
4 (stack — the length word, see below) + 24 (from `+0x04`) + 32 (from
`+0x1C`) + 5×u32 (from `+0x3C..+0x4C`, freq at file+64) + 12 (from
`+0x50`), then `ftell`, `fseek(pos1,SET)`, a 4B re-read into a dead stack
temp, `fseek(pos2,SET)` — position-neutral. All sizes are immediates;
the loop at `0x59749F` does no file I/O of its own.

But no fixed stride slices conquests.biq-inflated: strides 92 and 96 fail
for every header 0–48 (best 11/26 sane freqs), no header 0–300 aligns all
26 name blobs, and the GOOD section spans 29052→36036 (`RULE` next) =
6984 bytes ≈ 2.9× the 2392 of 26×92. Cause identified: conquests.biq's
GOOD section contains NO name strings at all (byte search over the full
209222B inflated image: zero hits for `Horses`, wide, Pascal, lower- or
upper-case variants). Its framing after `GOOD` is u16-based
(`1a 00 2c 01 58 00 …` = 26, 300, 88, …), not the u32/len-88 rows the
reader consumes. No Conquests `.biq` (all 9 scenarios + MP/scenario
variants + `civ3mod.bic`, inflated) contains `Horses`.

## Live GOOD rows: full memory dump (winedbg, EGYPT.SAV load)

Break `*0x5974BB` (row-loop exit) during the EGYPT load: count
`[0x9C3508+0x89C]` = 26, rows at `[0x9C3508+0x3CCC]` = `0x066BFFC8`,
2392 bytes dumped. Order, names (`+0x04`), freq (`+0x40`), category
(`+0x3C`: 2 = strategic, 1 = luxury, 0 = bonus):

Horses 160, Iron 160, Saltpeter 120, Coal 120, Oil 120, Rubber 120,
Aluminum 120, Uranium 100, Wines/Furs/Dyes/Incense/Spices/Ivory/Silks/
Gems 0, Whales/Game/Fish/Cattle/Wheat/Gold/Sugar/Tropical Fruit/Oasis/
Tobacco 0. `+0x48` mirrors the row index except Sugar 24 / Tropical
Fruit 22 / Oasis 23 (an ordering id, not the index). Luxuries and bonus
all carry freq 0 (the `freq != 0 ? freq : roll` fallback decides them).

The names-in-stream paradox (refined): the reader's 24B `fread`
provably targets `+0x04` (`lea eax,[esi+4]`, `0x5E3880`) and the 32B key
`fread` targets `+0x1C` (`0x5E389C`) — all flat, no string-table join in
the row function — so the reader's stream contained the full names. But
`Horses` appears in NO Conquests file on disk: not conquests.biq
(raw/inflated), not any of the 9 scenario `.biq`s, not `civ3mod.bic`,
not EGYPT.SAV raw, not save0.tmp (game's own 1748113B decompression),
not the raw autosaves, not the exe. Only the 4 uncompressed PTW `.bix`
files carry named GOOD rows (29/42-row custom sets — not the live 26).
Civilopedia is ruled out as the join source (`Silk/Spice/Wine` singular
vs live `Silks/Spices/Wines` plural). The 26-row named source is
unidentified; the decisive probe is a fresh load with breakpoints on the
reader's `fopen` (`0x5942CA`, path in `EBX`) and GOOD loop exit
(`0x5945B3`).

Open threads, in order:

1. Identify the GOOD stream: fresh-load trace with `*0x5942CA` (reader
   fopen, path in `EBX`) + `*0x5945B3` (GOOD rows filled) — in progress.
2. ~~Allocator `0x59BD90`~~ CLOSED: frees old `[ebp+0x3CCC]`, mallocs
   `((3×count)<<3 − count)<<2` = exactly 92×count
   (`lea eax,[edi+edi*2]; shl 8; sub edi; shl 2`, `0x59BDD2–0x59BDDB`).
3. conquests.biq's 6984B GOOD section (29052→36036): u16-framed,
   nameless — which consumer reads it, and does the Conquests exe ever
   feed it to `0x5E3860` (whose flat name reads would produce garbage)?
   Possibly editor-only data.
4. A TERR allow-matrix value for at least one known tile (pick a tile from
   a save and read its row's matrix bits out of the process).

## Open

* GOOD 26-row named source file (see paradox above; fresh-load trace
  with `*0x5942CA` in progress).
* UI meaning of the three class predicates `0x5E3700/30/20`.
* A TERR allow-matrix value for at least one known tile (pick a tile from
   a save and read its row's matrix bits out of the process).

## Reader/writer split (verified: `r2` + winedbg + probes)

The scenario codec has two mirrored halves. Prior notes attributed
everything to one "loader"; the split was proven by memmove operand
order + a live `"wb"` fopen + a live writer-skip.

| side | function | stream mode | transfer | row function | GOOD gate |
|---|---|---|---|---|---|
| reader | `0x594290` dispatch (`cmp eax,'GOOD'`, `0x5943BB` → arm `0x59454B`) | `"rb"` (`0x5942C4`) | `fread` = `0x64B3E3` (565 callers, `0x594318+`) | `0x5E3860` (flat reads into `+0x04/+0x1C/+0x3C…`, one `fseek` skip) | bit 7 of `[ebx]` (`0x59454D`); sets `[esi+0x848] |= 0x80` after |
| writer | `0x597070` (`"Saving version %d.%02d BIC file"`, `0x59712A`) | `"wb"` (`0x5970A8`, path e.g. `bic__out.tmp`) | `fwrite` = `0x64AF35` | `0x5E3740` (flat writes from row, `ftell`/`fseek` dance) | bit 7 of `[ebx]` (`0x59745A`); skip jumps to `0x5974BB` |

Writer/reader tells: the writer presets each transfer buffer
(`mov [esp+NN],'TAG'` / count) before the call; the reader never presets
and `cmp`s the tag after. The writer-skip was caught live: `EBP` still
held the path pointer (`0x72DA08` = `"bic__out.tmp"`, via global
`0x72D9CC`) at `0x5974BB`, proving `xor ebp,ebp` never ran (this also
explains the earlier "EBP should be 26" anomaly: the breakpoint was hit
via the skip jump, not the loop exit). Caller chain (stack): `? →
0x59A760 → 0x597070`.

File-row framing (PTW/raw BIC, ground truth from
`Ancient Mediterranean.bix`): `[GOOD][u32 count][rows…]`, each row
`[u32 len=88][24B name][32B key][u32 cls][u32 freq][u32 a][u32 b][u32
c][u32 d][8B tail]`. The reader loads `len` into row `+0x00`, subtracts
each `fread`'s byte count, conditionally reads the trailing 12B
(`cmp [esi],0xC; jb`), and `fseek`s past the remainder
(`fseek(remaining, SEEK_CUR)`, `0x5E396A`) — so longer file rows are
skipped cleanly. Live EGYPT rows ended with head `0` (len == 88 ==
consumed). The `.bix` GOOD parses 29/29 with this framing
(`Tin/Copper/Papyrus/Elephants…` custom set — not the live 26).

Live GOOD table (26/26, second capture identical to the first; rows at
`[0x9C3508+0x3CCC]`, count `[0x9C3508+0x89C]` = 26):

| # | name (`+0x04`) | key (`+0x1C`) | cls `+0x3C` | freq `+0x40` | a `+0x44` | b `+0x48` | c `+0x4C` |
|---|---|---|---|---|---|---|---|
| 0 | Horses | GOOD_Horses | 2 | 160 | 0 | 0 | 4 |
| 1 | Iron | GOOD_Iron | 2 | 160 | 800 | 1 | 7 |
| 2 | Saltpeter | GOOD_Saltpeter | 2 | 120 | 800 | 2 | 30 |
| 3 | Coal | GOOD_Coal | 2 | 120 | 400 | 3 | 44 |
| 4 | Oil | GOOD_Oil | 2 | 120 | 200 | 4 | 53 |
| 5 | Rubber | GOOD_Rubber | 2 | 120 | 0 | 5 | 57 |
| 6 | Aluminum | GOOD_Aluminum | 2 | 120 | 400 | 6 | 64 |
| 7 | Uranium | GOOD_Uranium | 2 | 100 | 100 | 7 | 65 |
| 8–15 | Wines Furs Dyes Incense Spices Ivory Silks Gems | GOOD_Wine GOOD_Furs GOOD_Dye GOOD_Incense GOOD_Spice GOOD_Ivory GOOD_Silk GOOD_Diamonds | 1 | 0 | 0 | 8–15 | -1 |
| 16–25 | Whales Game Fish Cattle Wheat Gold Sugar Tropical Fruit Oasis Tobacco | GOOD_Whales GOOD_Game GOOD_Fish GOOD_Cattle GOOD_Wheat GOOD_Gold GOOD_Sugar GOOD_Bananas GOOD_Oasis GOOD_Tobacco | 0 | 0 | 0 | 16–21,24,22,23,25 | -1 |

(`b` mirrors the row index except Sugar 24 / Tropical Fruit 22 / Oasis
23. `c` is an increasing id for strategics only — possibly a tech index.
`+0x50` holds 3 small u32s per row. Full bytes in
`/tmp/civ3dbg/good_rows.bin`, captured 2026-09-29.)

Load pipeline temp files (all in the exe dir, all captured): the EGYPT
load wrote `save0.tmp` (1748113B — the game's own DCL decompression of
EGYPT.SAV), `bic__in_.tmp` (8329B = 736B `BICQVER#` header + `GAME`
section at 736 — game settings, no GOOD), `bic__out.tmp` (736B header;
the writer target above). Our `dcl.rs` inflation of EGYPT.SAV is
BYTE-IDENTICAL to the game's `save0.tmp` (`cmp` clean) — the codec is
ground-truth validated. (Autosave `.SAV`s are stored raw/uncompressed,
1.3–1.7MB — `BadDictBits` under DCL.)

Data-path note: the game reads code/DLLs from `civ3-gog/app/Conquests`
but data files (`Sounds/`, `Text/version.txt`) from `civ3-complete/`
— check both trees (the two `conquests.biq` copies are byte-identical).
