# City removal and replacement capitals

Native `City::remove` is `0x4AECC0`, called after population-cost production
empties a city (`city-turn.md` 7), destruction during capture and razing
(`capture.md`). Capital replacement is `Player::capitalLost` `0x4482B0`.
The observations below are from opened raw disassembly, marked **V**;
unidentified side effects remain explicit gaps.

## Replacement capital (`0x4482B0`) V

Scan occupied city-pool ids in ascending order, skipping the departing city
argument and any city whose owner is not this player. Start best score at
zero and replace the winner only for a strictly larger score.

For each candidate:

1. Start with population plus twice the count of owner-race citizens, plus
   `0x5A6060(x,y,4,-1,0,-1)` military count (`0x448338..0x448367`).
2. Visit spiral indices **1..288**, stopping before 289 (`0x448424`). Fold x/y
   once at the configured wrap edges and skip invalid coordinates. At each
   tile containing an owned city other than the departing city, add 1 for
   population <= town maximum, 2 through city maximum, and 3 above it
   (`0x4483D7..0x44841F`). Shipped thresholds are 6 and 12. Do not deduplicate
   wrapped positions. The loop is much larger than a worked-city radius.
3. If a winner exists, register it with `0x55CB20`, then add the first BLDG
   row with Center of Empire (flag 1) via `0x4ACF40(city,id,1,0)`
   (`0x448464..0x4484A1`). Otherwise register a null capital (`0x4484AE`).

Mode 4 with these arguments requires PRTO class 0 and positive native attack
or defense (`0x5A61FE..0x5A6259`). There is no owner restriction because the
owner argument is -1. `0x5BE6E0`/`0x5BE820` are army-aware attack/defense;
the non-army tails read PRTO attack +0x60 and defense +0x58. This mode's
filters are now identified beyond the older hypothesis in `happiness.md`.

`rust/src/capital.rs` implements the score. `src/capital.rs` translates the
spiral into the clone's map coordinates, selects a replacement for a missing
or no-longer-owned capital, and installs the Palace. A valid capital stays
put. Explicit Palaces replace the clone's implicit Palace in culture and
city-screen display, avoiding a duplicate bonus or improvement row.

Native selection order is occupied city-pool id. The game currently keeps
first ties in its City query order; a persisted native city-id/free-list
migration remains separate work. The shipped map wraps x and does not wrap
y. This does not claim complete native city-pool or map-option fidelity.

## Removal observations and remaining work

Opened `0x4AECC0` through its trade/network tail:

- `0x4AED5A..0x4AED6F` removes the remaining population with race -1 and
  flag 0, through the shared native victim routine. Population-production
  abandonment reaches it at population zero.
- `0x4AED74..0x4AEDC9` decrements the old production's player bookkeeping.
- `0x4AEDCC..0x4AEE77` clears cooldown-related bookkeeping and references
  in player/other-city records. Exact meanings of those references remain
  unread.
- `0x4AEE8B..0x4AEEBC` invokes capital replacement when this city's id is
  the owner's capital, before removing every installed building in BLDG
  row order (`0x4AEECC..0x4AEEEE`, `city-buildings.md`).
- `0x4AEF0D..0x4AEF18` clears the center's city reference through cell
  vtable slot +0xE4. The 21-position spiral clears worked-by +0x6C when
  it names this city (`0x4AF091..0x4AF0CE`). A conditional +0xCC clear of
  group-2 mask 0x20000 inside this loop needs identification.
- `0x4AF0D4..0x4AF19B` updates owner/continent and global city counters,
  destroys the pool record, links its id onto the LIFO free head, and
  rebuilds the trade network (`0x4AF1AD`).
- `0x4AF1B2..0x4AF255` clears road/rail overlay bits when the owner lacks
  their enabling technologies; `0x4AF260` recomputes tile ownership.
  Later redraw/reach-set calls remain only partly identified.

The local-human ABANDONBASE path is now integrated in `src/abandon.rs`.
Accept creates the unit before native population payment, sets its nationality,
records the final foreign race's razed-city counter, removes the empty City and
its sprite/label roots, releases live worked-tile claims and clears city dialogs.
Capital, border, visibility, trade and wonder watchers observe removal next pass.
The center's road survives: stock Road enables at advance -1, which is always
known. Native tile +0x24 is set to ruin id 1 (`0x4AF14B`, setter `0x5EACB0`);
ruin persistence and map art remain unimplemented. Radius flag group 2 bit
0x20000 is rebuilt for all surviving cities later in the native removal tail;
the clone derives claims from live cities instead of storing this cache.

This does not implement the complete capture/raze destructor, native persisted
city ids/free-list ordering, railroad cleanup, reach-set bookkeeping or network
multiplayer suppression. AI's native immediate production reselection remains
pending; it never receives the human abandonment choice.

## Verification (2026-10-04)

615 native library tests, 22 integration tests, two doctests and 490 game
tests pass, as does the game build. Release clippy reports the ten existing
native-library warnings and none in `capital.rs`. Two native score tests
cover nationals/garrison versus raw size and strict class thresholds. Three
game regressions cover first ties, ownership loss/removal, unchanged valid
capitals, installed Palace culture and seam/neighbor scoring.

Actual F8/F5/F8 checks replace a missing capital with Home (population 3,
three owner nationals and two land military units: score 11) rather than
Foreign (population 10, no owner nationals or military: score 10). The
cities are outside each other's 288-position vicinity. The saved capital
names Home and contains exactly one actual Palace row. Rendered evidence
`/tmp/open4x-capital-render-final.png` shows one Palace and culture 1; the
new capital and Palace survive reload. This fixture tests replacement after
capital loss, not an entire combat/city-destruction sequence.

## Abandonment verification (2026-10-04)

Behavior regressions cover cancel/zoom preserving citizens, food, shields and
RNG; Worker/Settler acceptance; unit nationality and the final foreign-race
counter; stale choices; graphics cleanup; replacement capital; surviving road;
and the human nongrowing versus growing/AI gate. Save/load clears stale dialog
roots and pending choices without changing save version 10.

Actual F8/end/F5/F8 fixtures remove a size-1 Dutch city producing Worker and a
size-2 city producing Settler. Each adds exactly one owner unit, removes the
city/capital, retains the center road, and persists the result. Seed 1 reaches
1103527590 after one payment draw (Worker) and 2524885223 after two (Settler).
Rendered `/tmp/open4x-abandon-{worker,settler}-220.png` show no remaining city
sprite or label after reload. Cancel leaves citizen words, box 10, food 10 and
RNG 1 intact; Zoom opens the size-1 city screen with the same state. The initial
Monarchy fixture correctly waited because the Dutch agricultural bonus supplied
positive food surplus; the nongrowing acceptance fixtures use Despotism.
