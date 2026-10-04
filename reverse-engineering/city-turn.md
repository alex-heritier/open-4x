# The city turn: sequencer, production, completion

Clean-room specification of what `City::turn` (`0x4BE970`, no arguments, `this` = city, plain `ret`) does
once per city per turn, with the **production system** (shield box, costs, completing a building or a
unit, changing production, the build queue) decoded in full. Every claim cites the address it was read
at (raw disassembly; `r2`). Tags: **V** read from the instructions, **H** hypothesis (the data flow is
read but the meaning is inferred), **O** open (not decoded). No reference implementation is attached
on purpose: the algorithms and the golden vectors in section 9 are the contract.

Neighbours (not repeated here): `yields.md` (food, shields `0x4B05D0`, commerce `0x4B07C0`, the chain
`0x4B10F0`), `happiness.md` (`0x4BCFF0`, disorder `0x4BDFF0`, celebration `0x4BE440`, the
produces-units tick `0x4BE730`), `economy.md` (growth `0x4B2030`, culture `0x4B2680`, upkeep),
`world-events.md` (pollution `0x4B2F80`, meltdown `0x4B4970`), `buildable.md` (what a city may build),
`city-founding.md` (`City::init`, unit cost `0x56A210`), `primitives.md` (`startingExperience` `0x4B0160`,
`RACE → civ slot` `0x539D60`), `capture.md` (`0x4BA230` citizen removal, `0x4AECC0` city removal),
`turn.md` (the player-level turn that calls this).

## 1. Fields used here

City object (`this`; vtable `0x66DC78`). Owner slot is the byte `+0x28`; `P` = `Player[owner]`.

| field | meaning | evidence |
|---|---|---|
| `+0x24` / `+0x26` | x / y (int16) | `0x4B8D4B` |
| `+0x28` | owner slot (byte) | everywhere |
| `+0x30` | flag dword: bit 0 disorder, bit 2 cleared at the start of the turn, **bit 4 (`0x10`) set by every hurry (`hurry.md` 5, `0x4B5D45`, `0x4B5F20`) and cleared when an item completes** (a "hurried" marker; its readers are **O**), **bit 5 (`0x20`) = the current production is Capitalization (coinage)** | `0x4BE996`, `0x4B8CE4`, `0x4B9648`, `0x4AFB35` |
| `+0x34` | flag dword: **bit 4 (`0x10`) = the city manages its own production** (**H**: the governor; set by the UI, never by the exe's turn code). Cities of human owners with the bit clear never leave coinage by themselves and always get the completion popup | `0x4B99B1`, `0x4B90EE`, `0x4B96E6` |
| `+0x38` | flag dword, low two bits tested by the chooser (`0x4C1D50`) | `0x437115` |
| `+0x40` | stored food (int32) | `economy.md` |
| `+0x44` | **shield box** (int32) | everywhere |
| `+0x4C` / `+0x50` | production **id** / **kind** (0 nothing, 1 building, 2 unit) | getter `0x437E30(this; &kind, &id)` writes kind first, id second |
| `+0xDC` | the citizen pool: array `+0xE0` of 8-byte nodes (node pointer − `0x1C` = citizen), last index `+0xEC` | `0x4BEAAD` |
| `+0x138` | city size (citizens alive) | `0x4BEB82` |
| `+0x1C8` | food produced (gross), `+0x244` food eaten, `+0x250` food surplus (net) | `0x4BEB1A..0x4BEB62` |
| `+0x254` | **net shields** (after waste and support, `yields.md` 5.4) | `0x4B99CF` |
| `+0x1D8` / `+0x1DC` | rally point x / y (int16 each, `-1` none) | `0x4B900D`, `0x4AFF7C` |
| `+0x1E0` | the city name (char array) | message args |
| `+0x1F8` | build queue length (0..9) | `0x4B0090` |
| `+0x1FC + 8k` / `+0x200 + 8k` | queue entry `k` (0..8): id / kind | `0x4B00BD`, `0x4B00C3`; reset to `(-1, 0)` by `chooseProduction` (`0x437021..0x437048`) |
| `+0x3B8` | **bonus pool**: construction points accumulated while a building is in production | `0x4B9AA9..0x4B9B17`, reader `0x4B0AB0` |

Citizen object (`0x4AC140` / `0x4B2E10`): `+0x20` resister byte, `+0x130` turn the current race began,
`+0x134` city id, `+0x13C` job (CTZN row; 0 = ordinary worker), `+0x140` race, `+0x144` pending race
(`-1` none), `+0x148` turn the pending race was set (`-1` none).

Player object: `+0x1C` slot, `+0x20` race, `+0x2C` capital city id, `+0xA0` government, **`+0xA4`
mobilization (0/1)**, `+0x194` city count, `+0x183C` rating (`capture.md`), and four per-item `int16`
arrays: `+0x15DC[b]` buildings owned, `+0x15E0[b]` cities currently producing `b`, `+0x15F4[u]` cities
currently producing unit `u`; `+0x15E4[b]` and `+0x15F8[u]` are the advisor's "recent" markers (`turn.md`
step 2). A byte array `+0x15EC[b]` marks "small wonder `b` already announced as available".

Globals: BLDG `[0x9C40AC]` stride `0x110` (cost `+0x94`, improvement flags `+0xEC`, other
characteristics `+0xF0`, small-wonder flags `+0xF4`, spaceship part `+0xD8`; `biq/src/sections/bldg.rs`
bit names), PRTO `[0x9C71E0]` stride `0x138` (cost `+0x54`, population cost `+0x68`, name `+0x08`, class
`+0x9C`), CTZN `[0x9C40B0]` stride `0x80` (construction `+0x7C`), DIFF `[0x9C40C0]` stride `0x7C`
(cost factor `+0x68`), GOVT `[0x9C71D8]` stride `0x1E8` (`+0x1A4` assimilation percent), human mask
`[0xA526BC]`, local slot `[0x9FD4BC]`, game difficulty `[0xA52684]`, game flags `[0xA5267C]` (`0x200`
Accelerated Production), turn counter `[0xA526AC]`, gameplay RNG `rand(n)` = `0x60BAB0(0xA526B4; n)`,
multiplayer gate `0x47B530` (true in multiplayer), wonder registry `0xA52658` (`0x538FE0(id)` = built by
anyone).

