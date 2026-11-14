#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-009: register_example / AKANTU_TEST_EXAMPLES).
# examples: the PARALLEL examples (parallel_2d, phase_field_parallel) on real MPI at 2 ranks, release build
#
# Upstream registers its examples as tests and runs the ones marked PARALLEL
# (`parallel_2d`, `phase_field_parallel`) on 2 ranks (AKANTU_TEST_EXAMPLES).
# The serial examples' whole-run comparisons are the integration tests of the
# `akantu` crate, run by `cargo test --release -p akantu` (the `facade` run of
# the `release` tier: debug builds skip the whole-run cases). This tier builds
# the two parallel examples with the real-MPI feature and launches each with
# `mpirun -np 2` (every rank must exit 0 and write its output files;
# phase_field_parallel runs 20 of its 999 steps, `--steps`, as the example
# README documents for reduced runs). Docker backend (the MPI image) or a host
# with Open MPI.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin examples
if ! ci_backend_ready; then
  ci_skip parallel-examples "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
elif [ "$CI_BACKEND" = host ] && ! ci_have mpirun; then
  ci_skip parallel-examples "no mpirun on the host (the docker backend uses the MPI image)"
else
  # shellcheck disable=SC2016
  ci_step parallel-examples -- sh -c '
    set -eu
    cargo build --release -p akantu --features mpi --locked --example parallel_2d --example phase_field_parallel
    d="${CARGO_TARGET_DIR:-target}/release/examples"
    o="$(mktemp -d)"
    mpirun -np 2 --oversubscribe --bind-to none "$d/parallel_2d" --output-dir "$o/parallel_2d"
    mpirun -np 2 --oversubscribe --bind-to none "$d/phase_field_parallel" --steps 20 --output-dir "$o/phase_field_parallel"
    for e in parallel_2d phase_field_parallel; do
      test -n "$(ls "$o/$e")" || { echo "no output files of $e"; exit 1; }
      echo "$e: $(ls "$o/$e" | wc -l) output files"
    done
  '
fi
ci_tier_end
