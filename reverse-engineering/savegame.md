# Saved games (`.SAV`): the `CIV3` stream

Findings for `Civ3Conquests.exe` (PE32, image base `0x400000`; every address
below is a static VA). Rust reference: [`../biq/src/sav/`](../biq/src/sav/)
(`civ3_biq::Save`, standalone crate, no dependencies). Tools that run the
game's own loader: [`tools/emu/`](tools/emu/).

A save is **not** a scenario file. It is the whole live game, dumped object by
object by the same `game_data` routine that loads it: the embedded rules and
settings, the `Game` object, the map, 32 players, every unit and city, the turn
history, the replay log and some network state. The framing is two primitives
(a tagged chunk and a raw block, section 3) and the order is fixed by the
loader (section 4), so the grammar can be read off the exe and then checked by
running the exe.

Evidence tags used below:

| tag | meaning |
|---|---|
| **L** | read in the exe's code (address given) |
| **T** | the exe's own loader, run in an emulator over a real file, consumed exactly the chunks and raw blocks listed, in that order, to the last byte (section 7) |
| **S** | same, over a synthetic file (a real save re-laid out for another sub-version): the loader accepts it, but no real file of that sub-version exists to compare with |
| **C** | checked by value across the 22-save corpus (section 8) |
| **HYPOTHESIS** | inferred, not proven |

## 1. Results

* **The structure is complete for format version 24, sub-versions 2..=10.**
  Every byte of all 22 corpus saves belongs to a named chunk or raw block, and
  the sequence of (offset, tag, size) matches what the exe's loader consumes
  (**T**). Sub-versions 2..9 never occur in a real file; their gates were
  checked on synthetic files (**S**).
