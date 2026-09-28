# Civilization III Conquests — how random map generation works

Consolidated reverse-engineering notes for
`civ3-gog/app/Conquests/Civ3Conquests.exe` (PE32, MSVC 6.0, static CRT, image base
`0x400000`, 3 417 464 bytes), plus the `rust/` reference implementation in this
directory.

This file supersedes the earlier `CIV3_MAP_GENERATION.md` and the seven
`NOTES_*.md` working files. Where they disagreed, the disagreement is recorded
rather than silently resolved, and the corrections that were made during the
investigation are listed in §1.2.

---

## Table of contents

1. [Scope, method, and what was corrected](#1-scope-method-and-what-was-corrected)
2. [Master address map](#2-master-address-map)
3. [The `Map` object](#3-the-map-object)
4. [The `Cell` object — vtable and field layout](#4-the-cell-object--vtable-and-field-layout)
5. [The two PRNGs](#5-the-two-prngs)
6. [The fractal terrain generator (`0x5e1b60`)](#6-the-fractal-terrain-generator-0x5e1b60)
7. [`percentileLookup` (`0x5e2280`)](#7-percentilelookup-0x5e2280)
8. [`spiralOffset` and the nominal start slot](#8-spiraloffset-and-the-nominal-start-slot)
9. [`generateLandmass` (`0x5eceb0`)](#9-generatelandmass-0x5eceb0)
10. [`generateMap` (`0x5eb580`) — the exact pipeline](#10-generatemap-0x5eb580--the-exact-pipeline)
11. [Stage-by-stage detail](#11-stage-by-stage-detail)
12. [`postProcess` and the "Fixed on Nth try" fix-up](#12-postprocess-and-the-fixed-on-nth-try-fix-up)
13. [The tile record and the save format](#13-the-tile-record-and-the-save-format)
14. [Rivers — the investigation and the result](#14-rivers--the-investigation-and-the-result)
15. [The `.biq` container and the PKWARE DCL codec](#15-the-biq-container-and-the-pkware-dcl-codec)
16. [The four opt-in original bugs](#16-the-four-opt-in-original-bugs)
17. [Reference implementation status](#17-reference-implementation-status)
18. [Open questions](#18-open-questions)

---

## 1. Scope, method, and what was corrected

### 1.1 Method

`radare2` for discovery, cross-references and raw disassembly; Ghidra 12.1.4
headless via `pyghidra` for decompilation (`work/decomp.py`).

Two method notes that materially affect the results:

* **r2's `arg_XXh` / `var_XXh` labels are unreliable.** They are raw `[esp+N]`
  displacements, and MSVC6's `mov eax,size; call __alloca_probe` idiom defeats r2's
  frame analysis. Every stack slot quoted in this document was re-derived by
  simulating `esp` through the prologue and cross-checked against the epilogue.
* **Where Ghidra and raw disassembly disagree, the raw disassembly wins.** Ghidra
  reconstructs argument order for `__thiscall` virtual calls backwards in several
  places. Argument order for `vfunc(0x48)` in `0x5f21b0` was re-derived by hand
  from the push order and the two arguments are `(x, y)`, not `(y, x)`.

### 1.2 Corrections made during the investigation

These were wrong at some point and are now settled. They are listed because
several were load-bearing.

| claim | status |
|---|---|
| `map->vfunc(0x34)(i)` returns a per-civ object | **Wrong.** It returns a **`Cell*`** — `getCell(i)` indexed by the linear cell index. This is why `0x5eceb0`, which does nothing but call `vfunc(0x34)(i)` then `vfunc(0x128)`, is the land/sea stage. |
| `vfunc(0x8c)` is "is a start candidate" | **Wrong.** It is **"is water"**: `terrainClass >= 11 && terrainClass <= 13`. |
| `0x5f07d0` is the river generator | **Wrong.** It is the **hills / mountains** pass. Its chain endpoint writes BIQ terrain class **6**, and `vfunc(0x40)` is `isHillsOrMountains()`. |
| `0x5f1480` is resource placement | **Wrong.** It is the **biome / climate** stage. Resources are `0x5f22a0`. |
| `0x5f22a0` places goody huts too | **Wrong.** Goody huts are placed by `0x5f21b0` and barbarian camps by `0x5f2090`. |
| `0x5e1b60` reads out of bounds | **Wrong.** An exhaustive scan shows the flat index never leaves `0..=8384`; the height array is exactly `129*65 = 8385` bytes. |
| `h == p70` should be `h <= p70` | **Wrong.** `==` marks 0.0–0.5 % of land; `<=` would mark 52–61 %, i.e. most of the map as hills. Both readings are bad, so `==` is the default. |
| Sea level is a constant, or the water-level slider | **Wrong.** Both thresholds are **percentiles of the generated fractal**; the slider is only ever a *seed*. |
| Height grid is y-major | **Wrong.** It is **x-major**: `h[x*65 + y]`, stride on `x`. |

---

## 2. Master address map

### 2.1 Entry points and stages

| address | recovered name | role |
|---|---|---|
| `0x5d16f0` | `Map::generate(seed, isRandom, waterLevel)` | top-level entry |
| `0x5eb580` | `Map::generateMap()` | **the 12-stage pipeline** |
| `0x5f1f50` | `rollRandomOptions()` | the "Randomize" button |
| `0x5eceb0` | `generateLandmass()` | fractal landmass + start scoring + **retry loop** |
| `0x5ed440` | `landmassFix()` | only when Landmass == 1 |
| `0x5eeb00` | `deconflictStarts()` | two start-tile repair passes |
| `0x5edb70` | `convertDesertsAtStarts()` | desert → playable terrain |
| `0x5eddb0` | `paintContinents()` | paints the half-resolution region map |
| `0x5ee470` | `paintRegionId(x, y, id)` | the worker behind the above |
| `0x5f1480` | `assignBiomes()` | **terrain / climate placement** |
| `0x5f1ce0` | `writeBiomeClass(x, y, cls, bonus[])` | the class writer called by `0x5f1480` |
| `0x5ed5d0` | `assignStartsPerContinent()` | per-continent start assignment |
| `0x5f07d0` | `growHillsAndMountains()` | helpers `0x5f00c0`, `0x5f0530`, `0x5f1240` |
| `0x5f22a0` | `placeResources(1)` | `'GOOD'` × `'TERR'` |
| `0x5f21b0` | `placeGoodyHuts(1)` | feature 0, qty `0x20` |
| `0x5f2090` | `placeBarbarianCamps(1)` | feature 2, qty `0x10000` |
| `0x5eeee0` | `finalPass(...)` | civ list + start-tile content; helper `0x5eedb0` |
| `0x5d3100` | contour smoothing | after `generateMap` |
| `0x5d6500` | `placeStartLocation(x, y)` (2 933 B) | the actual site commit |
| `0x5ebe80` | `Map::postProcess()` → `0x5ebf30`, `0x5ec2d0` | the "Fixed on Nth try" fix-ups |

### 2.2 Supporting primitives

| address | meaning |
|---|---|
| `0x60ba80` | `double rand01(uint32 *s)` — the map generator's LCG |
| `0x60bab0` | `int rand_int(uint32 n)` = `(int)(n * rand01())` |
| `0x64a20e` | `rand()` — the *game* RNG, unused by map generation |
| `0x5e1b60` | fractal (midpoint displacement) |
| `0x5e1fd0` | fractal post-pass (directional smear) |
| `0x5e2180` | `sampleHeight(fm, x, y)` |
| `0x5e2280` | `percentileLookup(fm, pct)` |
| `0x5e6e50` | `spiralOffset(n, &dx, &dy)` |
| `0x5e6d20` | inverse of the above |
| `0x5d16a0` | `getCell(linearIndex)`, bounds-checked |
| `0x5dbe70` | `getTerrainRecord(playerIdx)` — 240-byte record, stride `0xF0` |
| `0x5eedb0` | best-ranked neighbour id |
| `0x5ea9f0` | `tile->vfunc(0xa0)() != -1` — "tile already has X" |
| `0x599600` | the 4-char-code dispatcher: `getListEntry('GOOD'\|'TERR', idx, &out)` |
| `0x594290` | scenario (BIC/BIX/BIQ) reader |
| `0x426710` | `clamp(v, lo, hi)` |
| `0x649a8b` / `0x649a80` | `malloc` / `free` |
| `0x64a800` | `__alloca_probe` / `_chkstk` |
| `0x67055c` | `{+1, +1, -1, -1}` — diagonal x offsets |
| `0x67056c` | `{-1, +1, +1, -1}` — diagonal y offsets |
| `0x6701c8` | the `Cell` vtable (78 slots) |

### 2.3 Data tables

| address | contents |
|---|---|
| `0x9c74b4` | the cell array pointer |
| `0x9c73ac` | cell count (`uint16`) |
| `0x9c74d4` | map width in cells (`int`) |
| `0x9c7328` | base of the 240-byte per-player record table |
| `0x728d8c` | `TERR_River`, `TERR_Grassland_with_Shield`, `TERR_Fresh_Water_Lake` (civilopedia keys) |
| `0x6699d8` | `Art\Terrain\deltaRivers.pcx` |
| `0x669adc` | `Art\Terrain\mtnRivers.pcx` |
| `0x680d6c` | `RiverBack.pcx` |
| `0x680db8` | `NOTtheRiver.pcx` |
| `0x680ddc` | `RiverFore.pcx` |

---

## 3. The `Map` object

`Map *this` in `ecx`.

```
+0x00  vptr
+0x04  option A  (0..3)     +0x08  derived, 0..2
+0x0c  option B  (0..4)     +0x10  derived, −2..3   <- goody-hut count
+0x14  option C  (0..3)     +0x18  derived, 0..2    <- "Landmass"
+0x1c  option D  (0..3)     +0x20  derived, 0..4    <- "Map Size"
+0x24  option E  (0..3)     +0x28  derived, 0..2    <- "Resources"
+0x2c  option F  (0..3)     +0x30  derived, 0..2    <- climate
+0x38  scratch: uint8  per-continent census (freed in the pipeline)
+0x3c  scratch: uint16 per-cell region id (freed in the pipeline)
+0x40  uint16 numPlayers
+0x148 Player* players[]     (numPlayers entries, 0xDC bytes each)
+0x154 int  H
+0x158 int  a radius (halved when level >= 3)
+0x15c int  a count/limit
+0x168 int  W
+0x16c int  civIndex[]       (built by 0x5eeee0)
+0x170 int  civIndex[]       (location seeds)
+0x1ec int  waterLevel  (0..100) — the "Oceans" slider
+0x1f0 int  wrapFlags   bit0 = wrap X, bit1 = wrap Y, bit2/3 used by the fractal
+0x214 Continent continents[]  (stride 0x28; +0x20 "in use", +0x24 tile count)
```

### 3.1 Map virtual methods

| slot | impl | meaning |
|---|---|---|
| `0x00` | `0x5f3cc0` | `setNumPlayers(n)` |
| `0x04` | `0x5f3e00` | `destroyPlayers()` |
| `0x08` | — | `intAt(x, y)` — opaque, used by `0x5eeee0` |
| `0x0c` | `0x5d16e0` | `invalidate()` — called between every stage |
| `0x30` | `0x5dc1c0` | `getTile(x, y)` |
| `0x34` | `0x5dc1b0` | `getCell(linearIndex)` |
| `0x44` | — | `mayPlaceResource(x, y, resId, flag)` → bool |
| `0x48` | — | `isSuitableBonusSite(x, y)` → bool |
| `0x4c` | `0x5d8ab0` | `createPlayers()` |
| `0x60` | — | start-tile validity predicate |
| `0x70` | `0x5f3f50` | wrap-aware direction normalisation |
| `0x78` | `0x5eb440` | `applyOptionToAllPlayers(idx, now)` |
| `0x7c` | `0x5eb7d0` | `finalizeMap()` — flood-fills and **numbers the continents** |
| `0x80` | `0x5ebe80` | `postProcess()` |
| `0x84` | `0x437a50` | `getContinent(i)` = `*(int*)(this+0x214) + i*0x28` |
| `0x88` | `0x5dc360` | `getNumContinents()` |
| `0x8c` | `0x5d90d0` → `0x599600` | `getListEntry(tag, idx, &out)`; `idx = -1` returns the count |

**Note on vtable slot reuse:** `vfunc(0x8c)` on the `Map` is the 4-char-code
lookup, while `vfunc(0x8c)` on a `Cell` is "is water". Different classes, same
slot number — an easy trap.

### 3.2 Cell virtual methods

See §4 for the full table and the field layout.

| slot | meaning |
|---|---|
| `0x00` | `return dword[cell+0x2c]` — the terrain class word |
| `0x04` | `setTerrainClass(T)` — normalises into `cell+0x2c` |
| `0x08` | `setWaterClass(T)` — clamps `cell+0x2c` bits 8..11 to 11..13 |
| `0x34` | returns a `Cell*` |
| `0x40` | `isHillsOrMountains()` |
| `0x44` / `0x48` | raw/secondary class accessors |
| `0x8c` | `return class >= 11 && class <= 13` — **is water** |
| `0x94` / `0x9c` | feature id, `dword[cell+8]`; `-1` = none |
| `0xa0` | a second "has X" id |
| `0xb8` | `word[cell+0x1e]` — **continent / region id** |
| `0xc4` | `(flags(cell+0x2c) >> 8) & 0xF` — secondary class |
| `0xc8` | `(flags(cell+0x2c) >> 12) & 0xF` — **BIQ terrain id** |
| `0xe0` | `setFlag(plane, mask)` |
| `0xcc` | `clearFlag(plane, mask)` |
| `0xe8` | `setContinentId(id)` |
| `0xf4` | `byte[cell+4] |= mask` |
| `0xd0` | `byte[cell+4] = value` |
| `0x128` | `setTerrainPreference(T)` — writes `T` into `cell+0x2c` bits 12..15 |
| `0x12c` | `setDifficulty(n)` |
| `0x11c` | resolves an override through the `'TERR'` list |

`vfunc(0x128)(-1,-1,T)` is the single most important tile write in the generator:
it sets the tile's BIQ terrain id. The values used across the pipeline are
`{4, 5, 6, 7, 10, 11, 12}`.

`0x5dbe70(player)` reads `biqTerrain[terrainID].field_0x4c` out of the `'TERR'`
list — the movement-cost / starting-defence figure of that terrain. It is what
start-site selection minimises, via `map->vfunc(0x84)(id)->[0x24]`.

---

## 4. The `Cell` object — vtable and field layout

Vtable at `0x6701c8`, **78 slots** (`0x00`–`0x134`). Built mechanically by
disassembling each entry and extracting the field it touches.

| slot | impl | field | meaning |
|---|---|---|---|
| `0x00` | `0x5ea4e0` | `dword[+0x2c]` | terrain class word |
| `0x04` | `0x5ea4f0` | | set terrain class |
| `0x08` | `0x5ea540` | | set water class |
| `0x0c` | `0x5ea5b0` | | |
| `0x10` | `0x5d9b10` | | **the Cell constructor** (stores vtable `0x6701c8`) |
| `0x14` | `0x5ea410` | `byte[+0x20]`, `dword[+0x2c]` | load-time water-depth normalisation |
| `0x18` | `0x5ea610` | `dword[+0xa8]` | |
| `0x1c` | `0x5ea630` | `dword[+0xa8]` | |
| `0x20` | `0x5ea650` | | |
| `0x24` | `0x5ea660` | `byte[+0x20]` | |
| `0x28` | `0x5ea680` | `byte[+0x20]` | |
| `0x2c` | `0x5ea6a0` | `dword[+0xb4]` | |
| `0x30` | `0x5ea730` | | |
| `0x34` | `0x5ea760` | `dword[+0xa8]` | returns a `Cell*` |
| `0x38` | `0x5ea780` | `dword[+0xa8]` | |
| `0x3c` | `0x5ea7a0` | `dword[+0xa8]` | |
| `0x40` | `0x5ea7c0` | | `isHillsOrMountains()` |
| `0x44` | `0x5ea7f0` | `dword[+0xa8]` | |
| `0x48` | `0x5ea810` | `dword[+0xa8]` | |
| `0x4c` | `0x5ea830` | `dword[+0xc]`, `dword[+0x50]` | |
| `0x50` | `0x5ea880` | `dword[+0xa8]` | |
| `0x54` | `0x5ea8a0` | | |
| `0x58` | `0x5ea850` | | |
| `0x5c` | `0x5da0d0` | | |
| `0x60` | `0x5ea8e0` | | start-tile validity predicate |
| `0x64` | `0x5d9ff0` | | |
| `0x68` | `0x5ea910` | | |
| `0x6c` | `0x5ea930` | | |
| `0x70` | `0x5ea940` | | |
| `0x74` | `0x5ea950` | | |
| `0x78` | `0x5ea980` | | |
| `0x7c` | `0x5ea990` | `byte[+4]` | |
| `0x80` | `0x5ea9c0` | | "start tile empty" test 1 |
| `0x84` | `0x5ea9d0` | `dword[+0xa8]`, `dword[+0xa0]` | |
| `0x88` | `0x5eaa10` | | |
| `0x8c` | `0x5eaa30` | | **is water** (class 11..13) |
| `0x90` | `0x5eaa60` | `dword[+0x24]` | |
| `0x94` | `0x5eaa70` | **`byte[+4]`** | connection mask getter |
| `0x98` | `0x5eaa80` | | |
| `0x9c` | `0x405d00` | **`dword[+8]`** | feature id, `-1` = none |
| `0xa0` | `0x5eaa90` | `dword[+0xc]` | second "has X" id |
| `0xa4` | `0x5eaaa0` | | |
| `0xa8` | `0x5dc300` | | |
| `0xac` | `0x5eaac0` | `dword[+0x30]` | |
| `0xb0` | `0x5eaad0` | `word[+0x18]` | |
| `0xb4` | `0x5eaae0` | `word[+0x1a]` | |
| `0xb8` | `0x5eaaf0` | `word[+0x1e]` | continent / region id |
| `0xbc` | `0x5eab00` | `word[+0x1c]` | |
| `0xc0` | `0x5eab10` | `word[+0x22]` | |
| `0xc4` | `0x5eab20` | | secondary class |
| `0xc8` | `0x5eab30` | | BIQ terrain id |
| `0xcc` | `0x5da3e0` | | clear flag |
| `0xd0` | `0x5eab70` | **`byte[+4]`** | connection mask setter (value) |
| `0xd4` | `0x5eab90` | | |
| `0xd8` | `0x5eabd0` | `dword[+0x10]` | |
| `0xdc` | `0x5eabe0` | | |
| `0xe0` | `0x5da2a0` | | set flag |
| `0xe4` | `0x5eac80` | `word[+0x1a]` | |
| `0xe8` | `0x5eaca0` | | set continent id |
| `0xec` | `0x5da1f0` | | set feature id |
| `0xf0` | `0x5eacb0` | `dword[+0x24]` | |
| `0xf4` | `0x5eacc0` | **`byte[+4]`** | connection mask setter (OR) |
| `0xf8` | `0x5eacf0` | | |
| `0xfc` | `0x5dc170` | `dword[+0x34]` | |
| `0x100` | `0x5dc180` | `dword[+0x34]` | |
| `0x104` | `0x5ead10` | `dword[+0xc]` | |
| `0x108` | `0x5ead20` | | |
| `0x10c` | `0x5ead30` | | |
| `0x110` | `0x5e9ad0` | | |
| `0x114` | `0x5d9e90` | | |
| `0x118` | `0x5d9ef0` | | |
| `0x11c` | `0x5e9a00` | | resolve override via `'TERR'` |
| `0x120` | `0x5e9a60` | | |
| `0x124` | `0x5e9c90` | | |
| `0x128` | `0x5d9d90` | | **set terrain preference** |
| `0x12c` | `0x5dc280` | | set difficulty |
| `0x130` | `0x5e9e10` | | |
| `0x134` | `0x5dc260` | | |

### 4.1 The fields

```
+0x00  vptr                       +0x04  uint8  connection mask
+0x05  uint8  (no vtable accessor; touched only by direct code)
+0x08  int32  feature id           +0x0c  int32  second feature id
+0x10  int32                       +0x14  int32
+0x18  uint16                      +0x1a  uint16
+0x1c  uint16                      +0x1e  uint16   continent / region id
+0x20  uint8                       +0x22  uint16
+0x24  int32                       +0x28  int32
+0x2c  int32  terrain class word   +0x30  int32
+0x34  int32
```

Two things worth noting:

* **`byte[cell+4]` is the only per-tile connection field in the struct.** It has
  both a getter (`0x94`) and two setters (`0xd0` value, `0xf4` OR). The hills pass
  `0x5f07d0` ORs bits into it and `0x5f0450` clears it.
* **`byte[cell+5]` has no vtable accessor at all.** It is only ever reached by
  direct (non-virtual) code. Any per-tile flag living there cannot be read through
  the published interface.

The Cell constructor (`0x5d9b10` → `0x5da7d0`) calls `FUN_005e9940(0xd)`, so a
freshly constructed cell is **class 13 (abyssal)**. That is load-bearing for
coast carving: in `0x5eceb0` a cell that fails the coastline test is simply left
untouched and therefore stays a water class.

### 4.2 The cell grid is `(W/2) x H`

Two map columns share one cell:

```c
cellIndex = (W >> 1) * y + (x >> 1)
```

and the inverse, used everywhere in the generator:

```c
y = i / (W >> 1);
x = 2 * (i % (W >> 1)) + (y & 1);
```

This is the *same* arithmetic as the nominal start slot (§8), which is why a civ's
nominal slot and a cell's linear index are interchangeable.

---

## 5. The two PRNGs

There are two, and they are completely separate.

**Game RNG** — `0x64a20e`, state in `this+0x14`, seeded from `timeGetTime()` in
`main` (`0x56c1f9`) and again at game start (`0x6389dd`). Used for AI and combat.
**The map generator does not use it.**

**Map RNG** — `0x60ba80` / `0x60bab0`:

```c
double rand01(uint32 *s) {                       // 0x60ba80
    *s = *s * 1103515245u + 12345u;
    return ((*s >> 16) & 0x7FFF) * (1.0/32768.0);
}
int rand_int(uint32 n) { return (int)(n * rand01(s)); }   // 0x60bab0
```

The classic ANSI-C LCG, but only the **high 16 bits** of the state are used for
the output. The state is always a *local* seeded from `waterLevel + K`, so every
stage is independently reproducible from the map seed.

| K | used by |
|---|---|
| `0xCC98` (52 376) | `0x5f1f50`, `0x5eceb0` first fractal warm-up |
| `0x71 * n` (113 per retry) | `0x5eceb0` landmass fractal seed |
| `0x3039` (12 345) | `0x5eceb0` second fractal (Landmass == 1) |
| `0x1D9D3 + 101*level` | `0x5ec2d0` "Fixed on Nth try" |
| `0x9A2112` | `0x5edb70` desert-conversion fractal |
| `0xD431` (54 321) | `0x5edb70`, `0x5eddb0` shuffle |
| `0x7C0` (1 984) | `0x5ed5d0` start-assignment shuffle |
| `0xF0FF3` | `0x5f1480` biome stage RNG |
| `0x1E1735` / `0x34B59E` | `0x5f1480` the two biome fractals |
| `0x87B01` | `0x5f07d0` hills stage RNG |
| `0x180E3` | `0x5f22a0` resources |
| `0x8ACE` | `0x5f21b0` goody huts |
| `0x8CF78` | `0x5f2090` barbarian camps |
| `0x16062` | `0x5eeee0` final pass |
| `29 * continentId` | `0x5ee470` region painting |

Stages that shuffle use **Fisher–Yates** with `rand_int(n - i)` and a 3-XOR swap:
`0x5eddb0` (16-bit permutation), `0x5ed5d0` (32-bit), `0x5f1480`, `0x5f22a0`,
`0x5f2090`, `0x5eeee0`.

---

## 6. The fractal terrain generator (`0x5e1b60`)

Midpoint displacement into a **129 × 65 byte heightmap**.

```c
struct FractalMap {          // 0x20E4 bytes
    int    W, H;             // +0x00, +0x04
    int    flags;            // +0x08  bit0 wrapX, bit1 force-wrap row,
                             //       bit2 scale-by-100, bit3 ocean poles
    uint32 seed;             // +0x0C
    double sx, sy;           // +0x10 = 128.0/W , +0x18 = 64.0/H
    uint8  h[129][65];       // +0x20, x-major: h[x*65 + y]
    uint8  wrapCol[65];      // +0x20A0, saved copy of x = 0
};

void fractal(FractalMap *fm, int W, int H, int level, int flags,
             uint32 seed, FractalMap *out)
{
    fm->W = W; fm->H = H; fm->flags = flags;
    fm->seed = seed ? seed : timeGetTime();
    fm->sx = 128.0 / W;  fm->sy = 64.0 / H;
    int L0 = clamp(6 - level, 0, 6);          // the "level" arg is INVERTED
    rand01(&fm->seed);                        // one warm-up draw

    for (int L = L0; L >= 0; L--) {
        int mask = (1 << (L+1)) - 1;
        if (flags & 8)  for (i=0;i<129;i++) h[i][0] = h[i][64] = 0;
        else if (flags & 2) for (i=0;i<129;i++) h[i][64] = h[i][0];
        if (flags & 1) memcpy(wrapCol, h[0], 65);

        for (int y = 0; y < (64 >> L) + ((~flags & 1) ? 1 : 0); y++)
          for (int x = 0; x < (128 >> L) + ((~flags>>1 & 1) ? 1 : 0); x++) {
              int fx = x << L, fy = y << L;
              if (L == L0)                            h[fy][fx] = rand_int(256);
              else if (!(mask & fy))  { if (mask & fx) h[fy][fx] = avg4(fx,fy); }
              else if (!(mask & fx))                  h[fy][fx] = avg2h(fx,fy);
              else                                    h[fy][fx] = avg2v(fx,fy);
          }
    }
    if (out) smearPass(fm, out);              // 0x5e1fd0
}
```

`avg*` are `(sum >> 1)` / `(sum >> 2)` followed by

```
value = clamp(avg + (rand_int(256) - 128), 0, 255)
```

**The jitter amplitude does not fall off per level.** The compiler's expression
is `rand_int(1 << ((7-L) + L + 1)) - (1 << ((7-L) + L))`, and the `±L` terms cancel
to the constant `rand_int(256) - 128`. The smoothing that turns white noise into
continents comes from (a) the `flags & 8` ocean poles, (b) `0x5e1fd0`'s smear pass,
and (c) `percentileLookup` thresholding downstream.

### 6.1 Grid orientation and sampling

* index = `x*65 + y` — **x-major**, stride on `x`. (Verified: the flat index never
  leaves `0..=8384`, and `129*65 = 8385` exactly, so there is no overrun.)
* `sampleHeight(fm, x, y)` = `h[(int)(x*128.0/W)][(int)(y*64.0/H)]`, clamped to
  `[0,255]`, and scaled to `0..100` if `flags & 4`.
* The fractal is generated in a **normalised 128 × 64 space** and stretched over the
  real `W × H` grid. A 60×60 and a 160×160 map therefore get structurally
  identical coastlines, only stretched — which is exactly why Civ3 maps of
  different sizes "feel" the same.

`0x5e1fd0` (only invoked when `out` is non-null) walks 65 columns × 16 rows,
reading a direction byte at `out+0x1880` and writing `h[i][j] = h[i][j] * ramp / 16`
along two wrapped diagonals — a directional smear that bends coastlines.

---

## 7. `percentileLookup` (`0x5e2280`)

```c
uint8 percentileLookup(FractalMap *fm, int percent)   // ecx = &fm->h
{
    percent = clamp(percent, 0, 100);
    if (percent == 100) return 255;
    uint8 lo = 0, hi = 255, mid = (percent*255)/100;
    while (mid != lo) {
        int below = 0;
        for (int r = 0; r < 64; r++)
            for (int c = 0; c < 128; c++)
                if (fm->h[c*65 + r] < mid) below++;
        if (below * 25 / 2048 < percent) hi = mid;   // below maxes at 64*128 = 8192
        else                             lo = mid;
        mid = (lo + hi) / 2;
    }
    return mid;
}
```

A binary search for **the elevation value such that exactly `percent` % of the map
is below it**. Every elevation band in the generator derives from this, so all
thresholds adapt automatically to the water level and to the random fractal.

This is the single most important structural fact about the generator: **there are
no absolute elevation constants.** The sea level is a percentile of the fractal
that was just generated.

---

## 8. `spiralOffset` and the nominal start slot

`spiralOffset(n, &dx, &dy)` (`0x5e6e50`) enumerates neighbours in expanding square
rings:

```
n = 0            -> ( 0,  0)
n = 1..8         -> ring 1, walking (1,-1)(1,0)(1,1)(0,1)(-1,1)(-1,0)(-1,-1)(0,-1)
n = 9..24        -> ring 2
n = 25..48       -> ring 3        ring r ends at (2r+1)^2 - 1
```

Used for every neighbourhood scan: radius 1 in `0x5eeb00`, up to `(2r+5)²` in
`0x5ed440`, 9 in `0x5ed5d0`, 25 in `0x5f1480`, 121 in `0x5f07d0`.

Every stage derives a cell's nominal position from its index with the same
arithmetic (§4.2), which walks the map in 2-wide interleaved columns — a
checkerboard that keeps initial guesses maximally spread out. Only the final
`0x5d6500` commits positions.

Wrap handling is a single-step add/subtract, valid because the offsets are small:

```c
if (wrapFlags & 1) { if (nx < 0) nx += W; else if (nx >= W) nx -= W; }
if (wrapFlags & 2) { if (ny < 0) ny += H; else if (ny >= H) ny -= H; }
```

---

## 9. `generateLandmass` (`0x5eceb0`)

1 413 bytes. Three `FractalMap`s on the stack in a `0x62F8`-byte frame. `fmA` is
the only one ever sampled.

### 9.1 Thresholds by map size

```c
switch (mapSize) {                 // this->[0x20]
  case 0: PL_HI = 82; PL_LO = 67; break;    // Tiny   60x60
  case 1: PL_HI = 72; PL_LO = 57; break;    // Small  80x80
  case 4: PL_HI = 42; PL_LO = 27; break;    // Huge   160x160
  default: PL_HI = 62; PL_LO = 47; break;   // Medium / Large
}
int ff = (wrapFlags & 2) ? 3 : 9;
if (landmass == 1) ff |= 0x10;
```

### 9.2 Fractal generation per landmass style

| `landmass` | calls `(fm, W, H, level, flags, seed, out)` |
|---|---|
| 0 | `fmA, W, H, **3**, ff, ~(wl+113n), 0` |
| 1 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC`<br>`fmD, W, H, **3**, ff, wl+113n, &fmC`<br>blend: `fmA[i] = (fmA[i] + fmD[i]) >> 1` |
| 2 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC` |

### 9.3 Retry loop

```
p96 = percentileLookup(fmA, 96)
pHi = percentileLookup(fmA, PL_HI)
pLo = percentileLookup(fmA, PL_LO)

reject if  113*n < 1130  and  70 <= pHi <= 130
reject if  113*n < 1695  and  80 <= pHi <= 120
   → seedOff += 0x71, draw a fresh fractal; give up after 10 outer attempts
```

i.e. it wants a **cleanly bimodal** land/sea split and rejects fractals whose
sea-level contour sits near the middle of the 0..255 range.

### 9.4 The per-cell decision — this is the height→land/sea step

```c
for (i = 0; i < numCells; i++) {
    y = i / (W>>1);  x = 2*(i % (W>>1)) + (y & 1);
    h = sampleHeight(fmA, x, y);
    if (h > p96 && rand_int(50) == 0) {         // top 4 %: 2 % become lakes
        getCell(i)->vfunc(0x128)(12, -1, -1);
        continue;
    }
    if (h > pHi) {
        for (n = 1; n < 9; n++) {              // ring-1 spiral
            (dx,dy) = spiralOffset(n, &dx, &dy);
            nx = wrapX(x+dx); ny = wrapY(y+dy);
            if (in bounds && sampleHeight(fmA, nx, ny) > pHi) {
                getCell(i)->vfunc(0x128)(2, -1, -1);   // land
                goto next;
            }
        }
        goto next;      // nothing written: keeps the constructor default (water)
    } else {
        getCell(i)->vfunc(0x128)(13, -1, -1);   // deep water
    }
next: ;
}
```

So the classification is:

| condition | result |
|---|---|
| `h <= pLo` | class **13** (deep ocean) |
| `pLo < h <= pHi` | class **12** |
| `h > pHi`, and ≥1 ring-1 neighbour also `> pHi` | class **2** (land) |
| `h > pHi` but no neighbour `> pHi` | **untouched** → stays class 13 |
| `h > p96` and `rand_int(50) == 0` | class **12** (inland lake) |

The ring-1 test is what carves out the 1-cell-wide coast that a plain threshold
would leave behind. The `h3 > pLo ? 12 : 13` split seen in the disassembly is
**dead in the 12 direction**, because that branch is only reached when `h <= pHi`
and `pHi > pLo` always holds; the class-12 path is reachable only via the p96 lake
rule. Reported as observed.

### 9.5 Start-slot scoring and the continent balance test

```c
for (civ = 0; civ < numPlayers; civ++) {
    y = civ/(W>>1);  x = 2*(civ%(W>>1)) + (y&1);
    h = sampleHeight(fmA, x, y);
    if (h > p96 && rand_int(50) == 0)      getCell(civ)->vfunc(0x128)(12,-1,-1);
    else if (sampleHeight(&fmG, x, y) > pHi) {
        for (r = 1; r < 9; r++) {
            spiralOffset(r, &dx, &dy);
            nx = wrapX(x+dx); ny = wrapY(y+dy);
            if (in bounds && sampleHeight(&fmG, nx, ny) > pLo) {
                getCell(civ)->vfunc(0x128)(sampleHeight(&fmG,nx,ny) > pLo ? 12 : 13, -1, -1);
                break;
            }
        }
    }
}

this->finalizeMap();                      // 0x5eb7d0: flood fill, number continents

n = getNumContinents();
switch (landmass) {
  case 0:  if (n > 2 && cont[2].used && cont[0].size < 2*cont[2].size)   return; break;
  case 1:  if (n > 2 && cont[1].used && cont[0].size < 1.5*cont[1].size &&
               (cont[2].used == 0 || cont[0].size > 4*cont[2].size))   return; break;
  default: if (n > 1 && cont[1].used && cont[0].size > 8*cont[1].size)   return; break;
}
if (iteration > 9) return;
```

The continent-size test is the whole point of the retry loop: re-roll the fractal
up to 10 times until the landmasses match the requested style — balanced
similar-sized continents for style 0, mid-sized for style 1, a dominant
supercontinent for style 2.

`finalizeMap` (`0x5eb7d0`) resets every continent id to `0xFFFF`, then repeatedly
takes an unassigned player, allocates a fresh id, flood-fills from its cell
(`0x5e0f30` helper) and records the id.

### 9.6 How `waterLevel` enters

**It is never a threshold.** `map->[0x1EC]` is read exactly twice in `0x5eceb0`,
both as seed derivations: `rng = waterLevel + 0xCC98` and
`fractalSeed = waterLevel + 113*n`. The slider chooses *which* fractal is
generated; because the sea level is then re-derived from that fractal's own
percentiles, the effect on land fraction is indirect and non-monotone. Nudging it
can produce *more* ocean, not less. This is original behaviour, not a bug, and it
is the opposite of what the UI implies.

---

## 10. `generateMap` (`0x5eb580`) — the exact pipeline

577 bytes. Twelve stages. The only branch is the Landmass == 1 guard on stage 3.
`vfunc(0x0c)` is `invalidate()`, called between every stage.

```c
void generateMap(Map *this, int ret)
{
    this->vfunc(0x78)(0xd, 0);            // applyOptionToAllPlayers(13, 0)
    this->[0x1EC] = waterLevel;           // from the caller

    rollRandomOptions();                  //  1  0x5f1f50
    invalidate();
    generateLandmass();                   //  2  0x5eceb0
    if (this->[0x18] == 1) landmassFix(); //  3  0x5ed440
    invalidate();
    deconflictStarts();                   //  4  0x5eeb00
    invalidate();
    convertDesertsAtStarts();             //  5  0x5edb70
    invalidate();

    if (getNumContinents() != 0) {
        this->[0x38] = new uint8 [getNumContinents()];  memset 0
        this->[0x3C] = new uint16[numPlayers];           memset 0xFFFF
    }
    invalidate();
    paintContinents();                    //  6  0x5eddb0
    invalidate();
    assignBiomes();                       //  7  0x5f1480
    invalidate();
    free(this->[0x38]); this->[0x38] = 0;
    free(this->[0x3C]); this->[0x3C] = 0;
    invalidate();
    assignStartsPerContinent();           //  8  0x5ed5d0
    invalidate();
    growHillsAndMountains();              //  9  0x5f07d0
    invalidate();

    for (i = 0; i < numPlayers; i++) {            // tidy up start tiles
        Cell *c = this->vfunc(0x34)(i);
        if (c->vfunc(0x60)() && c->vfunc(0xC8)() == 0)
            c->vfunc(0x128)(4, -1, -1);
    }
    invalidate();
    this->postProcess();                   //      vfunc 0x80
    for (i = 0; i < numPlayers; i++)
        this->vfunc(0x34)(i)->vfunc(0xCC)(2, 0x400000, -1, -1);
    invalidate();

    placeResources(1);                    // 10  0x5f22a0
    invalidate();
    placeGoodyHuts(1);                    // 11  0x5f21b0
    invalidate();
    placeBarbarianCamps(1);                // 12  0x5f2090
    invalidate();
    finalPass(1, 0, (ret == 0) * 2 - 1, ret == 0, &ret);   // 0x5eeee0
    invalidate();
}
```

`Map::generate` (`0x5d16f0`) then calls `0x5d3100` (contour smoothing),
`invalidate()`, and finally walks the players calling `placeStartLocation(x, y)`
(`0x5d6500`, 2 933 bytes) with the same slot arithmetic, followed by
`getNumContinents()` being pushed to `0x538750`.

**The three stages that take a literal `1`** — `0x5f22a0`, `0x5f21b0`, `0x5f2090`
— are the only three "place a discrete object on a tile" passes: resources, goody
huts, barbarian camps. That is now confirmed rather than assumed: the other nine
stages produce terrain, biomes, continents or start positions.

---

## 11. Stage-by-stage detail

### 11.1 `rollRandomOptions` (`0x5f1f50`)

Three discarded `rand01()` draws, then six options are resolved. A slider value of
`3` — or `4`, for the last — means "Random":

| source | resolution | stored at | meaning |
|---|---|---|---|
| `[0x1C]` | `(==3 ? rand_int(3) : as-is)` → `FUN_00426710(id, 0, 4)` | `[0x20]` | Map Size |
| `[0x04]` | `(==3 ? rand_int(3) : as-is)` → `FUN_00426710(id, 0, 2)` | `[0x08]` | |
| `[0x2C]` | `(==3 ? rand_int(3) : as-is)` → `FUN_00426710(id, 0, 2)` | `[0x30]` | climate |
| `[0x14]` | `(==3 ? rand_int(3) : as-is)` → `clamp(0, 2)` | `[0x18]` | Landmass |
| `[0x24]` | `(==3 ? rand_int(3) : as-is)` → `clamp(0, 2)` | `[0x28]` | Resources |
| `[0x0C]` | `(==4 ? rand_int(5) - 1 : as-is)` → `clamp(-2, 3)` | `[0x10]` | **goody-hut count** |

The first three are **not** clamps — they look the chosen id up in a `.biq` table
(`FUN_00426710`), which is why each passes an explicit pool size. Only the last
three clamp.

`this->[0x10]` is the goody-hut count and stages 11 and 12 size their work off
it. `-1` means "none".

### 11.2 `deconflictStarts` (`0x5eeb00`)

Two passes with a shared skeleton — scan the 8 ring-1 spiral neighbours of the
nominal slot for a tile satisfying a predicate, then write a terrain preference.

```c
for (i = 0; i < numPlayers; i++) {                        // PASS 1
    if (!getCell(i)->vfunc(0x8c)()) continue;             // civ gate: must be water
    y = i/(W>>1);  x = 2*(i%(W>>1)) + (y&1);
    for (n = 1; n < 9; n++) {
        spiralOffset(n, &dx, &dy);
        nx = wrapX((x&0xffff)+dx);  ny = wrapY((y&0xffff)+dy);
        if (out of bounds) continue;
        if (!getTile(nx,ny)->vfunc(0x8c)()) { found = 1; break; }
    }
    if (found) getCell(dx)->vfunc(0x128)(-1, -1, 11);      // NOTE: dx, not i
}

for (i = 0; i < numPlayers; i++) {                        // PASS 2
    if (getCell(i)->vfunc(0xc8)() != 13) continue;        // civ gate
    y = i/(W>>1);  x = 2*(i%(W>>1)) + (y&1);
    for (n = 1; n < 9; n++) {
        spiralOffset(n, &dx, &dy);
        nx = wrapX((x&0xffff)+dx);  ny = wrapY((y&0xffff)+dy);
        if (out of bounds) continue;
        if (getTile(nx,ny)->vfunc(0xc8)() == 11) { found = 1; break; }
    }
    if (found && !(getCell(i)->vfunc(0xac)() & 0x400000))
        getCell(i)->vfunc(0x128)(-1, -1, 12);              // correctly uses i
}
```

**Pass 1 passes `dx` — the found neighbour's x *offset*, a value in roughly
[-1,+1] — instead of the civ index `i` to `vfunc(0x128)`.** It therefore almost
always writes to cell 0 or 1. This is reproduced as observed; it looks like a real
bug in the original, masked by the retry loop.

### 11.3 `convertDesertsAtStarts` (`0x5edb70`)

Generates an independent fractal (`seed = waterLevel + 0x9A2112`, level 2, flags 1),
then takes six elevation percentiles chosen by `this->[0x30]`:

| `this[0x30]` | percentiles |
|---|---|
| 0 | 18, 28, 55, 65, 77, and a fixed 94 |
| 1 | 20, 25, 63, 70, 77, and a fixed 96 |
| 2 | 21, 24, 66, 70, 74, and a fixed 98 |

```c
st = water + 0xD431;
for (i = 0; i < npl; i++) rand_int(&st, npl - i);      // draw sequence, results discarded

for (i = 0; i < npl; i++) {
    y = i/(W>>1);  x = 2*(i%(W>>1)) + (y&1);
    if (getTile(x,y)->vfunc(0xc8)() != 2) continue;   // must be on class-2 land
    h = sampleHeight(fm, x, y);
    if (h > band[1]) continue;                          // too high
    if (h <  band[4]) {
        if (h >= band[5])                      setTerrain(5);
        else if (h <= band[0] && h >= band[2]) setTerrain(5);
        else continue;
    } else {
        setTerrain((rand_int(&st, 100) & 0xffff) < band[3] ? 6 : 10);
    }
}
```

This is what makes C3C start locations usable: a civ whose nominal slot landed in
a desert gets Grassland, Tundra or Plains-desert instead.

### 11.4 `paintContinents` (`0x5eddb0`) and `0x5ee470`

```c
uint16 *perm = malloc(2 * npl);
for (i = 0; i < npl; i++) { perm[i] = i; this->[0x3C][i] = 0xFFFF; }
for (k = 0; k < nCont; k++) this->[0x38][k] = 0;
st = water + 0xD431;
for (i = 0; i < npl; i++) {                          // Fisher-Yates
    j = (rand_int(&st, npl - i) & 0xffff) + i;
    if (j != i) { t = perm[i]; perm[i] = perm[j]; perm[j] = t; }
}

for (k = 0; k < npl; k++) {
    v = perm[k];
    if (this->[0x3C][v] != 0xFFFF) continue;
    if (getCell(v)->vfunc(0x8c)()) continue;          // must be land
    r = getCell(v)->vfunc(0xb8)();
    if (this->[0x38][r] != 0xFFFF) {                  // already on a continent
        y = v/(W>>1);  x = 2*(v%(W>>1)) + (y&1);
        paintRegionId(x, y, this->[0x38][r]);         // 0x5ee470
        continue;
    }
    placed = 0;
    for (kk = k; kk < npl && !placed; kk++) {         // try later civs in shuffled order
        w = perm[kk];
        y = w/(W>>1);  x = 2*(w%(W>>1)) + (y&1);
        j0 = rand_int(&st, 4);
        for (j = 0; j < 4; j++) {
            nx = wrapX(x + DXT[j&3]);  ny = wrapY(y + DYT[j&3]);
            if (out of bounds) continue;
            if (getTile(nx,ny)->vfunc(0xb8)() != getTile(x,y)->vfunc(0xb8)()) continue;
            if (this->[0x3C][(W>>1)*ny + (nx>>1)] == 0xFFFF) { placed = 1; break; }
        }
        if (placed) this->[0x3C][(W>>1)*ny + (nx>>1)] = v;
    }
}
```

`0x5ee470(x, y, id)` paints a continent id into the half-resolution region map
`this->[0x3C]`, indexed by `ridx(cx,cy) = ((W>>1)*cy + (cx>>1)) & 0xffff`. It runs
up to 16 iterations of "claim a free cell, or hop to a matching neighbour", capped
at 4 hops, seeded from `water + 29*id` with four discarded draws, then a
two-iteration merge pass and a bump of the per-continent census in `this->[0x38]`.

Two apparent inconsistencies in the original, both reproduced as observed:

* the success store in `0x5eddb0` and the merge stores in `0x5ee470` use
  `(W>>1)*ny + nx` with the **raw** `nx`, while every *read* uses `x>>1`;
* `0x5ee470` compares a region id against the **x offset returned by the previous
  `spiralOffset`** at two sites — a register mix-up in the original.

`this->[0x3C]` is allocated as `malloc(numPlayers * 2)` but indexed by a
map-sized expression (≈ 12 800 on a 160×160 map). Either the allocation is larger
than it looks or the writes are out of bounds in the original. Worth re-checking.

### 11.5 `assignBiomes` (`0x5f1480`) — the climate stage

2 143 bytes. This is the stage that turns the uniform class-2 land from `0x5eceb0`
into Civ3's actual terrain distribution.

**Setup.** One `FractalMap`, generated three times. A Fisher-Yates over all cells.
RNG seeded `water + 0xF0FF3`; first fractal `water + 0x1E1735`.

```c
lat = 180 * |H/2 - y| / H;                  // continuous latitude, 0..90
h   = sampleHeight(fm, x, y);
v   = ((h - 128) * 30) / 256 + lat;        // combined climate axis, ≈ [-15, 105]
```

**There is no explicit 5-way temperature band switch.** The bands are implicit
thresholds on `v` plus `lat`, and the magic numbers below.

| name | Res 0 | Res 1 | Res 2 | | opt 0 | opt 1 | opt 2 |
|---|---|---|---|---|---|---|---|
| **A** | 3 | 4 | 5 | | A−1 | A | A+2 |
| **B** | 35 | 50 | 55 | | — | — | — |
| **C** | 45 | 60 | 65 | | — | — | — |
| **D** | 10 | 12 | 16 | | — | — | — |
| **E** | 3 | 5 | 7 | | — | — | — |
| **F** | | | | | 7 | 10 | 15 |
| **G** | | | | | 2 | 5 | 8 |
| **H** | | | | | 12 | 14 | 16 |
| **I** | | | | | 44 | 42 | 40 |
| **J** | | | | | 13 | 17 | 20 |
| **K** | | | | | 34 | 30 | 27 |

plus `M = (B + A) / 2` = 18…31, computed from the *unmodified* `B`.

**Pass 1** — the class assignment, in shuffled cell order:

```c
for (k = 0; k < numCells; k++) {
    c = perm[k];
    y = c/(W>>1);  x = 2*(c%(W>>1)) + (y&1);
    lat = 180*|H/2 - y| / H;
    v   = ((sampleHeight(fm,x,y) - 128) * 30) / 256 + lat;

    if (getCell(c)->vfunc(0x8c)()) continue;          // already water -> skip

    nearLand = 0;
    for (n = 1; n < 25; n++) {                        // spiral: nearest water
        spiralOffset(n, &dx, &dy);
        nx = wrapX(x+dx);  ny = wrapY(y+dy);
        if (in bounds && getCell(nx,ny)->vfunc(0x8c)()) {
            nearLand = (n < 9) ? 2 : 1;
            break;
        }
    }

    if (23 <= v && v < 48) v += 2*nearLand;
    else                    v -= nearLand;

    if (v > B)                                    getCell(c)->vfunc(0xE0)(2, 0x200000, -1, -1);
    else if (v > M && rand_int(100) < 50)         getCell(c)->vfunc(0xE0)(2, 0x200000, -1, -1);

    if (bonus[c] != 0) continue;
    cls = 2;
    if (H < v && v < I)  cls = 1;
    if (J < v && v < K)  cls = 0;
    if (v < A)            cls = 8;
    else if (B < v && C < lat) cls = 3;
    if (v < D) {
        if      (cls == 2) { r = rand_int(100); base = G; }
        else if (cls == 8) { r = rand_int(100); base = F; }
        else goto write;
        if (r < nearLand*E + base) cls = 9;
    }
write:
    writeBiomeClass(x, y, cls, bonus);               // 0x5f1ce0
}
```

Reading of the classes (`v` small = equatorial, large = polar) — inference, not
proven:

| cls | terrain |
|---|---|
| 0 | Desert — `J..K` sits just above the equatorial band |
| 1 | Plains — `H..I` |
| 2 | Grassland — the default, exactly what `0x5eceb0` writes for land |
| 3 | Forest — `v > B` and `lat > C` |
| 8, 9 | the two equatorial-wet classes, the Jungle/Marsh pair; 9's probability grows with `nearLand` and with the Resources setting |
| 6 | the terrain the hills chains are made of (written by `0x5f00c0` and pass 3) |
| 7 | written by pass 2 |

**Pass 2** — the p70 contour. The same buffer, regenerated at
`water + 0x34B59E`, **level 5**:

```c
p70  = percentileLookup(fm, 70);
p100 = percentileLookup(fm, 100);
for (i = 0; i < numCells; i++) {
    if (getCell(i)->vfunc(0xC4)() == 0) continue;
    cls = getCell(i)->vfunc(0xC8)();
    if (cls == 8 || cls == 5 || cls == 6 || cls == 9 || cls == 10) continue;
    y = i/(W>>1);  x = 2*(i%(W>>1)) + (y&1);
    if (getCell(x,y)->vfunc(0x8c)()) continue;
    h = sampleHeight(fm, x, y);
    if (h != p70) continue;
    getCell(i)->vfunc(0x128)(7, -1, -1);
}
```

Every cell sitting **exactly** on the fractal's 70th-percentile height contour is
forced to class 7. The `if (ff == 1) h -= 10` guard is dead — `ff` is 3 or 9.

Using `==` rather than `<=` is deliberate: `==` marks 0.0–0.5 % of the land,
whereas `<=` would mark 52–61 %, i.e. most of the map as hills. Both readings are
bad; `==` is the faithful one and `<=` is available behind a bug flag.

**Pass 3** — the p100 neighbours of class 6:

```c
for (i = 0; i < numCells; i++) {
    if (getCell(i)->vfunc(0xC8)() != 6) continue;
    y = i/(W>>1);  x = 2*(i%(W>>1)) + (y&1);
    uint8 p = 50;
    for (n = 0; n < 49; n++) {
        spiralOffset(n ? n : 8, &dx, &dy);            // n == 0 -> spiralOffset(8)
        nx = wrapX(x+dx);  ny = wrapY(y+dy);
        if (!in bounds) continue;
        if (n == 9 || n == 25) p >>= 1;              // 50 -> 25 -> 12
        if (getCell(nx,ny)->vfunc(0xC4)() == 3) goto roll;
        if (n < 25 && getCell(nx,ny)->vfunc(0x74)()) goto roll;
    roll:
        if (rand_int(100) < p) {
            getCell(i)->vfunc(0xE0)(2, 0x100000, -1, -1);
            break;
        }
    }
}
```

**`0x5f1ce0` (614 bytes, 4 args) is still unmapped.** It is where the abstract
class `{0,1,2,3,8,9}` becomes actual `vfunc(0x04)` / `vfunc(0x08)` / flag writes,
and where the `bonus[]` array is consumed. Its prologue shows the index
arithmetic `x, y, W, cls, (W>>1)*cls`. This is the one gap in the biome stage.

### 11.6 `assignStartsPerContinent` (`0x5ed5d0`)

```c
uint32 *perm = malloc(4 * npl);
st = water + 0x7C0;
rand01(&st) x3;                                        // discarded
for (i = 0; i < npl; i++) perm[i] = i;
for (i = 0; i < npl; i++) {                           // Fisher-Yates, 32-bit
    j = (rand_int(&st, npl - i) & 0xffff) + i;
    if (j != i) { t = perm[i]; perm[i] = perm[j]; perm[j] = t; }
}

for (c = 0; c < getNumContinents(); c++) {
    if (getContinent(c)->[0x24] < 0x4B /*75*/) continue;     // skip small continents

    // phase 1: plain scan of the shuffled slots
    for (j = 0; j < npl; j++) { ... }
    // phase 2: spiral search, 9 candidates, tail-merged success/failure
    // phase 3: same spiral, but the tile test is only vfunc(0x8c)
}
this->finalizeMap();
free(perm);
```

Phases 2 and 3 differ in their tile predicate; on success neither writes anything
and the loop simply continues (a tail-merge in the original binary). Only the
spiral-exhaustion path performs the `vfunc(0x128)(-1,-1,11)` write and sets a bail
flag. Phase 2's fallback calls `vfunc(0x30)` with **two copies of `y`** while
phase 3 correctly uses `(x, y)` — another apparent original bug, reproduced as
observed.

### 11.7 `growHillsAndMountains` (`0x5f07d0`)

**This is not a resource stage and not a river stage.** Its chain endpoint writes
BIQ terrain class **6**, and `vfunc(0x40)` is `isHillsOrMountains()`. It produces
the mountain and hill chains that hug coastlines.

Two parallel `uint16[256]` arrays at `S+0x4C` (score) and `S+0x24C` (cell
position, packed as `y*(W>>1) + (x>>1)`), kept sorted by score. The cap is a
per-continent quota:

```c
maxExtra = clamp(numCells*75/5000, 0, 256);
rng = water + 0x87B01;
for (k=0;k<nCont;k++)  isBig[k] = (getCont(k)->tiles >= 37);
for (k=0;k<nCont;k++)  w[k] = -1 - isBig[k];
for (k=nCont-1;k>=1;k--) w[k] += getCont(k)->tiles * EBs / total;
contWeight[0] += maxExtra;
for (k=1;k<nCont;k++) contWeight[k] += contWeight[k-1];
```

**Pass A** — a candidate must be a genuine coastline: exactly two of its four
orthogonal neighbours are water, and they must not be an opposite pair (which
would be a one-cell isthmus). Each candidate is then scored by a 121-step spiral
(`n = 0..0x79`) adding `8*(n<25) + 16*(n<9) + 32*(n<1)` plus a per-terrain-class
bonus from a jump table at `0x5f1224` — a "distance from the sea, weighted by how
good the neighbour terrain is" score.

**Pass B** — growth from the centroid of each candidate's land neighbours:

```c
for (k = 0; k < 256; k++) {
    if (B[k] == 0) continue;
    y = B[k]/(W>>1);  x = 2*(B[k]%(W>>1)) + (y&1);
    sumX = sumY = 0;  landMask = 0;
    for (d = 0; d < 4; d++) {
        nx = wrapX(x+dx);  ny = wrapY(y+dy);
        if (getCell(nx,ny)->vfunc(0x8c)()) landMask |= 1 << d;
        else { sumX += nx;  sumY += ny; }
    }
    int kind = (ortho ? (diag ? 1 : 2) : (diag ? 0 : 3));
    growChain(sumX, sumY, kind, 0, 0, 0, 0);        // 0x5f00c0, ret 0x1c
}
```

`0x5f00c0` is the recursive worker:

```c
int growChain(int x, int y, int dir, int depth, int w, int z, int p)
{
    if (depth < 21) {
        markChain(x, y, ~dir & 1, depth);                     // 0x5f0370
        int c[3];
        c[0] = (z < -1) ? -1 : probe(x+DX[dir+3], y+DY[dir+3], dir+3, depth, (z>=0)&2);
        c[1] =       probe(x+DX[dir],   y+DY[dir],   dir,   depth, (p>=2)?1<<(p-1):0);
        c[2] = (z <  2) ? probe(x+DX[dir+1], y+DY[dir+1], dir+1, depth, (z>=1)&2) : -1;
        if (c[0] >= 0 || c[1] >= 0 || c[2] >= 0) {
            if (fanOut(x+DX[dir], y+DY[dir], dir, depth, w, z, p, c)) return 1;
            unmarkChain(x, y, ~dir & 1);                       // 0x5f0450
            return 0;
        }
        if (depth < 4 || w < 2) { unmarkChain(...); return 0; }
    }
    /* tail: 4-way switch on (dir+2)&3 */
    int d2 = (dir + 2) & 3;
    int ax = x + DX[d2],  ay = y + DY[d2];
    int nx = ax / 2,     ny = ay / 2 - 1;                      // cdq / sar 1 / dec
    wrapX(nx);  wrapY(ny);
    getCell(nx, ny)->vfunc(0x128)(6, -1, -1);                  // class 6
    return 1;
}
```

`0x5efa90` is the per-step admissibility test, `0x5f0530` the fan-out (four
discarded `rand01` draws, sorts the three scores, `rand_int(2)` when `w > 1` to
choose how many branches to take), and `0x5f0370` / `0x5f0450` are the
mark/unmark pair.

**Termination, three ways:** `depth >= 21` falls through to the write, so the last
tile of a chain is always class 6; no admissible direction combined with
`depth < 4 || w < 2` unmarks and returns 0 with no write; and `0x5f0530`
returning 0 unmarks and returns 0. So **chains are 4–21 tiles long** and the
recursion is bounded by the depth cap, not by the map size.

`0x5f00c0`'s `dir` switch is rotated by two relative to the `0x67055c` /
`0x67056c` tables — either an r2 labelling artefact or the original's source
numbering. Both readings are flagged rather than chosen between.

**The last loop** runs `0x5f1240(x, y, nx, ny)` over all cells and the four
neighbours, and ORs `1 << (i & 0x1F)` into `byte[cell+4]` via `vfunc(0xF4)`.
`0x5f1240` is a connection-mask continuity test: it asks `map->vfunc(0x70)(x,y,..,9)`
for a direction, reduces it mod 7, and checks whether the `1 << dir` bits continue
through two adjacent directions. Its first block calls `getTile(x1, x2)` where the
second block correctly uses `getTile(x2, y2)` — a variable mix-up in the original.

### 11.8 `placeResources` (`0x5f22a0`)

2 492 bytes. Iterates the **`'GOOD'`** resource list against the **`'TERR'`**
terrain list, both via `map->vfunc(0x8c)(tag, index, &out)` with `index = -1`
returning the count.

```c
nGoods = vfunc(0x8c)('GOOD', -1, NULL);
arrTerrain = malloc(4*nGoods);  fill -1;                 // "terrain already chosen for g"
arrCiv     = malloc(4*numPlayers);  identity;  shuffle;  // Fisher-Yates
for (i = 0; i < nGoods; i++) cntArr[i] = 0;              // this->[0x14C]

for (g = 0; g < nGoods; g++) {
    good = vfunc(0x8c)('GOOD', g, &out);
    if (!classOK(good)) continue;
    pct = good->[0x40];
    if (pct == 0) pct = rand_int(26) + rand_int(26) + 50;
    n1   = (this->[0x15C] * pct) / 32;
    nTerr = vfunc(0x8c)('TERR', -1, NULL);
    score = 0;
    for (t = 0; t < nTerr; t++) {
        tr = vfunc(0x8c)('TERR', t, &out2);
        if (tr && (tr->[8][g>>3] & (1 << (g&7)))) { score++;  if (t >= 11) score += 4; }
    }
    if (score == 0) continue;
    n = score < 2 ? (int)(n1*0.5f) : score < 4 ? (int)(n1*0.75f) : n1;
    n = max(n, score >= 4 ? 2 : 1);
    if (n <= 0) continue;
    ...
}
```

Three class-gated placement blocks:

| block | class predicate | behaviour |
|---|---|---|
| 1 | `0x5e3720`, non-inverted | a **cluster** of `n` copies, spread via 8-ring spiral offsets, refusing a neighbour that already holds ≥3 copies of the same resource |
| 2 | `0x5e3730` (`== 2`) | no spreading; each player gets up to `n` different candidate tiles |
| 3 | `0x5e3700`, **inverted** | a repeating fill-up pass |

All copies of one resource must share the same `vfunc(0xb8)` value. The write is
`tile->vfunc(0xEC)(g)` followed by `cntArr[g]++`. Block 3's coin-flip is
`rand_int(score<2 ? 6 : score<4 ? 4 : 2) > 1` — acceptance probabilities
**33 % / 50 % / 100 %** — and it repeats until `numPlayers >> 5` further
placements land.

**`.biq` dependencies, and only two:** `good->[0x40]` (per-resource frequency) and
`terr->[8]` (a byte whose bit `i & 7` says "resource *i* may appear here"), plus
the class predicates `0x5e3700` / `0x5e3730` / `0x5e3720`.

### 11.9 `placeGoodyHuts` (`0x5f21b0`)

```c
if (this->[0x10] == -1) return;
if ((uint16)(this->[0x10] & 0xFFE0) == 0) return;      // needs count >= 32
for (i = 0; i < (this->[0x10] >> 5); i++) {
    r = rand_int(this->[0x10]);
    y = r / (W >> 1);
    x = 2*(r % (W >> 1)) + (y & 1);
    if (this->vfunc(0x48)(x, y))
        this->vfunc(0x34)(r)->vfunc(0xE0)(0, 0x20, -1, -1);
}
```

Feature id `0`, quantity `0x20` — a **goody hut**. `vfunc(0x48)` is the map's
"may this tile hold a bonus" query. The argument order of `vfunc(0x48)` was
re-derived from the raw pushes: `push row` then `push x`, and with callee-cleanup
the last push is the first argument, so it is `(x, y)`.

`getCell(r)` is indexed by the same random value that was decoded into a tile
coordinate, so the cell and its coordinate are correlated by construction.

**The `>= 32` guard and the `>> 5` bound make this a no-op in every normal Civ3
game** (max 31 civs). The same idiom guards block 3 of `0x5f22a0`. Either the
original intent was a 32-player feature or these are leftovers from a
multiplayer-era build. Flagged as ambiguous.

### 11.10 `placeBarbarianCamps` (`0x5f2090`)

```c
idx = identity[0..this->[0x10]];  Fisher-Yates shuffle;
for (i = 0; i < this->[0x10]; i++) {
    Cell *c = this->vfunc(0x34)(idx[i]);
    if (c->vfunc(0xC4)() != 2) continue;      // must be land
    if (c->vfunc(0x9C)() != -1) continue;     // no feature here yet
    if (rand_int(3) != 0) continue;           // 1-in-3
    this->vfunc(0x34)(idx[i])->vfunc(0xE0)(2, 0x10000, -1, -1);
}
free(idx);
```

Feature id `2`, quantity `0x10000` — a **barbarian camp**. `vfunc(0xC4)` is the
secondary class, compared against 2.

### 11.11 `finalPass` (`0x5eeee0`)

2 449 bytes, 5 arguments, called as `(1, 0, ±1, ok, &ret)`.

**Phase A** — hand each eligible class-1 resource to the first civ whose start
tile has none, recording a per-terrain tally in two `getNumContinents()`-sized
arrays.

**Phase B** — build the slot array and the LCG:

```c
arrOrder = identity; shuffle;                // Fisher-Yates over numPlayers
arrD[i]  = map->vfunc(0x08)(x, y)            // for the shuffled order
st = this->[0x1EC] + 0x16062 + a2;

nSlots = 1;
if (a1 != 0) { for (i=0;i<32;i++) { this->[0x16C+4*i] = -1; rand01(&st); } }
else {
    this->[0x16C] = -1;
    n = this->[0x15C];
    for (i=0;i<n;i++) if (this->[0x170+4*i] != -1) nSlots++;
    rand01(&st) x4;
}
```

**Phase C** — eight level-gated passes of civ selection. A civ qualifies if:

* (levels < 4) its best-ranked 8-neighbour id, via `0x5eedb0`, has rank > 20;
* its candidate tile is not on the map border;
* its start tile passes **seven** different "empty" tests — `vfunc(0x80)`,
  `vfunc(0x8c)`, `vfunc(0x3C)(0)`, `0x5ea6c0`, `0x5ea6e0`, `0x5ea9f0`,
  `vfunc(0x1C)(0)` — all of which must return false;
* (level 0) the terrain does not already have too many civs;
* the terrain's rank clears 75, or 37 at levels < 2, or at level < 5;
* (levels < 7) its own `vfunc(0x08)` lookup is > 0;
* (levels < 6) it matches an entry of `this->[0x170]` within a radius of
  `this->[0x158]` (halved for levels ≥ 3).

Each acceptance appends the civ to the first free `this->[0x16C]` slot, bumps the
terrain tally and sets marker mask `0x80000`.

**Phase D** — shuffle `this->[0x16C]`, with a deterministic jump at `i == a3`.

**Phase E** — a cross-array terrain-balancing fixup between `+0x16C` and `+0x170`.
The swap at `0x5ef7e1` is a genuine cross-array exchange, but the invariant it
maintains is not recoverable from the code alone.

`0x5eedb0(x, y)` returns the highest-ranked terrain id among the 8 neighbours that
pass `vfunc(0x8C)`, ties keeping the spiral-order-first candidate. It is the
"good neighbourhood" gate.

---

## 12. `postProcess` and the "Fixed on Nth try" fix-up

`postProcess` (`0x5ebe80`, vtable slot `0x80`) runs *after* the pipeline. `0x5ec2d0`
prints

```
* * * * * * * * * * * * * * * * Fixed on 1st/2nd/3rd try!
```

It is the same "find a 2×2 block of matching terrain" trick as `0x5ebdc0` (its
non-trying sibling), generalised over `word[esi+0x40]` levels with seeds
`0x1D9D3 + 101*level + waterLevel`. For each level it builds a fresh fractal
(level 2, flags 3 or 9), walks the 4 corner tiles around a spiral offset,
increments a per-terrain counter (14 slots), and if fewer than 4 of the 14 got a
hit retries with the next offset, up to 3 attempts, otherwise emits
`setTerrain(12 or 13)` at the found location.

`0x5ebe80` also contains the game's only easter egg: if the system year is `> 2012`
it tries to unpack `Extras\Jaimo.zip` next to `Civ3XInf.dat`.

---

## 13. The tile record and the save format

The only per-tile record reader in the binary. It is what shows which field any
given feature occupies.

```
Map/scenario loader   0x594290   ('BIC '|'BIX '|'BIQ ' magic, then 4-byte section tags)
  'TILE'  -> FUN_00596ce0   (gated on param_2[1] >> 6 & 1)
  FUN_00596ce0  -> FUN_005f3cc0(count)          allocate/refresh the cell array
                -> loop i in 0 .. word[0x9c73ac]:
                     cell = FUN_005d16a0(i)
                     cell->vfunc(0x2C)(0)       reset
                     cell->vfunc(0xE4)(-1)
                     cell->vfunc(0xF8)(-1)
                     cell->vfunc(0x104)(-1)
                     cell->vfunc(0x108)(0)
                     FUN_005ea1f0(file, gameOption, iVar4)    <-- the record reader
```

`0x5d16a0` is the only caller of `0x5ea1f0`, and `0x5ea1f0`'s only caller is
`0x596ce0`. So **the tile record is read from a scenario/`.biq`/`.sav` and nowhere
else.** There is no code path that synthesises these records.

`0x5f3cc0` is `setCellCount` and `0x5da7d0` is the Cell constructor, so the record
target is a `Cell*` and the offsets below are Cell fields straight off disk:

| offset | size | notes |
|---|---|---|
| `+0x04` | 1 | the `byte[+4]` connection mask |
| `+0x05` | 1 | read, but has no vtable accessor |
| `+0x08` | 4 | feature id |
| `+0x10` | 4 | |
| `+0x14` | 4 | |
| `+0x18` `+0x1A` `+0x1C` `+0x1E` | 2 each | `+0x1E` is the continent id |
| `+0x20` | 1 | written as 2, or 6 if the record is short; compared against 4 and 5 |
| `+0x22` | 2 | |
| `+0x24` | 4 | |
| `+0x28` `+0x2C` `+0x30` | 4 each | read as one 12-byte blob |
| `+0x34` | 4 | last field read |

Reading is strictly sequential with a running "bytes remaining" counter, each field
guarded by `if (remaining >= size)`, so a truncated file is tolerated.

`0x5ea410` then demotes terrain: if `byte[+0x20] >= 5` it clears `+0x34`, and
classes 11/10/9 are forced to 13/12/11 — the load-time water-depth normalisation.

---

## 14. Rivers — the investigation and the result

**C3C random maps do have rivers in play. The map generator does not place them.**

### 14.1 What was ruled out

All twelve `generateMap` stages are now identified (§10) and **none of them places
a river**. The three stages that place a discrete object on a tile place
resources, goody huts and barbarian camps.

Independently, an exhaustive scan of the whole generator range (`0x5eb000`–`0x5f3000`)
for 4-byte uppercase immediates used as `push imm32` or `mov reg, imm32` finds
exactly two:

```
TERR   x8
GOOD   x7
```

`VUUU` ×3 is a jump-table artefact. There is no `RIVR`, `RIVE`, `RVRS` or `RVER`
FOURCC anywhere in the generator, and the `0x594290` dispatcher likewise has none
— its tags are `SLOC`, `LEAD`, `GOOD`, `HOOD`, `RACE`, `TILE`, `RULE`, `GAME`,
`HAME`, `DIFF`, `BLDG`, `TECH`, `TFRM`, `UFRM`, `ESPN`, `CTZN`, `PRTO`, `QRTO`.

So there is not even a **data path** by which a river could enter generation. This
confirms the architecture: river placement is a **separate system** applied to an
already-generated map.

### 14.2 The two misidentifications that cost the most time

| what it was called | what it is |
|---|---|
| `0x5f07d0` = "the river generator" | the **hills / mountains** pass. Its chain endpoint writes BIQ terrain class 6, and `vfunc(0x40)` is `isHillsOrMountains()`. |
| `0x5f1240` = "is this a river/lake source" | a **connection-mask continuity test** used by the hills pass, checking `1 << dir` bits through two adjacent directions. |

The mislabelling of `0x5f07d0` is what sent the search sideways for the longest
period, because connection-bit routing over a half-resolution grid looks exactly
like what a river generator should look like.

### 14.3 The strongest remaining lead

`byte[cell+4]` is the **only** per-tile connection field in the struct. It is
exactly one byte, it has a getter (`vfunc(0x94)`) and two setters (`vfunc(0xD0)`
value, `vfunc(0xF4)` OR), and the hills pass ORs bits into it and clears it via
`0x5f0450`.

That matters because a C3C river is drawn as a set of *segments*: the game ships
`RiverFore.pcx`, `RiverBack.pcx`, `deltaRivers.pcx`, `mtnRivers.pcx` and
`NOTtheRiver.pcx` — the last being a "do not draw a river here" mask used to break
a segment at map borders. A one-byte nibble of four edge bits is exactly the right
shape for that.

`byte[cell+5]` is the other candidate: it is read by the tile record reader and
written by it, but it has **no vtable accessor at all**, so nothing in the
published interface can read it back.

**Unproven.** What would confirm it: the renderer that draws `RiverFore.pcx`, which
must read the field and would settle the bit order.

### 14.4 Art evidence

| string | VA | referenced from |
|---|---|---|
| `Art\Terrain\deltaRivers.pcx` | `0x6699D8` | no direct code xref |
| `Art\Terrain\mtnRivers.pcx` | `0x669ADC` | no direct code xref |
| `RiverBack.pcx` | `0x680D6C` | `0x408246`, inside `0x407C30` |
| `NOTtheRiver.pcx` | `0x680DB8` | the same art table |
| `RiverFore.pcx` | `0x680DDC` | the same art table |
| `TERR_River` | `0x728D91` | `0x4D2800` ← `0x4C9930`, a civilopedia/editor path |

`0x407C30` turned out to be the **city view** background loader — its art list is
`BLDG_Courthouse … BLDG_Palace, RiverBack.pcx, Harbor.pcx, IslandLeft.pcx,
CoastBack.pcx, CoastRight.pcx, NOTtheRiver.pcx, RiverFore-FP.pcx, RiverFore.pcx`,
and it builds `art\city view\Backgrounds\<L><n>-SML.pcx` names where `<L>` is a
1-letter graphics-mode prefix and `<n>` is 1..10. That is the city view, not the
map view.

`TERR_River` and the three "terrain reference" strings are referenced only by
`0x4D2800`, whose sole caller is `0x4C9930`, which pulls in
`text\Civilopedia.txt`. Those are **civilopedia article-section keys, not map
data**.

---

## 15. The `.biq` container and the PKWARE DCL codec

`conquests.biq` is 30 501 bytes and contains **zero** section tags, so it is
compressed. The decompressor the game uses is embedded in the binary.

### 15.1 The load path

```
0x59AB50  scenario load sequence
  -> FUN_005f7800()
  -> FUN_005f76c0(path, 0)      inflate into a temp file, return its name
       -> FUN_00649400(&0x5F74B0, &0x5F7500, state, &inFile)
  -> FUN_00594290(tempName, opts)   the scenario / BIC reader
```

`0x5F74B0` is a plain `fread`, so the DCL stream starts at byte 0 of the file.

### 15.2 The decompressor

Seven functions, all identified:

| address | role |
|---|---|
| `0x649400` | init — reads the 3-byte header, builds the tables, runs the loop |
| `0x649580` | the main loop: 4 KB window, 0x1000-byte flush chunks, `len = sym - 0xFE` |
| `0x649680` | decode one symbol |
| `0x649830` | decode a match distance |
| `0x6498B0` | `getbits(n)` — MSB-first bit reader, value left in the low bits |
| `0x649940` | expand a (lens, bases) pair into a 256-entry peek table |
| `0x649980` | build the four-level literal tree |

The window is 4 KB: the decode buffer is `0x2000` bytes, the output pointer starts
at `0x1000`, and 0x1000 bytes are flushed and slid whenever the pointer passes
`0x1FFF`.

The header is three bytes: `byte0` = compression mode (0 = stored, 1 = Huffman),
`byte1` = dict bits, required to be 4, 5 or 6, `byte2` = the distance mask shift,
where the mask is `0xFFFF >> ((16 - byte2) & 31)` — **x86 masks shift counts to
five bits**, which matters.

There are two symbol paths, selected per symbol by `bitbuf & 0xFF & 1`:

* **short tree** — a 16-entry length class peeked from the next 8 bits, then extra
  bits, then `0x100 + LEN_BASE2[class] + extra`. The terminator is class 15 with
  extra 8, i.e. `0x30E`.
* **four-level tree** — a 256-entry level-1 table, with escapes into a 6-bit
  level-2 table (128 entries) or a 4-bit level-3 table (256 entries), plus a
  `peek == 0` path into a second level-1 table.

### 15.3 The static tables, read out of the executable

| VA | size | contents |
|---|---|---|
| `0x73A520` | 256 | literal code lengths, values 4..13 |
| `0x73A450` | 64 | distance code lengths |
| `0x73A500` | 16 | extra-bit count per length class: `3 2 3 3 4 4 4 5 5 5 5 6 6 6 7 7` |
| `0x73A510` | 16 | low canonical code per length class: `5 3 1 6 10 2 12 20 4 24 8 48 16 32 64 0` |
| `0x73A4D0` | 16 | extra bits in the short-tree length code: `0 0 0 0 0 0 0 0 1 2 3 4 5 6 7 8` |
| `0x73A4E0` | 32 | 16 `u16` short-tree length bases: `0 1 2 3 4 5 6 7 8 10 14 22 38 70 134 262` |
| `0x73A490` | 64 | low canonical code per distance class |

Both Huffman trees are **complete canonical** sets — the Kraft sums are exactly
1.0000:

```
literal  length histogram {4:1, 5:11, 6:20, 7:21, 8:16, 9:7, 10:5, 11:10, 12:91, 13:74}
distance length histogram {2:1, 4:2, 5:4, 6:15, 7:26, 8:16}
```

So they can be rebuilt from scratch without needing the code table that
`0x649980` reads.

A second, unrelated codec also lives in the binary at `0x648920` / `0x648AC0` with
tables at `0x73A088`, `0x73A188`, `0x73A058`, `0x73A068`, `0x73A078`, `0x739FD8`
and `0x73A018`. That one is the **compressor** for save-game writing, not the
`.biq` reader, and it should not be confused with the above.

The binary carries three copies of the string

```
PKWARE Data Compression Library for Win32
Copyright 1989-1995 PKWARE Inc.  All Rights Reserved
Patent No. 5,051,745
Version 1.11
```

at VA `0x739B10`.

### 15.4 What is not solved

**The `.biq` header framing.** The file begins `00 06 84 24 19 82 C5 4A …`. Taking
the first three bytes as the header gives mode 0 (stored) and dict bits 6, but
byte 3 would then be the first output byte and it is `0x24`, not the `BICQ` magic
that `0x594290` requires. No offset in the first 20 bytes yields a valid
mode/window/mask triple with the right magic, and the file has no section tags at
all, so it is definitely compressed.

`work/dcl.py` is a faithful reimplementation of the decompressor, complete except
for this framing question. It is not yet usable end to end.

### 15.5 Why the `.biq` matters, and what it does not block

The `.biq` supplies *definitions*, not *placement rules*. The placement algorithms
are in the executable and are recovered. The `.biq` is only the *data* those rules
consume: which resources exist, their frequency numbers, and their terrain
compatibility bitmaps.

So decoding it does **not** block implementing the placement rules — it blocks
supplying faithful *input data* to them. Rivers are a separate question again: if
they are a per-tile bitmask rather than a `GOOD` row, they have no `.biq`
dependency at all, which is the hypothesis §14.3 is testing.

---

## 16. The four opt-in original bugs

Preserving original-code behaviour is **opt-in, one flag per bug**, so the
default renderer is the faithful one. `OriginalBugs::default() == NONE`; the CLI
defaults to `ALL`.

| flag | what the original does | what the flag does |
|---|---|---|
| `start_slot_index` | `0x5eeb00` pass 1 passes the found neighbour's x *offset* to `vfunc(0x128)` instead of the civ index, so it writes to cell 0 or 1 almost always | passes the civ index |
| `contour_equality` | `0x5f1480` pass 2 uses `h == p70`, marking 0.0–0.5 % of land as class 7 | uses `h <= p70`, which marks 52–61 % |
| `sea_level_split` | `0x5eceb0` writes `h3 > pLo ? 12 : 13`, but that branch is only reached when `h <= pHi` and `pHi > pLo` always, so it always writes 13 | makes the 12 path reachable |
| `swapped_wrap_flags` | the wrap flags are read swapped in at least one site | corrects the swap |

Reproduced as observed where the original's behaviour is clearly a bug
(`start_slot_index`, `contour_equality`, `swapped_wrap_flags`); documented and
flagged where both readings are defensible (`sea_level_split`).

---

## 17. Reference implementation status

`rust/` in this directory. 3 832 lines, 93 tests, clippy clean, no dependencies.

| stage | address | status |
|---|---|---|
| option randomiser | `0x5f1f50` | exact |
| land/sea generation | `0x5eceb0` | exact |
| landmass fix | `0x5ed440` | not implemented |
| start deconfliction | `0x5eeb00` | exact, including the original's index bug |
| desert conversion at starts | `0x5edb70` | exact |
| continent painting | `0x5eddb0` | no observable effect on terrain |
| biome / climate assignment | `0x5f1480` | exact except the `0x5f1ce0` call |
| per-continent starts | `0x5ed5d0` | not implemented |
| hills / mountains | `0x5f07d0` | not implemented |
| post-process | `0x5ebe80` | not implemented |
| resources | `0x5f22a0` | not implemented — needs `.biq` data |
| goody huts | `0x5f21b0` | not implemented — fully specified, no data needed |
| barbarian camps | `0x5f2090` | not implemented — fully specified, no data needed |
| final pass | `0x5eeee0` | not implemented |
| contour smoothing | `0x5d3100` | not implemented |
| start location commit | `0x5d6500` | not implemented |

Everything not implemented only writes resource and feature ids into cells whose
terrain is already fixed, so the coastline, the ocean fraction, the biome layout
and the continent shapes — the parts that make a Civ3 map recognisable — are all
decided by the stages that *are* implemented.

`0x5f21b0` and `0x5f2090` are the cheapest remaining stages: both are fully
specified above, both depend on no `.biq` data, and both would drop straight into
the existing pipeline.

---

## 18. Open questions

1. **`0x5f1CE0`** (614 bytes) — the only unmapped function in the biome chain. It
   turns the abstract class `{0,1,2,3,8,9}` into actual `vfunc(0x04)` /
   `vfunc(0x08)` / flag writes and consumes the `bonus[]` array. Until it is read,
   `0x5f1480` is exact in its class *selection* but not in its class *writes*.
2. **The `.biq` header framing** (§15.4). Blocks faithful resource *data*, not the
   resource *algorithm*.
3. **The river system.** `byte[cell+4]` and `byte[cell+5]` are the candidates
   (§14.3). Finding the renderer that draws `RiverFore.pcx` would settle it.
4. **The `this->[0x3C]` allocation size** in `0x5eb580` — reads use map-sized
   indices but the caller's `malloc` looks like `numPlayers * 2`.
5. **`0x5D16A0` / the `Map` vtable base.** The complete-object vtable at `0x670120`
   (41 entries) overlaps the `numPlayers` field at `+0x40`, which is impossible for
   a single MSVC object. The table actually used at run time is most likely the
   derived table at `0x6707D0`. Slot *roles* above were established from behaviour
   and hold either way.
6. **The semantic mapping of the six option fields** onto the UI labels. `[0x20]`
   (0..4) and `[0x28]` (0..2, "None/Normal/Plentiful") are confident, `[0x18]`
   (0..2, continent-balance behaviour) is the Landmass slider, and the remaining
   four are ordered but not individually confirmed.
7. **The meaning of `vfunc(0xb8)`** (`word[cell+0x1e]`). It is *written* by
   `finalizeMap` as a continent id and *read* by the resource and region stages as a
   terrain grouping key. Both readings are consistent only if the id is reused as a
   partition label after `finalizeMap` has run — the placement stages all run after
   it, so that is almost certainly the intent, but it is not proven.
8. **The three resource-class predicates** `0x5e3700` / `0x5e3730` / `0x5e3720`
   gate the three placement blocks in `0x5f22a0`; their UI meaning is unidentified.
9. **The meaning of the `0x80000` marker** in `0x5eeee0`, and of the `+0x16C` /
   `+0x170` civ lists. They are very likely the engine's barbarian-camp list and
   settler-start list, but that is inference.
10. **Whether the `>= 32` civ guard** on `0x5f21b0` and block 3 of `0x5f22a0` is a
    multiplayer-era leftover or a genuine 32-player feature. As written, both are
    no-ops in every normal Civ3 game.
