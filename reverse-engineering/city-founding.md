# City founding: creation, initialisation, naming and the AI start bonus

How a city comes into existence in `Civ3Conquests.exe`: the pool allocator and legality gates of
`createCity` (`0x5663C0`), the initialiser `City::init` (`0x4AE2A0`), the automatic name generator
(`0x565BD0`), and the extra units an AI civilization receives with its first city (the "best defender" and "best
attacker" choosers `0x434D50` / `0x434C40` and the unit cost function `0x56A210`). Clean-room specification:
no Rust module accompanies this file.

Status: every routine named in the headings was read instruction by instruction (**V**) except where a line says
**H** (hypothesis, label inferred from use) or **O** (open, not decoded). Callees that this document only names
(`0x4B0470`, `0x4B10F0`, `0x4B0E80`, the City vtable slots `+0x38` and `+0x58`, `0x4BCEA0`, `0x4BCDE0`, `0x4BAAF0`,
`0x426710`, `0x55CB20`, `0x57E320`, `0x578E90`, `0x58B5D0`) are **O**; section 7 lists them. (`0x4ACF40` and `0x55CB20` are specified in `city-buildings.md`.)
The gameplay RNG (`0x60BAB0`) is called by none of `createCity`, `City::init`, the naming routine or the choosers
(**V** by call-site census over the full `pdf` listing of `0x5663C0`, `0x4AE2A0`, `0x565BD0`, `0x434C40`, `0x434D50`,
`0x56A210`: no `call 0x60BAB0`; control: the same census finds the one call in `0x55B7D0`).
The one random source in `City::init` is the C runtime `rand()` (`0x64A20E`) used for a cosmetic array (3.12).

Conventions: `P` = Player record (`0xA52E98 + 0x20E4 * slot`), `C` = City object (`0x544` bytes), `cell(x, y)` =
`0x437A70(x, y)` = the map cell at index `(W/2) * y + (x >> 1)` where `W = [0x9C74D4]` (`x` and `y` have equal
parity), cell vtable `0x6701C8` (slot meanings in `world-events.md` 1.2). Cities live in the pool `[0xA52E6C]` (array
of 8-byte nodes `{freeLink, objectPointer + 0x1C}`, last index `[0xA52E78]`, free count `[0xA52E74]`, free head
`[0xA52E70]`, capacity `[0xA52E7C]`, pool object `0xA52E68`).

## 1. Who creates cities

| caller | call | arguments `(x, y, nationality, cityId, name, borderFlag)` |
|---|---|---|
| goody hut outcome 2 | `0x55B8B0` (`goody-huts.md` 5.3) | `(x, y, -1, -1, 0, 1)` |
| any unit's "found city" (human UI `0x4D9C80`, orders executor `0x45F630`, network `0x4761E0`) | `Unit::foundCity` `0x5B34C0` (section 2A) | `(U.x, U.y, U.+0x38, id, name, 1)` with `id = -1` |
| scenario / save loading, network replication | other callers of `0x5663C0` (**O**) | an explicit `cityId >= 0` appears when a city must keep its id |

`nationality` is the race index given to the first citizen (`-1` = the owner's race). `name` is a C string or `0`
(automatic name, section 4). `borderFlag != 0` runs the border/territory update after the city exists.

## 2. `createCity(P; x, y, nationality, cityId, name, borderFlag)` `0x5663C0`, `ret 0x18`

Returns the new `C`, or `0`.

```
 1  if [0xA52E78] - [0xA52E74] + 1 >= 512:  return 0                   0x5663F0  live cities = last index - free count + 1
 2  if cell(x, y) has a city (0x5EA6C0):     return 0                   0x56642F
 3  cont = cell(x, y).vt+0xB8()                                         continent word
 4  if cityId >= 0 and pool[cityId] is an occupied node:  return 0      0x56647F
 5  allocate C (0x544 bytes, ctor 0x4AC590 / 0x56BD20, vtable 0x66DC78; list vtable at +0x1C = 0x66DC70)
      cityId < 0:   take the free-list head [0xA52E70] if [0xA52E74] > 0 (head = array[head].freeLink, free count--),
                    else cityId = ++[0xA52E78]  (grow the array with 0x4C22C0 when last == capacity - 1)
      cityId >= 0:  reserve exactly that slot (extend `last`, threading the skipped slots onto the free list,
                    or unlink it from the free list)
      node = {freeLink = -1, ptr = C + 0x1C}
 6  for every unit U in the unit pool [0xA52E84]/[0xA52E90] (index 0..last) with U.+0x34 (owner) != P.slot and
    U.+0x24 == x and U.+0x28 == y:
        multiplayer (0x47B530 true):  0x474140(0x74AF60; U.id, P.slot, 0, 0, 0, 1)     queue the kill
        otherwise:                    Unit::kill 0x5BBBC0(U; P.slot, 0, 0, 0, 1, 0, 0)
 7  terrain: if cell.vt+0x78() is false and cell.vt+0x11C() != -1:
        0x5D59E0(map 0x9C736C; cell.vt+0xC4(), x, y)                    set the tile's terrain to its base terrain
 8  cell.vt+0xCC(plane 0, mask 0x5C, x, y)                              clear mine, irrigation, fortress, pollution
 9  [0xA52690] += 1                                                     cities in the world
    [0xA52B48][cont] += 1                                               cities per continent (world array)
    P.+0x194 += 1;  P.+0x1610[cont] += 1                                cities of P; cities of P per continent
10  w = cell.+0x6C (int16): the city currently working this tile, if the id is a valid pool entry -> W
    if W exists:  n = 0x5F3F50(map; W.x, W.y, x, y, 21);  0x4BBC80(W; n)       W releases that work tile
11  cell.vt+0xE4(cityId);  cell.+0x6C (int16) = cityId
12  if W exists:  W.vt+0x38(0)                                           W is recomputed (slot +0x38, O)
13  0x4AE2A0(C; cityId, x, y, P.slot, nationality, name, borderFlag)    section 3
14  multiplayer: [0xA52680] &= 0xF3
    if ([0xA52680] & 8) != 8 and cell.+0x58 & (1 << Player[local].slot) == 0:  skip the next line
    if 0x4E69F0(0x9F8700; x, y, 0, 0):  [0xA281C4] = 1                   screen refresh when the local player sees the tile
15  return C
```

Notes.

* Step 7. `vt+0x11C` reads the TERR row of the tile's terrain nibble and returns its field `+0x70` (the
  worker job, `terr.rs`: a `TFRM` index, `-1` none). So a city founded on a tile whose terrain has a worker job
  and for which `vt+0x78()` (bit 29 of the dword returned by `vt+0xAC`, meaning **O**) is clear turns the tile
  into its base terrain: **H** that this is the "forest, jungle and marsh are cleared when a city is built" rule.
  `vt+0xC4` is `(vt+0x0() >> 8) & 0xF`, the base-terrain nibble (it takes no stack argument: the `x`, `y` pushed
  before it are consumed by `0x5D59E0`).
* Step 4/5: city ids are the order cities are processed in every per-turn loop (`turn.md`, pool order), so id
  allocation is observable. A new city takes the **most recently freed id** (free head, LIFO, **H** on the push
  side: the frees at `0x4AECC0..0x4AF19B` write the head) before it takes `last + 1`.
* Step 10/12: a city that was working the new city's tile loses it and is refreshed. The tile's worked-by word is
  `cell.+0x6C` (initially the new city's id).
* The 21-tile work radius is the spiral `0x5E6E50(n; &dx, &dy)` for `n = 0..20` (centre plus two rings, the
  "fat cross").

## 2A. Founding by a unit

### 2A.1 Site legality `0x5F3160(map; x, y, civ, flag)`, `ret 0x10` (**V**)

`map = 0x9C736C`. `flag` is `1` at every founding call site (the action gate `0x5C1AD0`, the UI command
`0x4DAA70`, the dialog `0x4DD800`, the orders executor `0x45F630`, the network handler `0x4761E0`,
the AI site valuation `0x442480`). Return value, first match wins:

```
if flag != 0 and cell(x, y) has a city (0x5EA6C0):                      return 2
if cell.vt+0x68() (hosts a colony) and cell.vt+0x118() != civ:          return 2     colony owner is another civ
if cell.vt+0x1C(0) (barbarian camp, overlay bit 7):                     return 2
row = TERR[cell.vt+0xC8()]                                              tag 'TERR' lookup of the terrain nibble
if row.byte[+0x78] == 0 (TERR allow_cities clear):                      return cell.vt+0x8C() (water) ? 3 : 4
for n = 1 .. 8:                                                         the eight neighbours, spiral ring 1
    (nx, ny) = (x + dx, y + dy), wrapped with the map's wrap flags (map+0x1F0 bit 0 = x, bit 1 = y,
    widths map+0x168 / map+0x154); if inside the map and cell(nx, ny) has a city:   return 5
return 0
```

Code `0` is the only legal result; the callers test `== 0` and never distinguish the others (the AI valuation
returns a rejected site as `0`). Notes: (i) a **foreign colony** blocks, an own colony does not (and is destroyed by
`City::init`, 3.16); (ii) only **adjacency** is tested, there is no larger minimum distance in this routine
(**V**; the AI's own spacing rules are in `0x442480`, **O**); (iii) the tile's border owner is **not** tested here.

### 2A.2 The action gate and the callers

* `Unit::canDoAction(U; 0x20000002)` `0x5C1AD0` (`ai.md`, `combat.md` "Action tokens"): action word 2, bit 1; the
  gate also requires movement left (`ai.md`) and then runs `0x5F3160(map; U.x, U.y, U.owner, 1) == 0`
  (`0x5C20CA`). A unit that cannot perform the action cannot found.
* Human UI (`0x4DAA70`, `0x4DB086`): after the gate and `0x5F3160`, in single player the tile owner
  `t = cell.vt+0x98(1)` goes through `0x5B5790(U; t)` (a border-intrusion permission check with a confirmation
  dialog, **O**; it returns false to abort); in multiplayer the check is skipped. Then `0x4D9C80` (the naming
  dialog): it calls `0x565BD0(P; buf, 0)` for the proposed name (`self = 0`, so no city is excluded from the
  collision test; the counter `P.+0x1308` **advances at this point**, even if the player edits or cancels), loops
  until the entered name is accepted (error key `BADCITYNAME`), then calls `foundCity(U; &name, -1)` and shows
  `SETTLEMENT_FOUNDED`.
* Orders executor `0x45F630` (`0x45FA79..0x45FA90`): when the unit stands on its target (and, for a civ with no
  city, `0x5F3160 == 0`) it calls `foundCity(U; 0x74A808, -1)`; the name argument is a global empty string, so the
  city is auto-named (3.2).
* Network `0x4761E0`: re-checks `0x5F3160(map; msg.x, msg.y, owner of U, 1)`; if it fails the unit's flag word
  `+0x48` bit 9 is cleared and the message dropped; otherwise it moves the unit with `Unit::setPosition`
  `0x5BD220`, calls `foundCity(U; msg.name, -1)`, copies three message words into the city with `0x4C1D20`,
  `0x4C1D40`, `0x4C1D60`, refreshes (`vt+0x58(1)`) and registers the city in the list `0xB72888` with
  `0x57DEF0(x, y)`.

### 2A.3 `Unit::foundCity(U; name, cityId)` `0x5B34C0`, `ret 8` (**V**)

```
ui    = 0x49FC50()                                            the UI singleton (used for the failure message)
local = [0x9FD4BC]
if 0x427310(0xA52658; 8) or 0x4A8350(Player[local]; U.x, U.y) or 0x4EDF20(U; local):
    0x4DF6B0(0x9F8700; U.x, U.y)                              centre the view on the tile
    if byte U.+0x38D != 0 and not multiplayer:                short animation:
        [0xA281D0] = 1;  U.+0x1DC = 1;  0x4F00F0(0xA268B8; U, 12, 0);  U.+0x1DC = 0;  [0xA281D0] = 0
t = cell(U.x, U.y).vt+0x98(1)                                 tile owner
0x5B5600(U; t)                                                border-intrusion consequence (O; a no-op when t <= 0,
                                                              t == U.owner, or the unit has ability 17)
C = createCity(Player[U.owner]; U.x, U.y, U.+0x38, cityId, name, 1)       section 2
if C == 0:
    if U.owner == local: show the message "TOOMANYCITIES" (ui vtable +0x170)       pool full, or the tile now has a city
    return 0
if multiplayer: [0xA52680] &= 0xF3
if ([0xA52680] & 8) or 0x4A8350(Player[local]; U.x, U.y) or bit 6 of the dword at 0xA53D48 + 4 * (0x20E4/4 * local + U.owner):
    U.+0x400 = 0                                              (H: the unit's on-screen byte)
Unit::kill(U; 0, 1, 0, 0, 0, 0, 0)                            the founding unit is consumed (unit-turn.md 5)
if not multiplayer: C.vt+0x58(1)
return C
```

The three tests of the first block (`0x5B34DA..0x5B3514`) only drive the camera and the animation; their exact
meaning (`0x427310` on the object at `0xA52658`, `0x4EDF20`) is **O**.

**The founding unit does not transfer its population cost.** Nothing in `foundCity`, `createCity` or
`City::init` adds citizens beyond the single first citizen of 3.12: **a city founded by a unit starts at size
1** regardless of the unit's PRTO `population_cost` (shipped Settlers `2`, Workers `1`). The cost is a *join* and
a *build* quantity: `Unit::joinCity` `0x5B39D0(U; C)` adds `population_cost` citizens one at a time
(`0x4B9F60(C; 1, U.+0x38)`, skipping a step when `0x4B1DC0(C)` is not `-1`, **O**) and then kills the unit.

## 3. `City::init` `0x4AE2A0`, `ret 0x1C`

Arguments in call order: `cityId, x, y, owner, nationality, name, borderFlag`. `this` = `C`. Steps in
execution order (addresses are the first instruction of the step):

| # | at | step |
|---|---|---|
| 3.1 | `0x4AE2B6` | `C.+0x20 = cityId`; int16 `+0x24 = x`, `+0x26 = y`; byte `+0x28 = owner`; `+0x1D8 = +0x1DC = -1`; `+0x1E0` (name buffer, 24 bytes) `= ""` |
| 3.2 | `0x4AE2E4` | name: if `name != 0` and `name[0] != 0` then `lstrcpynA(C+0x1E0, name, 24)` else `0x565BD0(Player[owner]; C+0x1E0, C)` (section 4) |
| 3.3 | `0x4AE315` | `+0x9C = 0`. The **citizen pool** at `+0xDC` is re-created: an existing array is freed, then `{+0xE0 = array of 8 entries x 8 bytes (every entry {-1, 0}), +0xE4 = -1 (free head), +0xE8 = 0 (free count), +0xEC = -1 (last index), +0xF0 = 8 (capacity, global [0x668F64])}` |
| 3.4 | `0x4AE385` | `+0x13C = 0`; the 32 dwords from `+0x140` = 0; `+0x1C0 = +0x1C4 = +0x30 = 0` (propaganda, hurry timer, flags); `0x4BCEA0(C)`; `+0x2C = +0x40 = +0x44 = +0x48 = +0xA4 = +0x50 = 0`; `+0x1F8 = 0x426710(0, 0, 9)`; nine dwords at `+0x200, +0x208, ... +0x240` = 0 (stride 8) |
| 3.5 | `0x4AE3EA` | the founding date object `+0x2FC` = `0x5DF100(round = [0xA526AC])`; `0x4BCDE0(C; 10)`; `+0x5C = 1` (the **border / culture level**, `economy.md` "Border shape by culture level", `capture.md` 4); byte `+0x68 = 0`; `+0xD8 = +0x6C = +0x70 = 0` (`+0x70` is the draft timer); `0x4BAAF0(C; [C+0x134], -1)`; zero 8 dwords at `+0x290` and 8 at `+0x2D4`; zero 3 dwords at each of `+0x244, +0x250, +0x25C, +0x268` (food eaten and surplus tables); set the nine dwords `+0xF4 .. +0x114` to `-1`; byte `+0xA0 = 0` |
| 3.6 | `0x4AE494` | work-area mark: for `n = 0..20`, if `(x + dx, y + dy)` (wrapped by `0x426C00` / `0x426C40`) is a valid tile (`0x426BD0`): `cell.vt+0xE0(plane 2, mask 0x20000, -1, -1)` sets bit `0x20000` of plane 2. `0x442480` (the site valuation, `goody-huts.md` 6.5) counts these marks |
| 3.7 | `0x4AE546` | **first city of the owner** (`P.+0x194 == 1`, the count already includes this city): `0x55CB20(P; C)` (**H**: registers the capital, `P.+0x2C`); the first BLDG row `b` whose byte `+0xEC` (BLDG base `[0x9C40AC]`, stride `0x110`) has bit 0 set is added with `0x4ACF40(C; b, 1, 0)` (**H**: the Palace); if `[0xA5267C] & 0x24000` (Capture the Unit, Reverse CTF) and the dword `P.+0x11D0 != 0`, a unit of prototype `[0x9C7314]` is created: `0x5694D0(P; [0x9C7314], x, y, -1, -1, 0, 0, -1)` |
| 3.8 | `0x4AE5E1` | if `borderFlag != 0`: `0x5D4830(map)` (the border / tile-ownership update; it reaches `0x5D3AB0`, which pops huts and destroys camps on tiles that change owner, `goody-huts.md` 2, `barbarians.md` 9). Then `0x578E90(0xA0E270; x, y, C.+0x5C)` (display, **O**) and the log call `0x58B5D0(0xC88588; flag = ([0x9C757C] == 0), owner, x, y, C+0x1E0, 0)` (**O**) |
| 3.9 | `0x4AE628` | road on the centre tile: if the TERR predicate `0x5DBF10(cell)` holds (**O**; **H**: roads are allowed on this terrain): if `Player[owner]` knows the technology `[0x9C7324] + 0x218` (`0x561440`) then `cell.vt+0xE0(0, 3, x, y)` (road and railroad) else `cell.vt+0xE0(0, 1, x, y)` (road) |
| 3.10 | `0x4AE6E7` | `0x57E320(0xB72888; C)` (**O**; **H**: adds the city to the trade network); `+0x3B8 = 0`; `+0x374 = [0x9C3D80]` (BLDG count); the vector at `+0x378` (begin `+0x37C`, end `+0x380`) is resized to the BLDG count; every element of `+0x37C[i]` = 0 (the per-building counters of `happiness.md`) |
| 3.11 | `0x4AE790` | `+0x1C8 = +0x1CC = +0x1D0 = 0`; `0x4B0470(C; 0, 1)` (the centre tile `n = 0` becomes worked, **O**); for `n = 1..20`: if the (wrapped, valid) tile's `cell.+0x6C == cityId` then `0x4B0470(C; n, 1)` (a ring tile already recorded as worked by this id; normally none); `0x4B10F0(C)` (**O**) |
| 3.12 | `0x4AE84F` | **first citizen**, identical to `0x4B9F60(C; 1, nationality)` (`goody-huts.md` 6.7): if `C.+0x138 < 255`: record = `0x4C2040(C+0xDC; &out)`; race = `nationality`, or `Player[owner].+0x20` when `-1`; `0x4ABD90(record; out, cityId, race)`; `+0x138 += 1` (size, saturating at `0x7FFFFFFF`); `0x4B0E80(C)` (the recompute, zeroes `+0x1C8..+0x1D0`); `C.vt+0x38(0)` |
| 3.13 | `0x4AE8E8` | **AI start bonus** (section 5): first city only, and only when `owner` is not in the human mask `[0xA526BC]` |
| 3.14 | `0x4AEA5B` | **AI garrison**: when `owner` is not human and `[0xA5267C] & 0x25C00` (City Elimination `0x400`, Regicide `0x800`, Mass Regicide `0x1000`, Capture the Unit `0x4000`, Reverse CTF `0x20000`) and the game difficulty `L = [0xA52684] > 2`: one unit `0x5694D0(P; C.vt+0x20(0, 1), x, y, -1, -1, 0, 0, -1)` (the best defender, section 5.2). Applies to every AI city, not only the first |
| 3.15 | `0x4AEAAA` | `C.vt+0x58(1)`; `C.vt+0x38(1)` (**O**, city recompute family) |
| 3.16 | `0x4AEABC` | **colonies on the tile are destroyed**, in this order: (a) plain Colony (`0x5EA6E0(cell)`): `0x5DAA90(colonyPool[0xA52E54][cell.vt+0xBC()]; 1)`; (b) airfield (`cell.vt+0x18(0)`): `0x5DAEC0(airfieldPool[0xA52E3C][id]; 1)`; (c) radar tower (`cell.vt+0x84(0)`): `0x5D6360(map; x, y)`; (d) outpost (`cell.vt+0x4C(0)`): `0x5D6430(map; x, y)`. Details of each destroyer: `world-events.md` 1.4 |
| 3.17 | `0x4AEC2F` | `[0xA0ED88] \|= 2` (a global dirty bit, **O**) |
| 3.18 | `0x4AEC43` | **cosmetic permutation**: `C.+0x74[0..5] = -1`, `C.+0x8C[0..3] = -1`; then six times, `v = rand() mod 8` (C runtime `rand`, `0x64A20E`, signed remainder, kept in 16 bits): if `v` already equals one of the entries filled so far, draw again; else `C.+0x74[i] = v` and, for `i < 4`, `C.+0x8C[i] = v`. Result: six distinct values of `0..7` in `+0x74`, the first four copied to `+0x8C`. Not gameplay RNG (**H**: art variant selection) |

Order notes. The border update (3.8) runs **before** the first citizen exists (3.12) and before the road (3.9).
The Palace (3.7) is added before the border update. The free units of 3.13 exist before the centre tile's colony
removal (3.16).

## 4. The automatic name `0x565BD0(P; out, C)`, `ret 8`

Inputs: the owner's race `R = P.+0x20`; the RACE row (`[0x9C71D0] + R * 0x974`) holds `names = +0x14` (array of
24-byte strings) and `count = +0x928` (the on-disk "number of city names", read by the RACE reader at `0x5E6324`;
`biq`'s `city_names`). The counter is the Player dword `P.+0x1308` (initialised to 0 by `0x567C80`, **V**: `0x567FE8` stores `ebx`, which `0x567C95` cleared and nothing in that function rewrites). `lang = [0xCC3E20]` is the `Language` setting of the INI file (`0x550760` stores
`0x585B00("Language", ...)` through `0x628AF0`); `0` is the base (English) build. `prefix` is the string-table
entry whose id is `[[0xCADC0C] + 0x34C]` (`0x60F6A0`); **H** that it is the label `New` (`Conquests/Text/labels.txt`
line 221).

```
ctr = P.+0x1308
loop:
    if count == 0:                                   RACE without names
        buf = decimal((ctr >> 1) + 1)                   no prefix, no suffix
    else:
        round = ctr / count      (signed idiv)      index = ctr % count
        nm    = names[index]
        buf   = ""
        if round is odd:
            if lang == 0 and nm == "Tokyo":   buf = "Neo-"
            else if strncmp(prefix, nm, strlen(prefix)) != 0:   buf = prefix + " "      0x565CCB
        buf += nm
        if round == 1:
            if lang == 0:
                if nm == "Istanbul":        buf = "Not Constantinople"                 replaces the whole buffer
                else if nm == "Tenochtitlan": buf = "Mingapulco"
        else if round > 1:
            buf += " " + decimal((round >> 1) + 1)
    out = first 23 characters of buf               lstrcpynA(out, buf, 24)
    taken = (out == "") or any live city K != C with K.+0x1E0 == out (strcmp, case-sensitive, all owners)
    ctr += 1
    if not taken: break
P.+0x1308 = ctr mod count                           0x565F8B (idiv; count == 0 would fault)
```

Properties.

* The stored counter always ends in `0 .. count-1` (`ctr mod count`), so a normal call starts in round 0 at the
  slot after the one used last; rounds above 0 are reached only through collisions inside one call.
* "Taken" compares against every city in the pool, of every civilization, including the barbarians.
  A destroyed city frees its name. Cities are compared by the stored name only; a user-renamed city counts.
* The prefix test is a **prefix match of `strlen(prefix)` characters**: with `prefix = "New"` the names `New York`
  and `Newark` are not prefixed again (so round 1 gives the same string as round 0 and collides), while `Newcastle`
  is likewise left alone.
* Round sequence (name `N`): `N`, `New N`, `N 2`, `New N 2`, `N 3`, `New N 3`, ... (round `r >= 2` appends
  `(r >> 1) + 1`).
* Easter eggs (only when `lang == 0`): round 1 of Tokyo is `Neo-Tokyo`; round 1 of Istanbul is
  `Not Constantinople`; round 1 of Tenochtitlan is `Mingapulco`. Rounds 3, 5, ... use the ordinary prefix;
  `Neo-` applies to every odd round of Tokyo (`Neo-Tokyo 2` at round 3).
* `count == 0` branch: the first city is `1`, the next `2`, ... (the decimal of `(ctr >> 1) + 1`, with the counter
  stepping by one after each collision). The final `idiv` would divide by zero (**H**: unreachable in
  practice; a clean-room port should guard it).

Shipped data (`conquests.biq`, 32 RACE rows): `Rome` 44 names, `Japan` 26 (`Tokyo` is index 2), `Aztecs` 36
(`Tenochtitlan` index 0), `Ottomans` 29 (`Istanbul` index 0), `America` 40 (`New York` index 1).

## 5. The AI start bonus (3.13) and the unit choosers

### 5.1 Rule

Condition: `P.+0x194 == 1` and `((1 << P.slot) & [0xA526BC]) == 0` (a computer civ founding the city that is its
only city). Let `D = DIFF[L]` with `L = [0xA52684]` (the game difficulty; DIFF base `[0x9C40C0]`, stride `0x7C`).
At the city tile `(x, y)` it creates, in this order, with `0x5694D0(P; proto, x, y, -1, -1, 0, 0, -1)`
(`barbarians.md` 3):

1. `D.+0x4C` (`defensive_land_units`) units of prototype `C.vt+0x20(0, 1)` (best defender, 5.2);
2. `D.+0x50` (`offensive_land_units`) units of `C.vt+0x1C()` (best attacker, 5.3);
3. `D.+0x54` (`start_unit_type_1`) units of prototype `[0x9C72CC]` (RULE `start_unit_1`, shipped PRTO `0`, Settlers);
4. `D.+0x58` (`start_unit_type_2`) units of `[0x9C72D0]` (RULE `start_unit_2`, shipped PRTO `1`, Workers).

Each loop re-reads `L` and the count from the table after every unit (the numbers never change inside the loop).
If a chooser returns `-1` the factory is still called with `-1` (**O**: its behaviour for `-1`).

Shipped DIFF rows (`conquests.biq`):

| level | `L` | defenders | attackers | start 1 | start 2 | `cost_factor` |
|---|---|---|---|---|---|---|
| Chieftain | 0 | 0 | 0 | 0 | 0 | 20 |
| Warlord | 1 | 0 | 0 | 0 | 0 | 12 |
| Regent | 2 | 0 | 0 | 0 | 0 | 10 |
| Monarch | 3 | 2 | 1 | 0 | 0 | 9 |
| Emperor | 4 | 4 | 2 | 0 | 1 | 8 |
| Demigod | 5 | 6 | 3 | 1 | 2 | 7 |
| Deity | 6 | 8 | 4 | 1 | 2 | 6 |
| Sid | 7 | 12 | 6 | 2 | 4 | 4 |

(The DIFF fields `additional_free_support` 0, 0, 0, 4, 8, 12, 16, 24 and `bonus_for_each_city` 0, 0, 0, 1, 2, 3, 4, 8
belong to the unit-support code, `economy.md`.) With these rows, 3.14's garrison applies from Monarch up (`L > 2`).

### 5.2 Best defender `City::vt+0x20 = 0x434D50(C; a1, a2)`, `ret 8`

Called by 3.13 and 3.14 as `(0, 1)`. Two passes over PRTO types `t = 0 .. [0x9C3DB0]-1` (row =
`[0x9C71E0] + t * 0x138`; `defense = row+0x58`, `ai = row+0x8C`).

```
qualifies(t):
    if a1 != 0 and not hasAbility(row, 9):                         return false     (0x5E4EF0, ability 9 = Draft)
    if not C.canBuildUnit(t, 1, 0, 0):                             return false     (0x4C04E0, buildable.md 3.1)
    if a1 != 0 and owner is human ((1 << owner.slot) & [0xA526BC]): return true
    if ai & 2 (Defense):                                           return true
    return a2 != 0 and (ai & 1) (Offense)
maxDef = max(0, defense(t))  over qualifying t
best = -1;  bestCost = 0x7FFFFFFF
for t in order:  if qualifies(t) and defense(t) == maxDef:
    c = 0x56A210(Player[C.owner]; t, 0)
    if c < bestCost:                                  best = t; bestCost = c
    else if a2 != 0 and c == bestCost and (ai & 2):   best = t                  tie: the later Defense-tagged type wins
return best
```

### 5.3 Best attacker `City::vt+0x1C = 0x434C40(C)`

```
qualifies(t) = C.canBuildUnit(t, 1, 0, 0) and (row+0x8C & 1)                       Offense strategy
maxAtk = max(0, row+0x60)  over qualifying t                                       attack strength
best = -1;  bestCost = 0x7FFFFFFF
for t in order: if qualifies(t) and row+0x60 == maxAtk:
    c = 0x56A210(Player[C.owner]; t, 0);  if c < bestCost: best = t, bestCost = c   (ties keep the lowest index)
return best
```

### 5.4 Unit cost `Player::unitCost(P; t, forceBase)` `0x56A210`, `ret 8`

```
base = PRTO[t].row+0x54                                   shield cost
f = 10                       if P.slot is in the human mask [0xA526BC] or forceBase != 0
  = DIFF[[0xA52684]].+0x68   otherwise                    the AI's cost_factor (percent of 10)
if [0xA5267C] & 0x200 (Accelerated Production):  f = (f - sign(f)) >> 1       f / 2 toward zero
if f < 1: f = 1
return (f * base) / 10                                    signed divide, truncation toward zero (0x66666667)
```

The choosers call it with `forceBase = 0`, so for an AI the comparison uses its difficulty-scaled cost.

## 6. Golden vectors

All computed from the algorithms above on the shipped `conquests.biq` names, empty world unless stated,
English build, `prefix = "New"` (**H**).

| id | input | output |
|---|---|---|
| N1 | Rome (44), counter 0, three cities in a row | `Rome` (counter 1), `Veii` (2), `Antium` (3) |
| N2 | Japan, counter 2 | `Tokyo`, counter 3 |
| N3 | Aztecs, counter 0, `Tenochtitlan` already a city | `Teotihuacan`, counter 2 |
| N4 | Rome, all 44 names in use, counter 0 | `New Rome`, counter 1; next call `New Veii`, counter 2 |
| N5 | Rome, counter `44 r`, nothing in use, `r = 0..6` | `Rome`, `New Rome`, `Rome 2`, `New Rome 2`, `Rome 3`, `New Rome 3`, `Rome 4` |
| N6 | Ottomans (29), counter 29, nothing in use | `Not Constantinople`, counter 1 (`(29 + 1) mod 29`) |
| N7 | Aztecs (36), counter 36, nothing in use | `Mingapulco`, counter 1 |
| N8 | Japan (26), counter 28, nothing in use | `Neo-Tokyo`, counter 3 |
| N9 | Japan, all 26 names in use, counter 2 | `New Kyoto`, counter 1 |

Note on N9: the walk collides on indices 2 to 25 (counters 2 to 25), reaches round 1 at counter 26 (index 0,
`Kyoto` prefixed) and that name is free, so the stored counter is `27 mod 26 = 1`; `Neo-Tokyo` (counter 28) is
never reached.

AI start bonus (5.1): a new Sid-level (`L = 7`) civ with no cities that founds its first city receives 12 best
defenders, 6 best attackers, 2 Settlers and 4 Workers; at Regent (`L = 2`) it receives none.

Unit cost (5.4): human, `base = 30`: 30; AI at Sid (`f = 4`): 12; AI at Chieftain (`f = 20`): 60; Accelerated
Production, human: `f = 5`, cost 15.

## 7. Open items and verification

Verified by reading in full: `0x5663C0`, `0x4AE2A0` (to its `ret 0x1C` at `0x4AECB2`), `0x565BD0`, `0x434C40`,
`0x434D50`, `0x56A210`, `0x437A70`, `0x5EA6E0`, `0x5E8CC0`, vtable targets `0x5EA980`, `0x5EAB20`, `0x5E9A00`,
`0x550760` (the language read), `0x5F3160`, `0x5B34C0`, `0x5B39D0`, `0x4761E0`, `0x4D9C80` (call structure only). Not decoded:

1. `0x4B0470(C; n, flag)`, `0x4B10F0`, `0x4B0E80` (read only as far as "zeroes `+0x1C8..+0x1D0`"), City vtable
   slots `+0x38` and `+0x58`, and `0x4BCEA0`, `0x4BCDE0`, `0x4BAAF0`, `0x426710`: the city recompute family and its
   initial tile assignment. The first work-tile choice of a new city is therefore **not** specified.
2. `0x55CB20` (capital registration) and `0x4ACF40` (add building) are now specified in `city-buildings.md` sections 4 and 6; `0x57E320` (list `0xB72888`), `0x578E90`,
   `0x58B5D0`.
3. `0x5D4830` (borders, `0x5D3AB0`), see `goody-huts.md` and `unit-turn.md` open items.
4. `0x5B5600` / `0x5B5790` (the border-intrusion hook and permission check: whether a civ may found a city on a
   tile owned by another civ is therefore decided there and in the diplomacy state, **O**), `0x4B1DC0`, the three
   display tests of `foundCity`, and the min-distance policy of the AI (`0x442480`).
5. The meaning of `cell.vt+0x78()` (bit 29 of `vt+0xAC`), the TERR predicate `0x5DBF10`, the exact English
   string behind `prefix`, and the push side of the city free list.
6. `0x5694D0` with prototype `-1` (a chooser that finds nothing).

Corrections to earlier documents made by this file: `goody-huts.md` 6.6 labelled the recompute at step 12 as
the new city's; it is the previous worker's. `goody-huts.md` 6.8 (draft of this section) read `+0x5C = 1` as a
citizen count; it is the border level.
