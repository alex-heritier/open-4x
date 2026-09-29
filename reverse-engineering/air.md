# Air combat

Owns: air-move dispatch, bombard-move logging. Reference: `rust/src/air.rs`.

## Air-move function `0x456840` (verified: region sweep)

`thiscall` (`mov ebp,ecx`), frame `0x2C`. Head requires `[ebp+0x4C] > 0`;
the fail tail (`0x4579C1`) calls `0x5B2F10(-1)` then `0x5B3040(1)`.
Four move sites push their `AirBombardMove` log strings and converge:

| site | string | shape |
|---|---|---|
| Move1 `0x456C39` | `AirBombardMove 1` (`0x684A8C`) | push + `jmp 0x457282` |
| Move2 `0x456F3E` | `AirBombardMove 2` (`0x684A78`) | push + `jmp 0x457282` |
| Move3 `0x45727D` | `AirBombardMove 3` (`0x684A64`) | push, falls through |
| shared tail `0x457282` | — | `call 0x5F98B0`; `call 0x5C71C0` |
| Move4 `0x457878` | `AirBombardMove 4` (`0x684A50`) | private tail, same epilogue |

`0x5F98B0` is a bare `ret` stub (50 binary-wide callers): the bombard-move
log call is a disabled no-op; the arg is discarded. Move→situation mapping
and the `0x5C71C0` commit: open. Each string has exactly one `.text`
occurrence (its push).

`rust/src/air.rs`: `MOVE_STRINGS` table + `log_is_noop()`, tested.
