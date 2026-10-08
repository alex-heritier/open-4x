> Current correction: `rust/src/rivergen.rs` recovers the river generator at
> `0x5F07D0`. Its actual mask is Cell +4; the historical owner-byte trace below
> was withdrawn. The playable square map now adapts this generator through
> `src/rivers.rs`, with native growth logic but custom topology and density.
> The mask supplies freshwater, commerce and directional defense. The 4x4
> art is addressed at blended tile corners (NW/NE/SW/SE branches); source
> inspection and rendered continuous rivers verify this art interpretation,
> not the old renderer addresses below.

# Rivers — placement system and art path

**Headline: no `generateMap` stage places rivers.** All twelve stages are
identified (`NOTES.md` §10); the three object-placement stages are resources,
goody huts, and barbarian camps. A raw `.text` scan finds zero `RIVR`/`RIVE`
tags. Rivers are a separate system applied to a generated map. What follows
is what that system looks like from the outside.

## Map-view art path (verified this session)

`Art\Terrain\deltaRivers.pcx` (`0x6699D8`) has exactly one `.text` ref,
at `0x4C5FDA`:

```asm
0x4c5fda  mov ebx, 0x6699d8            ; "Art\Terrain\deltaRivers.pcx"
0x4c5fdf  lea ecx, [eax+0x1801c]
0x4c5ff2  push 1
0x4c5ff4  push ebx
0x4c5ffa  call 0x598580                ; generic asset loader (ecx=0x9C3508)
```

The loader sits in a loop (`esi += 0x80` to `0x200`, `ebp += 0x40` to `0x100`):
a 4x4 terrain art-table load. `deltaRivers` is one entry of the map-view
terrain table, loaded through the same `0x598580` path as the other tiles.
The table load is inline in the map-view art-init function spanning roughly
`0x4C5BC0..0x4C60FF` (no direct callers to `0x4C5F9F` itself; art singleton
`0x9C3508`, string table near `0x66A504`). Per-tile river lookup inside the
frame renderer is still unmapped.

`Art\Terrain\mtnRivers.pcx` (`0x669ADC`) has **zero** `.text` refs — its name
is likely constructed at runtime (cf. the `x`/`w`/`l` prefix scheme in
`graphics-terrain.md`) or read from the second table. Unresolved.

On disk (`civ3/civ3-gog/app/Art/Terrain/`): `deltaRivers.pcx`, `mtnRivers.pcx`,
`waterfalls.pcx` — delta and mountain variants are separate sprites, so the
renderer picks a river sprite by terrain context, not by one universal sheet.

## City-view art path (verified this session)

`RiverBack.pcx` (`0x680D6C`), `NOTtheRiver.pcx` (`0x680DB8`),
`RiverFore-FP.pcx` (`0x680DC8`), `RiverFore.pcx` (`0x680DDC`) each have one
ref, all inside `0x407C30` — the city-view background loader. At `0x407D56`:

```asm
0x407d56  cmp edx, 9
0x407d59  je 0x407d92
0x407d5b  cmp edx, 0xa
0x407d5e  je 0x407d92
; else: patch local path with bytes at [0x680DDC] ("Rive..." = RiverFore)
; 9/0xa: patch with bytes at [0x680DD0] (the -FP floodplain variant row)
```

So the city view selects `RiverFore.pcx` vs `RiverFore-FP.pcx` on a mode
value in `edx` (9/10 = floodplain mode). On disk
(`Art/City View/Backgrounds/`): `D-H2O-RiverFore.pcx`,
`D-H2O-RiverFore-FP.pcx`, `D-H2O-NOTtheRiver.pcx`, `D-H2O-RiverBack.pcx` —
`<mode>-H2O-<layer>.pcx`. `NOTtheRiver` is the segment-break mask at map
borders, exactly as hypothesised in `NOTES.md` §14.3.

`TERR_River` (`0x728D8C`, one ref at `0x4D2B87`) is a civilopedia section key
copied into a struct next to a `clamp(...,0,0x19)` call — editor/civilopedia
data, not map data.

## Frame-renderer tile access (static census, superseded below)

A byte-pattern census of indirect vtable calls over `.text` (map-view
`0x4C` range only — the real tile renderer lives at `0x57Fxxx/0x580xxx`,
found dynamically; see next section):

| slot | sites | map-view (`0x4C`) sites |
|---|---|---|
| `0x94` connection-mask getter | 26 (15 mapgen, rest scattered) | **none** |
| `0xF4` mask OR | 4, all mapgen | none |
| `0xC4` secondary class | 61 | `0x4C3200`, `0x4C322A`, `0x4C5553` |
| `0xC8` terrain id | 260 | `0x4C38B0` (different object: extra out-param, `+0x2E0` field — slot numbers collide across classes) |
| `0x9C` feature id | 61 | none |

