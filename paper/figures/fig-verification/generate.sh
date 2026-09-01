#!/bin/sh
# Verification figure (log-log generations-to-boundary). Pure stdlib
# Python; data constants + provenance in generate.py.
set -e
cd "$(dirname "$0")"
python3 generate.py
