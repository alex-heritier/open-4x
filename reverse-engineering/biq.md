# The `.biq`/`.bic` container and codec (solved)

Companion to `NOTES.md` §15, which mapped the decompressor but left the
header framing open (§15.4). The framing is now solved and both shipped
scenario files decode end to end. Reference implementation: `rust/src/dcl.rs`
(mode-0 decoder and the section walker the game code uses).

The **complete codec** lives in the standalone crate [`../biq/`](../biq):
`src/dcl.rs` decodes both literal modes, `src/implode.rs` is the compressor
(see "The compressor" below), and `Biq::to_bytes` writes a file back in the
form it was read in. The section-level format is in [`biq-format.md`](biq-format.md).

**Read the correction below first.** The framing is solved, but the distance
mask was read from the wrong header byte until 2026-09-29: every file whose
third byte's low 5 bits differ from its `dict_bits` (i.e. every Conquests
`.biq`, `00 06 84`) decoded to the right *length* with wrong *bytes* past the
first match. `EGYPT.SAV` (`00 06 86`) was accidentally correct, which is why
the `.sav` golden tests never caught it.

## Result

| file | bytes | decodes to | magic | sections (in order) |
|---|---|---|---|---|
| `Conquests/conquests.biq` | 30 501 | 209 222 | `BICX` | `VER#` … `WSIZ` `FLAV` (18, table below) |
| `civ3mod.bic` | 17 368 | 111 308 | `BIC ` | `VER#` … `WSIZ` `GAME` (18, same order) |

Both end on symbol `0x305` (clean terminator, no truncation). Decoded
independently in Rust and Python: identical lengths, heads, tails, and
byte-sums. Byte sums after the mask correction:
`conquests.biq` = `0x56866f`, `civ3mod.bic` = `0x310158` (both pinned in
`dcl.rs` tests).

## Correction (2026-09-29): the distance mask is `(1 << dict_bits) - 1`

Init at `0x649448`-`0x649492` reads the mode byte from the read buffer,
copies `dict_bits` (`[esi+0xC]`) and the third byte (which it also stores as
the bit buffer, `0x64945C`), then:

```asm
0x649487  mov eax, 0xffff        ; mask
0x64948c  mov cl, 0x10
0x64948e  sub cl, dl            ; dl = [esi+0xC] = dict_bits
0x649490  sar eax, cl
0x649492  mov [esi+0x10], eax   ; -> distance mask
```

The **third header byte is not a mask parameter**: bytes 0/1 are `mode` and
`dict_bits`, and byte 2 is the compressed stream's first byte (biq.md's
"shift byte" is just bit-buffer seed material). The distance step then masks
the *bit buffer* with the stored mask (`and eax,[esi+0x14]`, `0x64988b`), so
with `dict_bits = 6` the mask is `0x3F` and the distance is
`(class << 6) | peek(6)`.

The earlier implementation derived the mask from byte 2 as
`0xFFFF sar (16 - byte2)` with x86 5-bit count masking. For `00 06 86`
(`0x86 & 0x1F == 6`) that coincidentally equals the right mask; for
`00 06 84` it produced `0xF` and truncated the low 6 distance bits to 4,
silently splicing wrong matches from `0x334` onward. Consequences, all
now retired: "`conquests.biq` GOOD has no names / is u16-framed", "no
Conquests file on disk contains `Horses`", the 3x3 stride/header search on
"conquests.biq-inflated", and the Civilopedia-join model of `save1.tmp`.

Oracle: the game's own staging file. Loading the default rules makes
`0x5F76C0` inline the DCL stream into the first free `save%d.tmp`
(`%s%d%s` from the literals at `0x72FDC4`/`0x72FDC8`, loop
`0x5F76E6`-`0x5F771D`) and `0x594290` parse *that* file. With the fixed
mask this crate's decode of `conquests.biq` compares byte-identical to
`/tmp/civ3dbg/save1.tmp` (209 222 B, captured live 2026-09-29 while
loading `EGYPT.SAV`).

## Framing (the §15.4 answer)

