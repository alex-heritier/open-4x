# The `.biq`/`.bic` container and codec (solved)

Companion to `NOTES.md` §15, which mapped the decompressor but left the
header framing open (§15.4). The framing is now solved and both shipped
scenario files decode end to end. Reference implementation: `rust/src/dcl.rs`.

## Result

| file | bytes | decodes to | magic | sections include |
|---|---|---|---|---|
| `Conquests/conquests.biq` | 30 501 | 209 222 | `BICX` | `VER# TERR GOOD RACE GAME RULE` |
| `civ3mod.bic` | 17 368 | 111 308 | `BIC ` | `VER# GOOD GAME RULE` |

Both end on symbol `0x305` (clean terminator, no truncation). Decoded
independently in Rust and Python: identical lengths, heads, tails, and
byte-sums (`0x110ea1f` / `0x743e33`).

## Framing (the §15.4 answer)

Three-byte header at file offset 0, then the DCL stream:

| byte | value here | meaning (`0x649400`) |
|---|---|---|
| 0 | `0x00` | mode 0 = binary (raw 8-bit literals; lengths via short tree) |
| 1 | `0x06` | dict bits 6 (must be 4..6, `0x649475`) |
| 2 | `0x84` | distance-mask shift; mask = `0xFFFF sar (16-0x84)` = `0xF` (`0x64948C`, x86 5-bit count) |

The subtlety that blocked §15.4: **the shift byte doubles as the first
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

State size `0x3134` confirms the table layout: peek tables, L1–L4, lens,
bases and distance tables end exactly at `0x3134` (`0x5F7746`).

## Codec semantics (all verified against output)

* LSB-first bit reader (`shr` consumption, `0x6498CA`); callers peek-then-consume.
* Mode 0: select bit 1 → short tree (8-bit peek → 16 length classes →
  `SHORT_BASE[class] + extra`, symbol `0x100 + …`); select bit 0 → raw
  literal byte. Match length `len = sym - 0xFE`; literals `< 0x100`;
  end on `sym >= 0x305`.
* Distance: 8-bit peek → 64 classes; `len == 2` uses 2 extra bits, else
  `dict_bits` bits masked by `mask`; distance `+1`; 4 KB window.
* `BICX` (Conquests) extends the `NOTES.md` §13 magic list: the loader
  accepts a fourth magic word for expansion scenarios.

## Corrections to `NOTES.md` §15

1. Bit order is LSB-first, not MSB-first (§15.2).
2. The terminator is symbol `0x305` (class 15 + extra 255; the `0x10E`
   tolerance check at `0x649703` is `class + extra`, un-biased), not `0x30E`.
3. Framing (§15.4): solved as above — no missing header bytes.
4. `0x648920`/`0x648AC0` confirmed as the separate implode *compressor*
   (window select `0x400/0x800/0x1000`, table builds); unused by scenario load.

## `.sav` files: same framing, optional

| file | stored | decodes to |
|---|---|---|
| `Saves/EGYPT.SAV` (112 586 B) | DCL, header `00 06 86` | 1 748 113 B starting `CIV3\0`, 20 000 `TILE` tags, clean `0x305` end |
| `Saves/Auto/…4000 BC.SAV` (1 346 958 B) | raw (`CIV3\0` first) | — (already a scenario stream; 20 000 `TILE` tags in place) |

Saves are scenario streams with `CIV3` magic, stored raw or DCL-wrapped —
the loader sniffs the magic. The 20 000-tag TILE array corroborates the
per-tile record work (`NOTES.md` §13): saves carry the live map.

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

## No scenario editor in this binary (verified: byte grep)

Case-insensitive `editor` scan of the whole image finds exactly 3 hits, all
uppercase data tags — `EDITORBLDG` (`0x684020`, pushed at `0x41FC75`),
`EDITORWHO` (`0x72951C`, pushed at `0x4E33D0`), `EDITORUNIT` (`0x729528`,
pushed at `0x4E3701`). Zero hits for `Civ3Edit`/`civ3edit`/`Scenario
Editor`/lowercase `editor`. The editor is a separate binary — confirmed.

## Open

* Mode-1 (tree-literal) streams: implemented per disassembly, unverified.
  Exhaustive survey of all 68 scenario/save files in the install: 32 are
  mode-0 DCL (`00 06 84/86`), 36 are raw (`BIC…`/`CIV3…` magic first),
  **zero** are mode 1. The `0x649980` build is mirrored on the exe's exact
  flat table layout, including its `0xFF`-marker writes. Verifying mode 1
  needs a file the game never ships — likely only producible by the
  `0x648920` compressor path, if at all.
