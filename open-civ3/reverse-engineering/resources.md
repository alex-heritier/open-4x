# Resource placement (`0x5f22a0`)

Stage 10 of `generateMap` (`0x5eb580`). Companion to `NOTES.md` §11.8, which
specifies the algorithm; this file adds the data path, the `.biq` record
layout it consumes, and what a clone needs to reimplement it.

**2026-09-29 correction: the `.biq` decode was broken** (`biq.md`: the DCL
distance mask was read from the wrong header byte). Everything this file
previously said about `conquests.biq` having "no name strings" and a "u16,
nameless GOOD framing" was an artifact of that, and the Civilopedia-join
model of `save1.tmp` is retired: `save1.tmp` is byte-identical to a correct
DCL decode of `conquests.biq`, and the file's `GOOD` rows carry the names
the live table showed. The tables below are re-derived from the corrected
decode.

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
| `TERR` +`0x08` | pointer to byte array (memory row) | bit `g&7` of byte `g>>3` = resource `g` may appear on this terrain (`0x5F2470`); the bytes come from a `u32`-wide allow mask in the file row (below) |
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
| `u32` at section `+4` | `0x1A` (26) | the row count: the reader reads it with one `fread 4` (`0x59456B`), the loader loop bound is the same value |
| file row framing | `[u32 len=88][88 B body]` | `0x5E3860` reads `len` into row `+0x00`, consumes 88 B and `fseek`s past any remainder (`0x5E396A`) |

`0x5E3740` memory layout per row (all `0x64AF35` reads into `edi`):

