#!/bin/sh
# Boot each committed fixture (a .SAV and a .biq) on an asset root with no Civ3
# install present, capture a frame of the map and one of the first city, and
# fail if the game exits non-zero or a frame is blank.
#
#   tools/smoke_assets.sh [ASSET_ROOT] [OUT_DIR]     # test-assets, /tmp/open-4x-smoke
#
# Each scenario converts its art into its own .cache/ namespace (a few
# minutes the first time per scenario and source root).
set -eu
cd "$(dirname "$0")/.."
root=${1:-test-assets}
out=${2:-/tmp/open-4x-smoke}
mkdir -p "$out"
cargo build --release --quiet >/dev/null 2>&1 || cargo build --release   # warnings only on failure
status=0
for fixture in "civ3_utils/biq/tests/data/TEST.SAV" "civ3_utils/biq/tests/data/Intro3 New Alliances.biq"; do
  name=$(basename "$fixture" | tr ' ' '_')
  rm -f "$out/$name"-*.png
  CIV3_DIR=/nonexistent CIV3_GOG=/nonexistent CIV3_NO_SPLASH=1 CIV3_REVEAL=1 MAP_ZOOM=1.2 \
    CIV3_SCRIPT='60:city' CIV3_SHOT="$out/$name-{}.png" CIV3_SHOT_FRAME=50,100 \
    timeout 900 target/release/civ3-clone "$fixture" --assets "$root" >"$out/$name.log" 2>&1 \
    || { echo "FAIL $fixture: exit $? (see $out/$name.log)"; status=1; continue; }
  for frame in 50 100; do
    python3 - "$out/$name-$frame.png" <<'PY' || status=1
import sys
from PIL import Image
path = sys.argv[1]
try:
    im = Image.open(path).convert("RGB")
except OSError:
    sys.exit(f"FAIL {path}: no frame written")
colours = len(im.resize((64, 40)).getcolors(64 * 40))
if colours <= 40:
    sys.exit(f"FAIL {path}: blank frame ({colours} colours)")
print(f"ok   {path} ({colours} colours)")
PY
  done
done
exit $status
