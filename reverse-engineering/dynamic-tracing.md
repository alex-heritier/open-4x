# Dynamic tracing runbook (Q1 partially executed — see corrections)

Static analysis is exhausted for two questions. Both need a running game
under the bundled Wine runtime. Q1's first half was executed live (Wine
Staging 11.16, `EGYPT.SAV` loaded and rendering); the mechanics below
are corrected for what actually works.

Runtime: `civ3/.runtime/Wine Staging.app/.../bin/wine` (wine-11.16 Staging,
verified `--version`), prefix `civ3/.civ3-gog-prefix/`, exe
`civ3/civ3-gog/app/Conquests/Civ3Conquests.exe`. Image base is `0x400000` —
static VAs are runtime VAs (verified: `x/2c 0x400000` → `M Z`,
module `civ3conquests`).

Session mechanics that work: launch the game *under* `winedbg` from the
start (`winedbg ./Civ3Conquests.exe` from the `Conquests/` dir) with
stdin on a held-open fifo and stdout to a log file. Hold the fifo open
without backgrounding: `mkfifo in && exec 3<>in` in the launch shell
before starting winedbg (`exec 3<>` opens RDWR, never blocks, never
EOFs); then one `printf 'cmd\n...' > in` per batch from other shells.
A non-held fifo EOFs winedbg the moment it stops at a breakpoint and
the debugger silently exits, stranding the game. While the debuggee
runs, commands queue in the fifo and execute at the next stop.
`lldb -p` attach is denied by macOS policy; `winedbg attach` to a
running game page-faults in wow64 glue and the stop is unresumable
(`cont` re-faults, `detach` leaves threads suspended, `quit` kills the
game) — do not attach, always launch under the debugger. GUI driving
works via the CUA driver (foreground clicks/keys) + `screencapture -l
<window-id>`; `press_key return` is more reliable than clicks for
dialogs. First launch under winedbg crashed pre-menu (flaky startup);
retry worked.

Address corrections: the byte-mask getter is at **`0x5EAA80`**
(`mov al,[ecx+5]` — cell in `ecx`, returns **`byte[cell+5]`**), not
`0x5EAA70` (that is the sibling `byte[cell+4]` getter, slot `0x94`).
`display` auto-print silently fails — use explicit `x`/`info reg` per
stop, or bulk `stepi` traces (each step auto-prints). Conditional
breakpoints with `&`/`==` expressions are rejected ("No type or type
mismatch"). **Breakpoints stall the save load** (the load screen
renders the map): disable all breakpoints (`disable N`) + `cont` to
let the load finish. `bt` without args errors; `info share` (not
`info shared`) lists modules. `x ADDR` prints ONE dword, not a dump —
use `x/Nx ADDR` for N dwords (`x/598x` for bulk). `x/s` untested.

## `cont` from a breakpoint is broken (verified 2026-09-29)

`cont` with an enabled breakpoint at EIP crashes the game: winedbg
emulates the breakpoint instruction to step over it
(`be_i386_is_jump`), chokes on unhandled opcodes (`fixme ... unknown
6a` for the `push`es around the `0x5942CA call fopen` site), and
resumes with a garbage EIP (`0x72DA18`, data) → `Unhandled exception:
illegal instruction`. Never `cont` with the current breakpoint
enabled. Two safe protocols (TF single-step itself works — bulk
`stepi` traces auto-print fine):

1. `stepi` once past the breakpoint address, then `cont` (bp stays
   armed for later hits). Cheapest per stop. VERIFIED live 2026-09-29:
   three stops (`0x5942CF` twice, `0x5945B3` once), `stepi` + `cont`
   each, zero crashes, load continued to `Building trade network…`.
2. `disable N` + `cont` (+ `enable N` when the site must fire again).
   Use when `stepi` misbehaves on the site.

Register expressions work in `x`: `x/8x $ebx` (path string),
`x $esi+0x3ccc` (table base) both resolve. At reader post-`fopen`
(`0x5942CF`) the path is in `EBX` and the `FILE*` in `EAX` — no stack
reads needed.

Chained one-shots (multi-stop traces without any step-over): at each
stop capture (`info reg` + `x/Nx` dumps), `disable` that breakpoint,
`cont`; re-`enable` sites that must fire again. `bt` at a stop is
often garbage: mid-argument-push stops show text bytes (`_in_`/`bic_`)
as the "return address" — read args/regs, not the backtrace.

GOOD-load capture spec (one EGYPT load, chained one-shots):

* `*0x5942CF` (NOT `0x5942CA`): post-`fopen` (`mov edi,eax`). `EAX` =
  `FILE*`, path pointer still at `[ESP+4]`, mode (`"rb"`) at `[ESP+8]`.
  Log every hit: `FILE*` → path mapping for the stream hunt below.
* `*0x5945B3` (GOOD loop exit, `mov eax,[esi+0x848]`): `ESI` = reader,
  live table at `[ESI+0x3CCC]`, `EDI` = the GOOD stream's `FILE*`
  (match against the fopen log — this names the source file), `EBP` =
  count, `EBX` = 92×count.
* `*0x5970AD` (writer post-`fopen`, `mov edi,eax`): path in `EBP`,
  `FILE*` in `EAX` (setup `push "wb" / push ebp / call`, `0x5970A2`).
  This is the BIC **save** writer, not a load step: all 7 call sites
  (`0x599FDF` … `0x59B410`) pass the `0x72D9CC` global, i.e.
  `bic__out.tmp`; nothing here writes `save*.tmp` (`biq.md`,
  `resources.md`). Do not arm it expecting the rules stream.