## 2. The sequencer `0x4BE970`

Order, exactly as the code runs (every step is unconditional unless a condition is given):

| # | site | step |
|---|---|---|
| 1 | `0x4BE973..0x4BE98A` | `[0x9C34E4] = 0` (the "open the city screen at the end" request); `vtable[+0x38](0)` (`0x435520`, refresh, **O**); `0x4B28D0(this)` is the culture-flip test (`economy.md`): **if it returns true the whole turn of this city is skipped** (`jne 0x4BEC3F`) |
| 2 | `0x4BE990..0x4BE9D1` | `+0x30 &= ~4`; `+0x36C = 0`; `+0x6C = 0`; `+0x70`, `+0x1C4` (hurry-sacrifice timer), `+0x1C0` (propaganda unhappiness) each decremented by 1 when positive |
| 3 | `0x4BE9DA` | `0x4BE730(this)`: the produces-units tick (`happiness.md` 9) |
| 4 | `0x4BE9DF..0x4BEAA6` | **small-wonder availability notice.** For every BLDG row `i` in `0..[0x9C3D80]-1` with `other_characteristics & 8` (small wonder), `P.+0x15EC[i] == 0`, and `0x4BFF80(this; i, 1)` (`buildable.md`) true: set `P.+0x15EC[i] = 1`; and when `owner == local slot` and not multiplayer, post the log line `SUMMARY_NEW_SMALL_WONDER` with the row's name (`row+0x44`) as argument 0 |
| 5 | `0x4BEAAD..0x4BEAEE` | `0x4AC140(citizen)` for every citizen in pool order (section 8.1) |
| 6 | `0x4BEAF2..0x4BEB15` | in order `0x4B2E10` (resistance quelling, 8.2), `0x4B2F80` (pollution event), `0x4B45A0` (disease), `0x4B4970` (meltdown), `0x4BDFF0` (disorder turnover), `0x4BE440` (celebration test) |
| 7 | `0x4BEB1A..0x4BEB62` | `+0x244 = (+0x30 & 1) ? +0x1C8 : (+0x138 - 0x4BB2A0(this; -1)) * [0x9C72B4]` (a city in disorder eats its whole gross; otherwise each non-resisting citizen eats `food_per_citizen`); `+0x250 = +0x1C8 - +0x244` |
| 8 | `0x4BEB68..0x4BEB7D` | `0x4B05D0` shields, `0x4B07C0` commerce, `0x4BCFF0` happiness recompute, `0x4B2030` growth/starvation |
| 9 | `0x4BEB82` | **if the size is now 0 the turn ends** |
| 10 | `0x4BEB90..0x4BEBC8` | **AI owners only** (bit of `[0xA526BC]` clear): `vtable[+0x14]` (`0x433D40`) then `vtable[+0x18]` (`0x433EE0`). `0x433D40` is the "replaced by this flag" cleanup: among buildings with improvement flag `0x2000` that the city has in the strict sense (`0x4ACB50(.., 1)` true, `0x4ACB50(.., 0)` false) it keeps the one with the largest `BLDG +0xD0`, then `+0xAC`, then `+0xCC`, and sells the others with `0x4B3400(this; b, 1)` (**V** structure, **H** intent). `0x433EE0` is the AI purchase decision (government `+0x1A0`, `ai.md` hurry validator): **O** |
| 11 | `0x4BEBCA` | **`0x4B9950(this)`: production** (sections 3 to 6) |
| 12 | `0x4BEBCF` | if the size is 0 the turn ends (a settler consumed the last citizen) |
| 13 | `0x4BEBDB` | `0x4B2680(this)`: culture accumulation (`economy.md`) |
| 14 | `0x4BEBE0..0x4BEC0E` | `+0x3B0` byte clear and `+0xA4 == 0xB`: `0x406200(0x73DA40; &city+0x3BC)`, `+0xA4 = 0` (**O**, UI state) |
| 15 | `0x4BEC0E..0x4BEC3A` | UI: when `this == [0x7405D8]` (the displayed city) and byte `[0x9C5B42] == 0`: `0x6027C0(0x73E090)`; when `[0x9C34E4] != 0`: `0x4216A0(0x740A00; this, 0)` opens the city screen so the player can pick production |

`[0x9C34E4]` is raised by exactly two places: the "abandon" dialog answered with option 1
(`0x4B8CA3`) and the wonder-lost dialog answered with 0 (`0x4B94AD`).

## 3. Production data and the three small getters

* `0x437E30(this; &kind, &id)` (`ret 8`): `*kind = +0x50`, `*id = +0x4C`; a null pointer is skipped.
* `0x4ACD70(this)`: the cost of the current item: kind 0 → 0; kind 1 → `P.buildingCost(id, 0)`
  (`0x569FE0`); kind 2 → `P.unitCost(id, 0)` (`0x56A210`); any other kind → undefined stack garbage
  (**V**, cannot occur).
* `0x4ACD40(this)`: the **stock** = `max(0, +0x44 + (kind == 1 ? +0x3B8 : 0))`.
* `0x4B0AB0(this)`: the bonus = `(kind == 1) ? +0x3B8 : 0`.

