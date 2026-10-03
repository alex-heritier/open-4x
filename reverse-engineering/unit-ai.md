# Unit commands and the unit AI: the order pump, the strategy dispatcher and the handlers that were read (clean-room specification, partial)

What is specified here: how the game decides, each turn, which units get to act and which routine drives them (`0x449B20`, `0x4611F0`), the
table that maps a prototype's AI strategy bits to the twenty handlers, the handlers that are small enough to have been read
(Flag Unit, Cruise Missile, ICBM, Explore's order ladder), and where the rest lives (worker automation is the Terraform handler). What is **not**
specified: the bodies of the large handlers (about 67 KB of code: the sum of the address distances in the table of section 3 for the handlers not read, an upper bound) and the target evaluators they call. This document replaces the guesses about
these routines in `turn.md` 2.3 and `worker-jobs.md` 12.

Tags: **V** read from the raw disassembly (function read to the addresses cited); **H** hypothesis; **O** open. No code was run.

Companion documents: `movement.md` (`maxMove`, the mover), `unit-turn.md` (per-unit turn, fields), `turn.md` (where the pump runs), `worker-jobs.md`
(what the Terraform handler ends up calling), `prto.rs` in the BIQ crate (`ai` module: the strategy bit names), `air.md` (order 15).

## 1. Fields and constants used

| datum | meaning |
|---|---|
| `U.+0x64` | the standing **order** (0 none, 1 sentry/fortify, 2..14 timed/job orders, 15 interception, 0x10 go-to, 0x11/0x12 road and railroad to, 0x1A/0x1B/0x1D used by the Explore handler; `unit-turn.md`, `worker-jobs.md` 2) |
| `U.+0x68` | byte, **H**: the automation flag of a human-owned unit (cleared by `0x56BE70`, `unit-turn.md` 3.8, together with the order); a human-owned unit with this flag set is driven by the AI |
| `U.+0x4C`, `U.+0x50` | hit-point damage; movement used (`movement.md` 1) |
| `PRTO.+0x8C` | the **AI strategy** dword (BIQ `ai_strategies`; bit names in section 3) |
| `[0xA526BC]` | human-slot mask; `Player.+0x1C` the slot |
| unit pool | `[0xA52E84]`, last index `[0xA52E90]`; node pointer minus `0x1C` is the unit |

`remaining(U) = clamp(maxMove(U) - U.+0x50, 0, 9999)` (`movement.md` 1).

## 2. The per-turn pump `Player::runUnitCommands(P; phase)` `0x449B20` (`ret 4`, `ecx` = player, `phase` a byte) **V** (read in full)

Callers (**V**, by scan): the turn processor `0x4F5EF0` (twice: `turn.md` 2.1 step 7, phase 0 then phase 1), `0x4DFB90` (three sites, the UI), `0x479BB0`.

```
for each unit u in the pool, index 0 .. last, with u.owner == P.slot (+0x1C):
  phase == 1:                                              // 0x449B3A..0x449BDB
      if remaining(u) <= 0:                                  skip
      if multiplayer (0x47B530) and 0x469590(0x74AF60; u.id):  skip          // the table at 0x74AF60: H, units whose input belongs to a remote player (the code tests 0x47B530 twice; the second test is redundant)
      if 2 <= u.order < 15:                                  u.vt[+0x54]()          // execute the standing order of this unit
  phase == 0:  repeat the following sweep for pass = 0 .. 14 (15 sweeps)           // 0x449BDE..0x449CD8
      r = maxMove(u) - u.+0x50;  if r < 0 or (r <= 9999 and r == 0):  skip           // i.e. remaining(u) <= 0
      if 2 <= u.order < 15:                                  skip                    // those ran in phase 1
      the same multiplayer test as in phase 1:               skip
      if owner is human (bit of [0xA526BC] for the owner's slot):
          if u.+0x68 == 0 and u.order in {0, 1, 15}:         skip                    // a manual unit that is idle, fortified or intercepting does nothing
      u.vt[+0x58](pass)                                        // the unit AI, section 3; the sweep number is its argument
```

So **phase 1** runs every standing timed or job order of every unit that still has movement; **phase 0** gives every other unit with movement up to 15 chances per turn
to act through the AI, and a human's unit takes part only when its automation flag is set or its order is an order of 16 or more (go-to and the like). The vtable slots `+0x54` (no argument) and `+0x58` (one argument, the pass number) of the
unit class were **not resolved** to addresses (**O**: a scan of the data sections for the addresses of the unit methods finds no table, so the Unit vtable is filled at run time or lies elsewhere).
What is known is that the strategy dispatcher of section 3 is reached only from `0x467130` (section 3.1), a no-argument method, **H** that `+0x54` or `+0x58` leads there.

## 3. The strategy dispatcher `0x4611F0` (`ret` plain, `ecx` = unit; returns a bool in `al`) **V** (read in full)

Its only caller is the method `0x467130` (section 3.1).

```
if 0x44E8E0(U) != 0:  goto tail                                          // 0x4611F3..0x4611FA: gate (H: the unit is escorting or following another unit; O)
if owner is not human and 0x5C32F0(U; 0):                                // 0x461200..0x461233  (section 3.2: a foreign-nationality unit)
      (a second, identical call 0x5C32F0(U; 0) follows; it is a pure predicate, so its false branch 0x5BEAE0(U) at 0x461244 is dead code)
      if a city is on U's tile (0x56D2C0):  0x5C2E80(U)                    // H: hand the unit to the city
      else:                                  0x466A60(U; 0x1D)              // H: give it the order 0x1D
      goto tail
if U.owner == 0 (barbarian):  0x44FAE0(U); goto tail                      // the barbarian unit AI (O)
s = PRTO[U.type].+0x8C                                                      // the strategy dword
dispatch on s: the FIRST set bit wins, in the order of the table below
if no bit is set: 0x5BEAE0(U)                                              // default behaviour, O
tail:  r = 0x5BE5B0(U) - U.+0x4C;  return (r < 0 or clamp(r, 0, 9999) == 0)    // 0x46144A..0x461469
```

The return value is true when `r` is negative or zero (H: `0x5BE5B0` is the unit's maximum hit points and `U.+0x4C` its damage, so the value says "the unit is dead"); the pump does not use it.

The strategy table (**V**: the bit tests and call targets at `0x4612A0..0x461445`; the bit names are `biq::sections::prto::ai`; "next" is the distance to the next handler, an upper bound on its size):

| bit | strategy | handler | next |
|---|---|---|---|
| 0 | Offense | `0x4507B0` | 7 520 B |
| 1 | Defense | `0x452510` | 11 312 B |
| 2 | Artillery | `0x455140` | 1 360 B |
| 3 | Explore | `0x455690` | 320 B (section 4.4) |
| 4 | Army | `0x4557D0` | 3 855 B |
| 5 | Cruise Missile | `0x4566E0` | 352 B (section 4.2) |
| 6 | Air Bombard | `0x456840` | 5 280 B |
| 7 | Air Defense | `0x4579E0` | 1 856 B |
| 8 | Naval Power | `0x458120` | 7 104 B |
| 9 | Air Transport | `0x459CE0` | 1 168 B |
| 10 | Naval Transport | `0x45A170` | 7 712 B |
| 11 | Naval Carrier | `0x45BF90` | 2 240 B |
| 12 | **Terraform** | `0x45C750` | 11 744 B (**the worker automation**, section 5) |
| 13 | Settle | `0x45F630` | 2 208 B |
| 14 | Leader | `0x45FED0` | 1 014 B |
| 15 | Tactical Nuke | `0x4602C0` | 640 B |
| 16 | ICBM | `0x460540` | 224 B (section 4.3) |
| 17 | Naval Missile Transport | `0x460620` | 2 240 B |
| 18 | Flag Unit | inline: `0x5B2F10(U; -1)` then `U.setOrder(1)` (`0x5B3040`) | - |
| 19 | King | `0x460EE0` | - |

(The bit-16 test is `test byte [row + 0x8E], 1` at `0x4613F6`, the same bit.) Stock units carry one strategy; a few carry a second through the extra rows
(`alt_strategy_of`), which is why the order matters.

### 3.1 The caller `0x467130` (no arguments, `ecx` = unit) **V** for the skeleton only

`0x467130..0x46758E` (1 132 bytes; the disassembly tables of this effort had mis-attributed it to a function starting at `0x466E98`, which is the middle of the previous routine): exits at once when the byte `U.+0x1E8` is nonzero (H: the unit is mid-action), the game is multiplayer and the unit has no movement left (`0x467141..0x46716D`); clears the shared path-finder scratch `0xB72890..0xB728A0` in multiplayer (`0x46717F..0x4671A0`), resolves the unit linked by `0x5B3030(U)` (H: its escort or
leader), calls the strategy dispatcher at `0x46745B`, then walks the order queue with `0x5B2B20` and the unit methods `+0x5C` ("execute now") and `+0x54` (`0x4674A3..0x4674E7`). Related routines (**V** for the call edges): the mover wrapper `0x466F40` (`ret 8`, arguments `dir, flag`, three calls of `0x5B8FC0`; **H**: the unit method `+0x4C` that
`worker-jobs.md` and `movement.md` call "the step"), and the order switch at `0x4677A2..0x467BC1` that calls the four go-to executors `0x461F90`, `0x4620D0`, `0x4622D0`, `0x462670` (not read).

### 3.2 `0x5C32F0(U; needCity)` (`ret 4`) **V**

True when all hold: `U.owner != 0`; U does **not** have ability 13; `0x5BE5B0(U) - U.+0x4C > 0` (alive); the owner passes `0x561480(P; 0x100000)` (a technology-flag test, `primitives.md`; the meaning of the
flag `0x100000` is **O**); `U.owner != U.nationality` (`U.+0x38`: a unit built by or captured from another civilization); and, when `needCity != 0`, a city stands on the tile. **H**: the gate for "convert a
foreign-nationality unit".

### 3.3 The default `0x5BEAE0(U)`: not read (**O**).

## 4. Handlers that were read

`0x5B2F10(U; -1)` is called before every order change below (**O**: H it clears the unit's go-to target); `0x5B3040(U; k)` is `setOrder` (`unit-turn.md` 3.6).

### 4.1 Flag Unit (bit 18) **V**

`0x5B2F10(U; -1)`, `setOrder(U; 1)`: the unit fortifies and does nothing else.

### 4.2 Cruise Missile `0x4566E0` **V** for the structure, **O** for the evaluator

```
R = PRTO[U.type].+0x4C                                   // the missile's reach, in tiles (mem +0x4C)
best = -1;  (bx, by) = (-1, -1)
for k = 1 .. (2R+1)^2 - 1:                                // every tile of the square around the unit, in the spiral order of vision.md 1.1
    (dx, dy) = 0x5E6E50(k);   (x, y) = (U.x + dx, U.y + dy) with the wrap flags [0x9C755C] bit 0 (x) and bit 1 (y)  // single fold
    if (x, y) is on the map:  v = 0x44CD40(U; x, y)      // the value of attacking that tile (O)
        if v > best:  best = v; (bx, by) = (x, y)
if (bx, by) is on the map:  0x5B2F10(U; -1);  0x5C1410(U; bx, by)        // launch at the best tile (0x5C1410: O, the attack command)
else:                       0x5B2F10(U; -1);  setOrder(U; 1)              // nothing worth hitting: fortify
```

### 4.3 ICBM `0x460540` **V** for the structure, **O** for the evaluator

```
best = 0;  (tx, ty) = (-1, -1)
for each city K in the city pool (index 0 .. [0xA52E78]) with K.owner != U.owner:        // owner byte at K +0x28
    v = 0x44C8E0(U; K, 1)                                // the value of nuking that city (O)
    if v > best:  best = v;  (tx, ty) = (K.x, K.y)       // words at K +0x24, +0x26
if (tx, ty) is on the map:  0x5B2F10(U; -1);  0x5C1410(U; tx, ty)
else:                       0x5B2F10(U; -1);  setOrder(U; 1)
```

The target value must be strictly positive for a launch; a tie keeps the first city of the pool.

### 4.4 Explore `0x455690` **V** for the order ladder, **H** for the meaning of each step

The handler first reads the water-body id of the unit's tile (`cell.vt+0xB8`) and, for some prototypes (the PRTO test at `0x4556C9..0x4556E2` was not resolved), asks `0x44EB10(U)`; if that answers true it returns.
If the owner's per-region table `[...][regionId]` is zero (not resolved, **O**) it runs the ladder; if nonzero and a city is on the tile it tail-calls `0x5BC300(U)` (**O**) and otherwise falls into the same ladder.
The ladder (each step is `0x5B2F10(U; -1)` followed by the call; the ladder stops as soon as `U.vt+0x5C()` returns true **or** `U.+0x64 != 0` after the step):

1. `setOrder(U; 0x1A)`, then `U.vt+0x5C()`;
2. if `U.+0x4C > 0` (the unit is damaged): `setOrder(U; 0x1B)`, then `U.vt+0x5C()`;
3. `0x454180(U; 3)` (O): stops the ladder if true;
4. `setOrder(U; 0x1D)`, then `U.vt+0x5C()`;
5. otherwise `setOrder(U; 1)` (fortify).

**H**: orders `0x1A`, `0x1B`, `0x1D` are the explore variants; `U.vt+0x5C` is "execute the order now".

## 5. Worker automation is the Terraform handler `0x45C750`

The automation of Workers (and of every prototype with the Terraform strategy, bit 12) is the handler `0x45C750` in the table above, an 11.7 KB routine that contains the code regions reported earlier as `0x45D814` and
`0x45EC1B` (neither has a direct caller; both are inside it). It calls `Player::canImprove` (`worker-jobs.md` 3) and the job issue of `worker-jobs.md` 6. Its body was **not** decoded; the rules it must
reproduce are the ones in `worker-jobs.md` (which jobs are legal, how long they take) plus the go-to-and-build executors `0x461F90..0x462670` that the order loop of `0x467130` runs (`worker-jobs.md` 5).
A port that wants Civ III's automated workers has to read `0x45C750`; nothing here replaces that reading.

## 6. Open items

1. Resolve the Unit class vtable (slots `+0x4C` step, `+0x54` execute order, `+0x58` unit AI, `+0x5C` execute now) and confirm the identification of section 2 and 3.
2. Every handler body of section 3 except 4.1-4.3; first the Terraform handler (`0x45C750`, section 5), Settle (`0x45F630`), Offense (`0x4507B0`), Defense (`0x452510`), Naval Transport (`0x45A170`).
3. The gates `0x44E8E0`, `0x5C32F0` (the technology flag `0x100000`), `0x5BEAE0` (default), `0x44FAE0` (barbarians), `0x5C2E80`, `0x466A60`, `0x5B2F10`, and the evaluators `0x44CD40`, `0x44C8E0`, `0x5C1410`.
4. The order queue and executor `0x467130` beyond its skeleton, the step method `0x466F40`, and the order setter `0x5B3040` (order to routine map).
5. The multiplayer table at `0x74AF60` (`0x469590`).