## Dynamic tracing: the render path (verified live under Wine/winedbg)

Setup: `Civ3Conquests.exe` under the bundled Wine Staging 11.16,
`EGYPT.SAV` loaded, breakpoints via a fifo-driven `winedbg` session.
Image base is `0x400000`, so static VAs are runtime VAs. Full runbook with
corrections in `dynamic-tracing.md`.

Cell field-getter jump table (`__thiscall`, cell in `ecx`, 16-byte aligned):

| addr | impl | slot users |
|---|---|---|
| `0x5EAA70` | `mov al,[ecx+4]; ret` | slot 37 (`0x94`) |
| `0x5EAA80` | `mov al,[ecx+5]; ret` | slot 38 (`0x98`) — the render path below |
| `0x5EAA90` | `mov eax,[ecx+0xC]; ret` | slot 40 |
| `0x5EAAA0` | `mov eax,[ecx+0x10]; ret` | slot 41 |
| `0x5EAAB0` | `mov eax,[ecx+0x28]; ret 4` | slot 42 |
| `0x5EAAC0` | `mov eax,[ecx+0x30]; ret` | slot 43 |
| `0x5EAAD0` | `mov ax,[ecx+0x18]; ret` | slot 44 |
| `0x5EAAE0` | `mov ax,[ecx+0x1A]; ret` | slot 45 |
| `0x5EAAF0` | `mov ax,[ecx+0x1E]; ret` | slot 46 |

Nibble extractors over the slot-0 base value (`slot0` =
`0x5EA4E0: mov eax,[ecx+0x2C]; ret`):

| addr | impl | slot (vtable `0x6701C8`) |
|---|---|---|
| `0x5EAB20` | `call slot0; shr 8; and 0xF` (bits 8–11) | slot 49 (`0xC4`) |
| `0x5EAB30` | `call slot0; shr 12; and 0xF` (bits 12–15) | slot 50 (`0xC8`) |

Vtable `0x6701C8` (observed cell class; other Cell subclasses exist with
different impls at the same slots — the render trace below hit a class
whose slot 35 (`0x8C`) is `0x5EAA30`, while `0x6701C8`+`0x8C` =
`0x5EA840`): slot 0 = `0x5EA4E0`, 37 = `0x5EAA70`, 38 = `0x5EAA80`,
40 = `0x5EAA90`, 41 = `0x5EAAA0`, 43 = `0x5EAAC0`, 44–46 =
`0x5EAAD0/0xE0/0xF0`, 48 = `0x5EAB10`, 49 = `0x5EAB20`,
50 = `0x5EAB30`.

Per-tile render (in `0x57Fxxx`/`0x580xxx`) calls slot 38 (`byte[cell+5]`)
**three times per tile** (return sites `0x57F895`, `0x57F8A8`,
`0x580B7A`; results masked to a byte into `ebp`/`edi`). The combine block:

```asm
; 0x57F8B9..: jne taken? else:
0x57f8bf  movb 0x48(%esp),%cl
0x57f8c3  testb $0x20,%cl; je SKIP
0x57fe46  testb $0x40,%cl; je SKIP
0x57fe94  testb $0x10,%cl; je SKIP
0x57fec6  testb $8,0x48(%esp); je SKIP
0x57fed1  testl %edi,%edi; jle SKIP     ; maskARG (2nd slot-38 result) > 0
0x57fed5  ecx = 263*esi; edx = esi + 8*ecx (= 2105*esi)
0x57fee4  movb 0xA53BC8(%edi,%edx,4),%al ; overlay gate table
0x57feeb  testb %al,%al; je SKIP
```

Overlay gate table at `0xA53BC8`, row stride 8420, rows observed
(`esi` = 1 live; rows 0–2 dumped, rest zeros):

| row | index → 1 |
|---|---|
| 0 | 1..31 |
| 1 | 0, 8 |
| 2 | 0, 20, 21, 22 |

