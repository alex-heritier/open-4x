#!/bin/sh
# r2q.sh '<r2 commands>' [exe] - quiet radare2 for scripting.
# Disables color at the source, drops WARN noise, makes no project writes.
# Default exe: Civ3Conquests.exe found by searching upward from $PWD.
set -u
CMDS=${1:?usage: r2q.sh '<r2 commands>' [exe]}
EXE=${2:-}
if [ -z "$EXE" ]; then
  D=$PWD
  for _ in 1 2 3 4 5 6; do
    if [ -f "$D/civ3-gog/app/Conquests/Civ3Conquests.exe" ]; then
      EXE=$D/civ3-gog/app/Conquests/Civ3Conquests.exe; break
    fi
    if [ -f "$D/re/Civ3Conquests.exe" ]; then
      EXE=$D/re/Civ3Conquests.exe; break
    fi
    D=$(dirname "$D")
  done
fi
if [ -z "$EXE" ]; then echo "r2q.sh: no exe found" >&2; exit 1; fi
exec r2 -q -c "e scr.color=0; $CMDS" "$EXE" 2>&1 | grep -v "WARN:"
