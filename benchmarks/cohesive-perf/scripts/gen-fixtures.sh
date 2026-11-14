#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: correctness reference for the future
# Rust driver (fixtures/perf_cohesive/, see its README.md).
#
# Usage: scripts/gen-fixtures.sh [--steps 1500] [--reps 3] [--no-run]
#                                [--out perf_cohesive_<platform>] [--replace]
#
# Runs (frozen mesh fixtures/perf_cohesive/meshes/cube-h0.12.msh, energies on,
# 17 significant digits, AKA_SEED=1, SCOTCH_PTHREAD_NUMBER=1):
#   strict  coh_np1          cohesive, 1 rank                               x reps
#   strict  ela_np1          elastic (no insertion), 1 rank                 x reps
#   strict  coh_np2          cohesive, 2 ranks, upstream local node 3,
#                            export_partition=true                          x reps
#   strict  coh_np2_global   cohesive, 2 ranks, blocked_node=global,
#                            export_partition=true                          x reps
#   strict  coh_np2_distribute  as coh_np2 with upstream mesh.distribute()  x 1
#   strict  coh_np1_fnte_joss   coh_np1 with the JOSS fillNodesToElements
#                            emulation (must give identical physics)        x 1
#   default coh_np1, ela_np1 (FP perturbation: FMA contraction, D-031)      x 1
# then scripts/make_fixtures.py validates every attempt (UD-216 / MUMPS
# messages, truncated histories, identity, counts, build records), enforces
# the serial repeatability and the equivalences, labels every file
# (qualification.json) and only then writes fixtures/<out>/ (default
# perf_cohesive; existing cases are replaced only with --replace) and the
# SHA256SUMS lines.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

steps=1500; reps=3; run=1; out=perf_cohesive; replace=()
while [ $# -gt 0 ]; do
  case "$1" in
    --steps) [ $# -ge 2 ] || exit 2; steps="$2"; shift 2 ;;
    --reps) [ $# -ge 2 ] || exit 2; reps="$2"; shift 2 ;;
    --out) [ $# -ge 2 ] || exit 2; out="$2"; shift 2 ;;
    --replace) replace=(--replace); shift ;;
    --no-run) run=0; shift ;;
    *) echo "ERROR: unknown argument $1" >&2; exit 2 ;;
  esac
done
perf_check_posint steps "${steps}" || exit 2
perf_check_posint reps "${reps}" || exit 2
out_re='^perf_cohesive(_[a-z0-9]+)*$'
[[ "${out}" =~ ${out_re} ]] || { echo "ERROR: --out must be perf_cohesive[_<tag>]" >&2; exit 2; }

mesh_host="${REPO_ROOT}/fixtures/perf_cohesive/meshes/cube-h0.12.msh"
mesh="/perf/meshes/fixture-cube-h0.12.msh"
common=(output_energy=true energy_precision=17)

if [ "${run}" = 1 ]; then
  for config in strict default; do
    docker run --rm -v "${PERF_VOLUME}:/perf:ro" "${PERF_IMAGE}" \
      test -x "/perf/builds/${config}/cohesive_extrinsic" || "${PERF_SCRIPTS_DIR}/build.sh" "${config}"
  done
  # Copy the frozen mesh into the volume (the driver reads it from there).
  perf_docker rm -rf /perf/runs/fixtures
  perf_volume_write "${mesh}" < "${mesh_host}"

  one() {  # one <config> <case> <rep> <ranks> <material> [key=value...]
    local config="$1" case="$2" rep="$3" n="$4" material="$5"; shift 5
    export AKANTU_RS_FILL_NODES_TO_ELEMENTS=upstream
    case "${case}" in *_fnte_joss) export AKANTU_RS_FILL_NODES_TO_ELEMENTS=joss ;; esac
    echo "$(date '+%H:%M:%S') fixtures ${config} ${case} rep${rep}"
    # a failed attempt is recorded and reported by make_fixtures.py
    perf_docker bash /harness/scripts/in-container/run-one.sh \
      "/perf/runs/fixtures/${config}/${case}/rep${rep}" "${config}" "${n}" "${material}" \
      "${mesh}" "${steps}" "${common[@]}" "$@" \
      || echo "$(date '+%H:%M:%S') attempt failed: ${config} ${case} rep${rep}" >&2
  }
  for rep in $(seq 1 "${reps}"); do
    one strict coh_np1 "${rep}" 1 cohesive
    one strict ela_np1 "${rep}" 1 elastic
    one strict coh_np2 "${rep}" 2 cohesive export_partition=true
    one strict coh_np2_global "${rep}" 2 cohesive export_partition=true blocked_node=global
  done
  one strict coh_np2_distribute 1 2 cohesive
  one strict coh_np1_fnte_joss 1 1 cohesive
  one default coh_np1 1 1 cohesive
  one default ela_np1 1 1 elastic
fi

python3 -I "${PERF_SCRIPTS_DIR}/make_fixtures.py" --steps "${steps}" --reps "${reps}" \
  --out "${out}" "${replace[@]+"${replace[@]}"}"