The box `+0x44` is therefore a plain shield counter; the **bonus pool** `+0x3B8` exists only for
buildings and wonders and counts toward completion and toward the "extra shields lost" warning, but is
never capped by the cost.

## 4. Costs

### 4.1 `Player::buildingCost(P; b, forceBase)` `0x569FE0` (`ret 8`)

```
base   = BLDG[b].+0x94                                                   0x56A00A
f      = 10                         if P's slot is in [0xA526BC] or forceBase != 0
       = DIFF[[0xA52684]].+0x68     otherwise (the AI's cost factor; the row is the GAME level)   0x56A01D..0x56A033
if [0xA5267C] & 0x200:  f = f / 2   (signed, toward zero: cdq; sub eax,edx; sar 1)               0x56A03A..0x56A048
f      = max(f, 1)
c      = f * base                                                        (no division by 10, unlike units)
flags  = BLDG[b].+0xF0
if flags & 4 (great wonder) or flags & 8 (small wonder):   no trait discount   (jump to 0x56A1A8)
else   one halving (c = c / 2, signed toward zero) if ANY of these holds (checked in this order,
       the first hit halves once):
         RACE(P).trait(0) and flags & 0x002      Militaristic
         RACE(P).trait(4) and flags & 0x100      Religious
         RACE(P).trait(6) and flags & 0x400      Agricultural
         RACE(P).trait(7) and flags & 0x800      Seafaring
         RACE(P).trait(3) and flags & 0x020      Scientific
       (RACE.vtable[0](t) = `0x53A080`, trait bit t of the RACE trait mask, `capture.md` 12)
if BLDG[b].+0xEC & 1 (Palace / Center of Empire):
       k = (6 * P.+0x194) / WSIZ[[0x9C73A0]].+4        (integer; WSIZ row stride 84, +4 = optimal cities)
       k = clamp(k, 3, 10);  c = c * k
return max(c, 1)
```

The shipped BLDG `cost` field is in units of ten shields (Barracks 4 = 40 shields for a human at
normal speed). Commercial, Expansionist and Industrious carry no discount here. **V** (`0x569FE0..0x56A205`).

### 4.2 `Player::unitCost(P; u, forceBase)` `0x56A210`

`c = max(1, (f * PRTO[u].+0x54) / 10)` with the same `f` as above (human or `forceBase`: 10; else
`DIFF[[0xA52684]].+0x68`; halved by Accelerated Production, minimum 1). The division is the magic
multiply by `0x66666667` (toward zero). No trait discount, no wonder logic (`city-founding.md` 5.4).

## 5. The production tick `0x4B9950`

`this` = city; plain `ret`. All branches **V**.

```
if +0x30 & 0x20:                                   # producing Capitalization (coinage)           0x4B9957
    +0x44 = (0x4ACD70() > 0) ? 0 : 0x4ACD70()      # always 0: the Wealth row costs max(1, 0) = 1
    if advanceQueue():                  return     # 0x4B0090 (section 7.1)
    if owner is human and (+0x34 & 0x10) == 0: return        # a human city keeps coinage
    vtable[+0x58](1);                   return     # chooseProduction(newQueueFlag = 1), section 7.2
                                                   # an AI owner (or a governor city) never stays on coinage
# ordinary production
if +0x254 > 0:                                                                                   0x4B99CF
    total = +0x44 + +0x254
    cost  = (kind==0 ? 0 : kind==1 ? buildingCost(id,0) : unitCost(id,0))
    +0x44 = (total < cost) ? total : cost          # the box never exceeds the cost; overflow is DISCARDED
switch kind:                                                                                     0x4B9A84
    0: +0x3B8 = 0;  vtable[+0x58](0)               # nothing in production: choose (queue flag 0)
    1: bonus = sum over citizens c (pool order, indices 0..+0xEC) with c.+0x20 == 0
               of CTZN[c.+0x13C].+0x7C             # the "construction" bonus; only specialists have one
       +0x3B8 += bonus
       0x4B9270(this)                              # building tick, section 6
    2: 0x4B8BA0(this)                              # unit tick, section 7
```

* `+0x254 <= 0` neither drains nor feeds the box. A full box (stock ≥ cost) still completes on a turn
  with no surplus.
* The bonus is added **every turn the kind is 1**, regardless of the surplus, and is not cleared when the
  player switches to a unit or to another building (`0x4AFAB0` never writes `+0x3B8`); it is zeroed on
  completion (6, 7), at kind 0, and by `City::init`.
* A city in disorder has net shields 0 (`yields.md` 5.4, `0x4B1190` kind 1 returns the gross), so it
  produces nothing but can still complete an already full box.

## 6. Building tick `0x4B9270` (the `ai.md` label "spaceship/production tail" is this routine)

Called only for kind 1. Locals: `id` = `+0x4C`, `row` = `BLDG[id]`. **V**.

**6.1 The great wonder was built elsewhere** (`0x4B92AA..0x4B92D9`): `row.+0xF0 & 4` and `0x538FE0(0xA52658; id)`
(some city, anywhere, owns the wonder):

* **Owner human** (`0x4B931E`): `vtable[+0x50](this)` = `0x436BB0`, "start the most expensive thing the
  city can build": over BLDG rows `i` with `0x4BFF80(this; i, 1)` true, skipping rows whose
  improvement flags have bit 0 (Center of Empire) or whose small-wonder flags have bit 5 (`+0xF4 & 0x20`),
  and, when this city is the capital (`city.+0x20 == P.+0x2C`), rows with improvement-flag bit 8
  (Reduces Corruption), it keeps the row with the highest `buildingCost(i, 0)` (a tie goes to the
  later row, `jl` at `0x436C5A`); then over PRTO rows with `0x4C04E0(this; u, 1, 0, 0)` true the same
  comparison against the running best (a tie goes to the unit); finally
  `0x4AFAB0(this; kind, id, 0)`. Then, if `owner == local slot`: in multiplayer fill the deferred
  record `[0x74CFC4..0x74CFD4] = { 0x10, newKind, newId, lostWonderId, cityId(+0x20) }`; otherwise
  argument slots (city name `+0x1E0`, the lost wonder's name, the new item's name) and the dialog
  `WONDERCHANGE` (`0x4B947D`); if the dialog returns 0 then `[0x9C34E4] = 1`.
