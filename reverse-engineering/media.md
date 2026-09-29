# Movies and victory media

Owns: intro gate, movie selection, victory/wonder art lookups. Reference:
`rust/src/media.rs`. The Bink engine itself (one linear player at
`0x62BA32–0x62BB7B`) is mapped in `REGIONS.md`; this file owns the UI
side: which movie plays and when.

## Intro gate `0x4E26C2` (verified: region sweep)

Pushes `PlayIntro` (`0x7294C8`) into the `0x585B00` pref reader; plays
only when it returns 1. The intro movie `Art\Movies\CivComplete.bik`
(`0x7294AC`) loads via the standard art-loader shape (`mov ecx,0x9C3508;
call 0x598580`), then `call 0x62BA10` (movie-player bridge). A resolution
branch plays it 640x280 centered from `[0x9C733C]`/`[0x9C7338]`.

## Victory/race selector `0x4E28C1` (verified: region sweep)

`Art\Movies\race.bik` (`0x7294F4`) vs `victory_movie.bik` (`0x7294D4`),
same loader shape; branch predicates `0x54C8C0`/`0x54C950` (DECODED —
sweep-found, parent-verified heads):

* `0x54C8C0` Min_Install predicate: strcpy of
  `SOFTWARE\Infogrames Interactive\Civilization III` (`0x72C2E4`) to
  stack, `RegOpenKeyExA(HKLM)`, query `Min_Install` (`0x72C2D8`).
* `0x54C950` CD-drive check: `GetDriveTypeA` loop over `A:`–`Z:`
  (`cmp 0x1A`, `add 0x41`), match on `DRIVE_CDROM` (5), caches the
  drive letter at `[0xB422BC]`.
* Selector (`0x4E2879…`, child-reported): `call Min_Install; cmp
  al,1` routes installed-movie vs CD-copy paths (HYPOTHESIS on exact
  value semantics).

## Wonder art lookups (verified: region sweep)

Single-xref art lookups through `0x598580`: `WONDERSPLASH` (`0x72F390`,
pushed only at `0x5CFA0F`) and `WONDER_WIN` (`0x72F3D4`, pushed only at
`0x5D06CA`), each followed by `0x60E870` + the `0x104`-fallback path.
Production/obsolescence/vote mechanics live outside (`WONDERCHANGE` only
at `0x46D36A`/`0x4B947E`, `WONDERPRODUCE` only at `0x4B9882`, `Victory
Point Limit` only at `0x5866B2`/`0x5876ED`; `VICTORY`/`SPACESHIP` have
zero `.text` imm32 hits) — open.

`rust/src/media.rs`: `intro_plays()`, `center_offset()`, tested.

## Sound engine anchors (verified: push-imm scan + `r2`)

Audio goes through `WINMM` (single import-table hit at file `0x27E8E2`;
zero hits for `dsound`/`DSOUND`/`DirectSound`/`waveOut`/`PlaySound`).
65 `.wav` + 21 `.mp3` filename strings cluster at file `0x32B342–0x32B689`
(VA `0x72B342–0x72B6C9`); 11 `.text` push-imm refs into that table:

* `0x536540`: `music.txt` playlist loader — **confirmed 2026-09-29**: the
  body opens with `sub esp,0x20C; push esi` and `push 0;
  push "text\\music.txt" (0x72B60C); mov ecx,0x9C3508; call 0x598580`
  (the standard art loader on the art singleton), then `push 4; push esi;
  call 0x64BE95` and compares the result with `-1` — so the handle comes
  straight from the asset loader and the `cmp eax,-1` gates the "no
  playlist" path. Slot positions of `"r"` (`0x72B608`) + `0x64B09F`
  (open) and `0x64BE33` (`MAX_PATH 0x104` readline) are as recorded.
* `0x537363–0x5374F9`: 9 pushes of `Sounds\Ambience Sfx\*.wav`
  (e.g. `Woodlark.wav` at `0x72B6BC`) — ambience table consumer,
  function start open.
