---
name: game-screenshotting
description: Capture screenshots of the Rust/Bevy civ3-clone game while it runs, for automated testing and debugging. Use for visual evidence of terrain, blending, sprites, city screen or HUD changes, for automated visual checks and regression diffs, for grabbing an already-running window, or when a capture run hangs or comes out blank.
---

# Screenshotting the Civ3 clone

`civ3-clone` is a Bevy app (`DefaultPlugins`), so it can screenshot itself
through the renderer instead of the desktop. Two paths exist:

| | in-engine (`CIV3_SHOT`) | desktop (`scripts/window-shot.sh`) |
| --- | --- | --- |
| timing | exact frame index | whatever is on screen |
| source | the render target the game drew | the composited desktop |
| permissions | none | Screen Recording for the caller |
| exit | automatic (0 = all shots written, 1 = stall) | not applicable |

Use the in-engine path for tests and anything reproducible; use the desktop
path for a process that is already running without the hook, or when you need
what the compositor actually shows (other windows, dialogs, notifications).

## In-engine capture

Implemented in `src/screenshot.rs`. Env vars, read once at startup:

- `CIV3_SHOT=<path>`: schedule captures. `{}` in the path becomes the frame
  number.
- `CIV3_SHOT_FRAME=<frames>`: comma separated frames, default `90`. With
  several frames and no `{}`, `-<frame>` is appended before the extension.
- `CIV3_SHOT_KEEP=1`: stay running after the shots are written.
- `CIV3_NO_SPLASH=1`: start on the map. Without it the greeting splash is up,
  and it swallows the first click or key.
- `CIV3_REVEAL=1`: start with fog off, like the `F9` toggle. Without it most of
  the map is black.
- `MAP_SEED`, `MAP_CENTER=x,y`, `MAP_ZOOM`: pick seed and camera framing
  (`MAP_ZOOM` clamps to 0.35..2.5).

```bash
# one shot of the map, exits by itself
CIV3_NO_SPLASH=1 CIV3_REVEAL=1 CIV3_SHOT=/tmp/map.png CIV3_SHOT_FRAME=120 cargo run
# a frame sequence for animation or state debugging
CIV3_NO_SPLASH=1 CIV3_SHOT='/tmp/f{}.png' CIV3_SHOT_FRAME=30,90,150 cargo run
# an exact view: seed 2, centered on the known hut tile
CIV3_NO_SPLASH=1 CIV3_REVEAL=1 MAP_SEED=2 MAP_CENTER=42,30 MAP_ZOOM=1.4 \
  CIV3_SHOT=/tmp/seed2.png CIV3_SHOT_FRAME=120 cargo run
```

Exit status is `0` when every scheduled shot was written and `1` when a capture
never completed; a killed run (see `timeout` in the script) reports `124`. Output
is a PNG at the window's physical size: the 1280x800 logical window is
**2560x1600** on a Retina display, so divide by 2 when converting to UI
coordinates.

### Script

`scripts/shot.sh` builds the game, runs it with the hook, then verifies what
came out.

```bash
.agents/skills/game-screenshotting/scripts/shot.sh \
  --out '/tmp/shots/seed2{}.png' --frames 30,120 --seed 2 --center 42,30 --zoom 1.4 \
  --reveal
```

Flags: `--out` (default `./shots/shot.png`), `--frames`, `--seed`, `--center`,
`--zoom`, `--reveal`, `--splash` (keep the greeting), `--keep`, `--release`,
`--timeout` (default 180s). It sets `CIV3_NO_SPLASH` unless `--splash` is
passed, parses the game's `shot: wrote <path>` lines, prints each file with its
size, and fails if the game exited non-zero, no shot was reported, or a file is
missing or blank (standard deviation below 0.0005). The game log lands next to
the output as `shot.log`.

### From game code

When a state cannot be reached by frame index alone (city screen open, unit
selected, mid-animation), spawn a screenshot at the point in the code that has
the state:

