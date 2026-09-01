#!/usr/bin/env python3
"""Decimate a flight CSV for committing: telemetry, not reconstruction
(flights regenerate deterministically from the source record; the
checkpoint chain covers replay-without-recompute). Keeps every row of
the launch/transient region (first 1000 hops, where log-axis plots
need density), every 100th row thereafter, and the final row.

Usage: decimate_flight.py <in.csv> <out.csv>"""
import sys

with open(sys.argv[1]) as f:
    rows = f.readlines()
header, body = rows[0], rows[1:]
keep = [r for i, r in enumerate(body) if i < 1000 or i % 100 == 0]
if body and (not keep or keep[-1] != body[-1]):
    keep.append(body[-1])
with open(sys.argv[2], "w") as f:
    f.write(header)
    f.writelines(keep)
print(f"{sys.argv[2]}: {len(keep)} of {len(body)} rows")
