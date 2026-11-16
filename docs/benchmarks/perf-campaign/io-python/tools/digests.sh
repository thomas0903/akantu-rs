#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# digests.sh size "binA binB ..." case... : one run per binary and case, prints digests
set -uo pipefail
size="$1"; bins="$2"; shift 2
for c in "$@"; do
  line="$c"
  for b in $bins; do
    d=$(~/w/io/bin/$b --case "$c" --size "$size" --reps 1 2>/dev/null | python3 -c "import sys,json; print(sorted({json.loads(l)[\"digest\"] for l in sys.stdin if l.startswith(\"{\")}))")
    line="$line $b=$d"
  done
  echo "$line"
done
