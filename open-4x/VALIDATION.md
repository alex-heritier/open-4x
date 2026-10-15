# Validation

## Right clicks choose, put down, and dismiss — 2026-10-15

- `cargo test --workspace`: **380 tests passed** at `bfe7c33` with this work on top (163
  simulation, 120 client, 37 content, 24 runtime unit tests, 3 + 4 integration, 4 economy
  integration, 1 server, 5 asset-forge and 19 scenario-forge). `cargo fmt --all --check`
  passes on the changed crates (`cargo fmt -p fourx-client --check`; the workspace-wide check
  reports only `tools/scenario-forge/src/aoi.rs`, which this work did not touch and which is
  still being written). The one warning left in the client is the pre-existing unused `Id`
  import in `crates/client/src/stage/mod.rs`.
- A right click on the square that is already the chosen one now clears the selection, the
  way Escape does, instead of leaving it chosen (`right_click_clears`, covered by
  `a_second_right_click_on_the_chosen_square_puts_it_down`). Any other square is chosen as
  before, a city or stack still opens its menu, and a chosen unit is still left in place so
  the menu's explicit attack and bombard entries keep working. A menu over the square goes
  down with the selection.
- A right click with the menu open now puts the menu down and does nothing else — the same
  shape as the left click away from the menu — so the click that dismisses no longer chooses
  the square under the pointer. This holds wherever the click lands, the menu's own card
  included, and the selection behind the menu is left alone. Choosing a second square's menu
  therefore takes two right clicks: the first dismisses, the second opens.
- Checked by hand in the running client under Xvfb/lavapipe at 1440 × 900 (`--nation japan
  --focus 40,80`): the first right click on the tundra at (40, 80) raised its card and the
  white outline (65,578 pixels changed against the frame before), the second right click on
  the same square took both down (65,578 pixels changed again), and a third brought them
  back. On the stack at Tokyo: right click opened the menu, **Tile info** chose the square,
  the next right click cleared it, and the one after that opened the menu again.
- The same run, on the dismiss rule (`--nation japan`, Tab onto the Dragoon Cavalry at
  Tokyo): right click opened the menu (36,243 pixels changed); a right click 100, 150 pixels
  away over empty ground took it down and left the unit chosen and no square chosen (283
  pixels against the frame before the menu opened — the pointer's own hover); a right click
  on that same square then chose it (Plains at (113, 32), its card and outline up); and after
  raising the menu again, a right click on the menu's own card put it down with the square
  still chosen (429 pixels against the frame with the menu closed). Captures are development
  artifacts under `/tmp` (`t1`–`t4`, `a1`–`a4`, `b0`–`b5`).

## Nation panel — the stockpile beside the flag — 2026-10-14

- `cargo test --workspace`: **372 tests passed** on master after `a5e7c02` (improve unit art)
  was pulled in with this work on top (162 simulation, 113 client, 37 content, 24 runtime unit
  tests, 3 + 4 integration, 4 economy integration, 1 server, 5 asset-forge and 19
  scenario-forge). `cargo fmt --all --check` and `git diff --check` pass on the changed files;
  the only formatting drift left is in `crates/client/src/theme.rs`, which this work did not
  touch.
- The top left is a row of two cards: the nation's banner (unchanged — the pack's nation
  picture in a portrait banner, with the city-count disc on its lower corner) and, to its
  right, the stockpile.
- The stockpile card lists the six goods the economy runs on, in `Good::ALL` order — coal,
  iron, food, cotton, steel and supplies — each under its own icon with the amount held, in two
  columns when the window is wide and one when it is narrow. A factory good wears the art of the
  natural resource it is gathered as (`visuals.resources`: coal, iron, wheat, cotton); steel
  wears the chrome emblem of its ingots and supplies the colonial cargo crate. It is paper
  rather than slate, because the resource pictures are painted to read on the map's ground.
  Nothing is drawn when `rules.economy` is off, since there is no stockpile then.
- The `Steel 8 · Supplies 12` line came out of the nation card: the new card is where those are
  read, and with icons rather than a cramped line.
- `--smoke --screenshot` frames under Xvfb/llvmpipe were checked by eye for Japan: the two cards
  stand on one line, the goods read as coal, iron, food, cotton, steel and supplies at the
  stockpile's starting figures (0, 0, 0, 0, 8, 12), the card clears the calendar card, and the
  flag is the vertical banner it was. The narrow layout (below 900 logical pixels wide, one
  column) was not captured on screen.

