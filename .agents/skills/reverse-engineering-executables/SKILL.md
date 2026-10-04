---
name: reverse-engineering-executables
description: Reverse-engineer the Civ3 Windows executable the way this repo does it. Static analysis with radare2 and byte-scan probes, region atlas plus read-only fan-out sweeps, Wine/winedbg live tracing when static stalls, per-system findings files, and address-annotated Rust reference implementations.
---

# Reverse-Engineering Executables

How to reverse-engineer `Civ3Conquests.exe` (and its sibling binaries) and land
findings in `reverse-engineering/`, matching the method that produced
the existing notes. Read this before opening the binary.

## Targets and layout

Paths below are relative to the repo root (`open-4x/`). The GOG install and
the reverse-engineering scratch tree both live under `civ3/`.

- Main target: `civ3/civ3-gog/app/Conquests/Civ3Conquests.exe`. PE32, MSVC 6.0,
  static CRT, image base `0x400000`, 3,417,464 bytes. Static VAs are runtime
  VAs under Wine. A working copy also lives at `civ3/re/Civ3Conquests.exe`.
- Editor: `civ3/re/Civ3ConquestsEdit.exe`. Separate binary. The main exe
  contains no editor (verified: case-insensitive `editor` scan finds only 3
  uppercase data tags). Check which binary a question belongs to before
  digging.
- Findings: `reverse-engineering/*.md`, one file per system, plus
  `reverse-engineering/rust/src/*.rs` reference implementations. `NOTES.md` is
  the map-generation source of truth. `REGIONS.md` is the code atlas.
  `dynamic-tracing.md` is the live-debugging runbook.
- Scratch probes go in `/tmp` (`/tmp/re_probeN.py`, `/tmp/sweep_R*.txt`).
  Never commit them. They are evidence of method, not deliverables.
- Static toolkit: `civ3/re/tools/` (`pe.py`, `xrefs.py`, `relx.py`) on
  `civ3/re/.venv` (pefile + capstone). String dumps: `civ3/re/allstr.txt`,
  `civ3/re/strings_all.txt` (r2 `iz` format: paddr, vaddr, section, string).

## The loop

1. **Orient.** Read `REGIONS.md` and the system file for your question first.
   Repeat the open questions there instead of re-deriving them.
2. **Strings and imports census.** `grep` the string dumps for the feature's
   vocabulary (log tags, FOURCCs, ini keys, art paths). Rank import-table
   usage: which DLLs, which functions, and whether calls are direct
   (`call [iat]`) or ordinal-only through middleware.
3. **Atlas before depth.** For unmapped code, place it in a `REGIONS.md`
   region group (64 KB text buckets + `push`-immediate string clustering)
   before disassembling. Never reverse a function without knowing its
   neighborhood.
4. **Static sweeps, read-only.** r2 with `-q` and no project writes, plus
   Python byte scans. Open every disassembly body you cite. Search output
   only locates candidates.
5. **Dynamic tracing when static stalls.** Zero xrefs to a string, ambiguous
   nibble/field selection, and turn-loop roots are dynamic questions. See
   `dynamic-tracing.md` and the section below.
6. **Land it.** Findings file entry with addresses, Rust reference code with
   tests, `cargo test --release` plus `cargo clippy --release` green, README
   ownership table updated. Mark what is verified, what is `HYPOTHESIS`, and
   what stays open. Never upgrade a guess by rewording it.

## Static toolkit

Prefer `r2 -q -c '...'` one-liners with color stripped. The established idioms:

```bash
Q=.agents/skills/reverse-engineering-executables/scripts/r2q.sh
sh $Q 's 0x5EAA70; pd 120' | head -n 130
sh $Q 'px 256 @ 0x6701C8' | head -n 20
sh $Q 'e bin.relocs.apply=true; axt 0x6690B0' | head -n 20
```

- `pd N @ VA`: read function bodies. `px`: dump tables and vtables.
  `axt VA`: cross-references (needs `e bin.relocs.apply=true`).
  Filter `grep -v "^WARN"`.
- `civ3/re/r2env` holds the standard r2 config (no color, bytes on, cache on).
  `civ3/re/script1.r2` shows the flag-plus-xref habit (`f name = VA`, then `axt`).

Python probes (`civ3/re/.venv/bin/python`) use `civ3/re/tools/pe.py` (`Image`: VA
translation, `u32`/`cstr` reads, capstone disassembly). The standard scans:

