# Reverse-engineering status: what is specified, what is not

This file is an **inventory by document**, not an audit and not a coverage percentage. It exists so a
clean-room implementer can see which systems have a specification written to the standard of
`../.agents/skills/reverse-engineering-executables/SKILL.md` (every claim cites an address, ordered steps,
integer semantics, golden vectors, open items listed) and which do not. The executable
(`Civ3Conquests.exe`, PE32, MSVC 6) is **not fully reverse engineered**; the lists below are the gaps the authors
know about. No function-count coverage figure is given: an earlier census exists only as scratch output outside
this repository, with no recorded method, so any number quoted from it would be unsupported. A census must be
rerun and its method written down before one is claimed.

Tags used in every document: **V** read from the instructions, **H** hypothesis, **O** open (not decoded),
**E** executed (emulator or live run), **A** read from raw disassembly with the function body opened.

## 1. Specified to clean-room grade (written in this effort)

Each has ordered steps with addresses and its own open-items section; most carry golden vectors (hand-derived
consequences of the text, not captured runs, unless a document says otherwise).

| document | system | open-items section |
|---|---|---|
| [`victory.md`](victory.md) | score, every victory-point source, `CheckVictory`, the eight victory types | 14 |
| [`turn.md`](turn.md) | the round processor and `Player::turn` | 7 |
| [`world-events.md`](world-events.md) | resource upkeep, pollution and meltdown, nuclear counter, global warming, volcanoes | 8 |
| [`barbarians.md`](barbarians.md) | barbarian slot, camps, tribes, uprisings | 10 |
| [`unit-turn.md`](unit-turn.md) | per-unit turn, ship sinking, disease, healing, unit death | 9 |
| [`goody-huts.md`](goody-huts.md) | goody-hut triggers, outcome roll and all outcomes | 10 |
| [`city-founding.md`](city-founding.md) | `createCity`, `City::init`, naming | 7 |
| [`city-turn.md`](city-turn.md) | the city turn sequencer, production system, building completion | 11 |
| [`hurry.md`](hurry.md) | hurrying production, citizen removal | 10 |
| [`city-buildings.md`](city-buildings.md) | adding and removing a building (`0x4ACF40`), culture per building | 10 |
| [`trade-network.md`](trade-network.md) | the trade network: road/air/water connection matrix, incremental update, resource availability and supply records, resource-deal writers | 10 |
| [`unit-upgrades.md`](unit-upgrades.md) | unit upgrades: eligibility, replacement type, price, execution, Upgrade All, the AI helper (its callers are open) | 11 |
| [`borders-culture.md`](borders-culture.md) | culture accumulation and level, ring enumerator, tile-owner recompute, tie-break, owner-write side effects, empire culture total, the culture flip | 11 |
| [`disease.md`](disease.md) | city disease: terrain tile count, the literal tech-8 cure, infection and cure rolls, citizen loss | 7 |
| [`worker-jobs.md`](worker-jobs.md) | worker jobs: job rows, order tokens, gates, work rate, completion effects (automation and movement excluded) | 12 |
| [`colonies.md`](colonies.md) | colonies, airfields, radar towers, outposts: pools, site gate, creators, vision footprints, destroyers, transfer, all triggers, bombard destruction | 11 |
| [`espionage.md`](espionage.md) | espionage: the nine ESPN rows, mission agents, start dispatcher, cost / quote / setup / dispatcher, the nine executors, the incident tail, the computer player's driver and pickers (what happens to the Diplomat/Spy unit afterwards, the human menus and the wire format are open) | 12 |
| [`vision.md`](vision.md) | vision: sight masks and primitives, line-of-sight mask (emulator-verified), unit sight predicate, unit sight refresh and its call sites, air reveal, territory sight (consumers of the masks in rendering / combat / AI are open) | 9 |
| [`movement.md`](movement.md) | movement points and allowances, `setPosition` (full), teleport, mover stages and return codes, evaluator entry guards (evaluator body, path finder `0x580540`, ZOC, order executors are open) | 9 |
| [`unit-ai.md`](unit-ai.md) | unit command pump, strategy dispatcher and handler table, three small handlers (handler bodies, notably Terraform / worker automation, are open) | 9 |

## 2. Specified earlier, not re-audited to the same standard

These were written before the clean-room directive. They carry V/H/O tags, but some statements are marked
"child-reported" or cite a Rust "Reference" module; treat the Rust as non-authoritative and the text as the claim.
Known corrections are applied where a later document found one (for example `economy.md` culture).

