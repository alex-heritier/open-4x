#!/usr/bin/env bash
# Build the browser game for hosting into dist/: fat-LTO release WASM through
# wasm-opt -O3. `dist/assets` is a symlink to the repo's assets/; pass
# `--copy-assets` for real (hard-linked) files, for hosts and upload tools
# that do not follow symlinks. Other arguments go to `trunk build`.
set -euo pipefail

cd "$(dirname "$0")/.."
unset NO_COLOR

args=()
copy=0
for a in "$@"; do
    if [ "$a" = "--copy-assets" ]; then copy=1; else args+=("$a"); fi
done

trunk build --release ${args[@]+"${args[@]}"}
if [ "$copy" = 1 ]; then
    python3 tools/web_assets.py --copy --dest dist
fi
