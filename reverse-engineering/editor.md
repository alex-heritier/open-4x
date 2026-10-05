# The Conquests scenario editor (second image)

Owns: everything about `civ3/civ3-gog/app/Conquests/Civ3ConquestsEdit.exe`
(1 824 120 B, md5 `249faa156f21b2492b3046f4bb793363`, MFC via
`oledlg`/`OLEPRO32`/`COMCTL32`). Base `0x400000`, `.text`
`0x401000–0x4D6E6B` (vsize `0xD5E6B`), `.rsrc` `0xBE6C0` (dialog-heavy).
The game-exe atlas stays in `REGIONS.md`; this file owns the editor image
only. Unprobed siblings: `Civ3Edit.exe` (1 258 872 B, md5 `45912d4b…`),
`civ3/civ3-gog/app/civ3PTW/Civ3XEdit.exe` (1 299 832 B, md5 `e8fd31e9…`).

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

**Correction (2026-09-29): these three are the statically linked CRT's write
cores, not per-section writers.** All four sites live in two CRT functions —
`0x4A68D0` (3 sites) and `0x4AEDB0` (1 site) — in the `0x4A0000`-`0x4B0000`
region that also holds `KERNEL32.DLL`/`DisableThreadLibraryCalls`/
`__MSVCRT_HEAP_SELECT` strings, i.e. the `/O`-built `_write` path every
`fwrite` funnels through. The real save path is `0x403F80` (section below);
no `3D` arm feeds it.

DCL is present: exactly one `push 0x3134` (`0x497487`), in a
`0x5F76C0`-shaped decode wrapper (`malloc` state, callbacks
`0x497200`/`0x497250`, `call 0x498380`, free via `0x4B6B2E`). Codec
`0x498380` has exactly one caller (`0x4974AB`), and the wrapper sits on the
*load* path (`0x497410` <- `0x404340`) — see the section below.

## Save path: a straight-line serializer, not a `3D` dispatch (verified 2026-09-29)

The editor's scenario save is **one straight-line writer**, `0x403F80`
(SEH scope `0x4CEC96`; sole caller `0x403F6C`), and it is an
instruction-level mirror of the game's `0x597070`:

| step | editor | game (`0x597070`) |
|---|---|---|
| clear flags bit 1 on the flags object | `[flags+8] &= ~2` (`0x403FE9`) | `[ebx+8] &= ~2` (`0x597093`) |
| write magic | `mov [esp+0x30],'BICX'` + 4-byte write (`0x40401C`) | `'BICQ'` (`0x5970C4`) |
| write `VER#` + version `1` | `0x404038`, `0x404052`, two vfunc-`0x38` writes | `0x5970DB`, `0x5970F2`, two `fwrite`s |
| init sub-object (`[+0xc]=0xC`, `[+0x10]=8`) | `0x404061` (`this+0xF43C`) → `0x4975A0` | `0x597102` (`this+0x8CC`) → `0x5E8080` |
| then the section writers | `0x40408C` ff | `0x59712A` ff |

