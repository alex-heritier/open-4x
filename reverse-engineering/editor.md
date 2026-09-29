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

## Open (next: writers)

* Which arm of each cluster is the **writer** (save-path mirror).
* Record layouts/validation/defaults per section (cross-check `NOTES.md`
  §13 tile record and the `GOOD`/`TERR` data path).
* Whether the editor compresses on save (DCL) or writes raw.
