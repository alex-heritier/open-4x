# Air combat

Owns: air-move dispatch, the bombing run, landing, and the defenses an
aircraft meets on arrival (SAM, flak, patrolling interceptors).
Reference: `rust/src/air.rs`; the interception duel itself is in
`combat.md` section 8.11 / `rust/src/combat.rs`. Image base `0x400000`,
static VA = runtime VA. Verified unless marked **HYPOTHESIS**.

## 1. What an arriving aircraft meets: `Unit::airDefenseAt` `0x5C68A0`

`thiscall`, `ret 8`, `this` = the aircraft, args `(x, y)`; returns **true
when the move is aborted** (the aircraft was shot down, or the SAM fired and
missed). Callers, all behind a `PRTO +0x9C == 2` (air domain) test:
`0x5B53EA`, `0x5C11D6` (just after `0x5B5600`), `0x5C1950`, `0x5C73FF` (the
bombing run, section 5). Three layers; the first that fires ends the routine:

```text
airDefense(aircraft, x, y):
  A  SAM ---------------------------------------------------------------
     city = cityAt(x, y)                                   0x56D2C0
     if city && Player[city.owner].atWarWith(aircraft)     0x558F70
             && (sam = samStrength(city)) > 0:             0x4C11E0
        p = stealth(aircraft) ? [0x9C72A4] : [0x9C72A0]    (5 : 50)
        if next(100) < p:
           aircraft.+0x50 += [0x9C72C8]                    one full move (3 units)
           if next(defense(aircraft) + sam) < defense(aircraft): return true  // survived
           message AIRSAMINTERCEPTED (if the aircraft's owner is the local player)
           kill aircraft; return true
  B1 flak ----------------------------------------------------------------
     slots[4] = candidates among units on the tile that are at war with the
        aircraft and have PRTO[u.proto].+0x134 > 0         (empty slot first,
        else replace the FIRST slot in index order with a lower strength)
     for slot in 0..4: T = 5 * defense(aircraft) * (aircraft.level + 1)
        if next(S + T) >= T: AIRFLAKINTERCEPTED; kill; return true
  B2 patrolling interceptors ---------------------------------------------
     for id in 0..=maxUnitId:                              [0xA52E84]/[0xA52E90]
        u must satisfy: owner at war with the aircraft; PRTO +0x9C == 2;
           u.+0x64 == 15; (maxMove(u) - u.+0x50) > 0 (clamped 0..9999);
           within(u.xy, (x, y), r)                         0x52C7A0
        r = 0 if PRTO +0x64 == 0 else max(1, range / 2)
        if next(100) < p:                                  same p as in A
           aircraft.+0x50 += [0x9C72C8]
           u.+0x50 = maxMove(u); Unit::setOrder(u, 0)      0x5B3040
           duel(u, aircraft, x, y)                         0x4A4520 (combat.md 8.11)
           return true
  return false
```

`stealth(aircraft)` is ability 21 on the aircraft's prototype **or** on the
prototype shared by the units loaded into the Army it carries (`0x5BC6D0`
returns that shared prototype id, else -1). Only the Stealth Fighter and the
Stealth Bomber have it in the shipped rules.

| constant | where | value |
|---|---|---|
| engage chance, ordinary aircraft | RULE `[0x9C72A0]` (body `+0xDC`) | 50 % |
| engage chance, stealth aircraft | RULE `[0x9C72A4]` (body `+0xE0`) | 5 % |
| movement units per full move | RULE `[0x9C72C8]` (body `+0x100`) | 3 |
| flak factor | `lea esi,[eax+eax*4]`, `0x5C6D68` | 5 |
| flak slots | `cmp edi,4`, `0x5C6C91` | 4 |

Probabilities (aircraft defense `D`, SAM strength `S`):

* SAM: `P(engage) = p`; `P(kill | engaged) = S / (D + S)`. A SAM Missile
  Battery (strength 8) against a Bomber (`D` 2) kills 80 % of the 50 % that it
  engages. A miss still returns true, so the **move is cut short** and a full
  move is charged.
