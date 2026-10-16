#!/usr/bin/env bash
set -euo pipefail
cd /root/code/open-4x
xvfb-run -a -s "-screen 0 1440x900x24" /root/code/open-4x/tools/tmp-stack-inner.sh
magick identify -format "%f %wx%h sd=%[fx:standard_deviation] colors=%k\n" \
  /tmp/open4x-lineup-clean-impact.png \
  /tmp/open4x-lineup-clean-mid.png \
  /tmp/open4x-lineup-clean-result.png
