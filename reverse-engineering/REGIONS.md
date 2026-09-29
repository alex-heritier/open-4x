# Executable region map (`Civ3Conquests.exe`)

High-level code atlas: PE skeleton, per-region ownership, and what is
already reversed vs open. Descriptive. Child agents own one region group
each; findings land in system files, never here. Method: PE header +
import table + `push`-immediate string clustering per 64 KB text bucket
(`probe: /tmp/region_probe.py`, reproducible) + all previously verified
addresses.

## PE skeleton

| part | range | notes |
|---|---|---|
| `.text` | `0x401000–0x665000` | all code below |
| `.rdata` | `0x665000–0x680000` | art paths, terrain filename table (`0x6690B0`) |
| `.data` | `0x680000–0x73E000` | script names, art rows, `GOOD`/`TERR` tags |
| `.rsrc` | `0xCD1000–` | BITMAP + ICON + GROUP_ICON + VERSION only — no dialogs/menus/strings: all UI is custom-drawn |
| entry | `0x64C9F0` (CRT startup) | `main` seeds RNG at `0x56C1F9` |
| heap singletons (not file-backed) | `0x9Cxxxx`, `0xA5xxxx` | map/cell/AI tables (`0x9C74D4` W, `0xA52EB4` bits) |

Imports of note: `WSOCK32` (23, multiplayer net), `OPENGL32` (17 — one helper at `0x62B2A9–0x62B5F5`: context setup,
ortho, a single stippled `glBegin`/`glEnd` line batch in 2 colors,
grid/paths),
`binkw32` (16 — one linear player at `0x62BA32–0x62BB7B`, each API
called exactly once), `WINMM` (10: `timeGetTime` pervasive
— densest `0x43`/`0x56`; mm timer `timeSetEvent`/`KillEvent` only in
`0x62`; mmio WAV-chunk parsing only in `0x63`, tag compares scattered
with a `0x5D` cluster suggesting save-chunk reuse), `IFC23` (7 —
Immersion force-feedback: `CImmProject`/`CImmDevice`, cf. the `0x60`
bucket's `Logitech iFeel MouseMan`), `USER32` (33), `GDI32` (3: pixel-format setup for the GL context in
`0x62`; `StretchBlt` imported but never directly called — frame present is
indirect), `KERNEL32` (107).

## Region groups (fan-out units)

| # | range | owns | state |
|---|---|---|---|
| 1 | `0x40–0x44` | City-view buildings art, city screen, advisors, diplo advice, AI flavor weights | swept (`graphics-city.md` BLDG/era-roads, `ai.md` flavors, `ui.md` advice idiom) |
| 2 | `0x45–0x46` | Air combat moves, diplomacy/government/victory vote | swept (`air.md`, `diplomacy.md` gov/vote) |
| 3 | `0x47–0x49`, `0x55`, `0x62–0x64` net | Multiplayer: GameSpy, DirectPlay, FNetQueue, lobby/staging UI. Verified negative: all 23 `WSOCK32` imports are ordinal-only with **zero** direct `call [iat]` sites — socket I/O runs through statically linked middleware; bound this region from GameSpy/DPN/FNetQueue strings, not IAT xrefs. First-sweep child stalled; second sweep retired it: vtable + `readData`/pack/measure, race assignment, full `0x48` bucket map (`multiplayer.md`) | swept (`multiplayer.md`) |
| 4 | `0x4A–0x4B` | Combat resolve + step guards; city disorder/hurry/riot/spaceship | swept (`ai.md` hurry/disorder/spaceship) |
| 5 | `0x4C–0x4D` | Map-view renderer, terrain art, units/actions/founding, civilopedia | swept (`ui.md` civilopedia callee, founding re-verified) |
| 6 | `0x4E–0x4F`, `0x54`, `0x56–0x58` | Movies, DataIO, setup/title, `main`, barbarians; palace/replay bodies live in `0x56–0x58`, not `0x54` (sweep correction) | swept (`media.md`, `ai.md` barbarian/log-queue, `resources.md` GOOD accessor) |
| 7 | `0x50–0x53` | Diplomacy engine, espionage missions, advisor UI, ambience | swept (`diplomacy.md` dialogs/espionage) |
| 8 | `0x59` | Scenario load/save (`Scenario::loadUNIT/BLDG/PRTO`), BIQ codec | swept (`biq.md` load sequence + tag inventory) |
| 9 | `0x5A–0x5D` | Unit-AI scans, combat strings/flow, wonders/victory, world setup | swept (`ai.md` assassin, `media.md` wonders) |
| 10 | `0x5E–0x5F`, `0x60–0x61`, `0x65` | Mapgen pipeline (done, audit only) + UI framework + CRT tail | swept (`ui.md`: event registry, tag parser, CRT tail) |

## Anchor addresses (verified)

Mapgen entry `0x5D16F0`, pipeline `0x5EB580`, fractal `0x5E1B60`,
map RNG `0x60BA80`, game rand/srand `0x64A20E`/`0x64A201`, art loader
`0x598580`, view vtable `0x66A508`, tile draw `0x4C3210`/`0x4C3880`,
action gate `0x5C1AD0`, combat `0x5B6820`/`0x4A53A0`, founding
`0x4D9D85`, disease `0x5C7C7B`, AI scans `0x5AF703` ff.
Details stay in the system files.