* **The Rust reader/writer reproduces every save byte for byte**, including the
  DCL recompression of the 10 compressed ones (`Save::to_bytes`; the compressor
  is the port of the game's `implode`, [`biq.md`](biq.md)).
* **Decoded meaning (C)**: turn, per-continent city counts, wonders' owners; the
  map header and every tile field the scenario format also has; per-player race,
  capital, government and gold; unit id, position, owner, type, experience,
  damage, movement spent, order; city id, position, owner, name, size; the turn
  history (turn, year, per-player series); the replay log; the rule counts that
  size the raw arrays.
* **Not decoded**: most of the bits inside the large runtime blocks (`GAME`
  body, `LEAD` body, `UNIT`, `CITY`, `CTZN`, `TILE` 128, `ESPN`, `CULT`) and the
  masks after `GAME`. The Rust model keeps them as `Body<N>` byte arrays so the
  save still round-trips; section 9 lists what is known about each.
* **A save contains live memory, not a clean encoding.** A chunk body is a
  `memcpy` of a range of an object (section 3), so it includes padding and
  uninitialised heap bytes (three bytes after the city owner, 19 dwords at the
  head of every `DATE`, the oceans percentage of random maps, ...). Two saves
  of the same state can differ. Compare through the accessors, never the raw
  bytes.

## 2. The file

### 2.1 Storage

A `.SAV` is either the stream itself (autosaves) or one PKWARE DCL stream that
decodes to it (manual saves; `00 06`: binary literals, 4 KiB window, the same
mode as the shipped scenarios). The loader sniffs the magic, so both load.
`Raw::parse` / `Save::parse` do the same and remember which it was
(`Storage::Plain` or `Storage::Dcl { mode: Binary, dict_bits: 6 }`);
recompressing reproduces the file exactly ([`biq.md`](biq.md), "The
compressor").

### 2.2 Header (loader `0x592100`)

| offset | size | content |
|---|---|---|
| 0 | 5 | `"CIV3"` and a NUL; compared with `repe cmpsb`, count 5 (`0x5921B9`); failure is load error 2 |
| 5 | 1 | one byte the loader steps over (`0x1A` in the corpus) |
| 6 | 4 | **version**, stored in `[0xA32BC4]`; below 14 is load error 3 (`0x5921CD`). The corpus has 24 |
| 10 | 4 | **sub-version**, stored in `[0xA32BC8]`; only read when version >= 17 (`0x5921DC`). `<= 0` is taken as is, **1 is load error 3**, 2 and above are accepted (`0x5921F0`). The corpus has 10 |
| 14 | 16 | GUID, only when sub-version >= 7 (`0x592201`); older saves get a fresh one from `CoCreateGuid` |
| 14 or 30 | | the `game_data` stream |

After the header the loader calls `game_data(stream, 0)` (`0x590030`, from
`0x59228F`); a failure is load error 4. The writer calls the same routine from
`0x591D89` after storing 24 and 10 (`0x591CCC`, `0x593F00`). The Rust
constants are `sav::VERSION = 24`, `sav::SUB_VERSION = 10`.

## 3. The two primitives

Everything in the stream is one of these.

**Chunk**: `tag[4]  size:u32  body[size]`. A chunk object (constructor
`0x4FCB20`, vtable `0x668F48`) holds a FOURCC (`[this+8]`, uppercased by
`mmioStringToFOURCC`), a file size (`[this+0xC]`), and a memory range
(`[this+0x14]`, `[this+0x18]`). `0x4FCAD0(this, start, last, extra)` sets
them: the file size is `last - start + extra` (the three arguments are the
address of the first byte, the address of the last dword, and the size of that
last dword, 4 in every use here), and it refuses a null argument or
`start > last`. The memory range is therefore `[start, last + extra)`, which is
what the "object range" columns below give.
`0x4FCAB0(this, save_flag, buffer)` dispatches to the saver `0x4FCB60`
(writes `tag, size, memcpy(range)`, returns `size + 8`) or the loader
`0x4FCBB0`.

The loader (`0x4FCBB0`) behaves as follows, and the Rust reader is stricter:

* a chunk object whose range is unset or empty does nothing and returns 0, and
  every caller treats 0 as failure (optional pieces are therefore gated by the
  caller, never by an empty chunk);
* a tag that differs from the expected FOURCC logs `Fourcc mismatch in
  readData` and returns 0, which aborts the load;
* it copies `min(size in the stream, expected size)` bytes into the object and
  advances by `size in the stream + 8`. The exe therefore tolerates a chunk
  that is larger or smaller than expected (this is how older saves with
  different object layouts load). The Rust reader requires the exact size and
  returns `Error::BadChunkSize` otherwise.

**Raw block**: bytes with no header, copied to or from an array whose length is
known from other data. Lengths come from three places:

* the rule counts (below);
* counts stored in an earlier chunk (units, cities, colonies in `GAME`,
  citizens in `POPD`, the continent count in `WRLD`/`GAME`, ...);
* fixed numbers (256, 32).

An array whose length is zero is not read at all.

### 3.1 Rule counts

The raw arrays are sized by the rules the game is running, which are the
tables of the **embedded BIQ** (section 5.1). Globals: buildings
(`BLDG` rows) `[0x9C3D80]`, unit types (`PRTO`) `[0x9C3DB0]`, advances (`TECH`)
`[0x9C3DBC]`, resources (`GOOD`) `[0x9C3DA4]`, spaceship parts (the length of
the `RULE` part array) `[0x9C72A8]`. A scenario without one of those sections
keeps the Conquests default:

| | `BLDG` | `PRTO` | `TECH` | `GOOD` | space parts |
|---|---|---|---|---|---|
| Conquests default | 83 | 141 | 83 | 26 | 10 |
| `yolo.SAV` (Sengoku) | 32 | 46 | 39 | 17 | 1 |
| TETURKAN family | 78 | 213 | 82 | 51 | 10 |
| RUS family | 37 | 48 | 50 | 19 | 10 |

Random-map games embed only `VER#` and `GAME` (a 8 081-byte `BICQ` 12.08), so
they use the default row. `RuleCounts::from_biq` implements this; pass
explicit counts to `Save::parse_with` for a game whose default rules are not
the shipped `conquests.biq`.

## 4. The stream, in order

Format 24. The gate column is on the sub-version `sub`; counts come from the
chunk named in the content column.

| # | segment | content | gate |
|---|---|---|---|
| 1 | `BIC ` 524 | u32 length `L` of the embedded BIQ, `char[260]` scenario directory, `char[260]` scenario file | |
| 2 | raw `L` | the embedded `BICQ` stream (a complete scenario file) | |
| 3 | `GAME` 848 | the `Game` object; then 9 raw arrays, five chunks and two dwords, see 5.2 | block version 5 |
| 4 | `CNSL` 228 | console object `0x9F8C74` | |
| 5 | map | 5.4 | before the players when `sub >= 9`, else after |
| 6 | 32 players | 5.5 | `listA` `sub > 2`, `listB` `sub >= 6` |
| 7 | `UNIT` records | `[GAME+0x18]` of them: `UNIT` 472 and, for record version >= 2, `IDLS` 8 + dwords | |
| 8 | city records | `[GAME+0x1C]` of them, 5.7 | `DATE` in a city when `sub >= 4` |
| 9 | `CLNY` 16 | `[GAME+0x20]` records (none in the corpus) | |
| 10 | raw 256 | copied to `0xA94B20` | 264 bytes when `sub < 4` |
| 11 | 32 x `PALV` 148 | objects `0xB71288 + k * 0xB0` | |
| 12 | history | 5.9, `HIST` | |
| 13 | `TUTR` 92, `FAXX` 88 | objects `0xC9C440`, `0xA46F38` | |
| 14 | replay | 5.9, `RPLS` | |
| 15 | network queue | 5.9, `FNetQueue` | `sub >= 1` |
| 16 | `PEER` 24 | object `0x74AF60` | `sub >= 1` |
| 17 | `AIBS`, `VLOC`, `RADT`, `OUTP` | `[GAME+0x12C]` x 20 B, `[+0x130]` x 16, `[+0x134]` x 16, `[+0x138]` x 16 | |

The stream ends there: the loader reads nothing after the last pool, and no
corpus file has trailing bytes. The Rust reader refuses trailing bytes
(`Error::Unsupported`) because they would not round-trip.

The map moves with the sub-version (gate at `0x590186`, and `cmp ecx, 9` at
`0x590250` for the late copy): sub-versions 9 and 10 write
`GAME`, `CNSL`, **map**, players, units; sub-versions below 9 write `GAME`,
`CNSL`, players, **map**, units.

## 5. Segment reference

Offsets in field tables are **body offsets** (from the first byte after the
chunk header) unless a `Cell+`/object offset is named. "Object" offsets are
body offset plus the range start given for the chunk.

### 5.1 `BIC ` and the embedded scenario

The `BIC ` chunk is 524 bytes: dword 0 is the length of the embedded BIQ,
bytes 4..264 and 264..524 are two NUL-terminated path buffers (scenario
directory and scenario file as the game recorded them; both empty for a
random-map game, while `yolo.SAV` has `"Conquests\7 Sengoku - Sword of the
Shogun\"` and `"Conquests\7 Sengoku - Sword of the Shogun.biq"`). The raw bytes
that follow are a full `BICQ` stream, parsed by `Biq::parse`
(`Save::embedded_biq()`); `biq-format.md` describes the sections. Sizes in the
corpus: 8 081 B (random map: `VER#` + `GAME` only), 114 609 B (Sengoku),
119 097 B (RUS), 225 500 B (TETURKAN).

The embedded BIQ is what makes a save self-contained: its tables are the
rules the loaded game uses, and its row counts size every per-building,
per-unit, per-tech and per-resource array below (3.1).

### 5.2 `GAME` and what follows

`GAME` 848 is `Game` (object `0xA52658`) bytes `+0x1C..+0x36C`; loader
`0x538960`, writer `0x538D30`.

| body | object | field | evidence |
|---|---|---|---|
| `0x08` | `+0x24` | flag word; `flags & 0x26000` adds a fifth array to every history record | L, C |
| `0x18` | `+0x34` | unit count = number of `UNIT` records | L, T |
| `0x1C` | `+0x38` | city count | L, T |
| `0x20` | `+0x3C` | colony count | L, T |
| `0x38` | `+0x54` | turn number (consecutive autosaves differ by exactly 1) | C |
| `0x124` | `+0x140` | continent count; length of the first array | L, T |
| `0x12C` / `0x130` / `0x134` / `0x138` | | counts of `AIBS` / `VLOC` / `RADT` / `OUTP` records | L, T |
| `0x320` | `+0x33C` | block version, `5`; selects how much follows | L |

The nine raw arrays that follow `GAME` (each read only if its length is
non-zero):

| # | length | element | meaning |
|---|---|---|---|
| 1 | continents | `u32` | cities on each continent (incremented at city founding `0x566861`, decremented at destruction `0x4AF110`; the sum is the city count in every corpus save) (C) |
| 2 | `TECH` | `u32` | **HYPOTHESIS**: a bit set of the player slots that know the advance (slot 0 never set; `510` = slots 1..8 for the starting advances of a nine-player game) |
| 3 | `BLDG` | `i32` | the city that holds the wonder, `-1` for none (consistent with array 4 in the corpus) |
| 4 | `BLDG` | `u8` | `1` once a wonder has been built |
| 5 | `BLDG` | `u32` | mask, identical in saves that share rules (**HYPOTHESIS**: cached bit set derived from the rules) |
| 6 | `BLDG` | `u32` | equals array 5 except for a few entries (**HYPOTHESIS**: array 5 plus runtime bits) |
| 7, 8 | `PRTO` | `u32` | masks, meaning open |
| 9 | `TECH` | `u32` | mask, meaning open |

Then, in order: `DATE` 84, `PLGI` 4, `PLGI` 8, `DATE` 84, `DATE` 84, a raw
`u32` (`Game+0x36C`: `500` in the RUS family, `0` elsewhere) and a raw `u32`
(`Game+0x4E8`: `2` everywhere).

`DATE` 84 is a date object: the first 19 dwords are uninitialised heap in
most saves, body `+0x4C` is the year as an `i32` (`-4000` = 4000 BC), body
`+0x50` is always `1`. The first `DATE` is the game date, which **lags the
history by one turn**: it equals the year of the previous history record in
the TETURKAN, RUS and EGYPT saves, and holds the default `-4000` (or a stale
value, `1456` in one TETURKAN turn-0 autosave) at turn 0. The current year is
the year of the last history record (5.9), which is also the year a mid-game
autosave is named after (`Save::year()`). The second `DATE` is `-4000` in the
whole corpus; the third equals the first in the RUS family (after turn 0) and
is `-4000` elsewhere. `PLGI` is all zero in the corpus (HYPOTHESIS: mod /
plug-in record).

### 5.3 `CNSL`

`CNSL` 228, object `0x9F8C74`: the message/console state. Not decoded; the
first dword is 1 or 2 and the second is `-1` or a small count in the corpus.

### 5.4 The map (loader `0x5D9580`, writer `0x5D9410`)

`Map` object `0x9C7560` (base `0x9C736C`). Order:

1. `WRLD` 2 = `Map+0x230`: `u16` continent count. Zero makes the loader
   derive one; that path is not modelled (`Error::Unsupported`).
2. `WRLD` 164 = `Map+0x150..0x1F4`: the same 41 dwords as a scenario `WMAP`
   row after its resource rolls ([`biq-format.md`](biq-format.md)). Known
   offsets: `+0x00` land continent count, `+0x04` height, `+0x08` start-site
   radius, `+0x0C` number of players, `+0x18` width, `+0xA0` wrap flags. The
   oceans percentage at `+0x9C` is not meaningful in a save (uninitialised
   in random-map games).
3. `WRLD` 52: further map state (format version >= 13, not decoded).
4. `width / 2 * height` cells in index order `(width / 2) * y + x / 2`
   (`x + y` is even on the isometric grid; an odd sum names the cell of
   `x - 1`, `0x5DC1C0`). Each cell is four chunks, named here by the offset in
   the `Cell` object (stride `0xF0`, [`NOTES.md`](NOTES.md) section 4) they
   start at:

   | chunk | `Cell` bytes | content |
   |---|---|---|
   | `TILE` 36 | `+0x04..+0x28` | the fields a scenario `TILE` row has (below) |
   | `TILE` 12 | `+0x28..+0x34` | overlay plane `+0x28`, terrain word `+0x2C`, feature plane `+0x30` |
   | `TILE` 4 | `+0x34..+0x38` | not decoded |
   | (12 raw bytes) | | only when `sub < 8` and the signed depth byte `Cell+0x20` is `>= 6`: skipped by the loader (`add ebp, 0xC` at `0x5D9C98`) |
   | `TILE` 128 | `+0x58..+0xD8` | per-game runtime state |

5. `CONT` 8, one per continent (the `WRLD` 2 count).
6. A raw array of one `u32` per `GOOD` row (`Map+0x14C`, `0x5D971A`).

Decoded fields (C, and cross-checked: every city is on a tile whose city id
is that city, and the tile's owner is the city's owner, a bijection in all
22 saves):

| `Cell` | size | field | note |
|---|---|---|---|
| `+0x04` | 1 | river connection mask | eight-direction set |
| `+0x05` | 1 | owner: the civilization whose border covers the tile, 0 = none | |
| `+0x08` | 4 | resource (`GOOD` row, `-1` none) | |
| `+0x1A` | 2 | city id, `-1` none | |
| `+0x1C` | 2 | colony id, `-1` none | |
| `+0x1E` | 2 | continent id | |
| `+0x20` | 1 | water depth (signed) | gate for the legacy bytes above |
| `+0x28` / `+0x2C` / `+0x30` | 4 each | overlay / terrain word / feature plane | terrain id = `(word >> 12) & 0xF` ([`biq-format.md`](biq-format.md)) |
| `+0x58` | 4 | claim mask: bit set of civilizations, the owner's bit is always set (**HYPOTHESIS**: the borders that reach the tile) | |
| `+0x6C` | 2 | id of the city that works the tile, `-1` none (a city within the 21-tile radius, possibly of another civilization than the tile's owner) | [`yields.md`](yields.md) |

### 5.5 Players (loader `0x5595E0`, writer `0x559040`)

32 slots; slot 0 is the barbarians. `Player` objects start at `0xA52E98`,
stride `0x20E4`. One slot is:

1. `LEAD` 5532: object `+0x1C..+0x15B8`, so object offset = body + `0x1C`.
2. 32 lists, each `u32 n` then `n` x 12 bytes (empty in the corpus).
3. When the in-use byte (body `0x1198`) is set, the raw tables below, in this
   order:
   * `BLDG` x `u16`, three times; `BLDG` x `u32`; `BLDG` x `u8`;
   * `PRTO` x `u16`, three times; space parts x `u16`;
   * `GOOD` x 96 bytes: for each resource, 32 three-byte records, one per
     civilization (`Player+0x1614[(good * 32 + civ) * 3]`; both of the first
     two bytes non-zero means the civilization supplies the resource);
   * `GOOD` x `u8`.
4. Five arrays of `n` dwords, `n` = body `0x180` (object `+0x19C`); `n` is
   zero in the corpus.
5. `CULT` 16, two `ESPN` 32, then a counted dword list (`u32 n` + `n` dwords).
6. `sub > 2`: 32 lists of 12-byte items ("listA"); `sub >= 6`: another 32
   ("listB"). Both are empty in the corpus.
7. A dword tail whose length follows the player-block version (body `0x1594`,
   `4` in the corpus): 0, 4, 6, 8 or 9 dwords for versions 0..4.

The in-use byte is set for slot 0 (the barbarians, race row 0) and for every
civilization of the game (3 slots in the two-player random games, 19 in
`yolo`, 32 in the TETURKAN family); an unused slot has race `-1` and no raw
tables. The history's player count `m` (5.9) is the number of in-use slots
minus one.

`LEAD` fields (C):

| body | object | field |
|---|---|---|
| `0x04` | `+0x20` | `RACE` row of the civilization, `-1` unused |
| `0x10` | `+0x2C` | capital city id, `-1` none (the city belongs to the player) |
| `0x28`, `0x2C` | `+0x44`, `+0x48` | two shares of the treasury; gold is their sum ([`economy.md`](economy.md)) |
| `0x84` | `+0xA0` | `GOVT` row of the government ([`government.md`](government.md)) |
| `0x180` | `+0x19C` | length of the five arrays in step 4 |
| `0x1198` | `+0x11B4` | in-use byte |
| `0x1594` | `+0x15B0` | player-block version |

The loader has a branch for `sub == 0` at `0x5595F2` and a "fixing corrupt
saved game" repair for a negative counter at `Player+0x18C` (`0x590200`);
neither is modelled.

### 5.6 Units (loader `0x5CD0A0`)

Records are in ascending id order; ids of units that no longer exist leave
gaps. A record is `UNIT` 472 (object `0x404` bytes, vtable `0x66DCF0`, object
`+0x20..+0x1F8`, so object offset = body + `0x20`) and, when the record
version (body `0x1D0`, object `+0x1F0`, `2` in the corpus) is at least 2, an
`IDLS` 8 chunk (dword 0 = `1`, dword 1 = slot count) followed by that many
dwords. The `IDLS` sub-object sits at object `+0x1F8` and is read through its
own vtable.

| body | object | field | evidence |
|---|---|---|---|
| `0x00` | `+0x20` | unit id | C |
| `0x04`, `0x08` | `+0x24`, `+0x28` | tile x, y | C (on the map, `x + y` even) |
| `0x0C`, `0x10` | `+0x2C`, `+0x30` | previous x, y (`-1` before the first move) | C |
| `0x14` | `+0x34` | owning player slot | C |
| `0x20` | `+0x40` | `PRTO` row of the unit type | C |
| `0x24` | `+0x44` | experience level, 0..=3 | C |
| `0x28` | `+0x48` | status bits; bit 2 is **HYPOTHESIS** "attacked this turn" ([`combat.md`](combat.md)) | |
| `0x2C` | `+0x4C` | damage taken | C |
| `0x30` | `+0x50` | movement spent this turn | C |
| `0x44` | `+0x64` | current order (`1` is fortified; the enumeration is not decoded) | C |
| `0x1D0` | `+0x1F0` | record version | L |

Every unit of the corpus has ten `IDLS` slots, each a unit id or `-1`
(**HYPOTHESIS**: a carried-unit list: one example has a transport whose first
slot names a unit).

Format versions below 24 use a different unit layout (the branch at
`0x5CD10E`, then `0x5CD21C` with its raw copy); only 24 is modelled.

### 5.7 Cities (loader `0x4BBED0`, object size `0x544`)

Records are in ascending id order. The `City` object is saved as five disjoint
ranges, the citizen list, a per-building array and the improvement bit set:

| chunk | size | object range | notes |
|---|---|---|---|
| `CITY` | 136 | `+0x20..+0xA8` | id, position, owner (below) |
| `CITY` | 16 | `+0xCC..+0xDC` | |
| `CITY` | 36 | `+0xF4..+0x118` | |
| `CITY` | 164 | `+0x13C..+0x1E0` | `+0x140 + 4 * civ` is the per-civilization culture stake ([`capture.md`](capture.md)) |
| `CITY` | 148 | `+0x1E0..+0x274` | `char[24]` name at the start |
| `POPD` | 8 | | dword 1 = citizen count = city size (object `+0x138`) |
| `CTZN` | 300 | | one per citizen, not decoded |
| `BINF` | 4 | | |
| raw | `BLDG` x 12 | | 12 bytes per building row (`0x4BCC0F`), not decoded |
| `BITM` | 40 | | improvement bit set |
| `DATE` | 84 | | `sub >= 4` |
| `CITY` | 8 | | dword 1 = city record version (object `+0x370`; `4` in the corpus) |
| `CITY` 4 + n x `CITY` 4 | | | version >= 2; the first dword is the count `n` |
| `CTPG` 4, `CTPG` 16 | | | version >= 3 |
| `CITY` | 4 | | version >= 4 |

Decoded fields (C): body `0x00` (object `+0x20`) id, the value `Cell+0x1A`
holds; `0x04` (`+0x24`) `u16` x; `0x06` (`+0x26`) `u16` y; `0x08` (`+0x28`)
owner byte, followed by three uninitialised bytes. The name is the NUL-
terminated string at the start of the `+0x1E0` chunk (Windows-1252, up to
23 characters).

### 5.8 Colonies, the reserved block, `PALV`

`CLNY` 16 records (airfields, radar towers, outposts and plain colonies,
**HYPOTHESIS** from the tile overlay bits) have no instance in the corpus; the
size is the loader's range size. The 256-byte block after the cities is
copied to `0xA94B20` (264 bytes when `sub < 4`); it is not decoded (all
corpus values are defaults). The 32 `PALV` 148 chunks are objects at
`0xB71288 + k * 0xB0`; they are mostly `-1` words in the corpus and not decoded.

### 5.9 Turn history, replay, network, pools

**History** (`HIST`, loader `0x542450`, object `0xB38C60`). Not chunk framed:

```text
u32 tag        'HIST'; never read by the loader
u32 n          record count
u32 x          bit set of player slots 1..=m  (2^(m+1) - 2)
n records: u32 a, u32 b, u32 m, 4 arrays of m dwords, plus a fifth array
           of m dwords when GAME flags & 0x26000
```

One record per turn: `a` is the turn number (the records are `0..=turn`
in every save, C), `b` the year of that turn as an `i32`, `m` the number of
tracked players (slots `1..=m`; `m` = 18 in `yolo`, 31 in the TETURKAN
family, 8 in RUS and EGYPT, 2 in the two-player random games). Series 0 is the
slot number itself (`1..=m`); series 2 and 3 never decrease from record to
record (cumulative counters), series 1 does (a current value). **HYPOTHESIS**:
series 3 is accumulated culture (in `yolo` every player has one palace and
series 3 grows by exactly one per turn). The year of the last record is the
current game year.

**Replay log** (`RPLS`, object `0xC88588`, loader `0x58B400`):

```text
u32 tag          ignored
u32 n            turns
n x { RPLT 5, u32 k, k x { RPLE 10, NUL-terminated string } }
```

Turn loader `0x58AE50`, event loader `0x58AD10`. `RPLE` 10 is a ten-byte
event header (not decoded) followed by the event's text (the first event of
`TETURKAN.SAV` has `"Suez Canal"`); a lone NUL is the empty string. `TETURKAN.SAV` has 72 turns and 3 517 events.

**Network queue** (`FNetQueue`, object `0x74D0CC`, loader `0x4842E0`):
`u32 n` then `n` x (`u32 len`, `len` bytes). Empty in the corpus.

**`PEER`** 24 (object `0x74AF60`), **`TUTR`** 92 (`0xC9C440`), **`FAXX`** 88
(`0xA46F38`): fixed objects, not decoded (defaults in the corpus).

**Pools after `PEER`**: `AIBS` 20, `VLOC` 16, `RADT` 16, `OUTP` 16, counted in
`GAME` (`0x12C..0x138`). Only `VLOC` ever occurs (zero records in the corpus).
The tag names are the only evidence for what the others hold.

## 6. Versions and gates

Two version numbers exist. The **sub-version** gates optional pieces and is
fully modelled for 2..=10; the **format version** (24) changes the layout of
whole objects and is only supported at 24. 21 instructions reference the
format version `[0xA32BC4]`; the xref scan reports the address of the memory
operand, one or two bytes into the instruction:

```text
0x4BC39D 0x4BCB01                                    city
0x53897C 0x5389A8 0x538BD3                           game
0x5596AE 0x55989E 0x559B14 0x559F74                  player
0x590062 0x590DA1 0x590E10 0x590E67                  game_data
0x591CCE 0x593F02 0x5921E0 0x593433                  writer, header
0x5CD10E 0x5CD21C                                    unit
0x5D95DB 0x5D9BD1                                    map
```

Reading any older version therefore needs a fresh reading of every object, not
a table.

Sub-version gates (`[0xA32BC8]`):

| sub | address | effect |
|---|---|---|
| `>= 1` | `0x590EBE`, `0x590F0B` | the network queue and `PEER` are present (code reading only) |
| `== 0` | `0x5595F2` | a different player loader (not modelled) |
| `> 2` | `0x559C9D` | player "listA" (32 lists) |
| `< 4` | `0x590C6F` | the reserved block is 8 bytes longer |
| `>= 4` | `0x4BC34C` | a `DATE` chunk in every city |
| `>= 6` | `0x559D1B` | player "listB" |
| `>= 7` | `0x592201` | the 16-byte GUID in the header |
| `< 8` | `0x5D9C8F` | twelve skipped bytes after `TILE` 4 for deep water |
| `>= 9` | `0x590186`, `0x590250` | the map precedes the players |
| `== 1` | `0x5921F0` | rejected |

Layouts for 8, 9 and 10 differ only in the map position at 9.
`Save::with_sub_version(sub)` rewrites a parsed save for another
sub-version (adding the gated pieces zero-filled); it exists to produce the
synthetic files of section 7 and is not something the game wrote.

Not supported (an `Error::Unsupported` or a framing error, never a panic):
format versions other than 24, sub-versions 0 and 1, a map with zero
continents, a `GAME` block version other than 5, trailing bytes, and the
loader's corrupt-save repair.

## 7. How the grammar was verified

The grammar was not transcribed by hand. The exe is a 32-bit x86 PE, so
`tools/emu/` maps it into Unicorn, stubs the Win32 imports and the CRT
allocator, runs the C++ static initialisers, puts a decoded save in memory and
calls the exe's own `game_data(stream, 0)`. Three hooks record the trace:

* every call of the chunk loader `0x4FCBB0`: offset, tag, size;
* every read of the input buffer by an instruction, coalesced into raw runs
  with the address of the reading instruction (this attributes raw blocks);
* every allocation.

Two checks then compare against the independent Python spec
(`tools/emu/savespec.py`, written from the loader code):

1. **Chunk-list identity.** The (offset, tag, size) sequence of the trace
   equals the spec's, and the spec ends at exactly the last byte. Result on the
   22 corpus saves: identical. The same on eight synthetic downgrades of
   `yolo.SAV` (`sav yolo.SAV --sub N OUT`, N = 2..9): the loader accepts all of
   them and consumes them to the last byte, with the same chunk lists, which
   fixes every gate in section 6 except the `>= 1` and `== 0` rows, which
   these files do not reach (code reading only).
2. **Byte-exact round trip.** `Save::parse` then `Save::to_bytes` reproduces
   all 22 files (including DCL recompression). Hostile inputs (truncations,
   byte flips, huge counts under a counting allocator, wrong versions, a
   scenario passed as a save) return an error without panicking.

Field meanings come from separate evidence: the accessors in section 5 are
checked across the corpus (`sav::tests::decoded_fields_are_consistent`:
unit positions on the grid with types and experience in range; every city on a
tile that names it back with the same owner, and no other tile naming a city;
worked-by tiles within the city radius; per-continent city counts summing to
the city count; wonder flags agreeing with their city index; players' race,
government and capital ownership; `HIST` having `turn + 1` consecutive records
over slots `1..=m` with the matching mask).

Reproducing (`pefile`, `unicorn` and `capstone` for Python; the GOG install
under `civ3/`):

```sh
cd biq
cargo run --release --example sav  -- SAVE.SAV --stream /tmp/s.raw
cargo run --release --example dump -- ../civ3/civ3-gog/app/Conquests/conquests.biq --stream /tmp/conq.raw
cd ../reverse-engineering/tools/emu
CIV3_DEFAULT_BIQ=/tmp/conq.raw python trace.py /tmp/s.raw /tmp/s.pkl   # runs the exe's loader
python savespec.py /tmp/s.raw /tmp/s.pkl   # "chunks OK ... EOF OK"
```

The emulated start-up logs one harmless failure, static initialiser #47
(`0x4A8450`), and one `eip=0x49F3D7` read error; the load does not depend on
them. See [`tools/emu/README.md`](tools/emu/README.md).

## 8. Corpus

22 saves from the GOG install and the "complete" Conquests tree (the git-
ignored `civ3/`; override with `CIV3_DIR`). All are format 24.10.

| family | files | size | map | turn | stored |
|---|---|---|---|---|---|
| random map, Conquests rules | `12345`, `1234`, `chaosnas156 ...`, `fanghenggang1987 ...`, `1111110 BC`, `Three players ` | 42-43 KB | 100x100 | 0-8 | DCL |
| Sengoku scenario | `yolo` | 96 KB | 140x132 | 3 | DCL |
| TETURKAN | `TETURKAN` and six `Conquests Autosave` | 0.5 MB (DCL) / 9-10 MB (plain) | 256x256 | 0, 68-72 | both |
| RUS (Middle Ages) | `RUS` and six `Conquests Autosave` | 0.2 MB (DCL) / 2.0-2.5 MB (plain) | 110x120 | 0, 32-36 | both |
| PTW-era Egypt | `EGYPT` | 113 KB | 100x100 | 143 | DCL |

The largest streams are 10 MB (up to 2 091 units and 278 cities). No save with
colonies, a non-empty network queue, non-empty player lists, or `AIBS`,
`RADT` or `OUTP` records exists, so those shapes rest on the loader's range
sizes only. An autosave is named after the year of its last history record
(5.2, 5.9).

## 9. Open items

| area | open |
|---|---|
| `GAME` body | every field beyond those in 5.2; the meaning of the nine arrays after it (masks) and of `DATE` +0x50, `PLGI`, `Game+0x36C` (`500` in the RUS family), `Game+0x4E8` |
| `LEAD` body | everything except the eight fields in 5.5; what clears the in-use byte (in the corpus only slots with race `-1` have it clear) |
| lists | what the 12-byte items of the player lists, listA and listB hold (always empty); the five dword arrays (length zero) |
| `UNIT` | status bits, the order enumeration, the `IDLS` slots (HYPOTHESIS: cargo), most of the 472 bytes |
| `CITY` | every block except id, position, owner, name and size; the `CTZN`, `BINF`, `BITM` and per-building contents; the two `CITY` 4 lists and `CTPG` |
| `TILE` | `Cell+0x34`, most of the 128-byte runtime chunk (visibility and knowledge masks are the likely content), the twelve skipped legacy bytes (the loader never reads them) |
| `HIST` | what the four (five) series count; `x` beyond its mask meaning |
| pools and objects | `AIBS`/`RADT`/`OUTP`/`CLNY` records, `PALV`, `TUTR`, `FAXX`, `PEER`, `CNSL` |
| versions | any format version other than 24; sub-versions 0 and 1; real files of sub-versions 2..9 (only synthetic ones were run) |
| loader side effects | the post-load repairs and cache rebuilds that make a loaded game consistent are not part of the stream and are not recorded here |

## 10. Rust map

| file | content |
|---|---|
| `sav/mod.rs` | `Save`, `Header`, the stream order, `with_sub_version`, `year()` |
| `sav/stream.rs` | the reader cursor and the chunk / raw-block primitives with checked sizes |
| `sav/counts.rs` | `RuleCounts` and the default row |
| `sav/game.rs` | `Game`, `History`, `Replay` |
| `sav/world.rs` | `Map`, `Tile` and the `Cell` accessors |
| `sav/objects.rs` | `Player`, `Unit`, `City`, the pools and the network queue |
| `sav/body.rs` | `Body<N>`, a fixed byte array with offset accessors |
| `sav/tests.rs` | round trip of every save, field consistency, sub-version round trips, writer refusals, corrupt inputs |

```rust
use civ3_biq::Save;

let save = Save::read_file("TETURKAN.SAV")?;
println!("turn {}, year {}", save.game.turn(), save.year());
for c in &save.cities {
    println!("{} owner {} size {} at ({}, {})", c.name(), c.owner(), c.size(), c.x(), c.y());
}
let bytes = save.to_bytes()?;                     // identical to the file
```

`cargo run --release --example sav -- FILE [--check | --cities | --biq OUT |
--stream OUT | --sub N OUT]` prints an overview (map, counts, one line per
player slot) or exports the pieces. Tests: `cargo test --release` in `biq/`
(they skip when the corpus is absent).

## 11. Address index

| address | what |
|---|---|
| `0x590030` | `game_data(stream, save_flag)`: the stream order |
| `0x592100` | save loader: magic, version, sub-version, GUID, calls `game_data` |
| `0x591CCC`, `0x593F00` | writer stores version 24 / sub-version 10 |
| `0x4FCAB0` / `0x4FCAD0` / `0x4FCB20` / `0x4FCB60` / `0x4FCBB0` | chunk dispatch / set range / constructor / save / load |
| `0x538960` / `0x538D30` | `Game` load / save |
| `0x5595E0` / `0x559040` | `Player` load / save |
| `0x5CD0A0` | `Unit` load |
| `0x4BBED0` | `City` load |
| `0x5D9580` / `0x5D9410` | `Map` load / save |
| `0x542450` | history (`HIST`) load |
| `0x58B400` / `0x58AE50` / `0x58AD10` | replay / replay turn / replay event load |
| `0x4842E0` | `FNetQueue` load |
| `0x590200` | "Fixing corrupt saved game" repair |
| `0xA32BC4` / `0xA32BC8` | format version / sub-version globals |
| `0x9C3D80` / `0x9C3DB0` / `0x9C3DBC` / `0x9C3DA4` / `0x9C72A8` | rule counts: `BLDG`, `PRTO`, `TECH`, `GOOD`, space parts |
| `0xA52658` / `0xA52E98` | `Game` object / first `Player` object (stride `0x20E4`) |
| `0x9C7560` | `Map` object (`Cell` stride `0xF0`) |
