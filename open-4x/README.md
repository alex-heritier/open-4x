# Open 4X — Dawn over the Straits

An independent Rust strategy game set at the dawn of the industrial age. The starter campaign combines exploration, city founding, industrial development, research, recruitment, occupation, and army/fleet battles. Fire and shock alternate every three days; regiments lose soldiers and morale, use generals and technology, and rout or surrender. Capture every enemy city or accumulate industrial output to win.

This is a playable first slice, not a complete commercial 4X. Diplomacy, trade, historical campaigns, a technology tree, animation, accessibility, multiplayer factions beyond the solo campaign, and polished painted art are future work. The reference images guide the isometric map, fleet/army presentation, parchment inspector, and campaign dispatches. Every shipped asset is original procedural art.

## Run

From this directory:

```sh
cargo run -p fourx-client
cargo test                     # headless simulation, content, runtime, protocol tests
cargo run -p fourx-server -- --simulate 10 --seed 42
```

The desktop client starts an embedded authoritative host. Bevy never mutates the simulation directly. Click a roster entry or army, then click a destination to march. Use the city buttons to develop industry and select production. Space advances one campaign day; Tab cycles armies; C cycles cities; 1–5 select the displayed production designs; F settles with pioneers; E develops the selected city; F5 saves; Escape deselects; arrows/WASD pan; wheel zooms. Touch uses taps and two-finger pan/pinch. On small screens the inspector moves below the map and supports swipe scrolling.

Save with F5 or the Save button; resume with `cargo run -p fourx-client -- --load dawn.save.json`. Custom saves require the matching `--pack` for their artwork. Browser saves use local storage and resume with `?load=browser`.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `fourx-sim` | Pure deterministic Rust state machine, map, combat, AI, orders, serializable protocol. No Bevy, Lua, IO, or clock. |
| `fourx-content` | Portable JSON pack schema, validation, pack-relative asset paths. |
| `fourx-runtime` | Authoritative host, transactional commands, bounded Lua execution, saves/replays. No Bevy. Runs on native and WASM. |
| `fourx-server` | Headless CLI and HTTP/WebSocket service. |
| `fourx-client` | Bevy rendering, UI, input, embedded/remote transport. |
| `asset-forge` | Reproducible original PNG/WAV starter art. |

This workspace has no dependency on `open-civ3/` or its reference code. Ordered containers, seeded integer combat, and explicit command/revision state make headless runs reproducible. Server snapshots mask uncharted terrain and enemies outside the player's vision. The spectator view reveals the campaign.

## Server

```sh
cargo run -p fourx-server -- --bind 127.0.0.1:7878 --save campaign.save.json
cargo run -p fourx-client -- --server ws://127.0.0.1:7878/ws
```

`GET /health` reports readiness, `/pack` exposes the current pack manifest, and `/assets/` serves its PNG/WAV files. Clients receive the manifest before their first snapshot and use the same pack visuals as the server. The first authorized connection is the commander; later connections spectate. Disconnecting releases the commander slot. This is a solo campaign with remote access/spectating, not simultaneous multiplayer factions.

For non-local exposure, set `FOURX_TOKEN`, pass `--token TOKEN` to the client, and place the server behind a TLS reverse proxy (`wss://`). Do not pass secrets in public URLs. There is no lobby/account service. The default bind is loopback. WebSocket messages are capped at 16 KiB; the server assigns player authority and checks versions, revisions, and strictly increasing request sequences. Saves are trusted local files and cannot be uploaded through the protocol.

For automation: `--simulate DAYS --seed SEED --save PATH`; `--load PATH` resumes a host. Simulation JSON is written to stdout. Script/validation failures exit unsuccessfully. `cargo run -p fourx-client -- --smoke --screenshot /tmp/fourx.png` captures a rendered desktop frame and exits after 200 frames.

## Modding

Use a complete replacement pack directory containing `pack.json`, scripts, and ordinary PNG/WAV files:

```sh
cargo run -p fourx-client -- --pack /absolute/path/to/my-pack
cargo run -p fourx-server -- --pack /absolute/path/to/my-pack
cargo run -p asset-forge              # regenerate only bundled original art
```

The base pack is the working format example in `assets/packs/base/`. `format: 1` is mandatory. Units have stable string IDs, names, costs, fire/shock values, movement, land/naval/pioneer capabilities, and sprite paths. All numeric values are bounded. Packs define combat width, research cost, starting treasury, victory industry, visual paths, and the campaign script. The optional `scenario` defines dimensions (8–128 per axis), faction names, starting cities and armies, and terrain overrides. Each override is a tile with `position`, `terrain` (`Grass`, `Sand`, or `Water`), `forest`, and `mountain`. Starting cities are forced to land and starting armies to their movement domain. This first campaign supports faction IDs 1 (human commander) and 2 (AI).

Paths may not traverse outside the pack; symlinks are canonicalized and checked. Current packs replace the entire definitions table; there is no implicit or filesystem-dependent merge order.

Scripts use **plain Lua 5.4 through OmniLua**, following the decision to use a pure-Rust interpreter for native/mobile/browser parity. There is no mlua dependency and no Bevy scripting bridge. The same interpreter executes the same script in browser offline campaigns and dedicated servers. `on_turn(context)` gets the next turn, seed (string), city count, and a `campaign` table containing the current turn, cities, armies, and factions. Entity keys in that table are strings. It returns integer `income_percent`, `fire_percent`, and `shock_percent`, each 25–400. The host creates a fresh sandbox for each turn, enforces a 200,000-instruction / 8 MiB budget, and removes host services, dynamic loading, randomness, and stdout printing. Scripts currently customize these effects; arbitrary custom gameplay commands and UI scripting are not exposed yet. A failure leaves the campaign unchanged. Saves include the exact pack and script, so they replay independently of later content edits.

## Terrain format

Gameplay uses logical square coordinates equivalent to Civ3's even-parity grid: native `(x+y, y-x)`. Rendering uses 128×64 diamonds and depth-sorted relief overlays. Base terrain draws on the **dual grid**: each rendered diamond has the four surrounding tile centers as its N/E/S/W vertices. The 9×9 PNG atlas contains all 3⁴ combinations (grass, sand, water):

```text
column = 3*W + N
row    = 3*S + E
index  = row*9 + column
```

That preserves tile-center terrain and smoothly joins coasts. It is freshly implemented from the documented geometry under `open-civ3/reverse-engineering/blending.md`; no reference implementation or Civ3 asset loader is used. The starter tileset has one terrain triple plus forest/mountain overlays; extra climate triples and variants can extend the pack format later. Unit PNGs are 80×80 (ships 128×96), city/relief PNGs 128×112; transparent padding defines their anchor.

## Platforms

- Desktop: Linux/X11/Wayland, Windows, macOS through Bevy. Linux requires the usual window/audio development packages (ALSA, udev, X11/Wayland).
- Web: `wasm32-unknown-unknown`, WebGL2, pure-Rust Lua. Install the Rust target and Trunk, then `cd web && trunk serve`. Offline local play is the default. Append `?server=ws://localhost:7878/ws` for a server; HTTPS pages require `wss://`. Remote packs load through the server's HTTP(S) asset URL and require appropriate same-origin hosting or CORS configuration.
- Android: the client exposes a `cdylib` and `android_main`, with the `android` feature enabling Bevy NativeActivity. APK metadata bundles the assets; `cargo apk run -p fourx-client --lib --no-default-features --features android` uses an Android SDK/NDK/toolchain. Desktop features must stay disabled.
- iOS: an exported C entry and XcodeGen project configuration live in `platforms/ios/`. Build a static client library for `aarch64-apple-ios`, generate the project, select a signing team, and run from Xcode. See `platforms/README.md`. Platform SDKs, signing, and device testing are required and have not been performed here.

Native gameplay and native/web compile checks are recorded in `VALIDATION.md`. Cross-platform source support does not imply that every device build has been tested.