* **Owner AI** (`0x4B94E4`): `vtable[+0x54](this)` = `0x436D10` ("take over a sibling's wonder"): over the
  owner's other cities `c` whose stock `0x4ACD40` is **smaller** than this city's, which are building a
  building (`kind 1`) flagged great or small wonder (`+0xF0 & 12`), and which `this` can build
  (`0x4BFF80(this; c.id, 0)`; for rows with `+0xF4 & 0x20` an additional tile-distance test between the
  two cities, `0x436E40..0x436F00`, **O** in detail), it remembers the one whose wonder `BLDG +0x94`
  cost is largest (strictly greater than the best so far). It then calls
  `0x4AFAB0(this; c.kind, c.id, 0)` and `c.vtable[+0x54]()` (the sibling is re-assigned in turn).
  Back in `0x4B9270`: re-read `(kind, id)`; if it is still `(1, id)` stop, else call `0x4B9950(this)` again.

**6.2 Otherwise (`0x4B9515`)** the completion gate:

```
has = 0x4ACB50(this; id, 1)
if not has and P.canBuildImprovement(id, 0)  (0x56A2A0):            goto complete
if owner is human and (+0x34 & 0x10) == 0 and P.canBuildImprovement(id, 0):  goto complete    # 0x4B9559
# cannot (or may not) complete this item: pick something else
if not advanceQueue():  vtable[+0x58](0)
return
```

**6.3 Completion (`0x4B95DC`):**

```
stock = max(0, +0x44 + +0x3B8);   cost = buildingCost(id, 0)
if stock < cost: return                                                   # 0x4B963D
+0x30 &= ~0x10
+0x44 = 0;  +0x3B8 = 0                                                    # no carry-over (0x4B964E..0x4B966A)
if owner is human: P.+0x15E4[id] += 2                                     # advisor marker
0x4ACF40(this; id, 1, 0)                                                  # add the building (section 10)
if not advanceQueue(): vtable[+0x58](1)
if owner == local slot:
    0x535D80()                                                            # UI (O)
    if (+0x34 & 0x10) == 0 and not multiplayer:  0x4B8740(this; 1, id)    # the IMPROVEMENT_COMPLETE popup with advice
    else: log: arg0 = city name, arg1 = the building's name; 0x4ED220(0x9F8700; x, y, "CITYPRODUCE", 1)
    if the city tile is visible to the local slot (cell +0x58 bit): [0xA281C4] = 1        # redraw request
else if row.+0xF0 & 4 (great wonder):       broadcast log "WONDERPRODUCE" (city name, civ, wonder) to everybody   0x4B97E9..0x4B989C
else if row.+0xD8 != -1 (spaceship part) and Player[local].+0xD70[owner] != 0:
                                            log "SUMMARY_THEIR_SPACESHIP_PART"        0x4B98A1..0x4B9923
```

(`Player +0xD70[other]` is a byte per other civilization; the local player has it set for civs it knows
about, **H**.) The popup (`0x4B8740(this; mode, id)`, mode 1 building, 2 unit) is the production-complete
advice dialog.

**6.4 Notes.** A building completes the turn the stock reaches the cost, in the same call that adds the
shields (5). Surplus beyond the cost is never banked: the box is clamped at the cost before the check and
reset to 0 after it.

## 7. Unit tick `0x4B8BA0`

Locals: `id` = `+0x4C`, `proto` = `PRTO[id]`, `pop = proto.+0x68`. **V**.

