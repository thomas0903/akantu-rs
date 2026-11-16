#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# ab.sh <series> <bin-name> <cases> <ranks> <reps> <size> <variant>... : interleaved, rotated A/B of
# suite binaries (~/w/dist/bin/<variant>/<bin-name>) under mpirun in the Scotch image; rank r on core r.
# Output ~/w/dist/runs/<series>/<variant>/np<P>/<case>.r<rep>.jsonl; EXTRA_ARGS are passed to the binary.
set -u
series=$1 bin=$2 cases=$3 ranks=$4 reps=$5 size=$6; shift 6
variants=("$@"); nv=${#variants[@]}
out=~/w/dist/runs/$series; mkdir -p "$out"
commits=""
for v in "${variants[@]}"; do commits+="$v=$(cat ~/w/dist/bin/"$v"/COMMIT 2>/dev/null) "; done
echo "{\"date_utc\":\"$(date -u +%FT%TZ)\",\"cpu\":\"$(lscpu | sed -n 's/^Model name: *//p')\",\"bin\":\"$bin\",\"cases\":\"$cases\",\"ranks\":\"$ranks\",\"reps\":$reps,\"size\":\"$size\",\"variants\":\"${variants[*]}\",\"commits\":\"$commits\"}" >> "$out/meta.jsonl"
IFS=, read -r -a cl <<<"$cases"
read -r -a extra <<<"${EXTRA_ARGS:-}"
for ((r = 0; r < reps; r++)); do
  for c in "${cl[@]}"; do
    for p in $ranks; do
      for ((k = 0; k < nv; k++)); do
        v=${variants[$(( (k + r) % nv ))]}
        mkdir -p "$out/$v/np$p"
        docker run --rm --cpuset-cpus 0-7 --shm-size 2g -e SCOTCH_PTHREAD_NUMBER=1 -v ~/w/dist/bin/"$v":/bench:ro akantu-rs-scotch:dev \
          mpirun --allow-run-as-root -n "$p" --bind-to core --map-by core -x SCOTCH_PTHREAD_NUMBER "/bench/$bin" --case "$c" --size "$size" --reps 1 ${extra[@]+"${extra[@]}"} \
          > "$out/$v/np$p/$c.r$r.jsonl" 2> "$out/$v/np$p/$c.r$r.err"
      done
    done
  done
done
echo "AB_DONE $series"