**Correction (2026-10-01, [`combat.md`](combat.md) section 14.3).** This table is not a
river-overlay gate. `0xA53BC8` is the base of every player's **at-war byte table**
(`Player +0xD30 + civ`, row stride 8420 = the player record stride): row 0 is the
barbarians, who are at war with civs 1..31; row 1 says civ 1 is at war with civs 0 and 8; row 2
that civ 2 is at war with 0, 20, 21, 22. The "mask" read through slot 38
(`0x5EAA80`, `byte[cell+5]`) is the **owner civ id of the tile** (the same byte is the civ id
handed to `provoke` at `0x5B358F` and compared with the victim civ in `declareWar`), so this
render block draws an overlay where the viewer is at war with the tile's owner. The
live-sample "mask" values (`0x06` and so on) are civ ids. Everything below that treats
`byte[cell+5]` as a river mask is withdrawn.
Live samples (load-screen render of `EGYPT.SAV`): cell `0x0B6BFCD0`
= `[vtable 0x6701C8, +4 = 0x0600, +8 = FFFFFFFF, +12 = 0xD9]`
(`byte[+5]` = `0x06`), `[+0x2C]` = `0x00001100` (nibble8-11 = 1,
nibble12-15 = 1); cell `0x0B6BC909` `[+0x2C]` = 0; cell `0x0B6BFD09`
`[+0x2C]` = `0xFF000000`. Flags byte `0x48(%esp)` = `0x09` on the
sampled tiles, so all four `0x78` gate bits clear → table skipped.
Cells are individually heap-allocated (pointer deltas 57, 13255,
13312 — no array stride).

`0x4C31A0` full body (was "pure wrapper" — corrected): saves
`edi`, calls `0x47B530`, on nonzero clears bits `~0xF3` in global
`0xA52680`, calls `0x5F4570` (overlay composer — head: null-checks
`[esi+0x2E0]+0x148`, predicate slot `0x14`, seeds `edi = 9`, big
`stdcall` dispatch to slot `0x54`), then `cell = [esi+4]`,
null-checked `call *0xC4(%eax)` (slot 49), `ret 0x1C`.

## Storage: what selects the river segment (narrowed, not closed)

* ~~Presence/gate: tile flags (`0x48(%esp)` bits `0x78` all set) AND
  `byte[cell+5] > 0` AND `table[esi][mask] != 0`.~~ **Withdrawn**: that is the
  owner/war overlay gate (see the correction above), not the river layer.
* Segment index candidates: ~~low nibble of `byte[cell+5]`~~ (withdrawn: it is the
  owner), `byte[cell+4]` (the gameplay river set), `[cell+0x2C]` bits 8–11 (slot 49, consumed by
  the `0x4C31A0` wrapper), or bits 12–15 (slot 35, consumed by the
  render overlay code). Which layer `esi` selects (river vs road vs
  irrigation) and which nibble feeds the sheet blit need one
  `table = 1` trace on a river tile — recorded as the next dynamic
  step, not guessed.
* **Gameplay consumer (2026-10-01, [`combat.md`](combat.md) section 4.1).** The
  combat river bonus reads slot 37 (`0x5EAA70`, `byte[cell+4]`), not slot 38, and
  tests `(byte >> dir) & 1` with `dir` in 0..7 (defender toward attacker). So the
  river *edge* data that gameplay uses is the sibling byte `byte[cell+4]` in an
  eight-direction index, while this section's renderer path reads `byte[cell+5]`.
  How the two bytes relate (shared source, or one derived from the other) is open.

## Reference implementation

`rust/src/rivers.rs`: `RiverMask` 4-edge nibble type, segment continuity
rule, delta/mountain variant selector, city-view background filename
builder. Marked `HYPOTHESIS` where it goes beyond the verified art path.

## Playable-map integration and setter verification (2026-10-04)

A fresh static read verifies the mark path, independent of the old owner-byte
trace: `0x5F0370` resolves the two edge tiles through map vtable +0x30, then
calls their vtable +0xF4 at `0x5F03E1` and `0x5F0444`. The target `0x5EACC0`
ORs the supplied mask into **Cell +4** (`0x5EACCA..0x5EACCC`), tests effective
terrain through +0xC8, and changes terrain 0 (Desert) to 4 (Flood Plain) at
`0x5EACD7..0x5EACE1`. This contradicts the historical hills-only reading.

The game rotates its square grid into a temporary native grid, using 24-tile
wrapped copies at each horizontal edge, numbers native continents, and calls
the recovered generator. It then folds the masks back, makes shared edges
reciprocal at the seam, and rebuilds diagonal continuity bits. The adapter
changes topology and source density; it is not exact native map generation.
Terrain generation otherwise remains the game's existing noise generator.

The shipped mtnRivers and deltaRivers 4x4 sheets now draw at blended tile
corners with NW, NE, SW, SE branch bits. Coastal corners select delta art.
Native sprite selection has not been decoded, so this selection is a clone
rendering choice verified visually for continuity. Asset preparation clears
the green exterior as well as magenta. Loading a save rebuilds these terrain
entities along with the ground.

Tile masks supply freshwater, the verified +1 commerce (`yields.md` 4.3),
Flood Plain food/mining rules, city river eligibility, and +25 directional
combat defense (`combat.md` 4.1), including diagonal and wrapped directions.
Save format 8 records the byte. River-crossing movement costs, disease and
special floodplain decoration still need integration. Agricultural freshwater
food rules also remain incomplete.
