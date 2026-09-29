#!/usr/bin/env bash
# Grab an already-running window with macOS `screencapture`. Use this when the
# game is up without CIV3_SHOT, or when you want what the desktop really shows.
#
#   window-shot.sh [out.png] [match]
#
# `match` is matched against the window owner and title, case-insensitive
# (default "civ3"). Needs Screen Recording permission for whatever process runs
# this script; the capture is at the display scale factor (2x on Retina).
set -euo pipefail

out=${1:-/tmp/window-shot.png}
match=${2:-civ3}

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
src="$script_dir/winid.swift"
helper="${TMPDIR:-/tmp}/omp-winid"
if [ ! -x "$helper" ] || [ "$src" -nt "$helper" ]; then
    swiftc -O -o "$helper" "$src"
fi

line=$("$helper" "$match" | head -1)
if [ -z "$line" ]; then
    echo "window-shot.sh: no on-screen window matching '$match'" >&2
    exit 1
fi

id=$(cut -f1 <<<"$line")
owner=$(cut -f2 <<<"$line")
title=$(cut -f3 <<<"$line")
bounds=$(cut -f4 <<<"$line")

screencapture -x -o -l"$id" "$out"
printf '%s %s owner=%s title=%s id=%s bounds=%s\n' \
    "$out" "$(magick identify -format '%wx%h' "$out")" "$owner" "$title" "$id" "$bounds"
