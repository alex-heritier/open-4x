# What a player may build

Owns: the two player-level eligibility predicates, `Player::canBuildImprovement`
(`0x56A2A0`) and `Player::canBuildUnit` (`0x56A7C0`), the free-building set
`0x55A560` rebuilds, and the player and world facts they read. Reference:
`rust/src/buildable.rs` (15 tests; the world is the `Realm` trait so a caller
supplies the facts). Neighbours: `economy.md` (the cost `0x569FE0` and the
wonder counters `0x55AA10`, `0x55A8D0`), `government.md` (mobilization),
`yields.md` (what a built improvement then does), `biq-format.md` and
`biq/src/sections/bldg.rs` (the BLDG field table).

What is **not** here: whatever a *city* adds on top for an **improvement** (the
required improvement in that city, a coastal site, the resources in its trade
network) is decided in the city-level routines and is not decoded. The city
layer for **units**, `City::canBuildUnit` `0x4C04E0`, is section 3.1, with the
upgrade-replacement walk `0x4C0690` of section 3.2.

Method as in `yields.md`: raw disassembly of `Civ3Conquests.exe`, data checked
against the decoded `conquests.biq`. Memory offsets below are the in-memory row
offsets (`BLDG` stride `0x110`, `PRTO` stride `0x138`); a `conquests.biq` body
offset is the memory offset minus 4.

## 1. At a glance

* Both predicates are decision trees whose every branch returns false and
  whose single exit is true, so the **order of the checks changes only which
  check fires first**, never the answer.