## Industrial milestone — 2026-10-14

- `cargo test --workspace`: **367 tests passed** (162 simulation, 109 client, 36 content,
  24 runtime unit tests, 11 runtime integration tests, 1 server, 5 asset-forge and
  19 scenario-forge). `cargo build -p fourx-client -p fourx-server -p fourx-runtime
  --bins --examples` passed. `git diff --check` passed.
- New simulation coverage includes factories with limited inputs, construction and
  capture, recruitment reservations/refunds, physical home/colony resource production,
  delivery to the current owner, cargo-only port restrictions, transport ferrying,
  fixed-price spread, loans/interest/credit limits, bankruptcy sale order, and AI
  factory overbuilding prevention. Diplomacy coverage includes resource demands,
  refusal pressure, stronger and weaker AI war declarations, peace weariness and
  quiet-war acceptance. Saves continue deterministically with economic/diplomatic state.
- Stack coverage verifies simultaneous lanes, fresh reserve replacement, one attack
  per participating attacker, terrain width limits, one defensive artillery shot,
  tile advancement, and **unchanged turn and date throughout the entire battle**.
  The reproducible `milestone_probe battle` fixture resolves 10 attackers against 3
  defenders in 12 combat rounds on turn 1 / 1 January 1895 (losses 1:3, stack advances).
- `milestone_probe campaign`, seed 42: 30 Japan world-campaign weeks in **28.68 seconds
  in the dev build**, with 2,076 units including 371 cargo carts, 26 factories, 255
  ports and 28 wars. China declared war on Japan during this run. Japan's original
  units remain under human control. This is a passive campaign/protocol check, not
  proof that an hour-long Manchuria campaign is balanced or enjoyable.
- Desktop visual checks ran on Linux/Xvfb/Vulkan llvmpipe at 1440×900: treasury and
  simultaneous battle panels render without panics. A real button click bought one
  Supplies for 100 gold (1,000 → 900 gold, 12 → 13 Supplies); F5 saved format 5 with
  exactly those values. Captures are development artifacts under `/tmp`.
- Final city/production UI changes passed **110 client tests** and a desktop shortage
  capture: each military design shows its required good, unavailable designs show a
  red missing-input label and have no clickable recruitment action. Tokyo also shows
  its homeland status and Factory/Port construction controls.
- Price/upkeep tuning is provisional. The compact starter can bankrupt an idle player
  under the base economy; explicit legacy-rule test fixtures retain their earlier
  settlement and industrial-victory behavior. Base industrial campaigns cannot be
  won by accumulating generic industrial output. Protocol is 6, save format is 5;
  earlier saves are rejected.

The records below describe earlier builds and their rules.

Tested on 2026-10-08 with Rust 1.98.1, Bevy 0.18.1, and OmniLua 0.7.1 in a Linux container.