```
stock = max(0, +0x44 + 0x4B0AB0())            # 0x4B0AB0 is 0 for units
cost  = unitCost(id, 0)
if stock < cost: return                                                              0x4B8C0A

# --- population gate (0x4B8C1D..0x4B8CDF)
if +0x138 (size) <= pop:
    if +0x250 (food surplus) >= 1: return                  # wait for growth
    if multiplayer and owner human: return
    if owner == local slot:
        r = 0x4DA120(0x9F8700; id, this)                   # ABANDONBASE dialog: "found a settlement and abandon the city?"
        if r >= 2: proceed                                 # accepted
        if r == 1: [0x9C34E4] = 1; return                  # open the city screen
        return                                             # r == 0: cancel
    elif owner is human (remote):   proceed
    else (AI):  vtable[+0x58](0); return                   # the AI never disbands: choose something else

# --- production commits (0x4B8CDF)
+0x30 &= ~0x10;  +0x3B8 = 0;  +0x44 = 0                    # box emptied, no carry-over
if owner is human: P.+0x15F8[id] += 2                      # advisor marker
u = 0x5694D0(P; id, x, y, -1, -1, 0, 0, -1)                # unit factory (barbarians.md 3, goody-huts.md 6.9)
if u == null: goto next_item                               # box already spent
u.+0x44 = clamp(0x4B0160(this; proto), 0, 3)               # starting experience (primitives.md 3.8: 1 regular, 2 veteran)

# --- population cost (0x4B8DC4..0x4B8EB1)
if pop > 0:
    if +0x40 < 0: +0x40 = 0                                # stored food
    n = min(pop, +0x138)
    removed = 0x4BA230(this; n, P.+0x20 (the owner's race), 0)       # kills n citizens of the owner's race first
    if removed < n:
        for j = 1 .. [0xA5279C], j != owner:                         # other civs in slot order
            k = 0x4BA230(this; n - removed, Player[j].+0x20, 0)
            if k > 0:
                removed += k
                u.+0x38 = Player[j].+0x20                  # the unit takes the nationality of the last foreign race taken
                if +0x138 == 0: Player[j].+0x1F0[19*owner] += 1     # 0xA53088 + 0x20E4*j + 0x4C*owner
            if removed == n: break
# The dword at 0xA53088 + 0x20E4*j + 0x4C*owner is the "razed cities" counter of the per-pair record
# (capture.md 224: Player[O] +0x1C4 + 0x4C*P.civ + 0x2C; government.md +0x2C; diplomacy.md +0x40). Here civ j's
# record of `owner` is bumped when the citizens that empty the city are of j's race (the city is "razed" by being
# consumed into the unit). It is not bumped when the last citizens are of the owner's own race.
if +0x138 == 0:                                            # the last citizen left: the city ceases to exist     0x4B8EBF
    0x4AECC0(this; owner, 0)                               # city removal (capture.md)
    if owner == local slot and the tile is visible: [0xA281C4] = 1; 0x568950(P; continent of the tile)
    return

# --- rally point (0x4B900D)
if +0x1D8 != -1 and +0x1DC != -1:
    if PRTO[u.+0x40].+0x9C == 2 (air):  if 0x5C3900(x, y) then script "CityProd" and 0x5C71C0(u; x, y)
    else:  0x5B3040(u; 0x10); u.+0xB0 = x; u.+0xB4 = y      # a go-to order (order kind 0x10) toward the rally tile
    if multiplayer: u.vtable[+0x54](); 0x4708B0(0x74AF60; u, 0)       # broadcast the new unit

next_item:                                                                            0x4B90C2
if not advanceQueue(): vtable[+0x58](1)
if owner == local slot:                                                               0x4B90D6
    0x535D80()
    if (+0x34 & 0x10) == 0 and ([0xA52678] & 0x10000) and not multiplayer:  0x4B8740(this; 2, id)    # popup
    else: log (city name, unit name) + 0x4ED220(0x9F8700; x, y, "CITYPRODUCE", 1)
    if the city tile is visible to the local slot: [0xA281C4] = 1
```

Consequences worth stating as rules:

* The box is **always emptied** before the factory runs, so a factory failure (unit pool full) still
  consumes the shields.
* A unit with population cost removes citizens of the **owner's race first**, then those of other
  civilizations' races in slot order; citizens whose race belongs to no civ in play cannot be taken by
  this loop (the unit is still created). Settlers shipped: population cost 2, Workers 1.
* The unit is created before the citizens are removed.
* Size ≤ cost with food surplus ≥ 1 stalls the build with a full box; size ≤ cost with surplus < 1 is
  the "disband the city into the unit" case, offered to humans and refused for the AI.

## 8. The citizen steps

### 8.1 `0x4AC140(citizen)` nationality drift (returns true when the race changed)

```
if c.+0x148 == -1 or c.+0x144 < 0:     return false              # no change pending
S     = RACE[c.+0x140].civSlot()                                 # 0x539D60, primitives.md 3.x
city  = cityById(c.+0x134);  owner = city.owner
if c.+0x20 != 0:                       return false              # resisters never drift
pending_for = [0xA526AC] - c.+0x148
lived_as    = c.+0x148 - c.+0x130
if pending_for <= lived_as:            return false              # must have waited longer than it had been that race
if Player[owner].+0x183C <= Player[S].+0x183C:   return false    # the owner must out-rate the old nationality's civ
if rand(100) >= GOVT[Player[owner].+0xA0].+0x1A4: return false    # assimilation percent
c.+0x140 = c.+0x144;  c.+0x130 = [0xA526AC];  c.+0x144 = c.+0x148 = -1
return true
```
**V** for the control flow (`0x4AC148..0x4AC27E`); the meaning of the writers of `+0x144`/`+0x148` is **O**.

### 8.2 `0x4B2E10(city)` resistance quelling

```
R = number of citizens with +0x20 != 0;  quelled = 0
n = 0x5A6060(x, y, 4, -1, 0, -1)                 # units that count as police on the tile (happiness.md martial law)
      * DIFF[Player[owner].+0x30].+0x78          # memory +0x78 = body +0x74 `citizens_quelled_by_military` (biq diff.rs)
if n > 0:
    for i in 0 .. n-1:
        if R <= 0: return                        # (also when R was 0 on entry) no message
        c = the FIRST citizen in pool order with +0x20 != 0       # the scan restarts at index 0 every iteration
        if 0x4ABE90(c; 0) == 0:  R -= 1; quelled += 1             # returns c.+0x20 afterwards: 0 = no longer resisting
if R > 0 and quelled > 0 and owner == local slot:
    log "RESISTANCEQUELLED" with argument `quelled`, located at the city tile
```
So the message appears only when some resisters remain; when the last one stops resisting,
`0x4AC000` prints `RESISTANCEENDS` instead (8.3).

### 8.3 `0x4ABE90(citizen; flag)` and `0x4AC000(citizen; resist)` (`ret 4` each)

`0x4ABE90` re-rolls the resister state and returns the byte `+0x20` afterwards (**V** control flow):

