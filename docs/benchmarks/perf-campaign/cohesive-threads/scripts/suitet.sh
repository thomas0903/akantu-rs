#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# suitet.sh <outdir> <cases> <size> <reps> <label=binary:threads:cores>...: interleaved, rotated runs of a
# suite binary with --threads, pinned to <cores>; one JSON file per (label, rep).
set -u
out=$1; cases=$2; size=$3; reps=$4; shift 4
vs=("$@"); n=${#vs[@]}; mkdir -p $out
for ((r=0; r<reps; r++)); do
  for ((k=0; k<n; k++)); do
    v=${vs[$(( (k + r) % n ))]}; label=${v%%=*}; spec=${v#*=}
    bin=${spec%%:*}; rest=${spec#*:}; threads=${rest%%:*}; cores=${rest#*:}
    taskset -c $cores $bin --case $cases --size $size --reps 1 --threads $threads --out $out/$label-rep$r.json > $out/$label-rep$r.log 2>&1
  done
done
