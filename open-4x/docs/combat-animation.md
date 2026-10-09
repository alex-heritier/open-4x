# Combat animation

This document is the normative specification of how Open 4X shows fights on the map. It owns
the replay record the simulation emits, the playback rules the client follows, and the timing
constants. It does not own combat *rules* (odds, hit points, retreat, capture): those live in
`crates/sim/src/combat.rs` and the README. Scenario and pack formats live in
[`scenario-format.md`](scenario-format.md) and the README.

## 1. Provenance (clean room)

The behaviour below was described from how the `open-civ3` reference plays a fight on screen:
what happens, in what order, at what moments, and what the player can and cannot do meanwhile.
The code in this repository was written from this description, not from the reference's source.

| Allowed | Not used |
| --- | --- |
| Observable behaviour: phase order, one hit point per round, blow lands mid-swing, both sides swing every round, retreat shows no death, unseen fights are not played, input waits | Reference source code, identifiers, data structures, or file layout |
| The idea of tunable timing constants (the numbers here are this project's own) | Civ3 art, animation (`.flc`), sound, or `.ini` files |

The reference animates a *live* world: the fight is resolved first, then played, and the world
changes as the playback lands each blow. Open 4X is client/server: the simulation resolves a
command atomically and the client only ever receives snapshots. §3 is the adaptation.

## 2. Scope

Included: melee duels (land and sea), defensive support shots, bombardment volleys (land and
sea), capture of defenceless units and cities, retreat, promotion, sinking and falling,
hit-point bars during a fight, sound cues, pacing, and skipping.

Excluded, and not to be built before a need is shown:

- Per-frame sprite sheets or a clip format. Art stays one image per design; motion is procedural.
- Camera moves to follow a fight. A fight outside the view is not played (§6).
- Replaying a fight after the fact, or a battle history screen.
- Multi-tile projectiles with terrain collision, weather, or wind.

## 3. Architecture

```
Command ──► sim: attack / bombard ──► Game.battles (records, in order)
                                          │  Game::view(player) keeps only what the player saw
                                          ▼
                              Response::Snapshot  (state is already final)
                                          │
                client: receive ──► Stage.queue ──► drive ──► actors, shells, effects, sounds
                                                       │
                                  refresh hides the tiles the stage is playing on
```

- The simulation **must** stay engine-free and unaware of timing. A record says what happened,
  never how long it takes.
- The snapshot is authoritative and already final. The stage **must not** write to the game; it
  draws *actors* built from the record, and `refresh` hides the real units on the tiles under
  play so each unit appears once. When playback ends the real units show again.
- The record **must not** change any dice draw: the same seed gives the same game with or
  without the log, and the log is identical for a given seed.
- `Game.battles` holds the records of the **latest command only** (a turn of computer play
  counts as one command). It is cleared at the start of the next `apply`, capped at 128 records
  (oldest dropped), and absent from JSON when empty so older saves and snapshots still load.

## 4. The record

```rust
enum Battle {
    Duel    { attacker: Fighter, defender: Fighter, support: Option<Support>,
              rounds: Vec<bool>,            // true = the attacker won that round
              outcome: Outcome,             // AttackerWon | DefenderWon | DefenderRetreated | AttackerRetreated
              retreat_to: Option<Coord>,    // where a retreating defender ended
              promoted: bool },             // the winner gained a level
    Capture { attacker: Fighter, target: Coord, taken: Vec<Fighter>, destroyed: Vec<Fighter>,
              city: Option<String>, advanced: bool },
    Bombard { shooter: Fighter, target: Fighter, shots: Vec<bool>, killed: bool, promoted: bool },
}
struct Support { shooter: Fighter, hit: bool }
struct Fighter { id, owner, kind: String, position: Coord, hp: i32, max_hp: i32 }  // as the clash began
```

- `rounds` ends at the round that decided the fight; every other round precedes it.
- `Fighter.hp` is the value *before* the clash (before a support shot or any round).
- A `Duel` with a `support` shot: the shot is applied to the attacker before round 1.
- `Bombard.shots` holds one entry per volley actually fired: a volley stops at the first kill,
  and, for a non-lethal shooter, at the target's last hit point.
- `Game::view(player)` drops a record unless the player owns a fighter in it or can see one of
  its tiles by the same rule that decides which units they see. A spectator (`player 0`) keeps all.

## 5. Playback

A battle is played as a **script**: a fixed list of beats built once from the record (so
playback is deterministic) and then sampled by the clock.

### 5.1 Phases

| Order | Phase | Present when | What plays |
| --- | --- | --- | --- |
| 1 | Lead-in | always | Actors fade in on their tiles and face each other. |
| 2 | Support | `Duel.support` | The shooter fires once; at the blow the attacker loses a point if `hit`. |
| 3 | Rounds | `Duel`, `Bombard` | One beat per round (duel) or per volley (bombard); see §5.2. |
| 4 | Finale | always | See §5.3. |
| 5 | Tail | always | A short hold, then the actors are removed and the real units show. |

### 5.2 A round

- **Both** fighters play their attack motion in **every** duel round, whoever wins it.
- The beat lasts as long as the longer of the two attack motions (every actor has one).
- The blow lands at `STRIKE_AT` of the beat: the round's loser loses exactly one hit point, its
  bar drops, it flinches, and a hit effect and sound play at it. Damage **must not** show earlier.
- Motions cycle through two variants by round number, so consecutive rounds do not repeat.
- A duel's firing styles (§7) launch a shell at `FIRE_AT` that lands exactly at `STRIKE_AT`: the
  winner's shell hits, the loser's falls short.
- A bombard volley is a beat with only the shooter acting. The shell leaves at `FIRE_AT` and its
  flight depends on the distance (§8); the target takes the blow *when the shell arrives*, and a
  missed shell lands short (splash on water, dust on land). The beat lasts until the shell has
  landed, if that is later than the motion ends. A `charge` style on a bombard has no shell: its
  blow is the contact. A `broadside` fires three shells, `SALVO_GAP` apart, for one hit point.
- A support shot is a one-motion beat before round 1; if it hit, the attacker takes the blow at
  the strike point.

### 5.3 Finale

| Outcome | Loser | Winner |
| --- | --- | --- |
| `AttackerWon`, `DefenderWon`, `Bombard.killed` | falls (land) or sinks (sea) | cheers; a promotion adds a sparkle |
| `DefenderRetreated` | steps to `retreat_to` over `STEP_SECS`; no death, no promotion | stands |
| `AttackerRetreated` | withdraws a few pixels and returns; no death, no promotion | stands |
| `Capture` | each unit in `taken` yields (turns toward the captor's colour); each in `destroyed` sinks | stands; if `advanced` it steps onto the tile |
| `Bombard`, target alive | stands | stands |

The finale lasts as long as its longest motion, and at least `MIN_FINALE`.

### 5.4 Hit-point bars

Each actor shows a bar for the whole script, starting at its recorded `hp` and dropping by one
at each blow it takes. Colours: green above two thirds, yellow above one third, red otherwise.
Real units keep the existing bar; actors replace them under play. A bar stays under its unit
through a lunge, a recoil, or a fall, but goes with it when it **steps** to another square (a
capturer advancing), and fades with it.

## 6. Pacing, visibility, and input

- **Seen only.** A battle is played only when some fighter's tile is inside the camera view
  (grown by one tile). Any other battle is dropped without a trace. The record has already been
  filtered by `Game::view` for what the player may know.
- **Order.** Battles play one at a time in record order.
- **Backlog.** With more than two battles waiting, playback speeds up by 25% per extra battle,
  capped at 4×, so a turn of computer play does not hold the player for minutes.
- **Speed.** `--combat-speed N` (or `FOURX_COMBAT_SPEED`) scales the clock; `N > 0`, default 1.
- **Input waits.** While any battle is queued or playing, orders do not reach the simulation
  (keys, buttons, clicks that would issue a command). Selecting, panning, zooming, and the
  minimap still work.
- **Skip.** `Space`, `Enter`, or the on-screen *Skip fight* button ends playback at once: queued
  battles are dropped and the real units show. Skipping loses nothing, because the snapshot was
  already final. While a fight is on stage `Space` only skips; it does not also end the day.
- **Silence.** Sounds play only for staged battles. A pack may omit any sound.
- **First snapshot.** The snapshot a client starts from (a new game, a loaded save, joining a
  running server) is a starting point, not news: its records are not played. Only a later snapshot
  with a newer revision brings fights to play. A snapshot of an unchanged revision is not replayed.

## 7. Motion and effects

Art is one image per design, so motion is a procedural *pose* (offset, rotation, scale, alpha,
white flash) sampled from a motion's progress `u` in `0..=1`. A design's **style** chooses its
attack motion; the default follows the design, a pack may override it per design.

| Style | Default for | Attack motion (length) | Cues |
| --- | --- | --- | --- |
| `volley` | land units that attack | recoil at `FIRE_AT` (0.80 s) | muzzle flash, smoke, small shell, `volley` |
| `charge` | explicit only (cavalry) | wind-up, thrust, contact at `STRIKE_AT`, recover (0.80 s) | dust, `charge` |
| `gun` | land units with a bombard | heavy recoil (0.90 s) | large flash and smoke, shell, `gun` |
| `broadside` | all sea units | roll and recoil (1.00 s) | three staggered flashes, shells, `broadside` |

| Other motion | Length | Notes |
| --- | --- | --- |
| Fall | 0.90 s | Topples and fades. Sound `fall`. |
| Sink | 1.80 s | Lists, drops, fades; splash and smoke. Sound `sink`. |
| Cheer | 0.70 s | Two small hops (a gentle bob at sea). |
| Yield | 0.90 s | Settles and turns toward the captor's colour. Sound `yield`. |
| Withdraw | 0.60 s | Slides away from the foe and back. |
| Step | `STEP_SECS` | Moves between two tiles. |

Every impact plays sound `hit`. Sound names a pack may declare: `volley`, `charge`, `gun`,
`broadside`, `hit`, `fall`, `sink`, `yield`.

## 8. Constants

Defaults, defined once in `crates/client/src/stage/script.rs` (`LEAD_IN`, `TAIL`, `MIN_FINALE`,
`SALVO_GAP`, the shell flight) and `stage/motion.rs` (`STRIKE_AT`, `FIRE_AT`, `STEP_SECS`):

| Name | Value | Meaning |
| --- | --- | --- |
| `LEAD_IN` / `TAIL` | 0.30 s / 0.20 s | Fade-in before the first beat; hold after the last |
| `STRIKE_AT` | 0.50 | Fraction of a beat at which the blow lands |
| `FIRE_AT` | 0.30 | Fraction of a beat at which a firing motion shoots |
| `SALVO_GAP` | 0.05 s | Spacing of a broadside's shells |
| `MIN_FINALE` | 0.60 s | Shortest finale |
| `STEP_SECS` | 0.40 s | A retreat or advance between tiles |
| Shell flight (bombard) | `distance / 600 px/s`, clamped to 0.25–0.90 s | Time from fire to impact |

## 9. Acceptance

1. Same seed, same game: `battles` are identical across runs and do not change any later dice.
2. Replaying a `Duel`'s `rounds` on the recorded hit points reproduces the final damage of both
   units for every outcome (the survivor's `damage` equals recorded `hp` minus the rounds lost).
3. `view(player)` never returns a record whose tiles the player cannot see and does not own.
4. A script has exactly one blow per duel round, at that beat's `STRIKE_AT`, and one per hit
   bombard volley, when its shell arrives; blows are in order and none precedes its beat.
5. A retreat script contains no fall or sink; a kill contains exactly one for the loser.
6. Total length equals lead-in + beats + finale + tail. Speed scales the *clock*, not the script,
   so the same script plays in `1/speed` of the time.
7. Poses are continuous: every non-terminal motion starts and ends at the rest pose.
8. A pack with an unknown style, an unknown sound name, or an unsafe sound path is rejected.

## 10. Looking at it

`--demo-combat NAME[@SECONDS]` queues hand-made fights around the middle of the screen once the
camera has settled, without playing to a real one. `NAME` is `duel`, `charge`, `support`,
`naval`, `bombard`, `broadside`, `capture`, `retreat`, or `all`. With `@SECONDS` the clock stops
at that moment of the script, so `--screenshot` catches one exact frame. The fights name made-up
unit ids and touch nothing in the game; put the camera over water with `--focus X,Y` to see the
naval ones at sea. `--combat-speed N` (or `FOURX_COMBAT_SPEED`) scales the clock, which also gets
a late freeze moment inside the screenshot's frame budget.

```bash
cargo run -p fourx-client -- --scenario dawn-straits --zoom 0.7 \
  --demo-combat duel@1.5 --screenshot /tmp/duel.png --smoke
cargo run -p fourx-client -- --scenario dawn-straits --focus 15,23 --demo-combat naval
```