* Flak: `P(kill) = S / (S + T)` per slot, `T = 5 * D * (level + 1)`, with `S`
  the shooter's PRTO `+0x134`. A unit with `D = 0` is always killed.
  **No shipped PRTO row has `+0x134 != 0`**, so this layer is dormant in the
  shipped rules. The field is read only at `0x5C6C76`, `0x5C6CBF`, `0x5C6CC6`
  and `0x5C6D78` (byte scan for `imul .., 0x138` + disp `0x134`), all in this
  routine; short 255-byte BIQ records do not carry it (default 0). That `S` is
  a "flak strength" is **HYPOTHESIS** (the `biq` crate calls the field
  `stealth_mission_flags`).
* Interceptors: every eligible patrol unit rolls separately in unit-table id
  order and the first success fights, so a stack of `n` patrollers intercepts
  with probability `1 - (1 - p)^n`. The engaged aircraft is charged a full
  move even if it wins the duel.

Each of the three layers also draws no number at all when it has nothing to
do: no SAM, no flak candidates and no eligible patroller leave the RNG
untouched (`air::tests::a_quiet_tile_lets_the_aircraft_through_without_a_draw`).

Messages (`script.txt`): `AIRSAMINTERCEPTED` "We were shot down by an enemy
SAM!", `AIRFLAKINTERCEPTED` "We were shot down by enemy anti-aircraft!",
`AIRINTERCEPTED` "We were shot down by an enemy interceptor!",
`AIRINTERCEPTORDESTROYED` "We shot down an enemy interceptor!",
`AIRINTERCEPTSUCCESS` "We intercepted an enemy!", `AIRINTERCEPTFAILURE` "Our
interceptor was shot down!". Civilopedia cross-checks: SAM "attack enemy air
units that attempt to attack the SAM site's city"; Stealth Bomber "very
difficult to intercept by enemy air superiority fighters, and/or SAM
batteries" (the 5 % word).

### 1.1 The interception duel `0x4A4520` and its report `0x4A46A0`

