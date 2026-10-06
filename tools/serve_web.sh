#!/usr/bin/env bash
# Serve the browser build at http://127.0.0.1:8080 and rebuild on changes.
#
# Uses the `profiling` Cargo profile: optimized like a release (a debug WASM
# build of Bevy is ~170 MB and runs at 15 fps) but without fat LTO, so a
# rebuild takes about a minute instead of several. For what ships, with
# wasm-opt, run `tools/build_web.sh`.
set -euo pipefail

cd "$(dirname "$0")/.."
unset NO_COLOR
exec trunk serve --cargo-profile profiling "$@"
