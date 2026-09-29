# Dynamic tracing runbook (Q1 partially executed — see corrections)

Static analysis is exhausted for two questions. Both need a running game
under the bundled Wine runtime. Q1's first half was executed live (Wine
Staging 11.16, `EGYPT.SAV` loaded and rendering); the mechanics below
are corrected for what actually works.

Runtime: `.runtime/Wine Staging.app/.../bin/wine` (wine-11.16 Staging,
verified `--version`), prefix `.civ3-prefix/`, exe
`civ3-gog/app/Conquests/Civ3Conquests.exe`. Image base is `0x400000` —
static VAs are runtime VAs (verified: `x/2c 0x400000` → `M Z`,
module `civ3conquests`).

Session mechanics that work: launch the game *under* `winedbg` from the
start (`winedbg ./Civ3Conquests.exe` from the `Conquests/` dir) with
stdin on a held-open fifo (`tail -f /dev/null > fifo` keeps the write
end open; one `printf 'cmd\n...' > fifo` per batch) and stdout to a log
file. While the debuggee runs, commands queue in the fifo and execute
at the next stop. `lldb -p` attach is denied by macOS policy; `winedbg
attach` to a running game page-faults in wow64 glue — do not attach,
always launch under the debugger. GUI driving works via the CUA driver
(foreground clicks/keys) + `screencapture -l <window-id>`; `press_key
return` is more reliable than clicks for dialogs. First launch under
winedbg crashed pre-menu (flaky startup); retry worked.

Address corrections: the byte-mask getter is at **`0x5EAA80`**
(`mov al,[ecx+5]` — cell in `ecx`, returns **`byte[cell+5]`**), not
`0x5EAA70` (that is the sibling `byte[cell+4]` getter, slot `0x94`).
`display` auto-print silently fails — use explicit `x`/`info reg` per
stop, or bulk `stepi` traces (each step auto-prints). Conditional
breakpoints with `&`/`==` expressions are rejected ("No type or type
mismatch"). **Breakpoints stall the save load** (the load screen
renders the map): disable all breakpoints (`disable N`) + `cont` to
let the load finish. `bt` without args errors; `info share` (not
`info shared`) lists modules.

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

## Hygiene

* Snapshot or copy `.civ3-prefix/` before running; the game writes saves.
* Headless CI cannot do this (needs a display + GOG assets); run on a Mac
  with the Wine staging runtime above.
* Record results back into `rivers.md` (Q1) and `ai.md` (Q2) with
  addresses, then extend `rust/src/rivers.rs` / `ai.rs` to match.

## No headless load (verified 2026-09-29)

`Civ3Conquests.exe 'Saves\EGYPT.SAV'` boots to the menu and ignores the
argument (no `save0.tmp` after 40s, menu screenshot). There is no CLI
save-load: every dynamic trace needs GUI driving (Load Game dialog) by
a human or the CUA driver — never foreground-drive while the user is
working in another window.