| memory offset | size | role |
|---|---|---|
| `+0x00` | u32 | the file row's `len` word (the reader subtracts each `fread` from it) |
| `+0x04` | 24 B | display name (`Horses`, NUL-padded) |
| `+0x1C` | 32 B | `GOOD_*` key (`GOOD_Horses`) |
| `+0x3C` | u32 | class: 2 = strategic, 1 = luxury, 0 = bonus |
| `+0x40` | u32 | **frequency** (confirmed) |
| `+0x44` | u32 | **disappearance probability** (the editor's second rate): 800/400/200/100 on strategics, 0 elsewhere. Read by the resource-upkeep routine `0x4F4CB0`, which runs only when `turn mod 5 == 0` and, for a worked or connected tile, removes the resource when `rand(p) == 0` (`p` this value, the draw only when `p > 0`); `world-events.md` section 2. It is **not** a per-turn rate |
| `+0x48` | u32 | `b`: **icon/ordering id = the `resources.pcx` cell** (equals the row index except Sugar 24 / Tropical Fruit 22 / Oasis 23; see the icon section below) |
| `+0x4C` | u32 | `c`: **the `TECH` row that reveals this resource** (8/8 correct below), `-1` on luxuries and bonus goods |
| `+0x50` | 12 B | 3 small u32s per row |
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
× header {4, 6, 8} on "conquests.biq-inflated" peaked at stride-92/header-8
with only 11/26 sane freqs and drifting names. The framing was never the
problem: the decode itself was wrong (see the correction at the top), which
also made the names disappear. With the fixed codec a plain PTW walk parses
26/26 rows in one pass. Memory rows are the fixed `0x5C`; file rows are
`[u32 len=88][88B data]` (see `## Reader/writer split`).

## Names are in the BIQ (verified: corrected decode)

`conquests.biq`'s `GOOD` rows carry the same 24 B names and 32 B keys the
live table showed (`Horses`/`GOOD_Horses` … `Tobacco`/`GOOD_Tobacco`), and
the 88 B file cores are byte-identical to the live memory rows
(`/tmp/civ3dbg/good_rows.bin`); `resources::tests::good_section_layout`
pins that. The old "names live outside the BIQ" reading came from decoding
the file with the wrong DCL mask: `Horses` and friends are present in
`conquests.biq` itself, as they are in the 9 Conquests scenario `.biq`s and
the custom `.bix` sets (only the obfuscated/custom-good sets lack them).

`Conquests/Text/PediaIcons.txt` (`#ICON_GOOD_*`, 26 entries) is still the
source of the icon keys, but it is no longer needed to name rows.

## TERR file rows: the allow-matrix source (verified: corrected decode)

`TERR` at inflated offset 195 819: `[u32 14]`, then 14 rows of
`[u32 len = 233][body]`, the last ending exactly on `WSIZ` at 199 145. The
row reader `0x5E9300` consumes the body in this order, which fixes the
layout:

| body offset | size | role |
|---|---|---|
| `+0x00` | u32 | goods count (26 = the GOOD row count) |
| `+0x04` | `ceil(goods/8)` = 4 B | **allow mask**, little-endian: bit `g` = resource `g` may appear here |
| `+0x08` | 32 B | display name (`Desert`, NUL-padded) |
| `+0x28` | 32 B | `TERR_*` key (`TERR_Desert`; spaces become underscores: `TERR_Flood_Plain`) |
| `+0x48` | 161 B | terrain properties, unmodelled (the reader `fseek`s past the rest) |

**Memory↔file mapping for TERR rows (verified 2026-09-29):** the reader
`0x5E9300` stores `len` at `+0x04`, allocates the mask and stores its
pointer at `+0x08`, then `fread 0x20` into `+0x0C` (`0x5E93D1`) = the name,
and the key follows at `+0x2C`. In the file the same fields sit at body
`+0x08` (name) and `+0x28` (key), so **memory = file body + 4** throughout
the row. Consequences: the tail byte that `0x5CD960(terrain)` tests at
`row+0x7A` is **body `+0x76`**, which is `1` for all land terrains except
Volcano and `0` for Volcano/Coast/Sea/Ocean (an is-land/enterable flag),
and body `+0x74`/`+0x75`/`+0x77` are `1` for the nine land terrains
excluding Marsh (`+0x78` = `3` on every row). See `workers.md`.

**The rest of the row is a u32 struct, not opaque bytes.** The reader's
field list (`0x5E9406` ff, one `fread 4` each) is `+0x4C`, `+0x50`,
`+0x54`, `+0x58`, … i.e. body `+0x48`, `+0x4C`, `+0x50`, `+0x54`, …
(Desert: `1, 1, 1, 10, 1, 0, 1, 0, -1, -1, 0x0101, 0x01010101, 3, 1, …`),
so the per-terrain bytes `0x5CD960` reads are *inside* those words. The row
ends with a second, **length-prefixed name string** (`10` then `"LM Desert"`;
Ocean has `"Ocean"`) followed by the `TERR_*` key again — the `LM ` prefix
reads as the terrain's landmark/improvement name.

That second name is a **preferred variant, not a duplicate**: its only two
consumers (`0x554D62`, `0x5AAFA5`) pick it over the primary name at `+0x0C`
when a tile predicate holds:

```asm
call [reg+0xC8]                       ; terrain id
imul ecx,0xF0 ; add [0x9C7328]        ; TERR row
tile->vfunc(0x78)() ? TERR?+0x0C (0x5E8D00) : TERR+0xA8 (0x5E8CF0)
; then strlen()
```

What `vfunc(0x78)` is, and what the `LM ` prefix stands for
(landmark/mode name most likely), stays open.

In memory the same row keeps the remaining length at `+0x04`, a *pointer*
to the mask array at `+0x08` (`malloc((goods+7)>>3)` at `0x5E936C`, stored at
`0x5E9382`) and the count at `+0x60`, in `0xf0`-byte rows (`0x596532`;
14 rows for `BICX`, 12 for the older `BIC ` layout, `biq.md`). Placement
reads the mask through that pointer exactly as the file bytes are laid out
(`0x5F2470`), so the file row above *is* the placement input.

Ground truth, `conquests.biq` (`resources::tests::terr_section_gives_allow_matrix`):

| # | terrain | mask | allows |
|---|---|---|---|
| 0 | Desert | `0x01000814` | Saltpeter, Oil, Incense, Oasis |
| 1 | Plains | `0x00582101` | Horses, Wines, Ivory, Cattle, Wheat, Sugar |
| 2 | Grassland | `0x02180101` | Horses, Wines, Cattle, Wheat, Tobacco |
| 3 | Tundra | `0x00020250` | Oil, Aluminum, Furs, Game |
| 4 | Flood Plain | `0x00100000` | Wheat |
| 5 | Hills | `0x0260094f` | Horses, Iron, Saltpeter, Coal, Aluminum, Wines, Incense, Gold, Sugar, Tobacco |
| 6 | Mountains | `0x0020808e` | Iron, Saltpeter, Coal, Uranium, Gems, Gold |
| 7 | Forest | `0x000276a0` | Rubber, Uranium, Furs, Dyes, Spices, Ivory, Silks, Game |
| 8 | Jungle | `0x0080d428` | Coal, Rubber, Dyes, Spices, Silks, Gems, Tropical Fruit |
| 9 | Marsh | `0x00060030` | Oil, Rubber, Game, Fish |
| 10 | Volcano | `0x00000000` | — |
| 11 | Coast | `0x00040000` | Fish |
| 12 | Sea | `0x00050000` | Whales, Fish |
| 13 | Ocean | `0x00000000` | — |

The mask bit order is the placement's own (`byte[ptr + (g>>3)] &
(1 << (g&7))`), so bit `g` of the little-endian `u32` is GOOD row `g`.
That closes the "TERR allow-matrix value" open thread: the hardcoded table
in the clone can now come from the file.

## Icon sheet order: **cell = the row's `b` field** (verified by eye)

`Art/resources.pcx` is 300x300 px, a 6x6 grid of 50 px cells (36 cells,
magenta gutters every 50 px), loaded once by the map-view art init
(single `.text` ref to the path `0x72848C` at `0x4C77B2`). Rendering it at
3x with a grid and reading the cells (2026-09-29) identifies every icon:

| cell | icon | GOOD row | row `b` |
|---|---|---|---|
| 0 | horse | Horses | 0 |
| 1 | grey ore | Iron | 1 |
| 2 | pale crystals | Saltpeter | 2 |
| 3 | black lumps | Coal | 3 |
| 4 | oil drop/derrick | Oil | 4 |
| 5 | black ring | Rubber | 5 |
| 6 | metal cylinder | Aluminum | 6 |
| 7 | green glowing rocks | Uranium | 7 |
| 8 | grapes | Wines | 8 |
| 9 | pelt | Furs | 9 |
| 10 | pigment bowl | Dyes | 10 |
| 11 | smoking censer | Incense | 11 |
| 12 | herb pile | Spices | 12 |
| 13 | elephant | Ivory | 13 |
| 14 | blue fabric bundle | Silks | 14 |
| 15 | jewel pile | Gems | 15 |
| 16 | grey whale | Whales | 16 |
| 17 | deer | Game | 17 |
| 18 | pale fish | Fish | 18 |
| 19 | cow | Cattle | 19 |
| 20 | sheaf | Wheat | 20 |
| 21 | gold nuggets | Gold | 21 |
| 22 | bananas | Tropical Fruit | **22** |
| 23 | palm + water | Oasis | **23** |
| 24 | cane/plant | Sugar | **24** |
| 25 | leaf bundle | Tobacco | 25 |

Cells 26-35 are magenta (empty) on this sheet. The three tail rows are the
proof of the rule: `Sugar` sits at row 22 but its icon is cell **24**, and
`Tropical Fruit` (row 23) is cell 22, `Oasis` (row 24) is cell 23 — i.e.
**the sheet cell is the row's `b` field (`+0x48`), not the row index**. That
also finally names `b`: it is the icon/ordering id, and it is identical to
the row index for all rows except those three (`resources.md` GOOD table).

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

## GOOD rows: exact writer, and what it writes (verified: `r2` + probes)

(This section describes the WRITER, `0x597070`/`0x5E3740` — formerly
mislabeled the reader. All transfers are `fwrite`s.)

`0x5E3740` emits exactly 92 flat bytes per row: `ftell`, then fwrites of
4 (stack — the length word, see below) + 24 (from `+0x04`) + 32 (from
`+0x1C`) + 5×u32 (from `+0x3C..+0x4C`, freq at file+64) + 12 (from
`+0x50`), then `ftell`, `fseek(pos1,SET)`, a 4B re-read into a dead stack
temp, `fseek(pos2,SET)` — position-neutral. All sizes are immediates;
the loop at `0x59749F` does no file I/O of its own.

**Writer call sites (verified: `E8` census + `r2`).** `0x597070` has 7
callers (`0x599FDF`, `0x59A631`, `0x59A7EF`, `0x59B18A`, `0x59B361`,
`0x59B3F9`, `0x59B410`), all in the 0x599Fxx–0x59B4xx save/export block,
and every one of them passes the path held in the global `0x72D9CC`
(initialised to `"bic__out.tmp"` at `0x72DA08`, table entry, next to
`"bic__in_.tmp"` at `0x72D9C8`). The writer never writes `save*.tmp`: that
name comes only from `0x5F76C0`'s `%s%d%s` loop (`biq.md`). `0x597070` is
the BIC **save** path (scenario editing/export), not the load staging path.

Sizes match the file exactly: `len` at row `+0x00`, name 24 B at `+0x04`,
key 32 B at `+0x1C`, `class` at `+0x3C`, `freq` at `+0x40`, `a`/`b`/`c` at
`+0x44`/`+0x48`/`+0x4C`, 12 B tail at `+0x50`.

Retired claim (2026-09-29): the earlier text here said no fixed stride
slices the inflated `conquests.biq`, that its GOOD section has no name
strings, and that no Conquests file contains `Horses`. All three were
artifacts of the DCL mask bug — the "u16 framing" (`1a 00 2c 01 58 00 …`)
was corrupted bytes. With the fixed decode: GOOD spans 29052 → 31452
(`GOVT` next) = exactly 2392 B = 26 × 92, the row walk parses 26/26, and
the "6984-byte span" came from a byte search on the corrupt image rather
than from the section walk.

## Live GOOD rows: full memory dump (winedbg, EGYPT.SAV load)

Break `*0x5974BB` (row-loop exit) during the EGYPT load: count
`[0x9C3508+0x89C]` = 26, rows at `[0x9C3508+0x3CCC]` = `0x066BFFC8`,
2392 bytes dumped. Order, names (`+0x04`), freq (`+0x40`), category
(`+0x3C`: 2 = strategic, 1 = luxury, 0 = bonus):

Horses 160, Iron 160, Saltpeter 120, Coal 120, Oil 120, Rubber 120,
Aluminum 120, Uranium 100, Wines/Furs/Dyes/Incense/Spices/Ivory/Silks/
Gems 0, Whales/Game/Fish/Cattle/Wheat/Gold/Sugar/Tropical Fruit/Oasis/
Tobacco 0. `+0x48` mirrors the row index except Sugar 24 / Tropical
Fruit 22 / Oasis 23 (the icon/ordering id = the `resources.pcx` cell; see
the icon section below). Luxuries and bonus
all carry freq 0 (the `freq != 0 ? freq : roll` fallback decides them).

The names-in-stream question (resolved): the reader's 24 B `fread`
targets `+0x04` (`lea eax,[esi+4]`, `0x5E3880`) and the 32 B key `fread`
targets `+0x1C` (`0x5E389C`), flat, with no string-table join — so the
stream really carries the names, and now `conquests.biq` is known to
carry them too (the "zero hits for `Horses`" search ran on the corrupt
decode).

**RESOLVED (2026-09-29, live chained one-shots + corrected codec):** the
GOOD stream is `save1.tmp` (209 222 B rules), opened second by the reader
(`*0x5942CF` post-`fopen`, `FILE* 0x73CE50`) right after `bic__in_.tmp`
(whose `FILE*` is reused — bic is closed first). At the GOOD exit
(`*0x5945B3`) `EDI` is save1's `FILE*`, `EBP`/`EAX` = 26, `EBX` = 2392,
the table is at `[ESI+0x3CCC]`. `save1.tmp` is **the game's own DCL decode
of `conquests.biq`**, written by `0x5F76C0` as the first free
`save%d.tmp` (`biq.md`): with the mask fix, this crate's decode of
`conquests.biq` is byte-identical to the captured `save1.tmp`, and its
GOOD section (`@29052`, `[u32 26][26×[len=88][88B]]`, `GOVT` at 31452)
parses to the 92 B live cores **26/26 byte-identical**. It is deleted
after the read, and it is NOT from EGYPT.SAV (one DCL stream → save0 only,
pinned by `egypt_sav_holds_one_stream`).

Retired model: "the writer `0x597070` assembles save1 per load from rules
staging joined with Civilopedia key+title strings". `0x597070` writes only
`bic__out.tmp` (all 7 call sites pass the `0x72D9CC` global) and is the BIC
**save** path; nothing joins Civilopedia into the rules stream. The
"86 % of bytes differ from `biq.inflated`" observation that motivated the
model was the mask bug.

Open threads, in order:

1. ~~Identify the GOOD stream~~ CLOSED (2026-09-29): `save1.tmp` is the
   loader's temp copy of decoded `conquests.biq` (byte-identical to this
   crate's decode after the mask fix; `FILE*` matched at the GOOD exit).
   Follow-ups both closed: `0x5F76C0` writes it (`save%d.tmp`), and
   `conquests.biq` *is* the source (the earlier "never opened during the
   load" note came from grepping a decode that had no usable strings).
2. ~~Allocator `0x59BD90`~~ CLOSED: frees old `[ebp+0x3CCC]`, mallocs
   `((3×count)<<3 − count)<<2` = exactly 92×count
   (`lea eax,[edi+edi*2]; shl 8; sub edi; shl 2`, `0x59BDD2–0x59BDDB`).
3. ~~conquests.biq's GOOD section framing~~ CLOSED (2026-09-29): 29 052 →
   31 452 (2392 B = 26 × 92), `[u32 count][rows]` like every PTW-BIC file.
   The "u16-framed, nameless" reading was the decode bug.
4. ~~TERR allow-matrix~~ CLOSED: all 14 rows decoded from the file, table
   above (`Horses`/`Oil`/`Incense`/`Oasis` on Desert, `Wheat` on Flood
   Plain, none on Ocean/Volcano — matching play).

## Open

* GOOD field `a` (`+0x44`: 800/400/200/100 on strategics, 0 elsewhere) and
   the 12 B tail at `+0x50` (three small u32s, all `<= 4`) are still
   unmapped — and they are **not** the icon (icons come from `b`, above).
   Tail distribution over the 26 rows (2026-09-29):
   `(0,0,1)` Horses/Saltpeter/Dyes/Tobacco, `(0,1,0)` Iron, `(0,2,1)` Coal,
   `(0,1,2)` Oil, `(0,0,2)` Rubber/Incense/Spices/Ivory, `(0,2,0)` Aluminum,
   `(0,2,3)` Uranium, `(1,0,1)` Wines/Sugar/Tropical Fruit, `(0,1,1)` Furs,
   `(0,0,3)` Silks, `(0,0,4)` Gems/Gold, `(1,1,2)` Whales, `(2,0,0)`
   Game/Wheat/Oasis, `(2,0,1)` Fish, `(2,1,0)` Cattle. So the first value is
   `0` for every strategic, `0` for luxuries except Wines, and `0/1/2`
   across the bonus group (2 = the Game/Wheat/Oasis/Fish/Cattle set) — a
   subtype/category id, not an icon cell (rows collide). The icon-cell
   reading is **rejected**: `Horses` and `Tobacco` share `(0,0,1)` while
   being different icons. Concrete probe: the editor's Good property page
   (its field order).
   Probe executed 2026-09-29 (`editor.md`): the Good page is dialog **135
   'Natural Resources'** and it names the candidates. `+0x44` is one of the
   two `Strategic and Luxury Resources` numbers — **`Appearance Ratio:`**
   (label 1088, value control 1017) or **`Disappearance Probability:`**
   (label 1091, value control 1018) — which fits `a` being non-zero *only*
   on strategics (0 = never/not applicable). The 12 B tail's three values
   line up with the other page fields the row needs: the `Type` radio
   (`Bonus Resource`/`Luxury`/`Strategic Resource`, controls 1082/1087/1086)
   and the `Bonuses` group (`Food:`/`Shields:`/`Commerce:`, 1083/1084/1085)
   — the doc's `(0,0,1)`/`(2,0,0)`/`(1,1,2)` values are small enough to be
   those three food/shield/commerce bonuses, but the corpus alone does not
   pin the order. Decisive step (still open): bind control id -> row offset
   through the editor's GOOD reader/serializer; see `editor.md` "Still
   open".
* TERR's tail is mapped down to the two gameplay numbers (`workers.md`):
  `u32` **movement cost** at body `+0x58` (1/2/3) and `u32` **defense
  bonus %** at `+0x54` (10/20/25/50/80/100), both matching Civ3's tables
  (with 8.8 fixed-point copies further down at `+0x94`/`+0x98`), and the
  land/enterable flag at body `+0x76`. What the trailing `"LM <name>"`
  string is used for stays open.

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
23. `+0x50` holds 3 small u32s per row. Full bytes in
`/tmp/civ3dbg/good_rows.bin`, captured 2026-09-29.)

### `+0x4C` is the revealing tech (verified)

`c` indexes the `TECH` section of the same file. All eight strategics name
their canonical reveal tech, in `conquests.biq` (83 TECH rows) and, with its
own indices, in `civ3mod.bic` (82 rows):

| GOOD | `c` | `TECH` row `c` |
|---|---|---|
| Horses | 4 | The Wheel |
| Iron | 7 | Iron Working |
| Saltpeter | 30 | Gunpowder |
| Coal | 44 | Steam Power |
| Oil | 53 | Refining |
| Rubber | 57 | Replaceable Parts |
| Aluminum | 64 | Rocketry |
| Uranium | 65 | Fission |

The Horses pairing is the game's own rule, not a data slip: The Wheel's
civilopedia text is "{New Resource} Horses appear on the map"
(`Conquests/Text/Civilopedia.txt`). Luxuries and bonus goods carry `-1`.
Pinned by `resources::tests::strategic_reveal_tech_indices` (which reads the
TECH row names out of the decoded file).

**Consumers (2026-10-01, `yields.md` section 4).** The three tile-yield
functions read this table: a tile's resource counts only when
`Player::hasTech(+0x4C)` (`0x561440`, -1 always true; `0x5D7B4C..0x5D7B99` in
the commerce twin), and then adds the three words at `+0x50 / +0x54 / +0x58`
as food, shields and commerce. The row's `+0x3C` category word is tested by
`0x5E3720` (`== 1`: a luxury) in the luxury-resource counter `0x4BAF80`
(`yields.md` section 8). The words at `+0x50` are therefore the bonus yields,
not "3 small u32s" of unknown meaning.

Load pipeline temp files (all in the exe dir, all captured): the EGYPT
load wrote `save0.tmp` (1748113B — the game's own DCL decompression of
EGYPT.SAV), `bic__in_.tmp` (8329B = 736B `BICQVER#` header + `GAME`
section at 736 — the loaded game settings, no GOOD). `save1.tmp` is the
game's own DCL decode of `conquests.biq` — the rules stream the live GOOD
table comes from, written by `0x5F76C0` (`biq.md`). `bic__out.tmp` is the
*save* target of `0x597070` (`0x72D9CC`), not part of the load. Our
`dcl.rs` decode of EGYPT.SAV is BYTE-IDENTICAL to the game's `save0.tmp`,
and after the mask fix its decode of `conquests.biq` is BYTE-IDENTICAL to
`save1.tmp` (`cmp` clean) — the codec is ground-truth validated on both.
(Autosave `.SAV`s are stored
raw/uncompressed, 1.3–1.7MB — `BadDictBits` under DCL.)

Data-path note: the game reads code/DLLs from `civ3/civ3-gog/app/Conquests`
but data files (`Sounds/`, `Text/version.txt`) from `civ3/civ3-complete/`
— check both trees (the two `conquests.biq` copies are byte-identical).
