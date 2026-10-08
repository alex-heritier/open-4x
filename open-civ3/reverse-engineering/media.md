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

## Diplomacy and wonder screens: art, leaderheads, text (verified: art files)

Everything here is read from the shipped data, not the executable; the
executable only names the files (`WONDERSPLASH`, `WONDER_WIN` above). The
clone's builders are `src/advisors.rs`, `src/leaders.rs`, `src/speech.rs`,
`src/wonders.rs` on the stage of `src/stage.rs`; `tools/prep_assets.py`
stages `leaders`, `diplomacy`, `wonders` convert the files.

**Frames** (`Art/Diplomacy/*.pcx`, 1024 x 768, magenta outside the panels so
the map shows through). All three hold the leader's frame, a 221 x 261 piece at
(401, 50) around a 200 x 240 hole at (411, 59). Measured panels (interior
parchment, x y w h):

| Frame | Panels |
| --- | --- |
| `talk_offer` | one box (298, 327, 436, 239) under the portrait |
| `consider` | one box (277, 326, 479, 377) |
| `counter` | side panels (41, 223, 200, 393) and (790, 223, 200, 393), each with an arrow pointing at the bar; top box (298, 327, 436, 97); bar (312, 439, 410, 101); lower box (298, 558, 436, 144); bottom bar (346, 726, 325, 28) |

The clone uses `talk_offer` for the greeting and the computer's proposal and
`counter` for the trade table (our offer left, theirs right, the leader's words
in the top box, standing and war in the bar, treaties in the lower box, Propose
and Leave in the bottom bar). `consider` is converted but unused (CLONE).

**Leaderheads** (`Art/Flics/*.flc`, 200 x 240). Each civ
has four clips, one per era, named by the leader's two letters: Japan `To_A01`,
`To_01`, `To_C01`, `To_D01`; Rome `Ce_01`, `Ce_B01`, `Ce_C01`, `Ce_D01`; Egypt
`Cl_01`, `Cl_B01`, `Cl_C01`, `Cl_D01`; China `Mo_A01`, `Mo_B01`, `Mo_C01`,
`Mo_01`. A clip has 120 or 121 frames plus one ring frame equal to frame 0
that FLC appends. It drifts away from frame 0 the whole way (no loop), and the
`_02` file beside it is the same frames reversed (a few pixels differ in a few
frames: `Mo_B02`), so the game plays `_01` then `_02`: a ping-pong over one
forward clip. The FLC header speed is not what plays (71 ms in `Mo_01` and the
Japan and Rome base clips, 0 or 20 in the rest); the clone plays every clip at
71 ms a frame (CLONE). The era variant follows the civ's research era (0 ancient
.. 3 modern).

**Speech** (`Text/diplomacy.txt`, Windows-1252). A block is `#KEY` followed by
`#civ n`, `#power n`, `#mood n`, `#random n` and the quoted lines. A block holds
`(32 if civ) * (3 if power) * (3 if mood) * random` lines, in that order of
nesting: text set, power tone, mood tone, phrasing. Comment lines above a block
(`; $CIVNAME1 = AI's Civ`) say what each `$NAMEn` in its lines is, the digit
being the argument number. The text set of a leader is `RACE.diplomacy_text_index`,
and all four of our `RACE` rows store -1, meaning the row minus one (`0x515557`):
Japan 8, Rome 0, Egypt 1, China 6. Which power and mood tone a leader speaks
in is not in the data; the clone derives them (CLONE: power from the score ratio,
mood from the attitude class, 0/1 friendly, 2 neutral, 3/4 hostile). `#ANGER_AT_LEVELS` lists
the five attitude words (Gracious, Polite, Cautious, Annoyed, Furious).

**Wonder splash** (`Art/Wonder Splash/wonderBackground.pcx`, 1024 x 768): a 320 x
320 hole at (351, 109) for the wonder's picture (`Art/Wonder Splash/<name>.pcx`,
found through the wonder's `civilopedia_entry` and `PediaIcons.txt`), the text
(`script.txt #WONDERSPLASH`) below it, two buttons ("Zoom to City.", "Sounds
Good.") and `Wonder.wav`. A rival's great wonder is only a news line
(`#WONDERPRODUCE`).

**Wonders of the World window** (F7): `Art/Advisors/wonders_background.pcx` (a
914 x 645 field at x 56..969, y 69..713), `wondersBOX.pcx` the 370 x 200 card,
whose picture well is 190 x 132 at (162, 47) with the top right 66 x 47 cut out
for the eye. `wondersBOXoverlay.pcx` is a plate the same size as the card that
hides the picture of a wonder not built yet. `wondersEye.pcx` is the "zoom to
city" button, three 66 x 47 states stacked at x = 1 (what each state is
stays HYPOTHESIS; the clone draws the first),
with pure green (0, 255, 0) in the rounded corners (a second key beside the
magenta). Each card says Owned by / Constructed in / Located in.

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

`civ3/civ3-gog/app/sound.dll` (454 656 B, md5 `f82a1295…`) is the mixer
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
