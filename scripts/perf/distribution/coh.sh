#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# coh.sh <series> <reps> <ranks> <variant>...: the cohesive driver with coh-input.dat (insertion off,
# the port's Eigen-order static solver), variants interleaved and rotated; one directory per run.
set -u
series=$1 reps=$2 ranks=$3; shift 3
variants=("$@"); nv=${#variants[@]}
base=~/w/dist/runs/$series; mkdir -p "$base"
for ((r = 0; r < reps; r++)); do
  for p in $ranks; do
    for ((k = 0; k < nv; k++)); do
      v=${variants[$(( (k + r) % nv ))]}
      d=$base/$v/np$p/rep$r; mkdir -p "$d"; cp ~/w/dist/coh-input.dat "$d/input.dat"
      docker run --rm --cpuset-cpus 0-7 --shm-size 2g -e SCOTCH_PTHREAD_NUMBER=1 -v ~/w/dist/bin/"$v":/bench:ro -v ~/w/dist/rs/fixtures/perf_cohesive/meshes:/m:ro -v "$d":/out -w /out akantu-rs-scotch:dev \
        mpirun --allow-run-as-root -n "$p" --bind-to core --map-by core -x SCOTCH_PTHREAD_NUMBER /bench/cohesive_extrinsic --aka_input_file input.dat --quiet > "$d/out.txt" 2> "$d/err.txt"
      echo $? > "$d/exit"
      sudo chown -R "$(id -u)" "$d" 2>/dev/null
    done
  done
done
echo "COH_DONE $series"