- **E8 census**: every `E8 rel32` in `.text` resolving to a target
  (`scripts/scans.py calls`). The game RNG (`0x64A20E`) has 70 callers;
  an intermediate recount of 65 wrongly dropped 5 real sites (four `0x5A`
  branch-target draws plus one `0x5C` site), a caution against excluding
  sites by pattern instead of by evidence. Cite your count and your method
  together. (`civ3/re/tools/xrefs.py` currently returns only 3 sites for this
  target; do not trust it.)
- **68 push-imm**: `push imm32` of a string VA or FOURCC tag, to find string
  users and tag dispatch (`GOOD` = `0x444F4F47`, `TERR` = `0x52524554`).
- **3D cmp tags**: `cmp eax, imm32` arms that enumerate scenario-tag
  dispatchers (the `0x594290` tag inventory came from one raw `3D` scan).
- **Vtable slot census**: byte-pattern scan of indirect calls (`call [reg+N]`)
  per slot, split by region, to map which class uses which slot where.
- **String clustering**: group `push`-immediate string refs per 64 KB text
  bucket to label regions (`/tmp/region_probe.py` pattern, reproducible).

Probe shape, from `/tmp/re_probe.py`: `sys.path.insert(0, 'civ3/re/tools')`, scan
`.text` once, print `name (hex): N refs [first 8 VAs]`, then disassemble 3-5
key sites inline. One probe per question, numbered, thrown away after.

Ghidra (12.1.4 headless via pyghidra, `work/decomp.py`) is a backup for
decompilation only. It is not the primary tool and its output never
overrules raw disassembly.

## Tool caveats learned the hard way

- r2 `arg_XXh` / `var_XXh` labels are raw `[esp+N]` guesses. MSVC6's
  `mov eax,size; call __alloca_probe` idiom defeats r2 frame analysis.
  Re-derive every stack slot by simulating `esp` through the prologue and
  cross-check the epilogue.
- Where Ghidra and raw disassembly disagree, raw wins. Ghidra reconstructs
  `__thiscall` virtual-call argument order backwards in places. Push order
  is authoritative (`vfunc(0x48)` takes `(x, y)`, not `(y, x)`).
- Vtable slot numbers collide across classes by design. `Map::vfunc(0x8C)`
  is the FOURCC lookup while `Cell::vfunc(0x8C)` is `isWater`. A
  `0xC4`-shaped call on a view object is not `Cell::secondaryClass`. Always
  name the class with the slot.
- Cells are individually heap-allocated (no array stride). Map/cell/AI
  tables live in heap singletons (`0x9Cxxxx`, `0xA5xxxx`), not file-backed
  sections. Do not expect them in the image.

## Dynamic tracing (live debugging the running exe)

Full trace specs: `reverse-engineering/dynamic-tracing.md`. The mechanics
below are settled and load-bearing; follow them exactly.

- Runtime: `civ3/.runtime/Wine Staging.app/.../bin/wine` (Staging 11.16),
  prefix `civ3/.civ3-gog-prefix/`, game dir `civ3/civ3-gog/app/Conquests/`.
  Snapshot or copy the prefix before running; the game writes saves.
  Image base is `0x400000`: static VAs are runtime VAs.
- Launch the game *under* `winedbg` from the start, stdin on a
  held-open fifo, stdout to a log. `scripts/live_dbg.sh` does this:
  `sh live_dbg.sh launch <tag>` (background it), then
  `sh live_dbg.sh send <tag> 'break *0xADDR\ncont\n'` per batch.
  While the debuggee runs, queued commands execute at the next stop.
  A non-held fifo EOFs winedbg the moment it stops and the debugger
  silently exits, stranding the game.
- Never attach: `lldb -p` is denied by macOS policy, `winedbg attach`
  page-faults in wow64 glue and the stop is unresumable (`cont`
  re-faults, `detach` suspends threads, `quit` kills the game).
  First launch under winedbg often crashes pre-menu; `wineserver -k`
  and retry with a fresh tag.
- **Never `cont` from an enabled breakpoint.** winedbg emulates the
  breakpoint instruction to step over it, chokes on unhandled opcodes,
  and resumes with a garbage EIP (data address) into an illegal
  instruction. Per stop: capture (`info reg` + `x/Nx` dumps), then
  `stepi` once past the site and `cont` (bp stays armed), or
  `disable N` + `cont` (+ `enable N` when it must fire again).
  Break on post-`call` instructions (e.g. post-`fopen`), never on the
  `call` itself.
