#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion step); not ported from Akantu.
# suite.sh <outdir> <cases> <size> <reps> <label=binary>...: interleaved, rotated A/B of suite binaries,
# each run pinned to core 2, one JSON file per (label, rep)
set -u
out=$1; cases=$2; size=$3; reps=$4; shift 4
vs=("$@"); n=${#vs[@]}
mkdir -p $out
for ((r=0; r<reps; r++)); do
  for ((k=0; k<n; k++)); do
    v=${vs[$(( (k + r) % n ))]}; label=${v%%=*}; bin=${v#*=}
    taskset -c 2 $bin --case $cases --size $size --reps 1 --out $out/$label-rep$r.json > $out/$label-rep$r.log 2>&1
  done
done
