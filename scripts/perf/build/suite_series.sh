#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): one repetition of an
# interleaved series of benchmark-suite binaries in several build
# configurations: for every case, every variant back to back in an order
# rotated by the repetition number, one process per run, pinned to CORE.
#   suite_series.sh OUT_DIR REP AREA SIZE "VARIANT..." "CASE..."
# Binaries: BIN_ROOT/<variant>/suite_<area>; output:
# OUT_DIR/<variant>/<case>.r<REP>.jsonl (the suite's JSON lines).
set -euo pipefail
out=$1 r=$2 area=$3 size=$4
read -r -a variants <<< "$5"
read -r -a cases <<< "$6"
broot="${BIN_ROOT:?}" core="${CORE:-2}"
n=${#variants[@]}
order=(); for ((i = 0; i < n; i++)); do order+=("${variants[$(( (i + r - 1) % n ))]}"); done
for c in "${cases[@]}"; do
  for v in "${order[@]}"; do
    mkdir -p "$out/$v"
    taskset -c "$core" "$broot/$v/suite_$area" --case "$c" --size "$size" --reps 1 --out "$out/$v/$c.r$r.jsonl" > /dev/null
  done
done
echo "{\"rep\":$r,\"area\":\"$area\",\"order\":\"${order[*]}\",\"utc\":\"$(date -u +%FT%TZ)\",\"load\":\"$(cut -d' ' -f1-3 /proc/loadavg)\"}" >> "$out/rounds.jsonl"
