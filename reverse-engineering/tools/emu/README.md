# Emulator harness: run the game's own save loader

Static analysis said what the loader *should* read; this harness runs it. It
maps `Civ3Conquests.exe` into [Unicorn](https://www.unicorn-engine.org/),
stubs the Win32 imports and the CRT allocator, runs the C++ static
initialisers, and calls `game_data(stream, 0)` (`0x590030`) on a decoded
`.SAV` held in emulated memory. Hooks record which chunks the loader accepts
and which input bytes each instruction reads. No Wine, no display, no game
data other than the exe (and the default rules `conquests.biq`, which the
loader opens during start-up).

Used for [`../../savegame.md`](../../savegame.md); a worked example of the
result is its section 7.

## Files

| file | role |
|---|---|
| `emu.py` | the harness: PE mapping, bump allocator, `call()` for thiscall / cdecl / stdcall, `hook_func`, read/write helpers |
| `imports.py` | Win32 import stubs (FOURCC, heap, time, GUID, file mapping, ...) |
| `vfs.py` | virtual file system behind `CreateFile`/`fopen`; serves the default rules from `$CIV3_DEFAULT_BIQ` |
| `run_init.py` | runs the exe's static initialiser table (`0x680000..0x6803C4`); initialiser #47 (`0x4A8450`) fails and is ignored |
| `loadsave.py` | `setup(stream)`: sets `[0xA32BC4]`/`[0xA32BC8]` from the header, stubs the UI and sprite calls the load reaches, returns the emulator ready to call `game_data` |
| `trace.py` | runs the load and writes a pickle with every chunk (`0x4FCBB0`), raw read run and allocation, in order |
| `savespec.py` | independent Python reading of the stream (format 24, sub-versions 2..10), and the comparison with a trace |

## Use

```sh
pip install pefile unicorn capstone

# the decoded stream of the save (DCL removed) and of the default rules
cd civ3_utils/biq
cargo run --release --example sav  -- ../../civ3/civ3-complete/Conquests/Saves/yolo.SAV --stream /tmp/yolo.raw
cargo run --release --example dump -- ../../civ3/civ3-gog/app/Conquests/conquests.biq --stream /tmp/conq.raw

cd ../../reverse-engineering/tools/emu
export CIV3_DEFAULT_BIQ=/tmp/conq.raw
python trace.py /tmp/yolo.raw /tmp/yolo.pkl      # ~3 s for a 5 000-tile map
python savespec.py /tmp/yolo.raw /tmp/yolo.pkl   # chunks OK ... EOF OK
```

`trace.py` prints `game_data -> END expected END OK` when the last input byte
the loader read is the last byte of the stream. `savespec.py` prints `chunks OK` when the spec's chunk list
equals the trace's, and `EOF OK` when the spec ends on the last byte; without
the pickle it only checks the spec against the file.

To test a sub-version gate, lay a real save out for another sub-version and
run the same pair of commands on it:

```sh
cargo run --release --example sav -- yolo.SAV --sub 5 /tmp/yolo5.raw   # in civ3_utils/biq/
```

Environment: `CIV3_EXE` overrides the exe (default
`<repo>/civ3/civ3-gog/app/Conquests/Civ3Conquests.exe`); `CIV3_DEFAULT_BIQ`
is the decoded stream of the game's default `conquests.biq` (needed by the
loader's rules path).

## Reading a trace

`trace.py` writes a pickle: `ev` (ordered events), `hdr` (header length), `n`
(file length), `ret` (end offset of the last input byte the loader read) and
`snap` (a copy of the `Game` object and the map header after the load).
Events are tuples:

* `('C', obj, off, tag, size, start, last, vtable, ret)`: a chunk accepted by
  `0x4FCBB0` (`off` is the buffer offset, subtract `hdr` for the stream offset;
  `start`/`last` is the object's memory range);
* `('D', obj, vtable, off, ret)`: a chunk dispatch `0x4FCAB0` (save/load flag
  resolved to the vtable slot);
* `('R', pc, start, end, edi)`: a run of input bytes read by the instruction
  at `pc` (raw blocks and the bytes the loader reads directly);
* `('A', ptr, size, caller)`: an allocation (shows array sizes).

The reading instruction's address is how a raw block is attributed to the
loader that consumes it. `trace.py` stops the emulation when the last byte of
the stream has been read, so "consumed exactly" means the last read ended at
the end of the file.

## Limits

* The harness runs the load routine only. Anything the game does after the
  load (rebuilding caches, the corrupt-save repair, the UI) is stubbed or not
  reached.
* Heap addresses are the emulator's, not the game's.
* The 32-bit exe has no ASLR and uses static VAs (image base `0x400000`), so
  addresses in a trace match the addresses in the findings files.