`combat.md` (12), `air.md` (5), `diplomacy.md` (13), `government.md` (10), `happiness.md` (12), `yields.md`,
`economy.md`, `capture.md` (13), `research.md` (13), `buildable.md`, `primitives.md` (7), `resources.md`,
`rivers.md`, `NOTES.md` plus `mapgen.md` (map generation, 18), `stacking.md`, `biq.md`, `biq-format.md` (7),
`savegame.md` (9).

`research-ai.md` (10) was **not written by this effort** and has not been audited by it.

## 3. Partial, lead-only, or presentation

| document | state |
|---|---|
| [`ai.md`](ai.md) | census, gates and several routines; the planners, the scoring body and the purchase logic are open (section 4 of this file) |
| [`workers.md`](workers.md) | leads only (string-negative verdict, goto enumerator, struct head); the job rules themselves are in `worker-jobs.md`, worker **automation** is open |
| [`multiplayer.md`](multiplayer.md) | mode global and gates only; the message protocol is not specified |
| [`editor.md`](editor.md) | tag census and dispatch map; the binding step is open |
| [`ui.md`](ui.md), [`graphics-terrain.md`](graphics-terrain.md), [`graphics-units.md`](graphics-units.md), [`graphics-city.md`](graphics-city.md), [`blending.md`](blending.md), [`media.md`](media.md) | art tables and renderer facts, not gameplay rules |
| [`dynamic-tracing.md`](dynamic-tracing.md) | a runbook; its traces were not run to completion |

## 4. Known gaps (gameplay systems with no specification yet)

Listed with the entry points already identified. Each is a piece of rule logic a port must reproduce.

1. **Leaders and armies** beyond the pieces in `research.md` 10.2 and `buildable.md`. (Trade network:
   `trade-network.md`; unit upgrades: `unit-upgrades.md`; espionage: `espionage.md`, whose open items include the
   Intelligence Agency link and the fate of the Diplomat/Spy unit.)
2. **Unit movement (partial), zones of control, worker automation.** Specified: vision / line of sight (`vision.md`), movement
   points and `setPosition` (`movement.md`), worker job rules (`worker-jobs.md`). Open: the step evaluator `0x57F360` beyond its
   entry guards, the path finder `0x580540` (modes other than the network fill), zones of control, the mover's interior
   (`movement.md` 9), the go-to order executors, and the worker **automation** handler, which is now located (the Terraform strategy handler `0x45C750`, `unit-ai.md` 5) but not decoded.
3. ~~Colonies~~ Specified in `colonies.md` (open items there, section 11). Borders and the culture flip:
   `borders-culture.md`.
4. **AI decision making**: planners `0x446840`, `0x445EA0`, `0x441F80`; the scoring body `0x442480`; the item chooser
   `0x42C8A0`; the purchase logic `0x433CD0`, `0x433EE0`; the category masks; every unit-AI handler body of `unit-ai.md` 3.
   (Settled: `0x449B20` and the strategy dispatcher `0x4611F0` are `unit-ai.md`; `0x4F4F70` is a sprite-pose reset, `turn.md` 2.3; `0x42BEE0`
   is `city-turn.md` 10.)
5. **The screens** `0x5CF640`, `0x5A3910`: assets and control flow only (`city-buildings.md` 11); layout and widget contents are open. The Golden Age start is in `research.md` section 11 only. (Disease:
   `disease.md`.)
6. **Whole-game persistence**: that a loaded save reproduces every field the specifications read (`savegame.md`
   has the grammar and the open list).
7. **Everything behind a virtual call that no document resolved**: each document's "O" list names its own.

## 5. Open consistency items between documents

* `primitives.md` and `research.md` still point at the pre-split `turn.md` section numbers in a few places.
* `economy.md` and `happiness.md` name the city-turn calls `0x4B2F80` and `0x4B4970` with older wording; the
  authoritative sequence is `city-turn.md` section 2.
* `diplomacy.md` still words the "split" rule differently from the multiplayer gate `0x47B530` it is actually
  tied to (to be corrected); `combat.md` has a few unresolved words.
* The Rust modules under `rust/` and the game crates under `../src/` predate the clean-room directive and are not
  the specification. In particular `rust/src/happiness.rs` should be treated as a historical reference.
* `README.md` still describes the directory as "descriptive (not normative) documentation plus a Rust reference
  implementation"; the documents are now the normative part.