```rust
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

commands
    .spawn(Screenshot::primary_window())
    .observe(save_to_disk("/tmp/city-screen.png"));
```

`Screenshot::primary_window()` is part of `DefaultPlugins`; no feature or extra
crate is needed. This is the cheapest way to check one sprite or overlay, and
the capture is asynchronous, so the observer (not the same frame) is what writes
the file.

### `P` key

While the game runs, `P` writes `shot-<unix seconds>.png` into the working
directory (the crate root under `cargo run`). It needs the window focused, and
it fires in the same frame the splash is dismissed, so one press on the greeting
already gets you the map. Capture the greeting itself with `CIV3_SHOT` and a low
frame count (`--splash`), which is the only way to see it in a shot. Both debug
keys are listed in the controls table of `README.md`.

## Desktop capture of a running window

```bash
.agents/skills/game-screenshotting/scripts/window-shot.sh /tmp/live.png          # first window matching "civ3"
.agents/skills/game-screenshotting/scripts/window-shot.sh /tmp/live.png "Civ3 Clone"
```

The script compiles `scripts/winid.swift` once into `$TMPDIR/omp-winid` (window
list via `CGWindowListCopyWindowInfo`), picks the first matching window owner or
title, then runs `screencapture -x -o -l<id>`. It prints the capture size plus
the matched owner, title, id and bounds. Requires Screen Recording permission
for the terminal or agent process, since it grabs another application's window;
an ungranted process cannot see window content.

Raw equivalents, if you need them:

```bash
screencapture -x out.png            # whole screen
screencapture -x -o -l<windowID> out.png
```

## Verifying a capture

```bash
magick identify -format '%wx%h %[fx:standard_deviation] %k colors\n' shot.png
magick compare -metric AE baseline.png shot.png null:   # 0 = pixel identical
```

- Standard deviation `0` means a uniform image: the failure mode when the
  window never rendered, or a capture landed before the first draw.
- Capture runs are deterministic: same seed, same frame index, same view gives a
  byte-identical PNG (`magick identify -format '%#'`). So a stored baseline plus
  `-metric AE` is a usable regression check for terrain, blending and overlays.
  Anything that depends on the mouse (hover label, drag) or on wall-clock-loaded
  assets breaks that equality, so capture those states with the window untouched
  or through a code-level observer instead.
- Inspect the image, do not just check the file exists: read it back (image
  tooling renders it) before claiming a visual change is correct.

## Pitfalls

- **Fog**: without `CIV3_REVEAL=1` unexplored tiles are black, so a "blank"
  looking map is usually correct gameplay, not a broken capture.
- **Splash**: with the greeting up, frame 90 captures the greeting itself. Use
  `CIV3_NO_SPLASH=1` (or `--splash` when the greeting is what you want).
- **Assets**: the game needs `assets/gen/` (gitignored). Run
  `python3 tools/prep_assets.py` first, or sprites and audio are missing from
  the shot.
- **Window visibility**: winit only drives updates for a window it considers
  visible, so a minimized or fully covered window can stop the frame counter and
  a run then sits until `--timeout` kills it. Leave the window uncovered.
  `src/screenshot.rs` forces `WinitSettings::continuous()` for capture runs
  (Bevy's default goes reactive while unfocused), and leaves through a direct
  process exit, because an exit request would sit unread in a stalled loop.
- **Frame index vs wall clock**: game state at frame *N* is fixed by the seed,
  but reaching frame *N* takes as long as the loop takes. Budget ~60 fps when
  choosing frames.
- **One run at a time**: two capture runs fight over windows and Cargo's build
  lock.

## Files

- `src/screenshot.rs`: the hook (schedule, auto-exit, `P` key).
- `src/splash.rs`: `CIV3_NO_SPLASH`.
- `src/main.rs`: `CIV3_REVEAL`.
- `scripts/shot.sh`, `scripts/window-shot.sh`, `scripts/winid.swift`: this
  skill's automation.