```
result = 0
if c.+0x148 != -1 and c.+0x144 >= 0:                                   # a race change is pending
    S     = RACE[c.+0x140].civSlot()                                   # 0x539D60
    owner = cityById(c.+0x134).owner
    if Player[S].+0x194 > 0 and Player[owner].+0xD30[S] != 0:          # S still has cities; byte table +0xD30 = "at war with S" (diplomacy.md 1; resolves the earlier hypothesis)
        row  = T[0x4F8C50(0xA2A7E8; Player[owner].+0x183C, Player[S].+0x183C)]   # table [0x9C40BC], stride 92 bytes
               # CORRECTION: 0x4F8C50 is `ret 8` (two stack arguments); an earlier reading of a third argument
               # (the caller's EBX) was wrong, the function never reads EBX (V, raw body 0x4F8C50..0x4F8CD2)
        term = flag ? row.+0x54 : row.+0x58
        rec  = *(GOVT[Player[owner].+0xA0].+0x19C) + 12 * Player[S].+0xA0         # pointer to a per-government array of 12-byte records
        if rand(100) < rec.+8 + term:  result = 1
0x4AC000(c; result)
return c.+0x20
```
`0x4AC000(c; resist)`: if `c.+0x20 != resist`: store it; `city.vtable[+0x38](0)`; when now resisting: for a
citizen whose job `+0x13C` is not the default job `[0x9C3D64]` call `0x4BAAF0(city; 1, c.+0x138)`
(take it off its task), otherwise `0x4BBC80(city; c.+0x21)` (release the worked tile); refresh again;
mood `c.+0x128 = 3`. When no longer resisting: mood `c.+0x128 = (job == default) ? 1 : 4`; if the owner
is the local slot and `0x4BB2A0(city; -1) == 0` (no resisters left), log `RESISTANCEENDS` at the city.
The table `[0x9C40BC]` is the `CULT` table and `0x4F8C50` is the culture-ratio row lookup (`espionage.md` 9.7: ratio =
`(int)(a * 100.0f / b + 0.5f)`, the row with the greatest `culture_ratio_percent <= ratio`, fallback `[0x9C3D6C]` else 0);
`Player +0xD30[q]` is "at war with `q`" (`diplomacy.md` 1), not "in contact", so the gate above is **at war with `S`**.

## 9. Golden vectors (derived by hand from the algorithms above; every input is a shipped value)

| # | input | result |
|---|---|---|
| C1 | Settlers (PRTO cost 30, pop 2), human, normal: `f=10` | cost `(10*30)/10 = 30` |
| C2 | same, AI on Chieftain (`f=20`) / Warlord 12 / Regent 10 / Monarch 9 / Emperor 8 / Demigod 7 / Deity 6 / Sid 4 | 60 / 36 / 30 / 27 / 24 / 21 / 18 / 12 |
| C3 | Warrior (10), AI on Sid: `(4*10)/10` | 4; at Chieftain `(20*10)/10` = 20 |
| C4 | Settlers, human, Accelerated Production (`f = 10/2 = 5`) | `(5*30)/10 = 15` |
| C5 | Wealth (BLDG cost 0), any `f` | `max(1, 0)` = 1 |
| C6 | Barracks (cost 4, other_characteristics 2), human, non-Militaristic / Militaristic | 40 / 20 |
| C7 | Temple (cost 6, other_characteristics `0x100`), human, Religious civ; with Accelerated Production | 30; `f=5`: 15 |
| C8 | Library (cost 8, other `0x20`), AI Chieftain, Scientific civ | `20*8 = 160`, halved: 80 |
| C9 | Apollo Program (cost 50, other 40: bits 3 and 5), Scientific civ | 500 (no discount: it is a small wonder) |
| C10 | Palace (cost 10, flags 1), human, 7 cities, WSIZ optimal 20 | `100 * clamp(42/20=2, 3, 10)` = 300; with 40 cities `100*clamp(12,3,10)` = 1000 |
| P1 | cost 30, box 20, net shields 8 → next turn net 5 | box 28 (no completion) then `min(33, 30) = 30`: completes, box 0 (3 shields lost) |
| P2 | box 28, net shields 10, cost 30 | box 30, completes the same turn, box 0 |
| P3 | net shields ≤ 0 | box unchanged |
| P4 | Barracks 40, box 10, net 5, two Civil Engineers (construction 2 each) | turn 1: box 15, pool 4 → 19; turn 2: 20+8 = 28; turn 3: 25+12 = 37; turn 4: 30+16 = 46 ≥ 40 → complete (without them: box reaches 40 on turn 6) |
| P5 | Settlers in a size-3 city (pop 2) | completes, size 1 |
| P6 | Settlers, size 2, food surplus +2, full box | no completion, box stays 30, retried each turn |
| P7 | Settlers, size 2, surplus 0: AI / human | AI: production re-chosen; human: ABANDONBASE popup |
| P8 | switching from a 60-shield item with box 50 to a 30-shield item | box becomes 30 (section 10.2) |
| P9 | city of 5 with 1 own-race and 4 foreign-race citizens, Settlers | 1 own-race citizen removed, then 1 citizen of the first foreign race that has a civ in slot order; the unit's `+0x38` becomes that race |

## 10. Changing production

### 10.1 `City::setProduction(this; kind, id, notify)` `0x4AFAB0` (`ret 0xC`)

