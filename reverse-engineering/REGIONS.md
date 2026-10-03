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
| 1 | `0x40–0x44` | City-view buildings art, city screen, advisors, diplo advice, AI flavor weights; the attitude and deal-scorer methods `0x440100` / `0x440AD0` / `0x440B60` / `0x440EE0` and the tech valuation `0x448BF0` | swept (`graphics-city.md` BLDG/era-roads, `ai.md` flavors, `ui.md` advice idiom, `diplomacy.md` 5, 6, 8, `research-ai.md`) |
| 2 | `0x45–0x46` | Air combat moves, diplomacy/government/victory vote | swept (`air.md`, `diplomacy.md` gov/vote) |
| 3 | `0x47–0x49`, `0x55`, `0x62–0x64` net | Multiplayer: GameSpy, DirectPlay, FNetQueue, lobby/staging UI. Verified negative: all 23 `WSOCK32` imports are ordinal-only with **zero** direct `call [iat]` sites — socket I/O runs through statically linked middleware; bound this region from GameSpy/DPN/FNetQueue strings, not IAT xrefs. First-sweep child stalled; second sweep retired it: vtable + `readData`/pack/measure, race assignment, full `0x48` bucket map (`multiplayer.md`) | swept (`multiplayer.md`) |
| 4 | `0x4A–0x4B` | Combat resolve + step guards; city disorder/hurry/riot/spaceship | swept (`ai.md` hurry/disorder/spaceship; the city's per-turn totals `0x4B0330..0x4B10F0` and the tile wrapper in `yields.md`); the happiness family decoded and differentially tested (`happiness.md`: recompute `0x4BCFF0` and helpers `0x4BD420..0x4BDBE0`, disorder and riot `0x4BDFF0`, celebration `0x4BE440`, city-turn order `0x4BE970`, produces-units tick `0x4BE730`; unread: the city-turn callees `0x4B2E10`, `0x4B2F80`, `0x4B45A0`, `0x4B4970`, `0x4AC140`, `0x4B9950`); combat core decoded (`combat.md`: odds `0x4A0ED0`, duel loop `0x4A53A0`, retreat flags `0x4A47C0`, defender choice `0x4A1590..0x4A1910`, ranged attack `0x4A3A70`, target loop `0x4A1FA0`, city strike `0x4A2650`, tile strike `0x4A2550`, cruise missile `0x4A2B40`, defensive bombard `0x4A1AE0`/`0x4A3280`, interception duel `0x4A4520` (`air.md`); unread: `0x4A4B30`, `0x4A7280`) |
| 5 | `0x4C–0x4D` | Map-view renderer, terrain art, units/actions/founding, civilopedia | swept (`ui.md` civilopedia callee, founding re-verified) |
| 6 | `0x4E–0x4F`, `0x54`, `0x56–0x58` | Movies, DataIO, setup/title, `main`, barbarians; palace/replay bodies live in `0x56–0x58`, not `0x54` (sweep correction) | swept (`media.md`, `ai.md` barbarian/log-queue, `resources.md` GOOD accessor); city capture and transfer `0x563370` / `0x563410` / `0x564800` decoded (`capture.md`); victory, score and elimination decoded (`victory.md`: `CheckVictory` `0x4F1B60`, final announcement `0x4F0E60`, Retire `0x4E0130`, sequential vote `0x4F6810`, elimination / respawn `0x568950`, score `0x5382B0` / `0x538480`, culture threshold `0x538230`) |
| 7 | `0x50–0x53` | Diplomacy engine, espionage missions, advisor UI, ambience | swept (`diplomacy.md`: contact `0x501CD0`, `declareWar` `0x501F20`, `makePeace` `0x5025B0`, the deal executor `0x502D90`, `canTalk` `0x501910`, dialogs/espionage); decoded and tested (`rust/src/diplomacy.rs`); unread: the per-type price bodies `0x438650..0x438F90`, `0x43B540`, `0x43B730` and the AI's own initiative `0x43E470` / `0x43D610` beyond their gates |
| 8 | `0x59` | Scenario load/save (`Scenario::loadUNIT/BLDG/PRTO`), BIQ codec | swept (`biq.md` load sequence + tag inventory) |
| 9 | `0x5A–0x5D` | Unit-AI scans, combat strings/flow, wonders/victory, world setup; the three tile-yield `Map` methods `0x5D7180` / `0x5D75F0` / `0x5D7AD0` | swept (`ai.md` assassin, `media.md` wonders, `yields.md` tile yields) |
| 10 | `0x5E–0x5F`, `0x60–0x61`, `0x65` | Mapgen pipeline (done, audit only) + UI framework + CRT tail | swept (`ui.md`: event registry, tag parser, CRT tail) |

## Anchor addresses (verified)

Mapgen entry `0x5D16F0`, pipeline `0x5EB580`, fractal `0x5E1B60`,
`Random` class `0x60BA80`/`0x60BAB0` (map instances and the gameplay instance
`0xA526B4`), MSVC rand/srand `0x64A20E`/`0x64A201` (no combat die), combat odds
`0x4A0ED0`, art loader
`0x598580`, view vtable `0x66A508`, tile draw `0x4C3210`/`0x4C3880`,
action gate `0x5C1AD0`, combat `0x5B6820`/`0x4A53A0`, founding
`0x4D9D85`, disease `0x5C7C7B`, AI scans `0x5AF703` ff.
Details stay in the system files.

Round and world-event anchors (specified in `turn.md`, `world-events.md`, `barbarians.md`): round processor
`0x4F5EF0`, `Player::turn` `0x5604B0` (slot 0 = the barbarian branch `0x5604D7..0x561200`), per-unit turn
`0x5C7700`, city turn `0x4BE970`, resource upkeep `0x4F4CB0`, camp founding `0x55F9F0`, uprising `0x55FD00`,
camp destruction `0x565A00`, tile-owner-change handler `0x5D3AB0` (called only by the border routine
`0x5D4830`; both are specified in `borders-culture.md`). The range `0x55–0x56` therefore holds the barbarian and event bodies, not only multiplayer.
