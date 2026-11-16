#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# mpiprof.sh: build the MPI+Scotch binary in the Scotch image, then perf
# profiles of rank 0 and rank 1 of the parallel cases (4 ranks)
set -u
# SUITE_WORK: the work directory (holds the checkout akantu-rs/, bin/, prof/);
# default: the current directory
W="$(cd "${SUITE_WORK:-.}" && pwd)"
TOOLS="$(cd "$(dirname "$0")" && pwd)"
cd "$W/akantu-rs" || exit 1
AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev AKANTU_RS_MPI_TARGET=/target/suite-setup \
  scripts/mpi/in-docker.sh cargo build --release -p akantu-bench --bin suite_setup --features mpi,scotch --locked || exit 1
mkdir -p "$W/bin" "$W/prof"
docker run --rm --user "$(id -u):$(id -g)" -v akantu-rs-mpi-target:/target:ro -v "$W/bin:/out" \
  akantu-rs-scotch:dev cp /target/suite-setup/release/suite_setup /out/suite_setup_mpi || exit 1
bin="$W/bin/suite_setup_mpi"
run() { # np stem args...
  np=$1; stem=$2; shift 2
  docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp --shm-size 2g --network none \
    -v "$W:$W" -w "$W" akantu-rs-scotch:dev \
    mpirun -n "$np" --bind-to core --map-by core "$bin" "$@" --out "$W/prof/$stem.jsonl" > "$W/prof/$stem.out" 2>&1 &
  job=$!
  declare -A rp=()
  for _ in $(seq 1 600); do
    rp=()
    for p in $(pgrep -f "suite_setup_mpi --case"); do
      [ "$(sudo readlink /proc/$p/exe)" = "$bin" ] || continue
      r=$(sudo cat /proc/$p/environ | tr '\0' '\n' | sed -n 's/^OMPI_COMM_WORLD_RANK=//p')
      [ -n "$r" ] && rp[$r]=$p
    done
    [ "${#rp[@]}" -ge "$np" ] && break
    sleep 0.05
  done
  perfs=()
  for r in 0 1; do
    [ -n "${rp[$r]:-}" ] || continue
    sudo perf record -q -F 499 --call-graph dwarf,32768 -p "${rp[$r]}" -o "$W/prof/$stem-r$r.data" > /dev/null 2>&1 &
    perfs+=($!)
  done
  wait "$job"; echo "$stem: exit $? ranks ${!rp[*]}"
  for p in "${perfs[@]}"; do sudo kill -INT "$p" 2>/dev/null; done
  wait
  for r in 0 1; do
    [ -f "$W/prof/$stem-r$r.data" ] || continue
    cp "$W/prof/$stem.jsonl" "$W/prof/$stem-r$r.jsonl" 2>/dev/null
    "$TOOLS/report.sh" "$W/prof/$stem-r$r"
  done
}
run 4 dist_mesh_tet4-l-p4 --case dist_mesh_tet4 --size l --loop-seconds 20
run 4 dist_facets_tet4-m-p4 --case dist_facets_tet4 --size m --loop-seconds 10
run 4 ghost_nodes_tet4-m-p4 --case ghost_nodes_tet4 --size m --loop-seconds 10
run 4 ghost_elements_tet4-m-p4 --case ghost_elements_tet4 --size m --loop-seconds 10
run 4 explicit_step_dist_tet4-m-p4 --case explicit_step_dist_tet4 --size m --loop-seconds 10
run 4 reduce_scalars-m-p4 --case reduce_scalars --size m --loop-seconds 5
echo done
