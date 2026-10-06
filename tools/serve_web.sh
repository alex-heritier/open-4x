#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
unset NO_COLOR
exec trunk serve "$@"
