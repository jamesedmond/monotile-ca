#!/bin/sh
# Heading-residual convergence figure. Pure stdlib Python; data and
# provenance in generate.py.
set -e
cd "$(dirname "$0")"
python3 generate.py