```
if (kind, id) == (+0x50, +0x4C): goto done                                           0x4AFAC0
if owner == local slot and notify and not multiplayer:
    if not 0x4AF5D0(this; kind, id): return                                          # the "confirm switch" check, 10.2
(oldKind, oldId) = (+0x50, +0x4C);  (+0x50, +0x4C) = (kind, id)
if kind == 1 and BLDG[id].+0xEC & 0x80000 (Capitalization): +0x30 |= 0x20 else +0x30 &= ~0x20
if kind != 0:
    cost = cost of the NEW item;  if +0x44 > cost: +0x44 = cost                      # 0x4AFB50..0x4AFC95
if Player[owner].+0xA4 == 1 (mobilized):                                             # 0x4AFCB0
    +0x1C8 = +0x1CC = +0x1D0 = 0;  0x4B0470(this; 0, 1)                              # re-pick every worked tile
    for k = 1 .. 20: if 0x4C2680(this; k): 0x4B0470(this; k, 1)
    0x4B10F0(this);  vtable[+0x38](0)
if new item is Capitalization or old item was:   0x4B0540, 0x4B05D0, 0x4B07C0, 0x4BCFF0 # yields change with coinage
# bookkeeping of "who is building what" (P arrays are int16)
old kind 1: P.+0x15E0[oldId] -= 1;  old kind 2: P.+0x15F4[oldId] -= 1; 0x55A080(P; oldId, 1)
            (if notify and human: the matching marker array -= 0x40)
new kind 1: P.+0x15E0[id] += 1; (notify and human: P.+0x15E4[id] += 0x80); +0x1D8 = +0x1DC = -1;
            if BLDG[id].+0xF0 & 4 (great wonder): 0x4DD420(0x9F8700; owner, id)    # announce construction
new kind 2: P.+0x15F4[id] += 1; 0x55A0E0(P; id, 1); (notify and human: P.+0x15F8[id] += 0x80)
done: if owner == local slot and byte [0x7423CD]: 0x41C0B0(0x740A00)                 # refresh the city screen
```
There is **no penalty for changing between unit, improvement and wonder**: the only loss is the clamp of
the box to the new cost. `+0x3B8` is not touched. Each `0x55A080`/`0x55A0E0` is a per-item counter
helper on the player (**O**).

### 10.2 `0x4AF5D0(this; kind, id)` (`ret 8`): the confirm dialog (`CONFIRMSWITCH`)

```
if (kind, id) == current:                         return false     # 0x4AF605
if owner != local slot ([0x9FD4BC]):              return true      # 0x4AF62C
if kind == 2:  if not 0x4C04E0(this; id, 1, 1, 0): return false;   newCost = 0x56A210(P; id, 0)
elif kind == 1: if not 0x4BFF80(this; id, 1):       return false;   newCost = 0x569FE0(P; id, 0)
else:                                                return false    # kind 0 is never confirmed
cur  = 0x437E30(this)                              # the CURRENT item, before the change
stock = max(0, +0x44 + (cur.kind == 1 ? +0x3B8 : 0))
if stock <= newCost:                               return true
dialog CONFIRMSWITCH (0x4AF7A9), text argument 0 = new item name (in-memory PRTO `+0x08`, BLDG `+0x44`; set by 0x61C5A0),
        number argument 0 = stock - newCost (via 0x61C570)
return (answer == 0)                               # 0x611530 returns 0 for the first (accept) button
```
`CONFIRMSWITCH` therefore warns about the surplus that the clamp of 10.1 will discard, and the stock
counted includes the construction bonus pool only when the item being abandoned is a building. The
dialog is the sole "penalty" and it never reduces the box by a percentage.

### 10.3 The build queue `0x4B0090` (returns true when an item was started)

```
if +0x1F8 == 0: return false
repeat:
    (id, kind) = entry[0]                       # +0x1FC, +0x200
    +0x1F8 = clamp(+0x1F8 - 1, 0, 9);  shift entries 1.. down by one slot
    ok = (kind == 1) ? 0x4BFF80(this; id, 1) : (kind == 2) ? 0x4C04E0(this; id, 1, 1, 0) : keep previous ok
until ok or +0x1F8 == 0
if ok: 0x4AFAB0(this; kind, id, 1)
return ok
```

### 10.4 `vtable[+0x58]` = `0x436FE0`, `City::chooseProduction(this; newQueueFlag)` (`ret 4`)

```
(kind, id) = current;  prevUnit = (kind == 2) ? id : -1
0x4AFAB0(this; 0, -1, 0)                         # clear the production
+0x1F8 = 0 (clamp);  all nine queue entries = (id -1, kind 0)
g = 0x5A6060(x, y, 6, -1, 0, -1)                 # land units (class 0) of any owner on the city tile
if multiplayer and owner human:   repeat = (P.+0x130C byte != 0) and (+0x34 & 0x10) == 0          # 0x43706A..0x4370B5
elif owner == local slot:         repeat = (+0x34 & 0x10) == 0 and ([0xA52678] & 0x20000) != 0     # 0x4370B7..0x4370DD
else:                             repeat = false
         # H: "repeat the last unit" option (a player option byte in multiplayer, bit 0x20000 of the global
         #    preference dword [0xA52678] otherwise; the completion popup for units uses bit 0x10000)
if repeat and prevUnit != -1 and 0x4C04E0(this; prevUnit, 1, 0, 0):   result = (2, prevUnit)
elif newQueueFlag and (+0x38 & 3) == 0 and (g == 0 or (g == 1 and P.+0x194 <= 2 and +0x138 == 1
        and +0x40 == 0 and +0x254 > 0 and 0x500F50(P; 1) == 0)):                    result = 0x42BEE0(this)
else:                                                                               result = 0x42C8A0(this)
0x4AFAB0(this; result.kind, result.id, 0)
```
`0x42C8A0` (30 KB) is the advisor/AI item chooser (**O**). `0x42BEE0` is specified here (**V**, read in full):

```
0x42BEE0(C; out)      // ret 4, ecx = city, out = {id at +0, kind at +4}
out.kind = 0;  best = 99999
for pass in (Defense strategy = PRTO.+0x8C bit 1, Offense strategy = bit 0):        // pass 1 then pass 2
    for t in 0 .. N-1  (N = [0x9C3DB0], the number of unit prototypes; row = [0x9C71E0] + 0x138*t):
        if C.canBuildUnit(t; obsoleteCheck = 1, unread = 1, allowKing = 0) (0x4C04E0, buildable.md 3.1) and row.+0x8C has the pass's bit:
            c = Player[C.owner].unitCost(t; 0)  (0x56A210, city-founding.md 5.4)
            if c < best:  best = c;  out = (id t, kind 2)                              // strict: ties keep the earlier type and the Defense pass wins ties
```