Three-byte header at file offset 0, then the DCL stream:

| byte | value here | meaning (`0x649400`) |
|---|---|---|
| 0 | `0x00` | mode 0 = binary (raw 8-bit literals; lengths via short tree) |
| 1 | `0x06` | dict bits 6 (must be 4..6, `0x649475`) |
| 2 | `0x84` | first byte of the compressed stream; the init copies it into the bit buffer (`0x64945C`) |

The subtlety that blocked §15.4: **the third byte doubles as the first
stream byte.** Init seeds the bit buffer with it (`mov [esi+0x14], dl`,
`0x64945C`), so the bitstream starts at file byte 2, not byte 3. Skipping
all three header bytes desynchronises the stream (garbage + early
`BadDistance`); starting at byte 2 decodes cleanly.

`file(1)` reports these as "TTComp archive" — that rule keys on the bare
2-byte prefix `\0\6` (its own source notes it is over-general) and is a
misidentification; the exe contains no TTComp code or strings and reads
these files through the DCL path below.

## Loader chain (verified)

`0x59AB50` (scenario load) → `0x5F76C0` (build temp path, `malloc 0x3134`
state, `call 0x649400` with `fread`/`fwrite` callbacks `0x5F74B0`/`0x5F7500`;
no seek — stream starts at byte 0) → `0x594290` (section dispatcher) on the
inflated temp file. `0x594290` has exactly one caller (`0x59AC5D`).

The temp path is `save%d.tmp`: `0x5F76C0` formats `%s%d%s` from the two
pointers at `0x72FDC4` (`save`) / `0x72FDC8` (`.tmp`) and takes the first of
`d = 0..5` whose `fopen("wb")` succeeds (`0x5F76E6`-`0x5F771D`), then stores
the name in its own 1024-byte state object. That is why a load shows
`save1.tmp`: the first stream of the same load already holds `save0.tmp`.
Live proof (2026-09-29): the reader's second `fopen` in an `EGYPT.SAV` load
took its path from a stack buffer reading `save1.tmp` while `EBP` still held
`"conquests.biq"` (`0x68529C`) — i.e. the rules stream is the game's decode
of `conquests.biq`, not save content.

State size `0x3134` confirms the table layout: peek tables, L1–L4, lens,
bases and distance tables end exactly at `0x3134` (`0x5F7746`).

## Codec semantics (all verified against output)

* LSB-first bit reader (`shr` consumption, `0x6498CA`); callers peek-then-consume.
* Mode 0: select bit 1 → short tree (8-bit peek → 16 length classes →
  `SHORT_BASE[class] + extra`, symbol `0x100 + …`); select bit 0 → raw
  literal byte. Match length `len = sym - 0xFE`; literals `< 0x100`;
  end on `sym >= 0x305`.
* Distance: 8-bit peek → 64 classes; `len == 2` uses 2 extra bits, else
  `dict_bits` bits, taken as `bitbuf & mask` with
  `mask = 0xFFFF >> (16 - dict_bits)` (`0x649888`-`0x649897`); distance
  `+1`; 4 KB window. `mask` and `dict_bits` come from the same variable, so
  the mask is a no-op — the byte the old implementation read it from is not
  a mask parameter at all.
* `BICX` (Conquests) extends the `NOTES.md` §13 magic list: the loader
  accepts a fourth magic word for expansion scenarios.

## Corrections to `NOTES.md` §15

1. Bit order is LSB-first, not MSB-first (§15.2).
2. The terminator is symbol `0x305` (class 15 + extra 255; the `0x10E`
   tolerance check at `0x649703` is `class + extra`, un-biased), not `0x30E`.
3. Framing (§15.4): solved as above — no missing header bytes.
4. `0x648920`/`0x648AC0` confirmed as the separate implode *compressor*
   (window select `0x400/0x800/0x1000`, table builds); unused by scenario load.
   It is used when the game *writes* (saved games), and it is **ported and
   verified** below: it reproduces every shipped compressed file byte for byte.
5. Distance mask: it is `(1 << dict_bits) - 1` from the *second* header byte's
   dict bits (`0x649487`-`0x649492`), not a value carried by the third byte.
   See the correction section above.

