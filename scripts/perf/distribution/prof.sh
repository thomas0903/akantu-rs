#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# prof.sh <label> <bin> <np> <case> <delay> <dur> <cpu-list> [dwarf|fp] [extra suite args]: perf record
# (from the host) of a running suite_setup case; LOOP overrides --loop-seconds (default delay+dur+5).
set -u
label=$1 bin=$2 np=$3 case=$4 delay=$5 dur=$6 cpus=$7 mode=${8:-dwarf}; shift 8 || true
out=~/w/dist/prof/$label; mkdir -p "$out"
docker run -d --name "prof-$label" --rm --cpuset-cpus 0-7 --shm-size 2g -e SCOTCH_PTHREAD_NUMBER=1 -v ~/w/dist/bin/"$bin":/bench:ro akantu-rs-scotch:dev \
  mpirun --allow-run-as-root -n "$np" --bind-to core --map-by core -x SCOTCH_PTHREAD_NUMBER /bench/suite_setup --case "$case" --size m --reps 1 --loop-seconds "${LOOP:-$((delay+dur+5))}" "$@" > /dev/null
sleep "$delay"
if [ "$mode" = dwarf ]; then cg=(--call-graph "dwarf,32768"); else cg=(--call-graph fp); fi
sudo perf record -F 999 -a -C "$cpus" "${cg[@]}" -o "$out/perf.data" -- sleep "$dur" 2> "$out/record.err"
docker logs "prof-$label" > "$out/run.jsonl" 2>&1 || true
docker wait "prof-$label" >/dev/null 2>&1 || true
sudo chown "$(id -u)" "$out/perf.data"
echo "PROF_DONE $label"
