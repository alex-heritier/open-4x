#!/usr/bin/env bash
# Runs the client on the arena save, orders the X attack, and captures the fight as it plays.
set -euo pipefail
cd /root/code/open-4x
target/quick/fourx-client --load /tmp/open4x-x-attack-save.json \
  --focus 17,11 --select unit:4 >/tmp/open4x-stack-clean.log 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
win=""
for _ in $(seq 1 80); do
  win=$(xdotool search --name "Open 4X" 2>/dev/null | head -1 || true)
  [ -n "$win" ] && break
  sleep 0.5
done
[ -n "$win" ] || exit 1
sleep 4
xdotool windowfocus --sync "$win"
sleep 0.5
# Real XTest input (winit ignores synthetic --window key events).
xdotool key x
sleep 0.5
xdotool mousemove 796 488
sleep 0.5
xdotool click 1
for step in $(seq -w 1 20); do
  sleep 0.35
  import -window root "/tmp/open4x-arena-$step.png"
done