The duel runs the ordinary round loop (`0x4A0ED0` odds, `0x4A53A0` loop) with
the interceptor as attacker and the aircraft as defender, retreat disabled.
The report: interceptor wins → `AIRINTERCEPTSUCCESS` (if its owner is the local
player) or `AIRINTERCEPTED` (aircraft's owner local), then `0x5BEF00(interceptor,
aircraft)` (victory bookkeeping, combat.md 6.2) and the aircraft is killed;
aircraft wins → `AIRINTERCEPTFAILURE` / `AIRINTERCEPTORDESTROYED`,
`0x5BEF00(aircraft, interceptor)`, interceptor killed. Rust:
`combat::interception_duel`.

### 1.2 Building strengths `0x4C11E0` / `0x4C1280`

Both walk the BLDG table (`[0x9C40AC]`, stride `0x110`, count `[0x9C3D80]`) and
**add up** the field of every building that `0x4ACB50(city, building, 1)`
says acts on the city and whose obsolete-by technology (`+0xE0`, `-1` none) the
owner has not learned (`0x561440`):

| function | BLDG mem offset (body) | meaning |
|---|---|---|
| `0x4C11E0` | `+0xC4` (`0xC0`) | SAM strength |
| `0x4C1280` | `+0xC8` (`0xC4`) | coastal-fortress strength |

(The first row was labelled "naval sibling" in older notes; it is the SAM sum.)
Consumers seen: `0x44B2A0` (AI), `0x5C68A0`, `0x421E9F`, `0x4B2814`,
`0x4CA87D` (open).

## 2. The interception order (order 15)

`Unit::setOrder(order)` is `0x5B3040` (earlier notes called it "wake"; wake is
`setOrder(0)`). In single-player a non-zero order other than 31/32/33 on a unit
with **no movement left and no container** becomes order 34 (`0x5B30B8`,
**HYPOTHESIS**: queued until movement refreshes); otherwise it clears the
unit's `+0xB0/+0xB4` (-1) and `+0x1C0`, then stores the order at `+0x64`
(`0x5B322E`).

Order 15 is the **Interception air mission**:

* The human command handler `0x4D93C0` tests `Unit::canDoAction(0x30000004)`
  (`0x5C1AD0`: token = `(wordIndex << 28) | mask`, word at PRTO `+0xA8 +
  4 * wordIndex`; word 3 = PRTO `+0xB4` air missions, mask 4 = bit 2 =
  Interception, matching `biq::air::INTERCEPTION` and `editor.md`), calls
  `setOrder(15)` and sets `unit.+0x50 = maxMove`. The unit is therefore
  **not eligible on the turn it was ordered** (no movement left) and becomes
  eligible after the next turn's refresh.
* The sibling handlers: `0x4D91xx` Bombing (`0x30000001`), `0x4D92C0` Recon
  (`0x30000002`), `0x4D93F0` Re-base (`0x30000008`), `0x4D94F0` Precision
  Bombing (`0x30000010`, also tested in `0x5C2AED`).
* The AI sets the same order in its air routine `0x4579E0` (`0x458104`): an
  undamaged fighter in a city gets order 15 while
  `Player::countUnitsAt(x, y, order 15)` (`0x44A490`: units of the player on
  the tile whose `+0x64` equals the order) is below the city's air-defense
  want (`vtable +0x64`, at least 1).
* Order 15 is cleared by the interception itself (`setOrder(0)` in `0x5C68A0`),
  and `0x5BD220` (move) keeps order 15 on cargo (aircraft on a carrier) while
  it sets every other cargo unit to order 1.

AI mirror `0x44B2A0(player, x, y)`: returns `(SAM city of an enemy on the tile
? 1 : 0)` plus the number of hostile air units with order 15 whose reach
covers the tile, using `(|dx| + |dy|) / 2 <= r` with the same radius rule and
the map wrap flags; the AI uses it to plan missions.

### 2.1 Range test `0x52C7A0(ux, uy, tx, ty, r)`

True when `(tx, ty)` is among the first `(2r + 1)^2` cells of the spiral
(`0x5E6E50`, `rust/src/spiral.rs`) around `(ux, uy)`. Each candidate is wrapped
in x (`[0x9C755C] & 1`, width `[0x9C74D4]`) and in y (`& 2`, height
`[0x9C74C0]`) and bounds-checked. That is every tile with
`(|dx| + |dy|) / 2 <= r` in the doubled-coordinate map. Radius `r = 0` for a
unit with no range (it covers only its own tile), else `max(1, range / 2)`:
a Fighter (range 6) covers 3 tiles, a Jet Fighter (range 8) 4.

## 3. Air-move dispatch `0x456840` (AI)

`thiscall` (`mov ebp,ecx`), frame `0x2C`. The head requires `[ebp+0x4C] > 0`
(damaged units go home: the fail tail `0x4579C1` calls `0x5B2F10(-1)` then
`setOrder(1)`). Four move sites push their `AirBombardMove` log strings and
converge:

| site | string | shape |
|---|---|---|
| Move1 `0x456C39` | `AirBombardMove 1` (`0x684A8C`) | push + `jmp 0x457282` |
| Move2 `0x456F3E` | `AirBombardMove 2` (`0x684A78`) | push + `jmp 0x457282` |
| Move3 `0x45727D` | `AirBombardMove 3` (`0x684A64`) | push, falls through |
| shared tail `0x457282` | - | `call 0x5F98B0`; `call 0x5C71C0` |
| Move4 `0x457878` | `AirBombardMove 4` (`0x684A50`) | private tail, same epilogue |

`0x5F98B0` is a bare `ret` stub (50 binary-wide callers): the log call is a
disabled no-op. `0x456840` also calls the bombing run `0x5C7350` at
`0x457992`. Move to situation mapping: open.

### 3.1 Landing / re-base `0x5C71C0(this, x, y)`

Callers: `0x45728E`, `0x457891`, `0x457E2D`, `0x45A107`, `0x47867D`, `0x4B907C`,
`0x4E70DF`, `0x4E7954`, `0x4EC176`, `0x5D3F83`.

```text
0x5BD220(this, x, y)                       move (cargo follows, section 2)
this.+0x50 += [0x9C72C8]                   an air move costs one full move
cell = Cell(x, y)
if cell.vslot(+0xB4)() != 0xFFFF                 -> done      0x5EA6C0 (HYPOTHESIS: the tile has a city)
if cell.vslot(+0x18)(0) && cell.vslot(+0x118)() == this.owner -> done   (HYPOTHESIS: own airfield)
carrier = 0x5C5F70(this, x, y, 1)          SELECT_TRANSPORT dialog if several (local human)
if none -> done (the aircraft stays in the air)
multiplayer: send 0x476110(this.id, carrier.id) (not for non-local AI owners)
else: this.+0x60 = carrier.id; setOrder(1); if carrier has ability 18 (Army): 0x5BCC90(carrier, this)
```

### 3.2 Bombing run `0x5C7350(this, x, y)`

Callers: `0x457992`, `0x4580C8`, `0x467B87`, `0x47868E`, `0x4E3066`, `0x4E7127`.

```text
0x49FC50()                                  (UI prelude, open)
city = cityAt(x, y); if none -> return
if !multiplayer: if !0x5B5790(this, city.owner, 1) -> return     (treaty / declare-war check)
mp: if owner is not flagged in [0xA526BC]: declareWar(Player[owner], city.owner, 0)  0x501F20
0x5B5600(this, city.owner, 1)               provoke (combat.md 14.3)
if airDefense(this, x, y) -> return         section 1
if 0x4ED120(0x9F8700, x, y, this): 0x5CA860(this, x, y)   (animation, open)
mode = 0x4B3220(city) ? 1 : 0               any destroyable building?  (1 facility / 0 population)
0x4A2650(this, city, mode)                  city strike, combat.md 8.5
if next(32) == 0: 0x4A2650 again, mode recomputed (a 1/32 second strike)
this.+0x48 |= 4; this.+0x50 += [0x9C72C8]
```

`0x4B3220(city)` is true when any BLDG satisfies: `0x4ACB50(city, i, 0)`,
`BLDG +0xF0 & 4 == 0`, `+0xF0 & 8 == 0`, `+0xEC & 1 == 0` and `+0xEC & 0x1800
== 0` (wonders, small wonders and the flagged building kinds are exempt).

## 4. Data used by this subsystem

| item | location | note |
|---|---|---|
| domain | PRTO mem `+0x9C` | 0 land, 1 sea, 2 air |
| operational range | PRTO mem `+0x64` | interception radius source |
| air missions | PRTO mem `+0xB4` | bit 0 Bombing, 1 Recon, 2 Interception, 3 Re-base, 4 Precision Bombing |
| flak strength | PRTO mem `+0x134` | long records only; all shipped values 0 |
| Stealth | PRTO ability bit 21 | `0x5E4EF0(proto, 21)` |
| Hidden Nationality | PRTO ability bit 17 | **bit 17, not bit 19** (`biq` crate's `1 << 0x13` is wrong) |
| unit order | unit `+0x64` | 15 = Interception |
| movement used | unit `+0x50` | in units of `[0x9C72C8]` (3 per full move) |
| experience level | unit `+0x44` | flak threshold input |

`biq` crate label notes: `intercept_air_missions_pct` is the exe's
`+0xDC` (body) = 50, `intercept_stealth_missions_pct` is `+0xE0` = 5; the
crate places them elsewhere and labels `+0xD8/+0xDC` as golden-age duration /
stealth chance, which the exe does not support (golden-age duration is body
`+0x140`).

## 5. Open items

* The AI's use of `0x44B2A0` (consumers) and the exact mapping of the four
  `AirBombardMove` sites to air missions.
* Flak: the meaning of PRTO `+0x134`; whether any scenario editor can set it.
* `0x5C71C0`: Cell vslot `+0x18` and `0x4ED120` / `0x5CA860` / `0x49FC50`.
* Ability bit 10 (set on every air unit; the `biq` crate calls it "Immobile").
