# Unit stacks: the per-tile unit list

Owns: **which unit the engine treats as "the unit on a tile"**, and the ordering
rule behind it. Reference: `rust/src/stack.rs`. The map's sprite z-order is a
separate, view-side mechanism — see [Open](#open) for what is and is not
pinned there.

## The structure (verified)

Civ3 keeps **one singly-linked list per tile**, stored in an index pool, with
the list head kept in the `Cell`:

```
Cell::vfunc(0xA0)  = 0x5EAA90   mov eax,[ecx+0xc]      ; +0x0C = list head node index, -1 = empty
Cell::vfunc(0x104) = 0x5EAD10   mov [ecx+0xc], ...     ; the same field's setter
```

The pool is the global container object at **`0xA52DD4`** (its first dword is
the vtable `0x66CAEC`, installed at `0x537CDA`; the object is destroyed at
`0x537D10`):

| container + | global | meaning |
|---|---|---|
| `+0x04` | `0xA52DD8` | node array; 8 bytes per node, `{i32 next, u32 unit}` |
| `+0x08` | `0xA52DDC` | free-list head (index, `-1` = empty); the freed node's `next` is the link |
| `+0x0C` | `0xA52DE0` | free count |
| `+0x10` | `0xA52DE4` | highest slot index handed out (`-1` before the first allocation) |
| `+0x14` | `0xA52DE8` | slot capacity (`0x64` = 100 at init; the grow trigger is `cap == capacity − 1`) |
| `+0x18` | `0xA52DEC` | out-of-range return value, **`-1`** |

The node's `unit` field is **not** a pointer: it is the unit's id, i.e. its
index in the unit pool (`0xA52E80`, entries `0xA52E84`, count `0xA52E90`), whose
entry `+0x04` holds `unit + 0x1C` (`container_of`, so the unit object is
`entry - 0x1C`). The unit's own copy of that id is at **`unit+0x20`** — that is
the value the mover writes into the node, and the value the walkers compare.

Node fetch helper `0x426C80(container, index, i32 *out_next)`:

```c
if (index < 0 || index > container->count) { *out_next = -1; return container->default; } // -1
*out_next = node[index].next;
return node[index].unit;
```

## The ordering rule: new placements go to the FRONT (verified)

The only runtime mutation of the pool is inside `Unit::setPosition`
(**`0x5BD220`**, 4 682 bytes; Ghidra-clean). No other function writes the pool
globals except the constructors/destructors and the two scenario-load resets
(see [Verified negatives](#verified-negatives)).

The function stores the new tile into `unit+0x24/+0x28` and the old tile into
`unit+0x2C/+0x30`, unlinks the unit from the **old** tile's list, and links it
into the **new** tile's list:

```c
// 0x5BD6DD-0x5BD763 (new tile; x,y from +0x24/+0x28)
cell      = getCell((W>>1)*y + (x>>1));
headField = cell + 0x0C;
unitId    = unit[0x20];
head      = cell->vfunc(0xA0)();
if (nodes == NULL) { *headField = -1; return; }              // 0x5BD6E7
if (cap == capacity - 1 && freeCount == 0) grow(0x4C22C0);   // policy unread
if (freeCount > 0) { idx = freeHead; freeHead = node[idx].next; freeCount--; }
else               { idx = (cap == -1) ? 0 : cap + 1; cap = idx; }
node[idx].next = head;      // >>> the new node points at the previous head
node[idx].unit = unitId;
*headField     = idx;       // >>> and the tile now heads at the new node
```

Removal (`0x5BD3C5`-`0x5BD4FA`) walks from the head, tracking `prev`, and on a
match unlinks (`prev == -1 ? *headField = node.cur.next : node[prev].next =
node.cur.next`), then pushes the node on the free list
(`node[cur].next = freeHead; freeHead = cur; freeCount++`).

Consequences, stated the way the rest of the engine reads them:

* **`Cell+0x0C` is the most recently placed unit on that tile.** Following
  `next` walks *backwards in time* — the tail is the unit that has held the tile
  longest (including the turn-1 settler/warrior that founded a city there).
* A unit that leaves and returns is re-inserted at the head, so "head" means
  "most recently placed", not "first to arrive".
* Order is **not persisted**. Both scenario/save inits wipe the list
  (`0x590305`, `0x59FDDD`) before units are placed, so the order after a load is
  the loader's placement order, not the pre-save order.

## Consumers — who reads the list, and from which end

| address | function | walks | picks |
|---|---|---|---|
| `0x56D340` | "foreign unit at tile?" (AI/UI helper) | head → tail | **first** unit passing `0x5BE6E0`/`0x5BE820` (+ optional `0x5BB650`, order gate `0x5BC8B0(0x11)`); returns an owner id |
| `0x4E7540` | give an order to the tile (stack-mate walk) | head → tail | every stack-mate matching the filters (`unit+0x44 == -1`, `+0x48 == 0`, same tile), each handed to `0x4DE6F0` |
| `0x4C0750` | **draw a row of the tile's units** into a rect | head → tail | all visible units, laid out at `rect.left + 0x20*k`, `0x5F82E0` blit per unit, `unit+0x1F4` art variant via `0x54C600` |
| `0x573B20` | draw **one** unit sprite for a tile rect | head → tail | **last** match — i.e. the tile's oldest resident |
| `0x4DBA70` | `MapView::setSelectedUnit` | (view's own sprite list, not the tile list) | selected unit at `view+0x4D74`, flag `unit+0x5C = 1` |

Both row/single draws skip units the viewer cannot see: `0x5B6630(unit)` tests
PRTO flag index `0x13` on the unit's own type **and** on the co-located unit's
type; the shared flag helper is `0x5E4EF0` (`index < 0x20` → row dword `+0x88`,
else row dword `+0x130`). `0x4C0750`'s callers pass the "hide invisible" mode.

`0x56D2C0` is the *other* per-tile occupant lookup and is easy to confuse with
this one: it reads `Cell+0x1A` (a **city** id, resolved through the city pool
`0xA52E6C`/count `0xA52E78`) — it is `city_at(x, y)`, not `unit_at`.

## The map is not drawn from the tile list (verified negative + what is known)

A sweep for readers of the tile list (`0xA52DD4`) finds **no map-view code**:
outside the AI, the order code and the two UI row draws above, the only sites in
the `0x4C`-`0x4F` range are the pool init at `0x4C8EF3` and the row draw at
`0x4C0750`. The map instead keeps **per-unit sprite objects**:

* `view+0x594`, stride `0x170`, `0x32` (50) slots, constructed/destroyed via
  `0x64A4AF(...,FUN_004EE490,FUN_004EE4D0)` in the view ctor/dtor
  (`0x4DF0D0` / `0x4DF360`) — the objects' vtable is `0x66B6E0`.
* a separate **doubly-linked list of unit ids** at `view+0x2E158` (head),
  `+0x2E15C` (tail), `+0x2E160` (cursor); nodes are 16 bytes
  `{vtable 0x666B34, unitId, next, prev}` allocated and **appended at the tail**
  by `0x4EDD40`, which can also splice a node *after* a given node.

So the tile stack orders *gameplay* consumers (query, orders, panels); the
map's on-screen z-order among co-located unit sprites is decided in the view's
sprite machinery, not by `Cell+0x0C`. See [Open](#open).

## Save/load

* `0x590305`-`0x590347` and `0x59FDDD`-`0x59FE18` reset the pool (free head
  `-1`, free count 0, slot count `-1`, every slot `{-1, 0}`) while re-creating
  the unit pool; the stack list is rebuilt by the unit placement calls.
* The pool initial state (`0x4C8EF3`-`0x4C8F28`) is **100 nodes**
  (`malloc 0x320`), `default = -1` (via `esi = -1` at `0x4C8EB9`).
* Growth (`0x4C22C0`, called when `count == cap_minus_1 && freeCount == 0`) is
  not yet read.

## Verified negatives

* **There is no "move to the bottom of the stack" operation.** A write sweep of
  the pool globals finds writes only in: the container ctor/dtor
  (`0x537CDA`-`0x537D2C`), the pool init/reset (`0x4C8F05`-`0x4C8F28`,
  `0x590305`-`0x590675`, `0x59FDDD`-`0x59FE18`), and the mover (`0x5BD51C`,
  `0x5BD732`, `0x5BD73F`, `0x5BD751`). Fortify/sentry/wake do not reorder a
  tile's list; the order changes only on placement.
* **A failed removal clears the tile head.** If the walk does not find the unit,
  the head field is set to `-1` (`0x5BD3F3`) — the tile's whole stack is
  dropped, not just the missing unit. Reproduced in the reference module.
* `0x56D2C0` is `city_at`, not `unit_at` (city pool, not the unit pool).

## Open

1. **The map's z-order among co-located unit sprites.** The sprite pool and the
   id list above are located; which sprite wins on a shared tile is not pinned
   statically. Concrete probe: two units on one tile, break on `0x5F82E0`
   (blit) and on `0x4EDD40` (sprite-list insert) and read the slot indices and
   the list order in the view object.
2. **`0x4C22C0`** (pool growth policy) is unread.
3. Whether the "hide invisible" mode (`param_5 == 1` in `0x4C0750`) is what the
   map's own draws use, or only the panels, is not established.

## Reference implementation

`rust/src/stack.rs`: the pool, the two mutators, the node fetch, and both walk
shapes, each annotated with its address; tests pin the ordering rule, the
free-list reuse, the walk termination on the `-1` default, and the
failed-removal quirk.