* `0x540902`: lone `0x54`-bucket ref to `0x72B608` (`"r"`/music path).

So the sound/ambience loader lives in the `0x53` thin bucket, not near the
`0x62Bxxx` Bink player. Mixer/channel internals still open.

## `sound.dll` backend + version gate (verified: export table, byte scans)

`civ3-gog/app/sound.dll` (454 656 B, md5 `f82a1295…`) is the mixer
backend: it imports `DSOUND.dll` + `mss32.dll` (Miles Sound System) +
`WINMM.dll` and exports 12 symbols — `create_sound`, `delete_sound`,
`get_sound_version`, `init_sound_timer`, `release_sound`, plus
`Dll_Midi_Device`/`Dll_Wave_Device`/`Dll_Wave_In_Device`
create/delete and a `WaveInDeviceMgr` callback (mangled C++).

The game does **not** link it (absent from its 11-DLL import list:
`OPENGL32 binkw32 IFC23 WINMM ADVAPI32 USER32 ole32 comdlg32 KERNEL32
GDI32 WSOCK32`) but imports `LoadLibraryA`/`GetProcAddress` (IAT names
at file `0x27EE64`/`0x27EE52`), so it resolves the five C exports at
runtime — now verified (`0x5F9D8B–0x5F9E12`, r2): `push
".\sound.dll" (0x72FE50); call LoadLibraryA` (handle → `[0xCAAD7C]`),
then an ordinal loop (`cmp esi,0xB`) resolving ordinals 1–11 via
`GetProcAddress` into `[0xCAAD2C]`, then `call 0x5F9E70`
(version-query, body unopened), `and eax,0xFF00`, `cmp eax,0xD00`.
Mismatch shows a dialog via `call 0x5FBE20` with caption `Sound
Version Warning` (`push 0x670C08` at `0x5F9DE5`) and text `The sound
header files used in the game do not match the ones used in sound.dll.
Check all sound.h and sound device.h versions!` (`push 0x670B88` at
`0x5F9DEA` — correcting the earlier `0x5F9DEB` claim, which was off by
one: the push is at `0x5F9DEA`).

Playlist/ambience selection lives game-side (`music.txt`, `Ambience`,
`.wav` strings: 86 in the game, zero in `sound.dll`), so `sound.dll`
is a dumb backend: game picks names, DLL plays bytes. Open: which
of the 12 exports ordinals 1–11 map to, `0x5F9E70` version-query body,
and `create_sound` signature/args.

## Hall-of-Fame writer `0x540710–0x540815` (verified: `r2`)

First body in the `0x54` thin bucket (`ret 8`). Appends one record to
`HighScores.cv3` via `call 0x64B09F` with `"a"` — confirming `0x64B09F`
as the fopen(path, mode) helper (music loader's `"r"` at `0x53657D`
is the same call). Leader name comes from `call 0x55A270` on the
`0xA52E98`-indexed owner row (`[0x9FD4BC]` key), is strcpy-copied,
then space-sanitized (`0x20 → 0x5F`, `0x54076D–0x54078A`). The write
(`0x5407F9–0x5407FF`, `call 0x64B0DC` = HYPOTHESIS fprintf, then
`call 0x64AEB8` = HYPOTHESIS fclose):

```text
"%s %d %d %d %d %d\n"   (fmt at 0x72BDE8)
  name, [esi+0xA52EB8], [esi+0xA52EC8], score, arg1, arg2
```

`esi` = owner-row index (standard 2105-stride math); `score` =
`fld [esi+0xA52EE4]` + `call 0x64A230` (HYPOTHESIS: float→int
conversion in the CRT cluster). A `[0xA5267C] & 0x26000` branch
(`0x540792`) picks an alternate score source (`[esi+0xA54064]`).
Field identities (score/year/difficulty/…) and the `0x26000` gate:
open.

`rust/src/media.rs`: `hof_sanitize()` (exact space→underscore map),
tested.
