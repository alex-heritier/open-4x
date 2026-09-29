# The Conquests scenario editor (second image)

Owns: everything about `civ3-gog/app/Conquests/Civ3ConquestsEdit.exe`
(1 824 120 B, md5 `249faa156f21b2492b3046f4bb793363`, MFC via
`oledlg`/`OLEPRO32`/`COMCTL32`). Base `0x400000`, `.text`
`0x401000–0x4D6E6B` (vsize `0xD5E6B`), `.rsrc` `0xBE6C0` (dialog-heavy).
The game-exe atlas stays in `REGIONS.md`; this file owns the editor image
only. Unprobed siblings: `Civ3Edit.exe` (1 258 872 B, md5 `45912d4b…`),
`civ3PTW/Civ3XEdit.exe` (1 299 832 B, md5 `e8fd31e9…`).

## Why it matters

The game exe contains no editor code (verified: `biq.md` — only
`EDITORBLDG/WHO/UNIT` data tags). The editor reads and writes the same
`.biq` sections, so its section handlers mirror the game's load path and
— more valuable — its **writers** are the only in-repo model of the
save path (`biq.md` save lead `0x59F752–0x59F930`, still open).

## Tag census: editor is tag-richer than the game (verified: byte grep)

| tag | game | editor |
|---|---|---|
| `PRTO` | 10 | 60 |
| `TERR` | 30 | 55 |
| `GAME` | 20 | 31 |
| `RULE` | 11 | 20 |
| `ESPN` | 5 | 10 |
| `FLAV` | 2 | 12 |
| `WSIZ` | 4 | 14 |
| `WMAP` | 3 | 21 |
| `BICX` | 10 | 2 |
| `BICQ` | 7 | **0** |

`BICQ` zero in the editor (HYPOTHESIS: it cannot open/write one variant,
probably `.sav`). Zero hits for `GCON_`/`DIPLOAD`/`FNetQueue`/
`CULTUREBORDER`: pure data editing, no AI or game logic. No `DCL`/
`decompress`/`implode` markers in either image (codec has no signature
strings, as expected from `biq.md`).

## Full `3D` tag dispatch (verified: imm32 scan, 241 arms, all 28 tags)

Unlike the game's single dispatcher (`0x594290`), the editor compares
tag dwords at per-section clusters — likely load+save+validate triplets
(HYPOTHESIS). Arms per tag (first VA each):

| tag | arms | first arm | tag | arms | first arm |
|---|---|---|---|---|---|
| `BLDG` | 9 | `0x4243A6` | `RACE` | 20 | `0x41A1A8` |
| `CITY` | 12 | `0x42982A` | `RULE` | 8 | `0x41E359` |
| `CLNY` | 4 | `0x471BF0` | `SLOC` | 3 | `0x471A3A` |
| `CONT` | 3 | `0x471BE5` | `TECH` | 15 | `0x41A386` |
| `CTZN` | 6 | `0x4280C3` | `TERR` | 10 | `0x453E7C` |
| `CULT` | 7 | `0x404C36` | `TFRM` | 8 | `0x450171` |
| `DIFF` | 8 | `0x41A37F` | `TILE` | 10 | `0x4565EA` |
| `ERAS` | 9 | `0x41A534` | `UNIT` | 15 | `0x469858` |
| `ESPN` | 7 | `0x431663` | `VER#` | 3 | `0x404F64` |
| `EXPR` | 8 | `0x4330C3` | `WCHR` | 3 | `0x471B51` |
| `FLAV` | 7 | `0x4344F3` | `WMAP` | 8 | `0x41A0A7` |
| `GAME` | 8 | `0x419A9C` | `WSIZ` | 6 | `0x466D73` |
| `GOOD` | 17 | `0x41E6D4` | `LEAD` | 13 | `0x419A91` |
| `GOVT` | 12 | `0x41A53F` | | | |

