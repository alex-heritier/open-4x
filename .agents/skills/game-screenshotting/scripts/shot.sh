#!/usr/bin/env bash
# Capture the Civ3 clone window with the in-engine screenshot hook.
#
#   shot.sh --out /tmp/shots/map.png                    # one shot at frame 90
#   shot.sh --frames 30,90,150 --out '/tmp/shots/f{}.png'
#   shot.sh --seed 2 --center 42,30 --zoom 1.6 --reveal --out /tmp/shots/seed2.png
#
# Runs the game with CIV3_SHOT set, so it exits on its own once every shot has
# landed (exit 0; 1 if a capture never completed). Fails if the game exits
# non-zero, reports no shot, or a written file is missing or blank.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
crate=${CIV3_CLONE_DIR:-$(git -C "$script_dir" rev-parse --show-toplevel)}

out=./shots/shot.png
frames=90
seed=
center=
zoom=
reveal=0
splash=0
keep=0
release=0
timeout_s=180

while [ $# -gt 0 ]; do
    case "$1" in
        --out) out=$2; shift 2 ;;
        --frames) frames=$2; shift 2 ;;
        --seed) seed=$2; shift 2 ;;
        --center) center=$2; shift 2 ;;
        --zoom) zoom=$2; shift 2 ;;
        --reveal) reveal=1; shift ;;
        --splash) splash=1; shift ;;
        --keep) keep=1; shift ;;
        --release) release=1; shift ;;
        --timeout) timeout_s=$2; shift 2 ;;
        -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "shot.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done

profile=debug
[ "$release" = 1 ] && profile=release
out_dir=$(dirname "$out")
mkdir -p "$out_dir"
log="$out_dir/shot.log"

cargo build --quiet ${release:+--release} --manifest-path "$crate/Cargo.toml"

env_args=(CIV3_SHOT="$out" CIV3_SHOT_FRAME="$frames")
[ "$splash" = 0 ] && env_args+=(CIV3_NO_SPLASH=1)
[ "$reveal" = 1 ] && env_args+=(CIV3_REVEAL=1)
[ "$keep" = 1 ] && env_args+=(CIV3_SHOT_KEEP=1)
[ -n "$seed" ] && env_args+=(MAP_SEED="$seed")
[ -n "$center" ] && env_args+=(MAP_CENTER="$center")
[ -n "$zoom" ] && env_args+=(MAP_ZOOM="$zoom")

run=(env "${env_args[@]}" "$crate/target/$profile/civ3-clone")
if command -v timeout >/dev/null 2>&1; then
    run=(timeout "$timeout_s" "${run[@]}")
fi

set +e
"${run[@]}" >"$log" 2>&1
status=$?
set -e
if [ "$status" -ne 0 ] && [ "$keep" = 0 ]; then
    echo "shot.sh: game exited $status (log: $log)" >&2
    tail -5 "$log" >&2
    exit "$status"
fi

paths=$(grep '^shot: wrote ' "$log" | sed 's/^shot: wrote //')
if [ -z "$paths" ]; then
    echo "shot.sh: no shots written (log: $log)" >&2
    exit 1
fi

fail=0
while IFS= read -r path; do
    if [ ! -s "$path" ]; then
        echo "shot.sh: missing or empty $path" >&2
        fail=1
        continue
    fi
    read -r size flat <<<"$(magick identify -format '%wx%h %[fx:standard_deviation]' "$path")"
    if awk -v v="$flat" 'BEGIN { exit !(v > 0.0005) }'; then
        echo "$path $size"
    else
        echo "shot.sh: $path is blank ($size, stddev $flat)" >&2
        fail=1
    fi
done <<<"$paths"
exit "$fail"
