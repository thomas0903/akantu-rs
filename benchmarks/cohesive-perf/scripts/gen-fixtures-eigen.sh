#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: Eigen-solver variant of
# the correctness reference, fixtures/perf_cohesive/{coh,ela}_np1_eigen/.
#
# Usage: scripts/gen-fixtures-eigen.sh [--steps 1500] [--reps 3] [--no-run] [--allow-dirty]
#
# Same runs as `coh_np1` / `ela_np1` of scripts/gen-fixtures.sh (strict build,
# frozen h = 0.12 mesh, energies with 17 digits, AKA_SEED=1) except that the
# static solve uses Eigen's SparseLU (`sparse_solver_type = eigen` in the
# `time_step_solver static` section) instead of upstream `_auto` = MUMPS.
# The port's `eigen` backend is a bitwise translation of that solver, so the
# Rust driver can be compared bitwise without MUMPS. Leaves the existing
# fixture cases untouched; scripts/make_fixtures_eigen.py validates every run
# (histories, counts, finiteness, Eigen solve) and writes the two cases,
# their determinism record and the SHA256SUMS lines.
#
# Refused before any run (make_fixtures_eigen.py --preflight): a harness
# tree with uncommitted changes (unless --allow-dirty, then recorded), and on
# x86_64 an oracle install without the D-050 Eigen cache-size pin
# (docs/ORACLE_BUILD.md): unpinned, the x86_64 oracle differs from the
# committed cases; pinned, it reproduces them byte for byte.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

steps=1500; reps=3; run=1; dirty=()
while [ $# -gt 0 ]; do
  case "$1" in
    --steps) steps="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --no-run) run=0; shift ;;
    --allow-dirty) dirty=(--allow-dirty); shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
perf_check_posint steps "${steps}" || exit 2
perf_check_posint reps "${reps}" || exit 2

mesh_host="${REPO_ROOT}/fixtures/perf_cohesive/meshes/cube-h0.12.msh"
mesh="/perf/meshes/fixture-cube-h0.12.msh"
common=(output_energy=true energy_precision=17)

if [ "${run}" = 1 ]; then
  docker run --rm -v "${PERF_VOLUME}:/perf:ro" "${PERF_IMAGE}" \
    test -x "/perf/builds/strict/cohesive_extrinsic" || "${PERF_SCRIPTS_DIR}/build.sh" strict
  python3 -I "${PERF_SCRIPTS_DIR}/make_fixtures_eigen.py" --preflight ${dirty[@]+"${dirty[@]}"}
  perf_docker rm -rf /perf/runs/fixtures-eigen
  perf_volume_write "${mesh}" < "${mesh_host}"
  export PERF_STATIC_SPARSE_SOLVER=eigen
  for rep in $(seq 1 "${reps}"); do
    for spec in "coh_np1_eigen cohesive" "ela_np1_eigen elastic"; do
      case_name="${spec% *}"; material="${spec#* }"
      echo "$(date '+%H:%M:%S') fixtures-eigen strict ${case_name} rep${rep}"
      perf_docker bash /harness/scripts/in-container/run-one.sh \
        "/perf/runs/fixtures-eigen/strict/${case_name}/rep${rep}" strict 1 "${material}" \
        "${mesh}" "${steps}" "${common[@]}"
    done
  done
fi

python3 -I "${PERF_SCRIPTS_DIR}/make_fixtures_eigen.py" --steps "${steps}" --reps "${reps}" \
  ${dirty[@]+"${dirty[@]}"}