Result of this spec (2026-09-29): two hits at `*0x5942CF` — `bic__in_.tmp`
(the 8329 B `GAME`-section staging file) and then `save1.tmp` (209 222 B) —
and the GOOD exit's `FILE*` matched the `save1.tmp` hit. `save1.tmp` turned
out to be the game's own DCL decode of `conquests.biq`, written by
`0x5F76C0` through `CreateFileMappingA`/`MapViewOfFile` (no `fopen`, so no
breakpoint catches it). Trying to break on its write is a dead end; decode
`conquests.biq` instead.

## Q1: which field selects river segments? (half done)

Done live: break `*0x5EAA80` (slot 38); render calls it 3×/tile
(`0x57F895`, `0x57F8A8`, `0x580B7A`); combine gate + table `0xA53BC8`
dumped; nibble extractors `0x5EAB20` (bits 8–11, slot 49) /
`0x5EAB30` (bits 12–15, slot 50) mapped; `0x4C31A0` full body with the
`call *0xC4(%eax)` tail verified. See `rivers.md` for the full dump.

Still open: one `table = 1` trace on a river tile. Next session:
disable nothing (game state is gone — relaunch), load `EGYPT.SAV`,
scroll the map to a visible river (arrow keys), `break *0x57FEE4`,
`cont`, then `stepi` ×200 in one batch and read which value (mask low
nibble vs `[cell+0x2C]` nibble vs slot-49 return) feeds the sheet blit.
`table[1] = {0: 1, 8: 1}` bounds the search: a lookup-taking tile at
`esi = 1` carries mask 8 (mask 0 is rejected by the `jle`).

Old decision rule (superseded): ~~if river tiles share a `0xC4` class
the dry tiles lack, presence arrives via the class query... If classes
match and `0x94` bytes differ...~~ — replaced by the gate + candidates
in `rivers.md`.

## Q2: who resolves the tutorial city-site names?

The five `TUT_GOOD_CITY_SITE_*` strings (`0x72ED1C`…`0x72EDA4`) have no
static callers. Break on string-compare against the first:

```text
# break on lstrcmpiA / strcmp in the tutorial lookup; condition on
# substring "CITY_SITE". Log the return address: its function is the scorer.
```

Alternatively break on settler city-founding gated through `0x5C1AD0`
and backtrace from there.

## Arming discipline (verified 2026-09-29)

`winedbg` only reads the fifo **at a prompt**. While the debuggee runs, queued
bytes are not consumed: sending `break *ADDR` to a free-running session does
nothing, and a raw `\003` does not interrupt it. So every breakpoint must be
armed in the batch *before* the `cont` that starts the run you want to trace;
adding a site mid-session means kill + relaunch.

Working sequence: `launch <tag>`; wait for the first `Wine-dbg>` prompt (a few
seconds); `send <tag> 'break *0xA\nbreak *0xB\ninfo break\ncont\n'`; then drive
the GUI (Load Game). The next hit stops the debuggee, and the *next* batch of
commands executes there. `info break` confirms the arm
(`Breakpoint N at 0x… civ3conquests+0x…`).

## Hygiene

* Snapshot or copy `civ3/.civ3-gog-prefix/` before running; the game writes saves.
* Headless CI cannot do this (needs a display + GOG assets); run on a Mac
  with the Wine staging runtime above.
* Record results back into `rivers.md` (Q1) and `ai.md` (Q2) with
  addresses, then extend `rust/src/rivers.rs` / `ai.rs` to match.

## Ready-to-run recipes for the three open traces

All three need the GUI (Load Game → `EGYPT.SAV` → Enter) and therefore the
foreground; nothing else is missing.

1. **River segment (Q1).** Arm `break *0x57fee4` (+ `break *0x5eaA80` if the
   gate never fires). On each hit: `info reg`, `x/8x $esp`, then step past the
   bitmap read and dump `[cell+0x2C]` (nibbles 8-11 / 12-15) and the slot-38
   byte, to see which value feeds the sheet blit. Note `0x57FEE4` only fires
   once the four flag bits (`0x48(%esp) & 0x78`), `byte[cell+5] > 0` and
   `table[esi][mask] != 0` all pass, so a hit is already a candidate river
   tile.
2. **Combat odds + HP field.** Arm `break *0x64a20e` (game RNG) and
   `break *0x5bbbc0` (kill path), then start a fight in the loaded save (the
   kill path fires on a death). Capture `info reg` + the caller's frame at
   each RNG draw; the draw that precedes an HP write names the dice.
3. **Settler scorer.** Arm `break *0x5c1ad0` (action gate) and
   `break *0x5b9f90` (the `0x20000002` founding-gate caller), then run AI
   turns (`press_key return` twice per turn) and log the return addresses on
   the founding path; the scorer is the function that decides *where*, i.e.
   the frame above the goto enumerator.

## Live state at hand-off (2026-09-29 late)

A session is running under tag `7` with `break *0x57fee4` and
`break *0x5bbbc0` armed and `cont` already sent (game booting to the menu).
Because the fifo is only read at a prompt, more sites cannot be added to it —
kill it (`pkill -f winedbg`, `wineserver -k`) and relaunch with the full set
if a different trace is wanted.

## No headless load (verified 2026-09-29)

`Civ3Conquests.exe 'Saves\EGYPT.SAV'` boots to the menu and ignores the
argument (no `save0.tmp` after 40s, menu screenshot). There is no CLI
save-load: every dynamic trace needs GUI driving (Load Game dialog) by
a human or the CUA driver — never foreground-drive while the user is
working in another window.
