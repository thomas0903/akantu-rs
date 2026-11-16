#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# pyab.sh <out-dir> <reps> <cases> : interleaved, rotated A/B of the Python suite, venv-a vs venv-b, pinned to core 2
out=$1; reps=$2; cases=$3
mkdir -p $out
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1
for r in $(seq 0 $((reps-1))); do
  if [ $((r % 2)) -eq 0 ]; then order="venv-a venv-b"; else order="venv-b venv-a"; fi
  for v in $order; do
    c=$cases; [ $v = venv-b ] && c="$cases,dirichlet_group_tri3"
    taskset -c 2 ~/w/io/pysuite.sh $v --case $c --size m --reps 1 > $out/$v.r$r.jsonl 2> $out/$v.r$r.err
  done
done
echo done
