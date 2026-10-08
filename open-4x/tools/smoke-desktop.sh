#!/usr/bin/env bash
# Run under xvfb-run on Linux. Uses an isolated save directory; leaves evidence there.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
evidence_dir="$(mktemp -d -t open4x-smoke.XXXXXX)"
cd "$evidence_dir"
"$project_dir/target/quick/fourx-client" --smoke --screenshot "$evidence_dir/campaign.png" > "$evidence_dir/client.log" 2>&1 &
client_pid=$!
trap 'kill "$client_pid" 2>/dev/null || true' EXIT
window_id="$(timeout 30 xdotool search --sync --name 'Open 4X')"
sleep 3
xdotool windowfocus "$window_id"
for key in c e 3 space space F5; do
    xdotool key --window "$window_id" "$key"
    sleep 1
done
node -e '
  const fs=require("fs");
  const g=JSON.parse(fs.readFileSync("dawn.save.json","utf8")).game;
  const capital=Object.values(g.cities).find(c=>c.owner===1);
  if(g.turn!==3 || capital.industry!==6 || capital.production!==null)
    throw new Error("UI orders failed: "+JSON.stringify({turn:g.turn,capital}));
  if(Object.values(g.armies).filter(a=>a.owner===1).length!==4)
    throw new Error("Recruitment did not finish");
  console.log("PASS: UI development, recruitment, turns, and saved authoritative state");
'
wait "$client_pid"
trap - EXIT
test -s "$evidence_dir/campaign.png"
printf 'Desktop evidence: %s\n' "$evidence_dir"
