# Unit upgrades (gold upgrades, "Upgrade All", Leonardo's Workshop)

Clean-room specification of how `Civ3Conquests.exe` (PE32, MSVC 6, image base `0x400000`) upgrades a unit to its
successor type: eligibility, the replacement type, the gold price, the execution (what is copied and what is lost),
the "Upgrade All" batch, the multiplayer message, and the one AI helper found. Tags: **V** read from the
instructions with the body opened, **H** hypothesis, **O** open (not decoded). No code is given on purpose.

Conventions follow `primitives.md`: `__thiscall` (`ecx` = this), `ret N`, the last pushed argument is the first
parameter; "unit" = the unit record (pool entry address minus `0x1C`); `P` = `Player` at `0xA52E98 + 0x20E4*slot`.

## 1. The five entry points

| function | role | signature | notes |
|---|---|---|---|
| `0x5C1AD0` | **can this unit do command `c`** (generic gate; the upgrade command is `c = 0x10000100`) | `Unit::canDo(this; c)`, `ret 4`, returns bool in `al` | section 3 |
| `0x5C0620` | upgrade eligibility (the `0x10000100` case of the gate) | `Unit::canUpgrade(this)`, plain `ret` | section 4 |
| `0x5C04D0` | upgrade price in gold | `Unit::upgradeCost(this)`, plain `ret`, returns the price in `eax` | section 6 |
| `0x5C0740` | execute one upgrade | `Unit::upgrade(this; free)`, `ret 4` (the argument is read as a byte), returns the **new unit** (or 0) | section 7 |
| `0x56AAE0` / `0x56AF00` | Upgrade All: ask+confirm / perform | `Player::upgradeAllPrompt(this; prto)`, `ret 4`; `Player::upgradeAll(this; prto)`, `ret 4` | section 8 |

The replacement type comes from `City::replacement(this; prto)` `0x4C0690` (`buildable.md` 3.2); the unit price base
is `Player::unitCost` `0x56A210` (`city-turn.md` 4.2).

## 2. Data read

