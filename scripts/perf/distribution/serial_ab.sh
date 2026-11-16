#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# serial_ab.sh <series> <case> <reps> <size> <variant>...: interleaved, rotated A/B of serial suite_setup
# builds (no mpi feature), each run pinned to vCPU 2; ~/w/dist/runs/<series>/<variant>/np1/<case>.r<rep>.jsonl
set -u
series=$1 case=$2 reps=$3 size=$4; shift 4
variants=("$@"); nv=${#variants[@]}
out=~/w/dist/runs/$series
for ((r = 0; r < reps; r++)); do
  for ((k = 0; k < nv; k++)); do
    v=${variants[$(( (k + r) % nv ))]}
    mkdir -p "$out/$v/np1"
    docker run --rm --cpuset-cpus 2 -v ~/w/dist/bin/"$v":/bench:ro akantu-rs-scotch:dev \
      /bench/suite_setup --case "$case" --size "$size" --reps 1 > "$out/$v/np1/$case.r$r.jsonl" 2> /dev/null
  done
done
echo "SERIAL_AB_DONE $series"