`TILE`/`UNIT`/`CITY`/`WMAP`/`SLOC`/`CONT`/`WCHR` cluster at
`0x468xxx–0x476xxx` (HYPOTHESIS: map-I/O region). `TERR` arms pack at
`0x453E7C–0x4542FC`. No push-imm refs to tag strings: the editor
compares dword immediates only.

## Resource math mirror at `0x4901FC` (verified: `r2`)

The editor carries a second copy of the game's resource-placement math —
a full cross-confirmation of `resources.md`:

```asm
0x4901fc  push 0x444F4F47            ; 'GOOD'
0x490203  call [eax+0x8C]            ; same vfunc-0x8C list query as the game
0x49021e  mov eax, [eax+0x40]        ; frequency field, same offset
0x490225  push 0x1A (26) ...         ; freq==0 fallback:
0x490249  lea eax, [edi+eax+0x32]    ; rand(26)+rand(26)+50, exact
0x490258  mov eax, 0x51EB851F        ; /32 magic multiply
0x49026c  push 0x52524554            ; 'TERR', then the TERR list query
```

Same tag pushes, same `+0x40` freq field, same fallback, same `/32`. The
editor links the same engine core (or list-manager class), so its GOOD/TERR
row handling is a second angle on the game's variable-length file rows.

## Save path narrowed (verified: IAT scan + `r2`)

`WriteFile` (IAT `0x4D731C`) has exactly 4 `.text` callers:

* `0x4A8C46` — stderr logger (`GetStdHandle(-12)`), excluded.
* `0x4B852A` — generic buffered writer method (`this+4` = handle,
  `this+0xC` = error sink). Zero direct `E8` callers: reached indirectly
  (vtable) or dead.
* `0x4A7370` / `0x4A73D5` — chunked stream writer with byte accounting
  (same function, `0x65` apart).

The per-section writer arms live behind the latter two; which `3D` arm of
each cluster feeds them is still open.

DCL is present: exactly one `push 0x3134` (`0x497487`), in a
`0x5F76C0`-shaped decode wrapper (`malloc` state, callbacks
`0x497200`/`0x497250`, `call 0x498380`, free via `0x4B6B2E`). Codec
`0x498380` has exactly one caller (`0x4974AB`). Whether the wrapper sits
on the save path (compress on save) or the load path (decompress on open)
is open; entry is ~`0x4973A8` (prior `ret 8` at `0x4973A5`).

## Open (next: writers)

* Which `3D` arm of each cluster feeds the `0x4A7370`/`0x4B850F` writers
  (save-path mirror). Entry leads: wrapper ~`0x4973A8`, writer callers
  via vtable scan.
* Record layouts/validation/defaults per section (cross-check `NOTES.md`
  §13 tile record and the `GOOD`/`TERR` data path). New lever: the
  `0x4901FC` mirror reads the same rows.
* ~~Whether the `0x497487` DCL wrapper sits on the save path~~ CLOSED:
  load path (codec identical to game `0x649400` decompressor; wrapper
  `0x497410` <- single caller `0x404340`). Corollary: no editor DCL
  compressor exists — saves likely raw BIC (verify by hexdump).

## DCL wrapper sits on the load path (verified: `r2` + `scans.py`)

Editor codec `0x498380` is instruction-identical to the game's DCL
*decompressor* `0x649400` (200-instruction normalized diff: only linked
address operands differ). The wrapper is `0x497410` (`sub esp,0x40C`;
`push 0x3134` at `0x497487`; pushes callbacks `0x497200`/`0x497250` at
`0x4974A1/A6`; single codec call `0x4974AB`), with exactly one caller:
`0x404340` (a `0x404xxx` UI-flow function). So the editor decompresses
on open through the same verified codec — no separate editor DCL
semantics to reimplement.

A second callback-push site (`0x497340/45`) feeds `0x4978E0` (buffer-size
buckets 1024/2048/4096 — setup helper, not a codec), not the
decompressor. Since `push 0x3134` occurs exactly once editor-wide, the
editor links NO DCL compressor: saves are most likely written raw
(uncompressed BIC, like the PTW `.bix` files) — worth one save-and-hexdump
check, not a code hunt.