| datum | where | meaning |
|---|---|---|
| `PRTO +0x78` | `[0x9C71E0] + 0x138*type` | `upgrade_to`, the next prototype in the chain (`-1` none) |
| `PRTO +0x8C` | same | AI strategy mask (bit 1 = Defense; section 9) |
| `PRTO +0x9C` | same | domain: `0` land, `1` sea, `2` air (selects the required facility, section 4) |
| `PRTO +0xA0` | same | alternate-of prototype (used inside `0x4C0690`) |
| `PRTO +0xAC` bit 8 | same | the special-action word; bit 8 is **Upgrade Unit** (`editor.md` row `+0xac`). With the command word `0x10000100` the gate reads `PRTO + 0xA8 + 4*(c >> 28)` = `+0xAC` and ANDs `c` into it (section 3) |
| `PRTO ability 0x12` (Army), `ability 0x0F` (Starts Golden Age) | tested through `Unit::hasAbility` `0x5BC8B0` | sections 4, 7, 9 |
| `BLDG improvement_flags` `+0xEC` | `[0x9C40AC] + 0x110*b` | bit 1 (`0x2`) Veteran Ground Units (Barracks), bit 17 (`0x20000`) Veteran Sea Units (Harbor), bit 18 (`0x40000`) Veteran Air Units (Airport) |
| `BLDG wonder_flags` `+0xF8` bit 6 (`0x40`) | same | **Halves Unit Upgrade Cost** (Leonardo's Workshop in the shipped rules) |
| RULE `upgrade_cost` | body `+0x134` -> global `[0x9C7318]` | gold per shield of price difference (stock 3; `biq` `GeneralRules::upgrade_cost`, `0..=1000`) |
| `[0xA52684]` | game difficulty index | selects the AI discount (section 6) |
| `[0xA526BC]` | human-player bit mask | bit `1 << P.slot` is set for a human |
| `P +0x44`, `P +0x48` | treasury cells | gold = sum; written back with the obfuscated split of section 7 (`combat.md` "Gold") |
| `Unit +0x20` id, `+0x24/+0x28` x/y, `+0x34` owner slot, `+0x38` nationality, `+0x40` PRTO row, `+0x44` experience index, `+0x50` movement used, `+0x60` carrier id, `+0x64` order (`1` fortified), `+0x74` custom name (NUL-terminated string copied byte-wise) | `unit-turn.md` 40-75 |

## 3. The command gate `Unit::canDo(c)` `0x5C1AD0` (the part every command shares) **V**

```
remaining = clamp(maxMove(this) - this.+0x50, 0, 9999)      # 0x5BE470 = movement allowance; clamp helper 0x426710(v, lo, hi)
if remaining <= 0:                    return false          # a unit with no movement left can never upgrade
kind  = c >> 28                       # 0 = standard order, 1 = special action, ...
field = PRTO[this.+0x40] word at  0xA8 + 4*kind
if (field & c) & 0x0FFFFFFF == 0:     return false          # the prototype must have the action bit (low 28 bits of c)
if !map.inBounds(this.x, this.y)  (0x426BD0):  return false
if kind == 1:  dispatch on c:   c == 0x10000100  ->  return Unit::canUpgrade()         # 0x5C1E76 -> 0x5C0620
```

Consequences: (1) an upgrade needs **movement left** (a unit that has used its whole move this turn cannot be
upgraded, and an upgraded unit has none left, section 7); (2) a prototype whose special-action bit 8 is clear is never
upgradable even if `upgrade_to` is set.

## 4. `Unit::canUpgrade` `0x5C0620` **V**

All of the following must hold, checked in this order (each failure returns false):

1. **Not inside an Army.** `carrier = pool.get(this.+0x60)`; when `this.+0x60 >= 0`, the pool entry exists and
   `carrier.hasAbility(0x12)`, return false.
2. `C = cityAt(this.x, this.y)` (`0x56D2C0`, any owner) exists.
3. `s = C.replacement(PRTO of this)` (`0x4C0690`) `!= -1` (section 5).
4. `upgradeCost(this) <= P.+0x44 + P.+0x48` (the owner's gold, section 6).
5. **Facility in the city standing on the tile**, by the unit's domain `PRTO.+0x9C`:
   domain 0 -> `C.countBuildings(improvement_flags & 0x2) > 0`; domain 1 -> `& 0x20000 > 0`; domain 2 -> `& 0x40000 > 0`;
   any other value -> false. The counter is `0x4B1F90(C; mask)`: the number of the city's **non-obsolete** buildings
   whose `improvement_flags` has any bit of `mask` (`primitives.md` 2.3 obsolescence convention).

Notes: the facility test and the replacement walk are made on **the city on the tile, whoever owns it** (a unit sitting
in an allied city is judged by that city; **H** on whether the game lets one stand there under every treaty). A
Barracks/Harbor/Airport that is obsolete (`BLDG +0xE0` tech known) does not count.

## 5. The replacement type (from `buildable.md` 3.2, restated because the upgrade depends on its exact shape)

```
s = PRTO[u].upgrade_to
while s != -1 and not City::canBuildUnit(C; s, 1, 0, 1): s = PRTO[s].upgrade_to
if s == -1: return -1
a = PRTO[s].+0xA0;  if a != -1: s = a
for i in 0 .. PRTO.count-1:  if (i == s or PRTO[i].+0xA0 == s) and PRTO[i].+0x8C == PRTO[u].+0x8C: return i
return s
```

`canBuildUnit(..., obsoleteCheck = 1)` accepts a type only when **none of its own upgrades is buildable** in that city,
so the result is the **last buildable unit of the chain**: a unit upgrades straight to the furthest successor the
city (its owner's techs and strategic resources, via `canBuildUnit`) can build, skipping intermediate types.

## 6. Price `Unit::upgradeCost` `0x5C04D0` **V**

```
C = cityAt(x, y);  if no city:  return 0
s = C.replacement(PRTO of this);  if s == -1:  return 0
old = Player(owner).unitCost(this.type, 0)           # 0x56A210, forceBase = 0
new = Player(owner).unitCost(s, 0)
cost = [0x9C7318] * (new - old)                      # 32-bit signed multiply; may be negative
if Player(owner).countWonders(0x40, 0) > 0:          # 0x55A8D0: a Halves-Upgrade-Cost wonder is active for the owner
      cost = trunc_div(cost, 2)                      # cdq; sub eax,edx; sar eax,1   (toward zero)
if owner is NOT a human (bit 1 << slot clear in [0xA526BC]):
      L = [0xA52684]
      if   L > 4:  cost = trunc_div(cost, 8)         # (cost + ((cost >> 31) & 7)) >> 3
      elif L > 3:  cost = trunc_div(cost, 6)         # multiply by 0x2AAAAAAB, add the sign bit of the high word
      elif L > 2:  cost = trunc_div(cost, 4)         # (cost + ((cost >> 31) & 3)) >> 2
      # L <= 2: unchanged
return max(cost, 0)                                  # setl/dec/and: a negative price becomes 0
```

* `unitCost(id, 0)` is `max(1, f * PRTO[id].+0x54 / 10)` with `f = 10` for a human and the difficulty's `cost_factor`
  for an AI (also halved by Accelerated Production, `city-turn.md` 4.2). So **an AI player's price difference is scaled
  by its own cost factor before the 1/8, 1/6, 1/4 discount**; a human's is the plain shield difference.
* The discount steps are on the *game* difficulty index `[0xA52684]` (0 Chieftain ... 5 Sid, as used by `research.md`
  `DIFF[level]`), not on the AI player's own level; humans never get it. **H** on the index names only; the thresholds
  `> 4`, `> 3`, `> 2` are **V**.
* The Leonardo test is `countWonders(mask 0x40, onlyCity 0)`: the wonder must be owned by the player, not obsolete
  (`primitives.md` 3.3: `BLDG +0xE0` tech known -> skipped) and available under the player's government (`BLDG +0xD4`).
  A scan of all 27 direct calls to `0x55A8D0` shows `0x40` is pushed only at `0x5C056D`; this is the only use of the
  bit through that accessor (other accessors were not scanned, **O**). **There is no code in the exe that upgrades
  units for free each turn: in this ruleset Leonardo's Workshop only halves the price.**
* Rounding: the halving and the three divisions all truncate toward zero; the final clamp makes negative prices 0.
  The order is halve, then AI discount, then clamp.

## 7. Execution `Unit::upgrade(this; free)` `0x5C0740` **V**

```
1  if free == 0:
       cost = upgradeCost(this)
       T = P.+0x44 + P.+0x48 - cost                         # no affordability test here (canUpgrade did it); T may be < 0
       if T > 0:  r = timeGetTime() mod T - 12345;          P.+0x44 = r;  P.+0x48 = T - r
       else:      r = timeGetTime() mod 54321 - 33333;      P.+0x44 = r;  P.+0x48 = -r          # treasury set to 0, never negative
2  C = cityAt(x, y);  s = C ? C.replacement(PRTO of this) : -1
3  n = P.createUnit(type = s, x, y, tribe = -1, unitId = -1, 0, 0, -1)       # 0x5694D0, the unit factory
4  if n != null:
       if this.+0x64 == 1 (fortified):  n.setOrder(0); [n.+0x38D != 0: animation bookkeeping on n.+0x280/+0x284 = 3]; n.setOrder(1)
       n.customName = this.customName                       # byte copy of +0x74 up to and including the NUL
       n.+0x44 (experience) = clamp(this.+0x44, 0, 2)       # an Elite (3) comes out Veteran (2)
       n.+0x38 (nationality) = this.+0x38
       n.+0x50 (movement used) = maxMove(n)                 # 0x5BE470: the new unit has no movement left this turn
5  for every unit k in the pool (index 0..last) with k.+0x60 == this.id:       # cargo of the old unit
       if n == null: skip
       k.cancelGoto(-1, -1)                                                    # 0x5C59B0 (H: clears a go-to)
       if n.canCarry(k)                                                        # 0x5C5BD0 (H: capacity/domain test)
           k.+0x60 = n.id;  k.setOrder(1)
           if PRTO[n.type] has ability 0x12 (Army):  n.addToArmy(k)           # 0x5BCC90 stack merge
6  this.kill(0, 0, 0, 0, 0, 1, 0)                                              # 0x5BBBC0, ALWAYS, even when n == null
7  return n
```

What survives and what does not:

* Kept: owner, tile, nationality, custom name, fortified status, experience (capped at index 2), cargo that the new
  hull can carry (re-attached and set to order 1).
* **Not** kept: the unit id (a fresh id is allocated by the factory), damage (`+0x4C` of the new unit is its initial
  value, so an upgrade fully heals), every other order (go-to, sentry, automate, ...), status bits `+0x48`, movement
  (`+0x50` is filled to the allowance), the goto path.
* **Latent original behaviour (V from the control flow):** if the factory returns null the gold has already been
  taken (when `free == 0`) and step 6 still removes the old unit. Cargo is only re-attached when `n != null`; cargo the new
  hull cannot carry keeps `+0x60` = the dead unit's id and is left to the kill routine (**O**: what `0x5BBBC0` does to
  cargo whose carrier dies; `combat.md`/`unit-turn.md` give the death-time rules).
* No caller found passes `free != 0` (the seven call sites: `0x466D1C`, `0x4DAEE6`, `0x4DEB47`, `0x4DEB5A`,
  `0x56AF7D`, `0x56AF93`, `0x571B74`; the last one pushes `edi`, whose value was not traced, **H** that it is 0).
  The `free` path is therefore a dead or scenario-only option (**H**).

## 8. Upgrade All

### 8.1 `Player::upgradeAllPrompt(this; prto)` `0x56AAE0` (human, local) **V**

```
count = 0; sum = 0
for each unit u in pool (index 0..last):
    if u.owner == P.slot and u.type == prto and u.canDo(0x10000100):  count += 1;  sum += u.upgradeCost()
if count == 0:                        show dialog NO_UNITS_TO_UPGRADE_ALL (argument: the prototype's name, PRTO +0x08) and return
if sum > P.+0x44 + P.+0x48:           show dialog NO_GOLD_TO_UPGRADE_ALL  (arguments: name, sum) and return
show the confirmation dialog UPGRADE_ALL (arguments: name, count, sum; the order of the two numbers is H)
if the dialog returns 0 (accepted):
       if multiplayer (0x47B530):  send message kind 47 {+0x30 = P.slot, +0x34 = prto}  via 0x473380(0x74AF60; ...)
       else:                       P.upgradeAll(prto)                                     # 0x56AF00
```

The dialog flags carry `0x4000` when the multiplayer gate is true. The Military-Advisor style screen `0x4DE8B0` contains
an inlined copy of the same count/sum/message/perform logic (strings `NO_UNITS_TO_UPGRADE_ALL`, `NO_GOLD_TO_UPGRADE_ALL`,
`UPGRADE_ALL`, `5C0740` called twice at `0x4DEB47`/`0x4DEB5A`); the unit-list screen `0x4DAA70` (call `0x4DAEE6`) and the
unit-action handler `0x571504` (price shown at `0x571675`, execution at `0x571B74`) are the single-unit entry points
(**H**: bodies not read beyond their call sites).

### 8.2 `Player::upgradeAll(this; prto)` `0x56AF00` **V**

```
for i = 0 .. last unit index (the bound is re-read every iteration):
    u = pool[i]
    if u.owner == P.slot and u.type == prto and u.canDo(0x10000100):          # re-checked per unit, so gold is re-tested per unit
        if u is the selected unit [0x9FD474] and P is the local slot [0x9FD4BC] and !multiplayer:
              n = u.upgrade(0);  UI.selectUnit(0x9F8700; n)                  # 0x4DBA70: the selection follows the new unit
        else: u.upgrade(0)
```

Units are processed in ascending pool index, each paid separately; a unit whose price no longer fits the remaining
treasury fails its own `canDo` and is **skipped silently** (the prompt's sum check normally prevents that). New units are
appended to the pool and are visited later in the same loop, but have a different prototype, so they are not upgraded
again.

### 8.3 The network message

Kind **47** of the dispatcher `0x46F8B0` (table `0x47055C`, kinds 11..115; handler `0x46FD4C`) runs
`Player[msg.+0x30].upgradeAll(msg.+0x34)` on every peer with no further validation (each peer re-runs `canDo` per unit,
so the result is deterministic when the states agree). The local prompt `0x56AAE0` is also called from `0x46B509`
(`0x46AD59`, the UI command handler) **H**.

## 9. The AI-side helper: `Unit::tryAutoUpgrade` (Unit vtable `+0x44`, `0x466CC0`) **V** (body) / **O** (callers)

The Unit vtable is at `0x66DCF0` (stored by the factory `0x5694D0` at `0x5695A4`); slot `+0x44` holds `0x466CC0`:

```
if not canDo(this, 0x10000100):  return
if this.hasAbility(0x0F)                                   # Starts Golden Age
   and ((PRTO[this.type].+0x8C >> 1) & 1) == 0             # AI strategy bit 1 (Defense) clear
   and P.+0x3C == -1:                                      # the civ is not in a Golden Age
       return                                              # keep the unit as it is
upgrade(this, free = 0)
```

It spends the owner's gold on every eligible unit it is called for, except that a unit that can start a Golden Age (and
is not a defender) is left alone while no Golden Age is running. **O:** the virtual call site was not located (a scan of
the 25 `call [reg+0x44]` sites found no unambiguous zero-argument Unit call), so *when* the AI invokes it (which planner,
what gold reserve it keeps, how often) is not specified. `ai.md` section on the planners is the place to continue.

## 10. Golden vectors (hand-derived from the text above, not captured runs)

Inputs: `[0x9C7318] = 3`.

| id | owner, state | old / new shield cost (after `unitCost`) | stages | result |
|---|---|---|---|---|
| U1 | human, no Leonardo | 30 / 80 | `3*50 = 150` | **150** |
| U2 | human, Leonardo | 30 / 80 | `150 -> 75` | **75** |
| U3 | human, Leonardo | 25 / 80 | `3*55 = 165 -> (165-0)>>1 = 82` | **82** (truncates toward zero) |
| U4 | human | 80 / 30 (new cheaper) | `-150` | **0** |
| U5 | AI, `[0xA52684] = 5` | 30 / 80 (already `unitCost`-scaled) | `150 -> (150+0)>>3 = 18` | **18** |
| U6 | AI, `[0xA52684] = 4` | same | `150 / 6` via `0x2AAAAAAB` | **25** |
| U7 | AI, `[0xA52684] = 3` | same | `(150+0)>>2` | **37** |
| U8 | AI, `[0xA52684] = 2` | same | unchanged | **150** |
| U9 | AI, `[0xA52684] = 5`, Leonardo | same | `150 -> 75 -> 9` | **9** |
| U10 | treasury 100, price 150 | - | `canUpgrade` step 4 fails | not offered |
| U11 | treasury 150, price 150 | - | `150 <= 150` passes; after paying `T = 0` | treasury cells become `r = t mod 54321 - 33333`, `-r` (sum 0) |

Treasury split: for `T > 0` the cells are `r = (timeGetTime mod T) - 12345` and `T - r`; the sum is exactly `T`.

## 11. Open items

1. **O** Where the AI calls `Unit::tryAutoUpgrade` and with what gold policy (section 9); the planners `0x446840`,
   `0x445EA0`, `0x449B20` are not read (`ai.md`).
2. **O** The bodies of the UI entries `0x4D8B80`, `0x4DAA70`, `0x4DE8B0`, `0x571504`, `0x46AD59` beyond their call sites
   (text and layout of the upgrade dialogs, hot keys, the Military Advisor list).
3. **O** `0x5C59B0(-1, -1)`, `0x5C5BD0`, `0x5BCC90` semantics are taken from other documents (`workers.md` 36, `air.md` 200,
   `ai.md` "Stack merge") and the call shapes here; `Unit::canCarry` (`0x5C5BD0`) is **H**.
4. **O** What the kill routine does to cargo left on a dying carrier (step 5 failing branch).
5. **H** Difficulty index names for `[0xA52684]`; **H** that no code passes `free != 0`.
6. **O** Whether any other accessor reads `wonder_flags` bit 6 (only `countWonders` callers were scanned).
7. **O** The animation bookkeeping on `n.+0x280/+0x284/+0x38D` (presentation only).
