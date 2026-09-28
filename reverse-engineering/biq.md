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

## Open

* Mode-1 (tree-literal) streams: implemented per disassembly, unverified.
  Exhaustive survey of all 68 scenario/save files in the install: 32 are
  mode-0 DCL (`00 06 84/86`), 36 are raw (`BIC…`/`CIV3…` magic first),
  **zero** are mode 1. The `0x649980` build is mirrored on the exe's exact
  flat table layout, including its `0xFF`-marker writes. Verifying mode 1
  needs a file the game never ships — likely only producible by the
  `0x648920` compressor path, if at all.
