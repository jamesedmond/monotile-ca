#!/bin/sh
# Space-time worldtube figure (three views of hat glider s21).
# Prerequisites: the web dev server (cd web && npm run dev) serving
# localhost:5173 with the current wasm pkg, and a directory with
# playwright installed (npm i playwright && npx playwright install
# chromium) — default /tmp/pw-monotile, override with PWDIR. The
# script is copied next to that install because ESM resolves bare
# imports from the script's own path, not NODE_PATH.
# Decisions: WRITEUP-PLAN (SpaceTimePanel), angle tests I/M/P
# (2026-08-05); camera/generation parameters documented in capture.mjs.
set -e
FIGDIR=$(cd "$(dirname "$0")" && pwd)
PWDIR="${PWDIR:-/tmp/pw-monotile}"
[ -d "$PWDIR/node_modules/playwright" ] || {
  echo "playwright not found in $PWDIR — npm i playwright && npx playwright install chromium" >&2
  exit 1
}
cp "$FIGDIR/capture.mjs" "$PWDIR/fig-worldtube-capture.mjs"
FIG_OUT="$FIGDIR" node "$PWDIR/fig-worldtube-capture.mjs"