So it returns the **cheapest buildable (non-obsolete) unit tagged Defense or Offense**, Defense winning an exact tie; `out.kind` stays `0` when none qualifies (and `out.id` is then unset).

## 11. Open items

1. ~~`0x4ACF40`~~ **Resolved**: the add/remove-building routine is specified in
   [`city-buildings.md`](city-buildings.md) (add path, remove path, the common tail, the replace-by-flag
   sale, free advances, the culture cache, all 21 callers). Its still-open callees are listed there in
   section 10.
2. `0x42C8A0` (item chooser), `0x433EE0` (AI purchase); the tile-distance test inside
   `0x436D10` (`0x436E40..0x436F00`).
3. `0x55A080` / `0x55A0E0`; `0x4DD420`; the meaning of the last-popup flags `+0x3B0`, `+0xA4`.
4. The writers of citizen `+0x144`/`+0x148` (the pending race change). (The `CULT` table behind `0x4F8C50` and the
   meaning of `Player +0xD30` are resolved, see 8.3.)
5. `0x4B45A0` (disease) is specified in `disease.md`; the culture flip `0x4B28D0` and the culture accumulation `0x4B2680` are in `borders-culture.md`.
6. The AI/human difference in `0x4B9270` 6.2 (the double test of `canBuildImprovement`) is read exactly;
   its design intent is unknown.
7. Hurry purchase cost (`0x4B5290`): decoded in [`hurry.md`](hurry.md); so is the citizen removal routine
   `0x4BA230` that the unit-completion step calls.

## 12. Corrections to earlier documents

* `ai.md` "Spaceship/production tail `0x4B9270`": `0x4B9270` is the building-completion routine
  (wonder lost, completion, announcements), not a spaceship tail.
* `happiness.md` section 9 step 10 "production and the rest (unread)": `0x4B9950` is sections 5 to 7
  above; `0x4B2E10` is the resistance-quelling step, `0x4AC140` the nationality drift, `0x4B45A0` the
  disease step.
* `ai.md` hurry rows call `0x4B9270` a "bombard/riot effect chain" in one place: the `0x61C5A0` calls
  there are message-argument setters (`0x4B98FA`, `0x4B990F`), not effects.

## 13. Gameplay production corrections (2026-10-04)

`src/cities.rs::process_city_turn` now clamps positive shield income to the
current price (`0x4B99CF..0x4B9A81`) and empties the box on unit/building
completion (`0x4B8CDF..0x4B8D08`, `0x4B9648..0x4B9662`). A full box still
completes with zero net shields. No excess reaches a queued or repeated item.
Wealth clears stored shields and advances an existing queue without producing
the newly selected item that turn (`0x4B9957..0x4B9981`). A human's empty
Wealth queue keeps Wealth.

`City::change_build`, shared by the city screen and production advisor, now
keeps shields up to the new price (`0x4AFB50..0x4AFC95`). The former
unit/building class switch penalty was incorrect and has been removed.
Same-item selections remain a no-op. Raw production/Wealth, completion and
switch-clamp instructions were reopened for these changes.

Verified 483 game tests and game build. Regressions cover surplus discarded
on unit and building completion, the following queued turn, Wealth's stored
box/queue, and cross-class switches with cheaper-item clamping. Actual
rendered F8/end/F5/F8 checks complete a Warrior from box 9 with income 3,
produce the additional unit and select queued Barracks at box 0. A following
Wealth fixture with box 30 advances to Barracks at box 0. Actual city-screen
Change/Pick switches Barracks box 30 to Warrior box 10; screenshots and saves
are `/tmp/open4x-production-{render,wealth,switch}.{png,json}`.

ABANDONBASE for local humans and empty-city removal are integrated; see
`city-removal.md` for verified behavior and remaining destructor gaps. Native
AI/governor production reselection remains pending.
CONFIRMSWITCH is integrated in section 14. The clone's AI still has
its prior conservative class-switch policy. Construction specialists,
rally points and the remaining non-Ancient production branches require the
broader audit. These corrections do not claim full native city-turn parity.

## 14. Shield-loss confirmation (2026-10-04)

`src/build_switch.rs` implements the Ancient Age CONFIRMSWITCH boundary:
validate the requested build, compare stored shields with the new cost, and
ask before committing a loss. A same-item selection does nothing; a switch
with no loss is immediate. Accept calls the shared native cost-clamping
setter. Cancel or Escape leaves production and shields untouched. Enter
accepts. Both the city screen and production advisor use the same request.
The native dialog call/first-answer acceptance at `0x4AF7A9..0x4AF7C8` was
reopened. Current Ancient specialists have no construction bonus pool.

The modal reports the item and exact shields lost, blocks ordinary city and
turn input, and rejects choices invalidated by a changed city, stock or owner.
Load discards the unanswered choice and its UI along with the old game's
other modal state; there is no save-format change. The production advisor's
original completion decision remains pending after either answer.

Verified 487 game tests and game build. Four behavior regressions cover
cancel/accept, immediate no-loss changes, Escape, stale/foreign choices and
production-advisor entry. The save round-trip regression additionally
loads over an unanswered switch and verifies that its choice/root are gone.
Rendered city-screen Change/Pick asks about exactly 20 lost shields when
switching Amsterdam's Barracks box 30 to Warrior cost 10. Cancel preserves
all City fields, turn and RNG even with Governor/Space input attempted
underneath. Accept keeps box 10, and F5/F8 preserves the result. Captures
`/tmp/open4x-switch-cancel-{90,190}.png` and
`/tmp/open4x-switch-accept.png` were inspected.

Local-human ABANDONBASE is integrated in `city-removal.md`. Complete native
city removal and AI production reselection remain pending. This does not
implement later-era Civil Engineer construction
bonuses or network multiplayer dialog suppression.
