# Validation

Tested on 2026-10-08 with Rust 1.98.1, Bevy 0.18.1, and OmniLua 0.7.1 in a Linux container.

- `cargo test`: 18 tests pass (5 content, 10 runtime, 2 terrain, 1 WebSocket integration). Tests cover deterministic replay/save restoration, JSON snapshot round trips, ownership/revision checks, atomic script failures, runaway scripts, content traversal, malformed scenarios, scenario overrides, settlement/production/victory, and multi-day fire/shock battle resolution. The network test starts a real listener and checks commander authority, spectator updates, duplicate sequences, and stale revisions.
- Native client and server compile; the `quick` profile produces runnable binaries. Default headless builds do not compile Bevy.
- `cargo check -p fourx-client --no-default-features --target wasm32-unknown-unknown`: passes with the complete embedded Lua host. No mlua, Lua C library, or Emscripten is used.
- `cd web && trunk build --cargo-profile quick`: produces a browser bundle. This deliberately unoptimized smoke bundle is about 204 MiB; use `trunk build --release` for distribution.
- Desktop render/input smoke: `xvfb-run -a -s '-screen 0 1440x900x24' tools/smoke-desktop.sh` runs the real Bevy client with software Vulkan. It develops a city, recruits infantry, advances two days, saves, asserts the authoritative state, and captures a screenshot.
- Browser render/input smoke: `node tools/smoke-web.mjs` serves the built bundle and runs installed Chromium with software WebGL. The game executes Lua locally, accepts the same development/recruitment/day orders, writes a browser save, and produces screenshots. The resulting day-three game state exactly matches the desktop state, including PRNG state, entities, economy, visibility, and battle state. Portrait/landscape phone viewport screenshots are included, and a real synthetic touch tap on the phone-layout Day button advances to day four and saves correctly. This fallback was used because the T3 preview browser's host sandbox was unavailable.
- Remote desktop render smoke: a real `fourx-server` on loopback serves the scenario and PNG assets to a Bevy client over HTTP/WebSocket; the client renders successfully and exits cleanly.
- `cargo fmt --all --check` and `git diff --check`: pass.

The smoke scripts leave screenshots, logs, and saved state in unique temporary directories and print their locations. They need Linux Xvfb/xdotool/Node or Chromium respectively. They do not require Civ3, licensed art, or anything from the reference codebase.

Native Android/iOS device builds have **not** been performed: the Android SDK/NDK and Apple SDK/signing/device environment are not installed here. Native mobile launch/build configuration is provided under the client manifest and `platforms/`. Mobile browser layouts have been rendered, but these do not substitute for native device tests. Windows/macOS native builds are likewise not tested in this Linux environment. Audio device output could not be checked because the container has no sound device.

The starter supports one human commander against AI plus remote spectators. It does not yet offer multi-faction multiplayer, full diplomacy/trade, script-defined custom commands/UI, or finished painted/animated artwork. Pack definitions, scenario data, PNG/WAV assets, and bounded turn effects are moddable today.