* An improvement needs: its tech (`-1` always met; the tech **count** means "no
  tech can grant it", never met), when `num_buildings_required > 1` that many
  of the required improvement empire-wide, its government, the spaceship rules
  for a part, "a great wonder exists once", "a small wonder once per
  civilization", the army conditions, half the world's optimal city count for
  a corruption-reducing small wonder, and mobilization.
* The **strict** variant (the second argument) additionally refuses what some
  city of the player already builds or has queued (Palace, wonders, small
  wonders, spaceship parts; never Wealth) and a spaceship part when built plus
  in production already equals the ship's limit.
* What a great wonder supplies to every city can never be built (the set at
  `Player +0x20D0` rebuilt by `0x55A560`).
* A unit needs: a civ that may build it (`RACE` index bit), its tech, a human
  may not build an AI alternate prototype, nobody builds the Great Leader, an
  army needs `(armies + 1) * 4 <= cities`, a nuclear weapon needs the
  Manhattan Project.

## 2. `canBuildImprovement` `0x56A2A0` (`ret 8`: building id, strict)

Order of the code (addresses are the check sites):

| step | site | condition that returns false |
|---|---|---|
| 1 | `0x56A2C1` | `hasTech(BLDG +0xDC)` fails (`0x561440`) |
| 2 | `0x56A2FA` | `+0x90 != -1` and `+0xC0 > 1` and the player owns fewer than `+0xC0` of building `+0x90` (`Player +0x15DC[building]`, words). With a count of 1 the prerequisite is a city matter and is ignored here |
| 3 | `0x56A32C` | `+0xD4 != -1` and `Player +0xA0 != +0xD4` (the government) |
| 4 | `0x56A358..0x56A469` (strict only) | the building is unique (flag `+0xEC & 1` Palace, or `+0xF0 & (4 or 8)` a wonder, or a spaceship part) and not Wealth (`+0xEC & 0x80000`) and some city builds or queues it (`0x437E30`; `City +0x4C/+0x50`, queue `+0x1F8` count and `+0x1FC` pairs) |
| 5 | `0x56A471..0x56A584` (strict only) | a part: built (`Player +0x15FC[part]`) plus in production equals the limit `[0x9C724C][part]` (**equality**, `jne`) |
| 6 | `0x56A592` | a part (`+0xD8 != -1`) and any of: bit 1 of `[0xA5267C]` is **clear** (Space Race is an enabled victory type only when the bit is set), the player owns no small wonder with "Build Spaceship Parts" that its government allows (`0x55AA10(0x10, 0)` is 0), parts built equal the limit |
| 7 | `0x56A60B` | `+0xF0 & 4` (great wonder) and it has been built by anyone (`WonderRegistry@0xA52658 +0x4FC[building]`, `0x538FE0`) |
| 8 | `0x56A649` | `+0xF8 & 0x2000` (Allows Diplomatic Victory) and bit 2 of `[0xA5267C]` is **clear** (the Diplomatic victory is enabled only when the bit is set) |
| 9 | `0x56A676` | `+0xF0 & 8` (small wonder) and the player owns it (`Player +0x15E8[building] != -1`) |
| 10 | `0x56A69E` (the army group, steps 10 and 11) | `+0xF4 & 0x400` (Requires a Victorious Army) and `Player +0x40` bit 0 clear; `+0xF4 & 0x800` (Elite Naval Units, never set in the shipped rules) and bit 8 clear (both bits HYPOTHESIS) |
| 11 | (same group) | `Player +0x188` (armies, word) `< +0xFC` (armies required) |
| 12 | `0x56A6F1` | `+0xF4 & 0x20` (Reduces Corruption: Forbidden Palace, Secret Police HQ) and `Player +0x194` (cities) `<` half of `WSIZ +4` (the world size's optimal city count; signed halving; shipped 14, 17, 20, 28, 36) |
| 13 | `0x56A732` | `Player +0xA4 == 1` (mobilized) and the building is **not** Militaristic (`+0xF0 & 2`), not Wealth, not Allows-Diplomatic-Victory, not a spaceship part |
| 14 | `0x56A76B` | the building index is in the set at `Player +0x20D0` |

`0x55A560` fills that set from the player's great wonders: for every great
wonder with a "gain in every city" improvement (`+0x88`) or a "gain in every city
on continent" improvement (`+0x8C`), whose government requirement is -1 or the
player's, that is not obsolete for the player (`+0xE0` unknown or -1) and
whose city is the player's, the key `gain_in_every_city` (an improvement
index) and the key `(continent + 1) * improvements + gain_in_every_city_on_continent`
(`continent` of the wonder's city). Plain improvement indices in the set are
unbuildable; the continental keys sit above every improvement index and serve
the city-level check. **In the shipped rules no wonder has a global grant**:
only the continental fields are set (Pyramids: Granary; Great Wall: Walls,
obsolete with tech 38; Sun Tzu: Barracks; Hoover Dam: Hydro Plant; The
Internet: Research Lab; Temple of Artemis: Temple, obsolete with tech 29), so
this step never fires in the shipped game.

The wonder counters are `0x55AA10(player, mask, cityFilter)` over the **small**
wonder flags (BLDG `+0xF4`) and `0x55A8D0` over the great wonder flags
(`+0xF8`); both count rows whose required government is -1 or the player's,
whose city (`Player +0x15E8[row]` for a small wonder, `0x539030` for a great
one) is the player's and, when `cityFilter` is non-zero, is that city.

## 3. `canBuildUnit` `0x56A7C0` (`ret 0xC`: prototype, unread, allow-king)

| step | site | condition that returns false |
|---|---|---|
| 1 | `0x56A7E8` | the player is human (bit in `[0xA526BC]`) and `PRTO +0xA0 != -1` (an alternate prototype of the AI) |
| 2 | `0x56A80A` | not allow-king and ability bit 29 (`0x5E4EF0(0x1D)`, a king) |
| 3 | `0x56A831` | `PRTO +0x90 & (1 << race)` is 0 (`race = Player +0x20`; `shl` masks the count to five bits) |
| 4 | `0x56A85A` | `hasTech(PRTO +0x74)` fails |
| 5 | `0x56A889` | the prototype is `[0x9C728C]` (RULE "Battle-Created Unit": the Great Leader; 47, Leader, in the shipped rules): never buildable |
| 6 | `0x56A89B` | ability bit 18 (`0x5E4EF0(0x12)`, an Army) and `(armies + 1) * [0x9C725C] > cities` (RULE "Cities Needed to Support an Army", 4) |
| 7 | `0x56A8CB..0x56A9B2` | ability bit 16 (`0x5E4EF0(0x10)`, a nuclear weapon) and no great wonder with BLDG `+0xF8 & 0x100` (Allows Construction of Nuclear Devices) has been built by anyone and the player owns no small wonder with it |

### 3.1 The city layer: `City::canBuildUnit(C; u, obsoleteCheck, a3, a4)` `0x4C04E0` (`ret 0x10`)

`C` is a city record (owner byte `+0x28`, tile `x` word `+0x24`, `y` word `+0x26`); `u` a PRTO index. `a3`
and `a4` are passed unchanged to the player predicate as its second and third arguments (section 3).
As in section 3 every branch returns false and the single exit is true.

| step | site | condition that returns false |
|---|---|---|
| 1 | `0x4C051C` | the owner's `Player::canBuildUnit(u, a3, a4)` (`0x56A7C0`) is false |
| 2 | `0x4C0531..0x4C05A2` (only when `obsoleteCheck != 0`) | **unless** (ability bit 15, Starts Golden Age, and `Player +0x3C == -1`): walking the upgrade chain `s = PRTO[u].upgrade_to (+0x78)`, then `PRTO[s].upgrade_to`, ..., some `s` satisfies `City::canBuildUnit(C; s, 0, a3, a4)`. The city must build the upgrade instead |
| 3 | `0x4C05BB..0x4C05F7` | `PRTO[u].unit_class (+0x9C) == 1` (sea) and `0x5EEDB0(0x9C736C; x, y)` (below) is `-1` or the size (`+0x24`) of that body in the water-body table `[0x9C7580]` (stride 40) is `<= 20` |
| 4 | `0x4C05FE..0x4C061A` | one of the three required resources (`PRTO +0x7C`, `+0x80`, `+0x84`; `-1` skipped) fails `City::resourceUsable(C; r)` (`0x4ADE30`, `primitives.md` 4.1: the resource must be in the city's own mask or, when the city is connected to the capital, supplied to the owner) |
| 5 | `0x4C062F..0x4C066C` | ability bit 18 (an Army) and `0x55AA10(Owner, 2, C)` is 0: the city does not hold a small wonder with BLDG `+0xF4 & 2` (in the shipped rules the Military Academy only), counted with the government filter of section 2 |

`0x5EEDB0(cells; x, y)` (`ret 8`) scans the eight neighbours of the tile (offsets from `0x5E6E50`; the map
wraps in x when `cells +0x1F0` bit 0 is set and in y when bit 1 is), keeps those inside the map whose
terrain is water (cell vtable `+0x8C`), reads each one's water-body id (vtable `+0xB8`, a signed word),
and returns the id of the **largest** bordering body by the size word `+0x24`, the first seen winning
ties; `-1` when no neighbour is water. A sea unit therefore needs a city that borders a water body of
more than 20 tiles (a lake of 20 or fewer does not qualify).

### 3.2 The replacement walk `0x4C0690(C; u)` (`ret 4`)

Used by the post-research hook (`research.md` 10.1) to switch a city that is building an obsolete unit:

```
s = PRTO[u].upgrade_to
while s != -1 and not City::canBuildUnit(C; s, 1, 0, 1):  s = PRTO[s].upgrade_to
if s == -1: return -1
a = PRTO[s].+0xA0;  if a != -1: s = a                         // the AI alternate prototype's base
for i in 0 .. PRTO.count - 1:
    if (i == s or PRTO[i].+0xA0 == s) and PRTO[i].+0x8C == PRTO[u].+0x8C: return i      // same AI strategy mask
return s
```

Because the walk asks with `obsoleteCheck = 1` (step 2 above), a unit is accepted only when none of its own
upgrades is buildable, so the result is the last buildable unit of the chain. The final scan substitutes
a prototype with the same `ai_strategies` mask (`+0x8C`) as the original `u` when `s` has an alternate
(`+0xA0`) that qualifies; otherwise `s` itself.

## 4. Facts read from the world

| fact | where |
|---|---|
| technology known | bit `1 << Player +0x1C` in `[0xA52B4C][tech]` |
| technology count | `[0x9C3DBC]` (83 in the shipped file) |
| government | `Player +0xA0` |
| mobilized | `Player +0xA4 == 1` |
| improvements owned | `Player +0x15DC[building]` (words) |
| small wonders owned | `Player +0x15E8[building]` (city id, -1 none) |
| spaceship parts built | `Player +0x15FC[part]` (words) |
| part limit | `[0x9C724C][part]`: the RULE int array behind the count `[0x9C72A8]` (body `+0x60`, 10; the ten dwords at body `+0x64..+0x8B` are all 1 in the shipped file) |
| armies, cities | `Player +0x188` (word), `Player +0x194` |
| game flags | `[0xA5267C]`: bit 1 (`& 2`) Space Race victory enabled, bit 2 (`& 4`) Diplomatic victory enabled (`biq::sections::game::flags`) |
| free-building set | `Player +0x20D0`, rebuilt by `0x55A560` |
| victory bits | `Player +0x40` bit 0, bit 8 (HYPOTHESIS: a victorious army, elite naval units) |

## 5. The shipped data these tests meet

(Checked against the decoded `conquests.biq`; names are BLDG rows.)

* **Wall Street needs 5 Stock Exchanges, Strategic Missile Defense 5 SAM
  Missile Batteries, Battlefield Medicine 5 Hospitals**: the only rows with
  `num_buildings_required > 1`.
* **Spaceship parts** (type, tech): SS Thrusters (0, Satellites), Engine (1,
  Space Flight), Docking Bay (2, Space Flight), Cockpit (3, Space Flight), Fuel
  Cells (4, Superconductor), Life Support (5, Superconductor), Stasis Chamber
  (6, Robotics), Storage/Supply (7, Synthetic Fibers), Planetary Party Lounge
  (8, The Laser), Exterior Casing (9, Synthetic Fibers); the limit is 1 for
  every type.
* **Small wonders** (category "Sm. Wonder", one per civ): Heroic Epic (needs a
  victorious army), Iron Works, Forbidden Palace and Secret Police HQ
  (Reduces Corruption; the latter requires Communism, government 3), Military
  Academy (victorious army), The Pentagon (armies required 3), Wall Street,
  Apollo Program (Build Spaceship Parts), Strategic Missile Defense,
  Intelligence Agency, Battlefield Medicine. 29 great wonders.
* **Militaristic** rows (buildable while mobilized, the "Militaristic" box):
  Barracks, Walls, SAM Missile Battery, Coastal Fortress, Harbor, Airport,
  The Great Wall, Sun Tzu's Art of War, Leonardo's Workshop, The Manhattan
  Project, Military Academy, The Pentagon, Strategic Missile Defense,
  Intelligence Agency, Battlefield Medicine, The Internet, Civil Defense, The
  Statue of Zeus, Knights Templar.
* The Manhattan Project carries "Allows Construction of Nuclear Devices"
  (`+0xF8 & 0x100`), the United Nations "Allows Diplomatic Victory"
  (`+0xF8 & 0x2000`).

## 6. Verified, hypothesis, never located

Verified: the two decision trees and their addresses, the field offsets and the
shipped values above. Rust tests assert each branch (including the equality
test of step 5 and the signed halving of step 12).

HYPOTHESIS: `Player +0x40` bits 0 and 8 as the army and navy victory facts.

Verified (section 3.1, 3.2, read from `0x4C04E0`, `0x4C0690`, `0x5EEDB0`): the city layer for units and
the replacement walk. In the shipped `conquests.biq` the only row with `small_wonder_flags & 2` is the
Military Academy (`0x402`; Heroic Epic is `0x401`, The Pentagon `0x4`), so step 5 means "an Army is built
only in the city holding the Military Academy".

Never located or unread: the city-level checks that complement the improvement predicate (required
improvement in the city, coast, resources); `0x437E30` and the queue layout beyond `+0x1F8 / +0x1FC`; the
AI's own eligibility filters; what `Player +0x40` really holds.
