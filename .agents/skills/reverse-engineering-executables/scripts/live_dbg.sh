#!/bin/sh
# live_dbg.sh -- launch Civ3Conquests.exe under winedbg with a held-open fifo.
#
# Usage (run from anywhere; paths resolve from this script's location):
#   sh live_dbg.sh launch <tag>            # blocks holding game+winedbg; background it
#   sh live_dbg.sh send <tag> 'break *0x5942cf\ncont\n'   # one command batch
#   sh live_dbg.sh tail <tag>              # follow the log
#
# The launch shell holds the fifo open (exec 3<>) so winedbg never sees EOF
# at a breakpoint stop. While the debuggee runs, sent commands queue and
# execute at the next stop. Never `cont` from an enabled breakpoint site:
# `stepi` once past it first, or `disable N` + `cont`. See SKILL.md.
set -u
ROOT=$(cd "$(dirname "$0")/../../../../.." && pwd)
GAMEDIR="$ROOT/civ3-gog/app/Conquests"
PREFIX="$ROOT/.civ3-prefix"
WINEBIN="$ROOT/.runtime/Wine Staging.app/Contents/Resources/wine/bin"
DBGDIR="/tmp/civ3dbg"

cmd=${1-help}; tag=${2-}
fifo="$DBGDIR/in$tag"; log="$DBGDIR/log$tag"

case $cmd in
  launch)
    [ -n "$tag" ] || { echo "usage: $0 launch <tag>" >&2; exit 1; }
    [ -x "$WINEBIN/wine" ] || { echo "missing runtime: $WINEBIN" >&2; exit 1; }
    mkdir -p "$DBGDIR"
    rm -f "$fifo"; mkfifo "$fifo" || exit 1
    # shellcheck disable=SC2094
    exec 3<>"$fifo"   # held writer: fifo never EOFs winedbg at a stop
    cd "$GAMEDIR" || exit 1
    echo "fifo=$fifo log=$log -- arm with: sh $0 send $tag 'break *0xADDR\ncont\n'"
    WINEPREFIX="$PREFIX" PATH="$WINEBIN:$PATH" winedbg ./Civ3Conquests.exe \
      < "$fifo" > "$log" 2>&1
    echo "winedbg exited: $?"
    ;;
  send)
    [ -n "$tag" ] && [ -n "${3-}" ] || { echo "usage: $0 send <tag> 'cmd\\n...'" >&2; exit 1; }
    [ -p "$fifo" ] || { echo "no fifo $fifo (launch first)" >&2; exit 1; }
    printf '%b' "$3" > "$fifo"
    ;;
  tail)
    [ -n "$tag" ] || { echo "usage: $0 tail <tag>" >&2; exit 1; }
    tail -n 30 -F "$log"
    ;;
  *)
    sed -n '2,12p' "$0"
    exit 1
    ;;
esac