- `cargo test`: the headless crates pass (2026-10-09: 14 content, 27 runtime, 1 WebSocket integration, and the simulation's unit tests), plus 14 for `scenario-forge` (the Mercator crop, rasterization, climate-to-terrain table, river snapping onto tile edges, water depth classes, and the ETOPO5, Köppen–Geiger, and Natural Earth parsers), the sim's layered terrain tests (glyph and layer round trips, Civ3 move cost and defense, river edges shared by two tiles and four dual cells), and 2 client tests for overlay selection and tonal variants. Tests cover deterministic replay/save restoration, JSON snapshot round trips, ownership/revision checks, atomic script failures, runaway scripts, content traversal, malformed scenarios and units, scenario overrides, and settlement/production/victory. Rules tests cover Civ3's round odds and hit points by level, fortification and terrain/city defence, retreat, blitz (cavalry attacking more than once), bombardment that wounds but never kills, capture of defenceless artillery and workers, promotion, healing, and the odds estimate; eight-way movement in thirds with road and railroad costs; roads, railroads, mines, and farms with their prerequisites, work rates and lost progress; and fixed per-city borders (level reach and tile counts, the nearest city winning an overlap, a new city taking only unclaimed land, capture flipping a whole region, saves round-tripping). The network test starts a real listener and checks commander authority, spectator updates, duplicate sequences, and stale revisions.
- Compact starter: the `russo-japanese-war` scenario loads through full validation and builds a game. Tests assert an upright 12 × 36 theater chart in a 30 × 30 grid, a start date of Monday 1904-02-08, Japan and Russia each with exactly one capital, the Russo-Japanese War under way, and either belligerent commandable. They check that 30 turns are deterministic and round-trip through a save, that the charted Age of Imperialism campaign is fully known to its commander while foreign armies are seen only nearby and foreign fog is not leaked, that an uncharted scenario starts in fog, that the calendar advances one day per turn including leap years, and that pathfinding gives identical routes across maps of different sizes. (The retired `world-1876` whole-world scenario previously covered the same ground at 256 × 684 tiles in a 598 × 598 grid from Saturday 1876-01-01 with 100+ nations; about 10–25 ms per early turn headless, about 25 s for 366 turns, a 0.47 MB commander snapshot after a month.)
- Economy (2026-10-09, macOS; tests in the dev profile, timings in release): `cargo test` passes with 132 tests (86 sim, 14 content, 27 runtime, 4 in `crates/runtime/tests/economy.rs`, 1 server). The 17 tests in `crates/sim/src/economy.rs` cover the ground and river yields, each improvement adding its own resource, the city-square bonuses, a city gathering exactly the squares of its claim (no double counting between neighbours, following a reassigned claim, a new city taking only unclaimed land, a captured city paying its new owner), the harvest updating at once when a worker finishes an order, daily gold and shields with `income_percent` applied once, growth, famine, a fogged view keeping its harvest, and save round trips including old saves that lack the new fields. The integration tests check that the Russo-Japanese War theater at peace ends in an industrial victory within 20–150 days, that the default campaign is deterministic across a save and resume, and that a client snapshot carries the economy. Recounting every city's land on the world takes about 0.7 ms, against about 44 ms for a whole turn.
- The Bevy client compiles against the layered terrain, including the economy panels (`cargo check --workspace --all-targets` is clean on macOS, 2026-10-09). The desktop and browser smoke assertions (day 3, industry 6, no production, 7 units) were reproduced headless against `russo-japanese-war` with the same orders, but the scripts themselves were not re-run.
- Rendering the world (macOS, Metal, `quick` profile): `--smoke --screenshot` frames of the Age of Imperialism map at default zoom, as a remote client of a loopback `fourx-server`, and of `russo-japanese-war` all render without errors. The camera opens on the commander's capital, the minimap shows the whole map, and 200 frames take about 3 seconds in every case (the vsync floor), so frame time stays within one 60 Hz frame even without optimization. The client also passes `cargo check --target wasm32-unknown-unknown --no-default-features`.
- Native client and server compile; the `quick` profile produces runnable binaries. Default headless builds do not compile Bevy.
- `cargo check -p fourx-client --no-default-features --target wasm32-unknown-unknown`: passes with the complete embedded Lua host. No mlua, Lua C library, or Emscripten is used.
- `cd web && trunk build --cargo-profile quick`: produces a browser bundle. This deliberately unoptimized smoke bundle is about 204 MiB; use `trunk build --release` for distribution.
- Desktop render/input smoke: `xvfb-run -a -s '-screen 0 1440x900x24' tools/smoke-desktop.sh` runs the real Bevy client with software Vulkan. It develops a city, recruits infantry, advances two days, saves, asserts the authoritative state, and captures a screenshot.
- Browser render/input smoke: `node tools/smoke-web.mjs` serves the built bundle and runs installed Chromium with software WebGL. The game executes Lua locally, accepts the same development/recruitment/day orders, writes a browser save, and produces screenshots. The resulting day-three game state exactly matches the desktop state, including PRNG state, entities, economy, visibility, and battle state. Portrait/landscape phone viewport screenshots are included, and a real synthetic touch tap on the phone-layout Day button advances to day four and saves correctly. This fallback was used because the T3 preview browser's host sandbox was unavailable.
- Remote desktop render smoke: a real `fourx-server` on loopback serves the scenario and PNG assets to a Bevy client over HTTP/WebSocket; the client renders successfully and exits cleanly.
- `cargo fmt --all --check` and `git diff --check`: pass.
- The Linux desktop and browser smoke scripts were updated to start `russo-japanese-war` (their assertions describe its two-belligerent opening and seven starting units for the commander) but were **not** re-run after the scenario change, because they need Linux Xvfb/xdotool and Chromium.
- Upright map, city flavors, culture borders, and fog (2026-10-09, macOS, Metal, dev profile): `cargo build --workspace` and `cargo test --workspace` pass with 230 tests (5 asset-forge, 52 client, 23 content, 27 runtime, 4 in `crates/runtime/tests/economy.rs`, 1 server, 103 sim, 15 scenario-forge), `cargo check -p fourx-client --no-default-features --target wasm32-unknown-unknown` passes, and `cargo fmt --all --check` flags only `crates/sim/src/combat.rs`, which this work did not touch. The tests added for this work cover: the lattice's native/grid round trip, void cells, and screen bounds; a lattice map's JSON round trip and the rejection of a non-empty void; the theater chart's dimensions, its February 1904 date, its two belligerents and their capitals; Age of Imperialism flavor spot checks and a city sprite existing for every flavor; the fog sheet's indexing by corner states, the fog thinning toward whatever is known, what counts as charted versus seen, the draw-order bands, and the minimap chart standing a lattice map upright; and the forge's lattice cut keeping 175,104 tiles. `--smoke --screenshot` frames were checked by eye: the Age of Imperialism map and the theater chart stand upright with a zigzag edge against black void (the old build showed a diamond turned 45°); the capitals of several nations drew visibly different city skins; borders are dashed lines in each nation's colour just inside the territory edge, with no tint over the territory; and the fog is black over unexplored land and clear where something of yours sees, with soft gradients between and the remembered cities dimmed. A frame at maximum zoom-out over the world was checked as well. A remote client of a loopback `fourx-server` was not repeated after these changes, and the desktop and browser smoke scripts were not re-run (they need Linux Xvfb or Chromium).
- Units, borders, and workers on screen (macOS, Metal): `--smoke --screenshot` frames from a mid-campaign save show nation-tinted territory with border lines, merged same-nation regions, roads and railroads, farm and mine overlays, unit stacks with HP bars and order glyphs, the selected city's region outlined, goto lines, and the unit panel with attack odds. This was checked before the terrain-layer rework and not repeated afterwards; the frames in the next item show the new terrain only.
- Roads and railroads (2026-10-11, macOS, Metal; contract in `docs/roads.md`): `cargo test --workspace` passes (6 new client tests: bit order against the screen directions, links and cells, variant indexing, base-pack sheet sizes), `cargo check -p fourx-client --no-default-features --target wasm32-unknown-unknown` passes, and `art/tests/test_roads.py` (8 tests with the relief ones) checks the cell layout and that cells meeting at each of the eight exits leave no gap, colour jump or change of width. `--smoke --screenshot --zoom 0.4 --focus 73,46` frames of the Age of Imperialism map show roads that curve, fork and cross in cells, a railway with turnouts, and joins between neighbouring tiles without seams; the road art was also composited over grassland, plains, desert and tundra. **Not checked:** the browser build, roads at the world scale or `--zoom` above 1, tiles of a wrapped map at the seam, and the sheets' load time (about 6.5 MB of PNG and about 50 MB of GPU memory).
- Layered terrain (2026-10-09, macOS, Metal, `quick` profile): `--smoke --scenario world-1876 --focus X,Y --zoom 1.0 --screenshot` frames over the Alps and Balkans, the Nile delta, the Ganges and Himalaya, the Congo basin, and Siberia show the ground, the water over it, rivers running between tiles (and through the cities on them), hills and snow mountains, pines on slopes and in the taiga, jungle, and no seams between cells after the terrain sheets were quantized to 256 colours. The minimap draws the whole non-square world.
- World forge (2026-10-09, macOS, release profile): `scenario-forge fetch` verifies the five pinned downloads (Natural Earth countries, lakes and rivers, ETOPO5, and the zipped Köppen–Geiger table, checked before and after unzipping). `build` takes about 0.3 s and writes an upright map of 175,104 tiles, cut from a 512 × 342 Mercator picture (53,144 land tiles): grassland 19.5%, plains 40.8%, desert 9.5%, tundra 30.2%; hills 16.5%, mountains 6.2%; forest 27.6%, jungle 1.9%, marsh 0.6%; 5,440 river edges from 359 rivers; 20,984 coast, 22,759 sea, and 78,217 ocean tiles. The build is deterministic.
- Combat animation (2026-10-09, macOS, Metal, Rust 1.92, dev profile; behaviour in `docs/combat-animation.md`): `cargo test --workspace` passes, `cargo clippy` is clean for the new code, and `cargo check -p fourx-client --no-default-features --target wasm32-unknown-unknown` passes. The sim's 13 `battle` tests cover the record for duels, support shots, captures, fallen cities, bombardment and lethal bombardment, trimming to the newest 128, one command's records only, no record from a failed command, per-player filtering by sight, save round trips with old saves loading, and the same seed recording the same fights (the record changes no dice). The 7 `combat` tests in `fourx-content` cover styles, cues, and rejection of unknown names and unsafe paths. 50 client tests cover the poses and the script (one blow per round at the strike point, a shell landing exactly at its blow, no death on a retreat, deterministic and finite poses, the same sound never played twice at once) and run the stage's real systems in a headless Bevy app on a manual clock: all eight demo fights play to the end and take down every entity, Space skips, a fight out of view is dropped, there is one skip button however many fights wait, and a real `Attack` sent through `Connection::local` is played from the snapshot that answers it, hides the real units, holds orders back until it ends, and then lets them through. `--demo-combat NAME@SECONDS --screenshot` frames of each demo fight (duel, charge, support shot, naval duel, bombardment, broadside, capture, retreat) at several frozen moments were looked at: sprites face each other, bars drop at the blows, smoke, flashes, shells and splashes appear, a loser falls or sinks and fades, a winner is crowned with sparkles, captives take the captor's colour, and a capturer's bar crosses with it. The eight combat WAVs are regenerated byte-identically by `cargo run -p asset-forge`. **Not checked:** the sound itself (this was a file-level check of length, level and determinism, nobody listened to it), the fights in a real campaign at the world scale or over a remote `fourx-server` (the record travels in the same snapshot, and a test covers the local path), touch input for the skip button, and a run of the browser build.

The smoke scripts leave screenshots, logs, and saved state in unique temporary directories and print their locations. They need Linux Xvfb/xdotool/Node or Chromium respectively. They do not require Civ3, licensed art, or anything from the reference codebase.

Native Android/iOS device builds have **not** been performed: the Android SDK/NDK and Apple SDK/signing/device environment are not installed here. Native mobile launch/build configuration is provided under the client manifest and `platforms/`. Mobile browser layouts have been rendered, but these do not substitute for native device tests. Windows/macOS native builds are likewise not tested in this Linux environment. Audio device output could not be checked because the container has no sound device.

The starter supports one human commander against AI plus remote spectators. It does not yet offer multi-faction multiplayer, full diplomacy/trade, script-defined custom commands/UI, or finished painted/animated artwork. Pack definitions, scenario data, PNG/WAV assets, and bounded turn effects are moddable today.

Terrain surfaces (2026-10-09, Linux/software Vulkan): the headless workspace suite
(`cargo test --workspace --exclude fourx-client --profile quick`, 179 tests) and client
suite (`cargo test -p fourx-client --lib --profile quick`, 59 tests) pass. After the
scenario-forge jungle change, its 15 tests and the 103 simulation/24 content tests were
rerun successfully. All three `art/tests/test_relief.py` checks pass, including an
analytical transparent-mask occluder test and contracts for all 18 context/border pairs.
Regenerating `hills_cold_forest` produces identical relief and border pixels. The
`wasm32-unknown-unknown` client check with desktop features disabled passes. The real
Bevy `terrain-study` capture under Xvfb/llvmpipe exits successfully after 200 frames; its
PNG was inspected for overlap, climate/vegetation styles, two-sided ownership borders,
and lifted/occluded border segments. Rust formatting of changed files and the scoped
whitespace check pass. See `docs/terrain-surfaces.md` for reproduction and the known
limits; the shipped historical world was not regenerated and browser rendering was
not exercised in this terrain check.

- Age of Imperialism default (2026-10-11, macOS, Metal): `tools/biq-import` converts `AoI5.0.biq` to the default scenario, which loads through full validation. Tests assert a 90 × 180 lattice, 43 resource kinds (no `industry` or `colonialism`), icons for each, rivers carried over, one capital per nation, a square's resource adding its yield, the resource layer round-tripping and rejecting undeclared kinds, A `--smoke --screenshot` frame over Japan shows the terrain, rivers, cities, and resource icons on open squares. The east-west seam is not joined (the sim has no wrap), so river edges across it are dropped.
