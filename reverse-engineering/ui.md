# UI text and hypertext engine

Owns: civilopedia markup tokenizer, UI string tables. Reference: `rust/src/ui.rs`.

## Civilopedia hypertext tokenizer (verified: `0x5FE287` ff; `0x5FE280` as a literal entry lands mid-instruction)

A jump-table lexer (`jmp [edx*4+0x5FE4E0]`, class table at `0x5FE500`,
input chars `<= 'Z'`) recognizes `$`-tags with `strncmp` (`0x64AE80`):

| tag | match len | token field |
|---|---|---|
| `$LINK<` (`0x728D40`) | 6 | `[edi+0x3C] = 1`, `ebp = 3` |
| `$DROPDOWN` (`0x72FE98`) | 9 | `[edi+0x48] = 1` |
| `$DROPLINK` (`0x72FE8C`) | 9 | `[edi+0x48] = 1`, `ebp = 3` |
| `$$` | 2 | literal `$` escape |
| `$~` | 2 | second escape (same path) |

`[edi+0x48]` is a dropdown state (`1 → 2` on close); `[edi+0x34]` nonzero
re-enters the token path. Sibling match sites: `0x5FE63A`/`0x5FE65F`
(`$LINK`/`$DROPDOWN`), `0x5FE98A`/`0x5FE9DB`, `0x61C168`/`0x61C2C3`.
Renderers in `0x4CE–0x4D1` (civilopedia UI) enforce the link cap
(`Maximum hypertext links exceeded`, `0x728C98`, 7 push sites).

`rust/src/ui.rs`: `tokenize()` (exact tag/literal/escape split), tested.

## Advisor/diplo text lookup idiom (verified: region sweep)

Four identical sites (`0x428CC5` DIPLOADVICEBESTUNIT, `0x429240`
DIPLOADVICEFEAR, `0x429BEB` MILITARYADVICERANK_WEAK, `0x42A01E`
MILITARYADVICEBARBARIANS): `mov eax,[0x72BC58]` (`text\script.txt`);
`push STR; push 1; push eax; mov ecx,0x9C3508; call 0x598580`;
`push eax; call 0x60E6B0; add esp,8; test; jne`. Fail path: `0x60E6D0`,
then `'#'`-scan at `[0xCAD74C]`, then `0x49FC70`. Two sites are preceded
by `0x61C5A0` (report-text slots — corrected role, see `ai.md`).

## Advice branch predicates (verified: preludes at `0x428C70`/`0x429B90`)

* BESTUNIT (`0x428CC5`): `call 0x448B00` on the owner row picks a best
  unit (`-1` = none, skip); `PRTO+8` field (`imul idx,0x138` — the
  PRTO stride) feeds text slot 1. Branch = "counter-unit exists".
* RANK (`0x429BEB`/`0x429C8B`): loop `0x429A1E–0x429BB9` sums military
  power over `[0xA52E90]` entries; `(sum*4)/10` magic-divide compares
  pick `RANK_WEAK` (`0x429BEB`, taken when ratio exceeds the foe
  threshold) vs `RANK_STRONG` (`0x429C8B`); rendering reuses
  `0x517B10` (deal-selector family).
* FEAR (`0x429240`): same idiom as BESTUNIT — `esi != -1` gate
  (`je 0x428491` shared exit), owner-row `0x55A210` thunk, PRTO
  `imul 0x138` + field into slot 1. Branch = "threat unit found".
* BARBARIANS (`0x42A01E`): owner-row `call 0x501760` → nonzero gate
  (`je 0x429D76`); slot 0 from `[0x9C71D0]` record math, slot 1 =
  `threat+0x1E0`. Branch = "barbarian threat present".

`rust/src/ui.rs`: `rank_ratio()`, tested.

## Civilopedia record callee `0x4D2800` (verified: region sweep)

SEH prologue; installs vtable `[esi]=0x66B04C`; `+0x124`/`+0x128`=0,
`+0x12C`=flag from `[esp+0x24]`, `+0x10C`=8, `+0x110`=arg. Flag 0 skips
the alloc path; else `push 0xF0; call 0x649A8B`, construct via `0x5E8AF0`
into `[esi+0x130]`. Loads `TERR_River` (`0x728D8C`) into `[esi+0x44]`
(`0x4D2B86`), calls `0x426710`. Exactly two direct callers (`0x4CAFBD`,
`0x4CB02F`), both inside the function starting at `0x4C9930` — NOTES'
sole-caller claim holds at function granularity. `text\Civilopedia.txt`
(`0x728BF0`) push sites: `0x4CA4C8`/`0x4CA4DB` (same fn), `0x4CD770`/
`0x4CD787`, `0x59A8E4`. Branch semantics on `[esi+0x110]`: open.

## `0x53`/`0x4F` advisor + governor + HOF (sweep; GCON head verified)

* GCON region tooltips `0x5356D0` (parent-verified): SEH frame,
  point-in-rect gates (e.g. x 735–807, y 290–310 →
  `GCON_Agreements` at `0x729D4C`), all via `this=0x9E85F0`. This
  explains every `GCON_*` push (`economy.md` leads included).
* Foreign advisor `0x533290` + FA-trade `0x534CF0` + NODIPLO
  `0x5354B0` + ambience `0x537700`/`0x535D20` + governor window-init
  `0x53ABE0` + HOF dialog-init `0x53F773` (child-reported,
  byte-evidenced; HOF *file* writer is separately mapped at
  `0x540710` in `media.md`).
* DataIO error family `0x50xxxx` + culture advisor `0x4FA5BD`
  (`0x4F` bucket) + credits/cursor-anim ini (child-reported).
* `0x63` GameSpy/mmio spillover (child-reported): version-gate +
  mmio chunk-walk callers; volume mixer tables nearby.

## UI framework: event registry + tag parser + CRT tail (verified: `r2`)

The last unswept region group (`REGIONS.md` #10), now mapped:

* Event registry `0x60BBB0..0x60BC48`: null-checked singleton
  `[0xCAD4B0]`, then the repeated idiom `push 0; push 0; push
  "mouseover..."; call esi` registering `mouseoverdoubleclick`,
  `mouseoverleft/right`, `mouseoverdoubleclick/left|right`,
  `mouseoverwonder`, `mouseoverbuilding`. The `0x60` bucket is the UI
  event-name registry (plus the map RNG `0x60BA80` and CRT heap/new).
* `$TAG` parser continuation at `0x61C2C1` (sibling of the tokenizer's
  `0x61C168`): after the `$DROPDOWN`/`$DROPLINK` `strncmp` matches via
  `0x64AE80`, `[ebx+9]-0x30` parses the control index digit (`cmp 0xA`),
  then a `rep movsd`/`rep movsb` memcpy copies the token span. The `$TAG`
  + digit shape is the control-creation syntax.
* `0x65` CRT tail: SEH scope-table trampolines (`0x657E6B`/
  `0x657E8E`: `mov eax, scopetable; jmp 0x649B2A`) with C++ unwinding
  funclets between them (`call 0x649A80` frees), plus the MSVC
  `Runtime Error!` strings. No game logic; the turn-step SEH handlers
  are vanilla CRT scaffolding.