## The compressor (`implode`): ported and verified (2026-10-02)

`0x648920` is PKWARE DCL 1.11 `implode` (the library banner at `0x73A388`).
Reference port: [`../biq/src/implode.rs`](../biq/src/implode.rs), every routine
annotated with its address; `compress(input, mode, dict_bits)`.

| exe | what | notes |
|---|---|---|
| `0x648920` | `implode`: checks the window (`0x400`/`0x800`/`0x1000`, else error 1) and mode (0/1, else error 2), builds the code tables, calls the main loop | literal codes `0x6489AA` (binary: 9 bits, code `2*byte`) / `0x6489DC` (ASCII: `len+1` bits, `code*2`); length codes `0x648A12` (flag bit 1, class code, extra bits) |
| `0x648AC0` | main loop: reads 0x1000-byte blocks, indexes them, emits literals and matches, slides the window, writes the end symbol | block shift `0x648CE0`; lazy match rule `0x648C1D`..`0x648D59` |
| `0x648E40` | `FindRep`, the match finder | see below |
| `0x649340` | `SortBuffer`: counting sort of positions by hash | hash `b0*4 + b1*5`, 0x900 buckets, each bucket ascending |
| `0x649180` | `OutputBits` (LSB-first, recursion above 8 bits) | flush `0x6492C0` only chunks the output |

**Why the match finder had to be transcribed, not guessed.** A DCL stream does
not record how the compressor chose its matches, so format knowledge alone
yields a *valid* compressor, not the game's. Reproducing the shipped files
needs these details of `FindRep`, all read from the disassembly:

* candidates come from the position's hash bucket, oldest first; positions
  older than the window are dropped from the bucket head for good
  (`0x648E68`..`0x648EA6`);
* a candidate must start at least two bytes before the current position
  (`src - 1 > cand`, `0x648ED1`);
* early reject on the byte at `best - 1`, then on byte 0 (`0x648EDF`); bytes 0
  and 1 are assumed equal (same hash, same first byte forces the second byte),
  so the comparison starts at byte 2 (`0x648EEF`);
* **an equal-length candidate replaces the best** (`0x648F0C`: only a shorter
  one is skipped), so the nearest of equal matches wins; a `0x204` match returns
  at once (`0x648F50`; its distance needs a `-1` fix-up because the compare loop
  stops one byte early);
* a 2-byte match is only usable below distance `0x100` (`0x648BF8`, and again after the end-of-data clamp at `0x648C09`);
* once `best > 10` the finder builds the KMP failure table of the best match
  (`0x648F8A`, `fail[0] = -1`) and skips candidates that cannot be longer
  (`0x64900B` LOOPB), still replacing on equal length;
* the caller adds one-byte lazy evaluation (`0x648C1D`..`0x648D59`): a match
  of 8 or more is taken at once; a shorter one is retried one byte later and
  the later match wins only if it is longer by 2, or longer by 1 when the first
  distance is above `0x80`. Otherwise the first match is kept.

**The work area is one flat struct** (36 312 bytes): `work_buff` at `+0x27CC`
(`0x2204` bytes: window, `0x204` look-ahead, 0x1000 new input) is followed
immediately by `hash_offs` at `+0x49D0`. A match compare near the end of a block
can run past `work_buff` and read the position table, so the port models them
as one buffer. At end of input the finder also reads up to `0x204` bytes past the
data; the original never clears the area, so those bytes are the zeros of the
first block or the previous block's tail. Starting zeroed with the same
`memcpy` shift reproduces every shipped file.