- winedbg dialect: `x ADDR` prints ONE dword; use `x/Nx` for dumps
  (`x/8x $ebx` for a path string). Register expressions resolve
  (`x $esi+0x3ccc`). `info reg` (whole dump; per-register is
  rejected). `display` auto-print silently fails. Conditional
  breakpoints with `&`/`==` are rejected. `bt` at a stop is often
  garbage (argument bytes as return address): read args and registers,
  not the backtrace. Module list is `info share`. `x/s` untested.
- Settled breakpoint sites (`GOOD` load, verified live 2026-09-29):
  `*0x5942CF` reader post-`fopen` (`EAX` = `FILE*`, path in `EBX`,
  mode at `[ESP+8]`); `*0x5945B3` GOOD loop exit (`ESI` = reader,
  live table at `[ESI+0x3CCC]`, `EDI` = the GOOD stream's `FILE*`,
  `EBP` = count, `EBX` = 92xcount); `*0x5970AD` writer post-`fopen`
  (path in `EBP`, `FILE*` in `EAX`). Open trace sites: river
  `*0x57FEE4`, combat `0x5BBBC0`, settler `0x5C1AD0` / `TUT_*`.
- There is no headless load: CLI save arguments are ignored, so every
  trace needs GUI driving (Load Game dialog) by a human or the CUA
  driver. `press_key return` beats clicks for dialogs. Never
  foreground-drive while the user works in another window.
- Record results back into the owning system file with addresses, then
  extend the Rust module to match. Headless CI cannot do this; it needs
  a display plus the GOG assets on a Mac.

## Pixel and asset forensics

On-disk art settles questions disassembly cannot. The verified habits:

- Decode PCX headers directly (bytes 4..12: `xmin,ymin,xmax,ymax`) and check
  sheet geometry by division: 1152x576 of 128x64 diamonds is a 9x9 matrix,
  2048x1024 is a 16x16 neighbor-mask table, 512x256 is 4x4 (2^4 edge mask).
  Power-of-two cell counts are evidence for mask addressing.
- Classify cells by pixel sampling (vertex/edge regions, 3x3-median) across
  whole sheets. The 9x9 terrain cells turned out to be 3^4 vertex blends
  (`cell = row*9 + col`, `col = 3*W + N`, `row = 3*S + E`), which superseded
  the earlier north-edges-only reading. Re-measure before theorizing.
- Transparency rules (verified by probe): magenta `(255,0,255)` and palette
  index 255 are transparent; pure red `(255,0,0)` is unit shadow rendered as
  translucent black. `tools/prep_assets.py` encodes them.
- `gameplay_screenshots/` is the visual oracle for the clone. Compare with
  `magick identify` and `magick compare -metric AE`. See the
  game-screenshotting skill for capture mechanics.

## Fan-out protocol

For mapping whole regions (the `REGIONS.md` groups), the parent maps and
children sweep:

- Parent writes the atlas first: region ranges, ownership, state, anchor
  addresses. Children never define regions.
- Children are **read-only**: `r2 -q`, Python byte scans, `NEVER write
  files`. Each child owns one region group or one never-located item, reads
  `REGIONS.md` plus the relevant system files first, disassembles 2-4
  concrete functions or sites, and returns `complete` / `evidence` /
  `unresolved`.
- Evidence strings carry exact VA, bytes/mnemonic, string literal, and
  nearest marker. Guesses are marked `HYPOTHESIS`. Sweep reports land in
  `/tmp/sweep_R*.txt` for the parent to synthesize.
- The parent spot-checks child claims with r2 before writing them into
  findings files (`parent-verified` / `child-reported` annotations), then
  extends the Rust modules and runs the test gates itself.

## Evidence and doc standards

- Every verified claim cites its address or its on-disk measurement.
  Findings files are descriptive, not normative: what the binary does, with
  the bytes that prove it.
- Keep a corrections record. `NOTES.md` section 1.2 lists load-bearing
  mistakes that were settled (wrong stage identities, `(y,x)` vs `(x,y)`,
  `==` vs `<=`, x-major vs y-major). Record disagreements rather than
  silently resolving them.
- Verified negatives count as findings: zero `RIVR`/`RIVE` tags in `.text`,
  zero `.text` refs to a string, zero direct `call [iat]` sites for all 23
  `WSOCK32` imports. State the scan that bounds the claim.
- One system owns one findings file and one Rust module. Cross-reference
  instead of duplicating. `reverse-engineering/README.md` holds the
  ownership table and the headline findings; update it when systems land.
- Never-located lists and open questions are deliverables too
  (`ai.md` next targets, `biq.md` open mode-1 streams). An unresolved item
  with a concrete next probe beats a guessed answer.

## Rust reference rules

