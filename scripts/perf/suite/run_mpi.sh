#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the parallel cases of a
# benchmark-suite binary built with the `mpi` feature in the project's MPI
# image (e.g. built with scripts/mpi/in-docker.sh and AKANTU_RS_MPI_TARGET, then
# copied out), at several rank counts. The binary's directory is mounted
# read-only at /bench in the container.
#
#   run_mpi.sh --out DIR --bin HOST_PATH/suite_solid \
#              --cases a,b --ranks "1 2 4 8" --reps N --size m \
#              [--image akantu-rs-mpi:dev] [--cpus 0-3] [--wrap "tjob LABEL --"]
#              [--rep-offset K]
#
# Each run: one container with the binary's directory read-only and the
# cores --cpus (default 0-3: the tjob cores of the measurement machine, up
# to 4 ranks; 0-7 under xjob for 8 ranks) as --cpuset-cpus, `mpirun -n P --bind-to core --map-by core`
# (rank r on core r), output DIR/np<P>/<case>.r<rep>.jsonl (the layout of
# run_ab.sh, variant = np<P>, so summarize.py reads DIR). The rank counts are
# rotated per repetition; each run is wrapped by --wrap.
set -euo pipefail
out="" bin="" cases="" ranks="1 2 4 8" reps=5 size=m image=akantu-rs-mpi:dev wrap="" cpus=0-3 offset=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --bin) bin="$2"; shift 2 ;;
    --cases) cases="$2"; shift 2 ;;
    --ranks) ranks="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --image) image="$2"; shift 2 ;;
    --wrap) wrap="$2"; shift 2 ;;
    --cpus) cpus="$2"; shift 2 ;;
    --rep-offset) offset="$2"; shift 2 ;;
    *) echo "run_mpi.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$out" ] && [ -n "$bin" ] && [ -n "$cases" ] || { sed -n 8,11p "$0" >&2; exit 2; }
[ -x "$bin" ] || { echo "run_mpi.sh: $bin is not an executable file" >&2; exit 2; }
bindir="$(cd "$(dirname "$bin")" && pwd)"; binname="$(basename "$bin")"
IFS=, read -r -a case_list <<<"$cases"
read -r -a rank_list <<<"$ranks"
read -r -a wrapper <<<"$wrap"
mkdir -p "$out"
nr=${#rank_list[@]}
{
  echo "{\"date_utc\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"image\":\"$image\",\"image_id\":\"$(docker image inspect -f '{{.Id}}' "$image")\",\"bin\":\"$bin\",\"sha256\":\"$(sha256sum "$bin" | cut -d" " -f1)\",\"cpu\":\"$(lscpu | sed -n 's/^Model name: *//p' | head -1)\",\"ranks\":\"$ranks\",\"size\":\"$size\",\"reps\":$reps,\"cpus\":\"$cpus\"}"
} > "$out/meta.r$offset.$$.json"
for ((r = offset; r < offset + reps; r++)); do
  for c in "${case_list[@]}"; do
    for ((k = 0; k < nr; k++)); do
      p=${rank_list[$(( (k + r) % nr ))]}
      mkdir -p "$out/np$p"
      f="$out/np$p/$c.r$r.jsonl"
      cmd=(docker run --rm --cpuset-cpus "$cpus" --memory 24g --memory-swap 24g --shm-size 2g
           -v "$bindir:/bench:ro" "$image"
           mpirun -n "$p" --bind-to core --map-by core "/bench/$binname" --case "$c" --size "$size" --reps 1)
      echo "[run_mpi] rep $r case $c np $p" >&2
      if [ ${#wrapper[@]} -gt 0 ]; then
        "${wrapper[@]}" "${cmd[@]}" > "$f"
      else
        "${cmd[@]}" > "$f"
      fi
    done
  done
done
echo "[run_mpi] done: $out" >&2