**Verification (the oracle is the game's own output).** Decode every
compressed file in the install, compress the result with the port, compare:

| set | files | byte-identical |
|---|--:|--:|
| `.biq`/`.bic`/`.bix`, DCL, all `00 06 84` | 83 | 83 |
| saved games, DCL `00 06 86` (largest: `TETURKAN.SAV`, 507 KB on disk, many 4 KiB blocks) | 10 | 10 |

`biq::implode::tests::recompresses_every_shipped_file_exactly`. The oracle is
sensitive to each quirk above; one deliberate mutation at a time, files out of 93
that stop matching:

| mutation | files that fail |
|---|--:|
| equal-length candidate no longer replaces (`len <= best` skipped) | 93 |
| same, in the long-match tail (`esi <= best` before the distance store) | 93 |
| long-match tail removed (stop at the first match above 10) | 93 |
| lazy rule without the `+1` case | 88 |
| 2-byte cutoff `0x100` -> `0x80` | 82 |
| lazy distance threshold `0x80` -> `0x7F` | 5 |

What the shipped data does not cover is stated, not assumed: dictionary bits 4 and 5 and mode 1 are checked by
round trip (all lengths 0..20 000, all three windows, both modes) but have no
shipped stream to compare against.

**Provenance.** The editor links no compressor and writes raw files
(`editor.md`), and the game compresses only saved games. The 83 compressed
scenarios therefore came from an external tool; they are byte-identical to what
this routine produces, so it ran the same library at version 1.11 with the same
parameters (binary literals, 4 KiB window).

**Mode 1 (ASCII literals), now supported.** A literal is flag `0` plus the
byte's code from `0x73A088` (lengths 4..13, +1 for the flag) / `0x73A188`
(codes). The exe keeps a second copy of both tables for the decompressor
(`0x73A520` lengths, `0x73A620` codes, the table `rust/src/dcl.rs` mirrors as
`TREE_CODES`); **the two copies are byte-identical** and a test pins every
table constant to the exe image. The code is a complete prefix code
(`sum 2^-len = 1` exactly, so a wrong byte would break it) and every 13-bit
window decodes to exactly one byte. Still no shipped stream uses mode 1, so
there is no file from the game to decode; the encoder side is the exe's own
init, the decoder is a 13-bit lookup over the same table.

## Section framing (verified: walk of the corrected decode)

`[4B ASCII tag][u32 count]`, then `count` rows of `[u32 len][len bytes]`.
Every arm of `0x594290` reads its own count with one `fread 4` (GOOD at
`0x59456B`, TERR at `0x596490`) and every row reader consumes its `u32 len`
(`0x5e3860` GOOD, `0x5e9300` TERR). Walk of `conquests.biq` (offset, count,
end), all 18 landing on each other with no slack:

| offset | tag | count | end |
|---|---|---|---|
| 4 | `VER#` | 1 | 736 |
| 736 | `BLDG` | 83 | 23 320 |
| 23 320 | `CTZN` | 6 | 24 096 |
| 24 096 | `CULT` | 6 | 24 656 |
| 24 656 | `DIFF` | 8 | 25 656 |
| 25 656 | `ERAS` | 4 | 26 736 |
| 26 736 | `ESPN` | 9 | 28 868 |
| 28 868 | `EXPR` | 4 | 29 052 |
| 29 052 | `GOOD` | 26 | 31 452 |
| 31 452 | `GOVT` | 8 | 36 036 |
| 36 036 | `RULE` | 1 | 36 768 |
| 36 768 | `PRTO` | 141 | 74 971 |
| 74 971 | `RACE` | 32 | 184 667 |
| 184 667 | `TECH` | 83 | 194 303 |
| 194 303 | `TFRM` | 13 | 195 819 |
| 195 819 | `TERR` | 14 | 199 145 |
| 199 145 | `WSIZ` | 5 | 199 573 |
| 199 573 | `FLAV` | 1 | 199 592 |

`FLAV` is the last one: its body goes to `0x52d2c0` (`0x594CB8`) and is not
count/row framed, so the walk ends there. `UNIT` has an arm in the dispatcher
but no section in this file. Pinned by `dcl::tests::conquests_biq_section_walk`.

## `.sav` files: same wrapper, a different stream

| file | stored | decodes to |
|---|---|---|
| `Saves/EGYPT.SAV` (112 586 B) | DCL, header `00 06 86` | 1 748 113 B starting `CIV3\0`, 20 000 `TILE` tags (a 100x100 map has 5 000 cells, four `TILE` chunks each) |
| `Saves/Auto/…4000 BC.SAV` (1 346 958 B) | raw (`CIV3\0` first) | already the stream |

A save is a DCL-wrapped or raw stream behind the magic `CIV3`; the loader
sniffs the magic, so both load, and the wrapper is the same codec (the
compressor reproduces all 10 compressed saves of the corpus byte for byte).
The body is **not** a scenario stream: it is a dump of the live game objects in
a fixed order, with the scenario embedded in it as one `BICQ` block. The 20 000
`TILE` tags are the per-cell chunks of the map section. Format and loaders:
[`savegame.md`](savegame.md); Rust: `civ3_biq::Save`.

## Scenario load sequence `0x59AB50` (verified: region sweep)

SEH prologue; logs `\nBeginning scenario load sequence ( %s )...\n`
(`0x72E53C` via `0x5F9920`); `call 0x5F7800`, `call 0x5F76C0` (DCL/temp-path
wrapper), `call 0x594290` (dispatcher — exactly 1 caller in `.text`, the
single-caller claim confirmed). Fallbacks: `civ3F.bix` (`0x6855D8`),
`conquests.biq` (`0x68529C`). `0x59AB50` itself has 4 callers
(`0x59AD22`/`0x59B105`/`0x59B2F0`/`0x59B494`); `0x5F76C0` has 7.

Dispatcher `0x594290`: `fopen "rb"` + fread/fseek, magic gate on `BIC `/
`BICX`/`BICQ` (else `Scenario::load ERROR _ Invalid scenario file format`),
then per-tag `cmp` arms over the full inventory (raw `3D`-scan):
`PRTO GAME GOOD VER# SLOC LEAD RACE TILE RULE TFRM DIFF BLDG TECH ESPN
CTZN CULT TERR WMAP WCHR EXPR ERAS UNIT CLNY CONT GOVT FLAV CITY WSIZ`.
Sampled arms: `UNIT` (`0x594AC8` → flag gate → `0x596DE0` loadUNIT, or-bit
8 into `[esi+0x84C]`), `BLDG` (`0x59472C` → `0x594FF0`, or-bit 1 into
`[esi+0x848]`), `PRTO` (`0x594399` → `0x595C40`, or-bit into `[esi+0x848]`),
`VER#` (logs `Loading version %d.%02d BIC file`, float version check into
`[esi+0xBA0]`). Worker bodies (`0x596DE0`/`0x594FF0`/`0x595C40`): open.
Getter `0x599600` has zero direct callers — indirect/virtual only
(`Map::vfunc(0x8C)`). Save path lead: `0x59F752–0x59F930` (`BIC ` compares,
`BICX` mem-compare, `call 0x597E20`). Duplicate `CULT` compare
(`0x594EA4` vs `0x59494E`): unexplained.

## Loader worker bodies (verified: sweep + parent spot-checks)

All three share a prologue shape (`fread` via `0x64B3E3`, then an
alloc call) and a single caller each from their dispatch arm:

| worker | sole caller | count | base | stride | per-item |
|---|---|---|---|---|---|
| loadUNIT `0x596DE0` | `0x594AE4` | `[esi+0x8C0]` | `[esi+0x3E24]` | `0x7C` (`add ebp`) | `0x5EADA0` |
| loadBLDG `0x594FF0` | `0x5947E3` | `[esi+0x878]` | `[esi+0xBA4]` | `0x110` (`add edi`) | `0x5DFF40` |
| loadPRTO `0x595C40` | `0x594935` | `[esi+0x8A8]` | `[esi+0x3CD8]` | `0x138` (`add edi` @`0x595CED`, parent-verified) | `0x5E54B0` |

Version gates: UNIT compares `percent*0.01` (`fild`/`fiadd`/`fmul
[0x666AC8]`), BLDG `fcomp [0x66E9D4]=2.08`, PRTO `fcomp
[0x665838]=10.0` (parent-verified at `0x595CFB`, followed by the
`[Scenario::loadPRTO()]: Translating Civ3 unit actions to Civ3X unit
actions` log). PRTO counts `[item+0xA0]==-1` into `[esi+0x874]`
(HYPOTHESIS: `-1` = no prerequisite). BLDG first-settled fixup
(tests/clears bit 0 at `[eax+0xEC]`) and `-1` normalize at
`[eax+0xD8]`: child-reported. PRTO v11.01/11.03/11.05 arms past
`0x595F9A`: open.

`rust/src/dcl.rs`: `SCENARIO_TAGS` inventory + `magic_valid()`, tested.

## Older `BIC ` files use the 12-terrain layout (verified)

`0x596490` (the TERR loader) checks the row count and accepts only `14` (the
Conquests `BICX` layout) or `12` (the older `BIC ` layout, branch at
`0x59692B`); both allocate that many rows with the literal count (`push 0xe`
/ `push 0xc` to `0x59c420`) at `0xf0` bytes each (`add ecx,0xf0`,
`0x596532`). Ground truth: `conquests.biq` TERR = 14 rows, `civ3mod.bic`
TERR = 12, GOOD = 22, and its last section is `GAME` (not `FLAV`). The rows
themselves are length-prefixed; field layout in `resources.md`.

## No scenario editor in this binary (verified: byte grep)

Case-insensitive `editor` scan of the whole image finds exactly 3 hits, all
uppercase data tags — `EDITORBLDG` (`0x684020`, pushed at `0x41FC75`),
`EDITORWHO` (`0x72951C`, pushed at `0x4E33D0`), `EDITORUNIT` (`0x729528`,
pushed at `0x4E3701`). Zero hits for `Civ3Edit`/`civ3edit`/`Scenario
Editor`/lowercase `editor`. The editor is a separate binary — confirmed.

## Open

* Mode-1 (tree-literal) streams: supported in `../biq` (see "The compressor"),
  tables verified in both exe copies, but **no stream from the game exists to
  decode**: of all 68 scenario/save files in the install, 32 are mode-0 DCL
  (`00 06 84/86`), 36 are raw, **zero** are mode 1. `rust/src/dcl.rs` still
  mirrors the `0x649980` build on the exe's flat table layout and still reports
  mode 1 as unverified; the `biq` crate's decoder replaces it for scenarios.
* Mode-0 splits `00 06 84` (every compressed `.biq`/`.bix`) from `00 06 86`
  (`EGYPT.SAV`). Before the mask fix the `84` files *all* decoded wrong,
  which is what produced the earlier "no Conquests file contains `Horses`"
  and "`conquests.biq` GOOD is nameless" claims. Re-run over the whole
  install with the fixed decoder: every `.biq` with a GOOD section carries
  `GOOD_*` names and the 9 Conquests scenarios carry `Horses` (the custom
  sets — `Apshai`, the `BTM`/`Barbarian Stronghold` family, the two `Intro`
  scenarios — do not).

## RACE civ colors (verified: decoded `conquests.biq`)

Each `RACE` row, after the city list, great-leader list, leader name (32),
title (24), civ key (32), three 40-byte noun/adjective strings and eight
260-byte era art paths, continues with little-endian ints: culture group,
leader gender, civ gender, aggression, civ index, shunned government,
favorite government, **default color**, **unique color**. Colors index
`Art/Units/Palettes/ntpNN.pcx`, each a 16-entry ramp from light (0) to dark
(15). Examples: Romans 1/1 (red), Americans 5/5 (cyan), Japanese 4/11
(green `ntp04`, unique red `ntp11`), Vikings 8/31. Decoded from the shipped
`civ3mod.bic` (the layout above reproduces the Romans and Japanese values):
Egyptians 3/3 (yellow `ntp03`), Chinese 5/15 (cyan `ntp05`, unique lilac
`ntp15`). The Chinese row's 18 city names open with Beijing, Shanghai,
Canton, Nanking, Tsingtao, Xinjian, Chengdu, Hangchow. The rule for choosing
unique over default is not traced; `civ3-clone` uses Japan's default color
at ramp index 4, `(25,148,24)`, for label badges and border beads
(`cities::CIV_COLOR`).