- Every constant and control-flow decision carries the address it came from,
  so a claim can be checked against the disassembly.
- Original bugs get flags, not silent fixes. `bugs.rs` (`OriginalBugs`)
  models each divergence with its address; the default is the deliberate
  choice documented in the module, and `--bugs` selects variants. A fifth
  suspicion (fractal buffer underflow) died under an exhaustive bounds
  check; record dead suspicions too.
- Port exact integer and float semantics: x87 truncate-to-int matches Rust
  `as i32`; percent-to-fraction through a double intermediate reproduces
  bit patterns a float multiply does not. Byte-verify constants.
- Gates: `cargo test --release` and `cargo clippy --release` clean before a
  finding lands. Independent double implementation for codecs (Rust plus
  Python agreeing on lengths, heads, tails, byte-sums) before calling a
  format solved.
- No stubs for open math. Combat odds and the turn-loop root have models for
  their surroundings and nothing for the core.

## Pitfalls

- The Oceans slider is only a seed, never a threshold. Land fraction comes
  from percentiles of the generated fractal. Do not model it as a water level.
- Cell grid is `(W/2) x H`: `cell = (W>>1)*y + (x>>1)`. Halved-coordinate
  indexing (`(nx>>1) + (W>>1)*ny`) recurs in the renderer, AI scans, and
  step guards. Either cells are half-resolution in X or callers pass doubled
  coordinates; treat it as one convention.
- Flat field reads plus drifting row offsets mean variable-length data sits
  between the reads (`GOOD`/`TERR` file rows vs fixed-stride memory rows).
  Do not force fixed-stride slicing onto a wandering layout.
- Container framing hides in the first bytes: the DCL shift byte doubles as
  the first stream byte, and `file(1)` misidentifies the prefix as TTComp.
  Start the bitstream at byte 2, and distrust magic-based identification.
- Overlay sheets select by (relief, cover) pair (`mountain forests.pcx`,
  `Mountains-snow.pcx`), not by blending. A tile that looks different
  usually draws from a different sheet; find the selection predicate.
- Mode values pick art rows (city-view `RiverFore` vs `RiverFore-FP` on
  `edx` 9/10). Read the compares around the loader before assuming one path.

## Quick reference

```bash
S=.agents/skills/reverse-engineering-executables/scripts
sh $S/r2q.sh 'pd N @ VA' | head -n M
sh $S/r2q.sh 'e bin.relocs.apply=true; axt VA' | head -n 20
grep -i -m 40 "pattern" civ3/re/strings_all.txt
python3 $S/scans.py calls 0x64A20E        # E8 caller census
python3 $S/scans.py pushes GOOD           # 68 push-imm refs
python3 $S/scans.py tags                  # 3D cmp FOURCC inventory
python3 $S/scans.py slots                 # indirect-call slot census
python3 $S/scans.py buckets               # 64KB string clustering
sh $S/live_dbg.sh launch 7               # game under winedbg (background it)
sh $S/live_dbg.sh send 7 'break *0x5942cf\ncont\n'
cd reverse-engineering/rust && cargo test --release
```

## Files

- `reverse-engineering/README.md`: ownership table, quick start.
- `reverse-engineering/REGIONS.md`: code atlas and fan-out units.
- `reverse-engineering/NOTES.md`: mapgen source of truth, method.
- `reverse-engineering/dynamic-tracing.md`: Wine/winedbg runbook.
- `reverse-engineering/tools/emu/`: Unicorn harness that runs the exe's own save
  loader headless on a decoded `.SAV` and traces every chunk and raw read
  (`savegame.md` section 7); use it to verify a grammar instead of transcribing it.
- `reverse-engineering/rust/`: reference implementation + tests.
- `scripts/scans.py`: stdlib-only `calls`/`pushes`/`tags`/`slots`/`buckets`
  scans over any PE32 exe (`--exe` overrides the default search).
- `scripts/r2q.sh`: quiet r2 one-liner wrapper (no color, no WARN lines).
- `scripts/live_dbg.sh`: `launch <tag>` runs the game under winedbg with
  a held-open fifo (`/tmp/civ3dbg/in<tag>`, log `/tmp/civ3dbg/log<tag>`);
  `send <tag> 'cmd\n...'` writes one command batch; `tail <tag>` follows.
- `civ3/re/tools/`: `pe.py`, `xrefs.py`, `relx.py` on `civ3/re/.venv` (needs the
  venv; `xrefs.py` undercounts, prefer `scripts/scans.py calls`).
- `civ3/re/allstr.txt`, `civ3/re/strings_all.txt`: string dumps for census greps.