So **no `3D` arm feeds the writers**: the tag compares (`editor.md` tag
census) belong to the readers/validators, and the writer reaches section
data through the list accessors, i.e. the `push TAG` sites at `0x40808D`
(`GOOD`), `0x408186` (`PRTO`), `0x408298` (`TERR`), `0x408455` (`RULE`),
`0x410B35` (`GOOD`), … (the game's `0x599600`-shaped `getListEntry`).

Corollary for the earlier lead: `0x4A7370`/`0x4A73D5`/`0x4B852A` are **not**
per-section writers. They sit inside the statically linked CRT
(`0x4A68D0` / `0x4AEDB0`: the `/O`-built `_write` cores — `KERNEL32.DLL`,
`DisableThreadLibraryCalls`, `__MSVCRT_HEAP_SELECT` live in that range), so
they are the low-level `WriteFile` path every `fwrite` funnels through. The
editor has exactly four `WriteFile` call sites in `.text` (`0x4A7370`,
`0x4A73D5`, `0x4A8C46`, `0x4B852A`) and three `ReadFile` sites.

Raw-vs-DCL on save is settled by code: the only `push 0x3134`
(decompressor state) is in the load-side wrapper, and the writer emits
`BICX` into a stream opened for write (`0x4B7D79`) with no compressor call,
so saves are raw BIC.

## Open

* Per-section writer bodies behind `0x403F80` (record layouts/validation
  per section) — cross-check `NOTES.md` §13 tile record and the `GOOD`/`TERR`
  data path; the editor's row accessors are the `push TAG` sites above.
* Record layouts/validation/defaults per section — cross-check `NOTES.md`
  §13 tile record and the `GOOD`/`TERR` data path. New lever: the
  `0x4901FC` mirror reads the same rows, and `0x403F80` writes them.
* Raw-vs-DCL is settled by code (2026-09-29): the writer emits `BICX`
  straight into a stream opened for write, and the editor links **no**
  compressor (the only `push 0x3134` state is the load-side decompressor
  wrapper `0x497410`). A confirming hexdump needs a GUI editor session
  (open a scenario, Save As, hexdump the first 16 bytes): recipe only.

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

## Property pages name every rule field (verified 2026-09-29)

The editor's rules UI is plain Win32 dialogs, so the **dialog resources**
carry a human-readable name for every field of every BIQ rules section. This
is the cheapest oracle in the repo for "what is row offset `+0xNN`" — no
GUI session needed, the labels are bytes in the exe.

Regenerate with `scripts/pe_dialogs.py`:

    python3 scripts/pe_dialogs.py civ3/re/Civ3ConquestsEdit.exe [substring ...]

It walks the PE resource tree (`RT_DIALOG` = type 5) and prints
`id <n> <KIND> (<x>,<y>,<cx>,<cy>) '<text>'` per control; the **control id**
is the key back into the code path, because the page handlers push exactly
those ids. Template strings are UTF-16 (the `style` field is at 0 and
`DS_SETFONT` adds point size + face after the title). 34 of 40 dialogs walk
cleanly; six `DLGTEMPLATEEX` ones (141, 155, 179, 187, 188, 189) still
desync on the DWORD menu field and print `PARSE-FAIL` — their titles are
recoverable from the file if ever needed.

Page inventory (id, title, control count):

| 131 General Settings (153) | 132 Worker Jobs (18) | 135 Natural Resources (35) |
| 137 Terrain (86) | 139 World Sizes (24) | 143 Civilization Advances (50) |
| 147 Combat Experience (12) | 151 Units (160) | 153 Diplomats and Spies (16) |
| 157 Improvements and Wonders (166) | 161 Citizens (29) | 163 Difficulty Levels (52) |
| 165 Culture (25) | 169 Civilizations (123) | 190 Scenario (110) |
| 191 Players (34) | 206 Flavors (8) | 207 Locked Alliance (30) |
| 208 Victory Point Limits (43) | 209 Disasters! (25) | 210 Crop Map (10) |

### Answers this settles

* **Road movement is RULE data.** Page 131 has the group `Road Movement`
  (id 1030) with `Movement Rate Along Roads:` (label 1031, edit 1032,
  spin 1033). So the step cost is data-driven exactly as `workers.md`
  concluded from the absent `/3`; there is no fraction reader to find in
  the movement code.
* **Zone of Control is a unit ability, not a movement rule.** Page 151 has
  `Zone of Control` (id 1535) next to `HP Bonus`/`Bombard Str.` — a PRTO
  ability checkbox. That is why no ZOC string exists in the game image:
  the only labels live in these dialog resources. The "ZOC gate" is a test
  of that ability bit in the movement chain, not a separate rule.
* **`Good` fields have names**: page 135 = `Type` radio group
  (`Bonus Resource` 1082 / `Luxury` 1087 / `Strategic Resource` 1086),
  `Bonuses` (`Food:` 1083 / `Shields:` 1084 / `Commerce:` 1085), `Icon:`
  (1013 + spin 1077), `Prerequisite:` (1074), `Civilopedia Entry:` (1622),
  and the group `Strategic and Luxury Resources` (1534) holding
  **`Appearance Ratio:`** (label 1088, value 1017, spin 1089) and
  **`Disappearance Probability:`** (label 1091, value 1018, spin 1092).
  These two are the named candidates for the unmapped GOOD `+0x44` and the
  `+0x50` tail — see `resources.md`.
* **Worker-job timing**: page 132 = `Worker Job:`, `Prerequisite:`,
  `Required Resources`, `Civilopedia Entry:`, `Order:`, and `Turns to
  complete:` (label 1063, value 1013, spin 1014) — the TFRM row's turn
  count.
* **Culture page 165** = `Cultural Opinion:` list, `Culture Ratio:` (two
  spins, i.e. a ratio a:b), `Chance of Successful Propaganda:` (%),
  `Resistance Chance:` `Initial:` (%) + `Continued: (%)`.
* **Terrain page 137** confirms the `workers.md` mapping and names the rest:
  `Tile Values` (Food/Shields/Commerce), `Terraform Bonuses`
  (`Irrigation (Food)`, `Mining (Shields)`, `Road (Commerce)`),
  `Movement Cost:` (label 1116, value 1013), `Defense Bonus:` (1118, 1014),
  `Worker Job:`, `Pollution Effect:`, `Possible Resources:`, `Flags`
  (`Allow Cities`/`Allow Colonies`/`Allow Airfields`/`Allow Outposts`/
  `Allow Radar Towers`/`Allow Forts`/`Impassable`/`Impassable by Wheeled
  Units`), the `Landmark Information` block (its own tile values, movement
  cost and defense bonus) and `Disease` (`Causes Disease`/`Cured by
  Sanitation`/`Disease Strength`).
* **Page 157** lists every BLDG flag by name (e.g. `Center of Empire`,
  `Resistant to Propaganda`, `Reduces Corruption`, `Allows City Size Level
  2/3`, `Doubles City Growth Rate`, `Increases Shields in Water`, …), and
  page 151 every PRTO ability/action (`Ignore Move Cost:`, `Charm`,
  `Stealth Attack`, `Enslave`, `Teleportable`, `Telepad Range:`, `Worker
  Strength`, `Create Craters`, `Collateral Damage`, `Sacrifice`,
  `Science Age`, plus the whole worker/air mission/standard order lists).

### Still open (the binding step)

The labels are UI-order evidence, not offsets. A page handler reads its own
object, e.g. the Goods page does `lea edx,[edi+0xa4] ; push edx ; push 1082 ;
push hwnd ; call 0x4be1c0` (setter helpers `0x4be0be`/`0x4be11c`/`0x4be1c0`/
`0x4be293`), so `+0xa4` is the *editor's* mirror field, not the GOOD row
byte. To bind control id → BIQ row offset, follow the editor's row
serializer behind `0x403F80` (or its reader) for one section and match the
control-id set of that page; the wrapper pattern to grep is
`push 1 ; push 0 ; push <id> ; mov ecx,esi ; call 0x4b59da` followed by
`mov [row+off], eax` (13 sites in the editor, none yet attributed to GOOD).

Why this is a dig rather than a lookup (all verified 2026-09-29):

* The page handlers are **shape-uniform setter code**, not a descriptor
  table: `lea reg,[edi+<objoff>] ; push reg ; push <id> ; push hwnd ; call
  <helper>` — e.g. the Goods page at `0x434D00` binds 1011←`+0x8c`,
  1074←`+0xa0`, 1082←`+0xa4`, 1013←`+0x90`, 1622←`+0xb0`; the Units page at
  `0x459E7F` binds 1535 (`Zone of Control`) ← `+0xb4` through the bit
  helper `0x4be16e`. Those are *editor object* offsets; the object is not
  the row (the Goods row is 88 B while the object carries lists at
  `+0x8c`/`+0xa0`/`+0x524`).
* The writer is abstract: `0x403F80` reaches section data through
  `push TAG` lookups, then calls a shared section helper `0x404EF0` with
  the **same opcode `0x8033` at 33 sites** (`0x404F52`, `0x4080A6`,
  `0x4081A2`, `0x4082B5`, `0x408372`, `0x4084F1`, `0x410081`, …) — i.e.
  one call per BIQ section, with the real per-section body behind it.
* The editor's *document* model is **OLE structured storage**, not the BIQ
  layout: it imports `ole32.dll`/`oleaut32.dll` and its reader driver
  `0x49B15B` (a 64-field pointer block at `[esi+0xb8]`/`[esi+0xc8]`/…)
  dispatches through a vfunc with the GUID pair at `0x4E5EF0`
  (`CF51ED10-62FE-11CF-BF86-00A0C9034836`) and `0x4E5F00`
  (`0000010A-0000-0000-C000-000000000046`). So the row layout exists only
  on the flattening path.

Two ways forward, both cheap relative to reading a writer body: (a) a live
editor session — open a scenario, change exactly one field (e.g. the road
movement rate), *Save As*, and diff the two files: the changed bytes are the
field's row offset, and the page/control tells you its name; (b) read the
single section helper body behind `0x404EF0` for the section of interest.

### ZOC progress (game side, verified)

The ability bit has at least one confirmed consumer: `0x45AE9F` loads the
unit's PRTO row (`[unit+0x40] * 0x138 + [0x9C71E0]`), reads the ability dword
at row `+0x8c`, and tests **bit 13 (`0x2000`)** (`shr eax,0xd ; test al,1`)
before calling the goto validator `0x5C59B0(-1,-1)` — an ability-gated
movement/order path, exactly the shape a ZOC gate would take. Which bit is
`Zone of Control` specifically still needs the editor's flag unpacker or a
live toggle; do not assume 13 without that check.

## Units page: the PRTO AI-strategy dword is bit-mapped (verified 2026-09-29)

Flag unpackers are findable by idiom; the page that owns a slot by its
checkbox run:

```asm
    mov  edx, [eax + 0x8c]   ; the flag dword *inside the row*
    shr  edx, N              ; shr edx,1 uses the short form `d1 ea`
    and  edx, 1
    mov  [esi + SLOT], edx   ; the page object's flag slot
...
    lea  eax, [edi + SLOT]   ; the page binds slots to controls
    push eax ; push <controlId> ; push hwnd ; call 0x4be16e
```

`0x4be16e` is a **tri-state** checkbox helper (`SendMessage 0xf0`
`BM_GETCHECK` / `0xf1` `BM_SETCHECK`, value clamped to 0..2), so editor flags
are 0/1/2, not booleans.

Units page (dialog **151**): unpacker run `0x45e658`-`0x45e787`, checkbox run
`0x459e7f`-`0x45a3c0`. Joined with `scripts/edit_slots.py`, the row `+0x8c`
bit map is:

| bit | slot | control | label | | bit | slot | control | label |
|---|---|---|---|---|---|---|---|---|
| 0 | 0x168 | 1540 | Offense | | 10 | 0x190 | 1550 | Naval Transport |
| 1 | 0x16c | 1541 | Defense | | 11 | 0x1a4 | 1556 | Naval Carrier |
| 2 | 0x170 | 1542 | Artillery | | 12 | 0x180 | 1547 | Terraform |
| 3 | 0x17c | 1545 | Explore | | 13 | 0x184 | 1548 | Settle |
| 4 | 0x178 | 1544 | Army | | 14 | 0x188 | 1546 | Leader |
| 5 | 0x174 | 1543 | Cruise Missile | | 15 | 0x1a8 | 1559 | Tactical Nuke |
| 6 | 0x198 | 1551 | Bombard | | 17 | 0x1b0 | 1561 | Naval Missile Transport |
| 7 | 0x19c | 1557 | Defense (air) | | 18 | 0x1d8 | 1563 | Flag Unit |
| 8 | 0x18c | 1549 | Naval Power | | 19 | 0x1dc | 1564 | King |
| 9 | 0x1a0 | 1558 | Transport | | | | | |

(Bit 0 has no shift instruction (`and ecx,1` right after the load); the
bit-0 entry of each dword below is read from the run order — the slot
immediately preceding bit 1 — and is hand-verified for `+0x8c` at
`0x45e63e`.)

The same block continues over **four** flag dwords of the PRTO row
(`0x45e7a8`-`0x45eabf` is the tail of the run), so the whole order/action set
is bit-mapped:

| row | field | bit -> label (control id) |
|---|---|---|
| `+0x8c` | AI strategies | 0 Offense (1540) · 1 Defense (1541) · 2 Artillery (1542) · 3 Explore (1545) · 4 Army (1544) · 5 Cruise Missile (1543) · 6 Bombard (1551) · 7 Defense air (1557) · 8 Naval Power (1549) · 9 Transport (1558) · 10 Naval Transport (1550) · 11 Naval Carrier (1556) · 12 Terraform (1547) · 13 Settle (1548) · 14 Leader (1546) · 15 Tactical Nuke (1559) · 17 Naval Missile Transport (1561) · 18 Flag Unit (1563) · 19 King (1564) |
| `+0xa8` | standard orders | 0 Skip Turn (1219) · 1 Wait (1220) · 2 Fortify (1221) · 3 Disband (1222) · 4 Go To (1224) · 5 Explore (1247) · 6 Sentry (1248) |
| `+0xac` | special actions | 0 Load (1223) · 1 Unload (1225) · 2 Airlift (1226) · 3 Pillage (1227) · 4 Bombard (1228) · 5 Airdrop (1229) · 6 Build Army (1230) · 7 Finish Improvements (1231) · 8 Upgrade Unit (1232) · 9 Capture (1851) · 14 Telepad (1854) · 15 Teleportable (1853) · 17 Charm (1566) · 18 Enslave (1856) · 19 Collateral Damage (1857) · 20 Sacrifice (1858) · 21 Science Age (1263) |
| `+0xb0` | worker actions | 0 Build Colony (1234) · 1 Build City (1235) · 2 Build Road (1236) · 3 Build Railroad (1237) · 4 Build Fort (1238) · 5 Build Mine (1239) · 6 Irrigate (1240) · 7 Clear Forest (1241) · 8 Clear Jungle (1242) · 9 Plant Forest (1243) · 10 Clear Pollution (1244) · 11 Automate (1245) · 12 Join City (1246) · 13 Build Airfield (1850) · 14 Build Radar Tower (1261) · 15 Build Outpost (1260) |
| `+0xb4` | air missions | 0 Bombing (1249) · 1 Recon (1250) · 2 Interception (1251) · 3 Re-base (1253) · 4 Precision Bombing (1254) |
| `byte +0x8e` | misc | bit 0 -> ICBM (1560) |

The clone's worker/order ids (road 22, mine 25, irrigate 26, automate 31 …)
are UI cell indices; these are the **row** bits behind them. Still not
located for the Units page: `Zone of Control` (slot `0xb4`), `Build Barricade`
(`0x1d0`), `Bombard Fx` (`0x1d4`), `Req. Support` (`0x1e0`), `Stealth Attack`
(`0x1e8`), `Create Craters` (`0x1f8`) — a different source (no dword or
`movzx` idiom in `0x45d000`-`0x465000` fills them).

**Correction to the ZOC paragraph above.** Row `+0x8c` is the AI-strategy
field, so the game-side test at `0x45AE9F` (bit 13) is **`Settle`**, not ZOC —
a settler-strategy unit, which is why that path ends in the goto validator.
`Zone of Control` binds to slot `+0xb4`, which this dword does not fill: ZOC,
the standard orders, the worker actions and the air missions (slots `0xd4`-
`0x160`, `0x1b8`-`0x1f8` in the same run) come from **another** PRTO flag
source. Same method with a different `--src` is the probe.

Re-run / extend:

    python3 scripts/edit_slots.py civ3/re/Civ3ConquestsEdit.exe \
        --block 0x459e7f:0x45a3c0 --src 0x8c --dialog 151
