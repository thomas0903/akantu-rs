#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-003…CI-006, CI-020: ctest → cargo test).
# test: cargo test --workspace --locked --no-fail-fast, every test binary accounted for;
# then the threaded crates again at 3 threads, finest grain (step `threads`)
#
# Steps: `cargo fetch --locked` (every package of Cargo.lock in the cargo
# registry: the licence tests `license_separation` / `license_guard` run
# `cargo tree --offline` for feature sets that the test build itself never
# downloads, e.g. `mpi` and `python`; without this step they passed only on
# machines whose registry earlier builds had filled),
# `cargo metadata --no-deps` and `cargo test --no-run
# --message-format=json` (the expected test binaries), then the full run
# (stdout and stderr merged inside the container so cargo's "Running" lines
# keep their order), then the verdict by ci/tools/analyze_tests.py
# against ci/tools/known-failures.txt: every test binary
# must have run and reported, and the failing tests must equal the reviewed
# baseline exactly (its only waiver). The analyser is
# REQUIRED: cargo's exit code alone would pass a test
# binary that exits 0 early (the UD-018 shape), so a tree without
# ci/tools/analyze_tests.py + known-failures.txt (or the directory
# AKANTU_CI_ANALYSER_DIR) makes the tier FAIL, never pass. A last step
# (`floor`, ci/tools/count_tests.py) fails a run that executed fewer result
# lines / tests than the floors in ci/floors.txt.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
# CI_TEST_TIER / CI_TEST_FEATURES / CI_TEST_FLOOR_KEY: the `exportall` tier
# (ci/tiers/exportall.sh) is this script run
# with the test-only feature `akantu-core/export-all` and its own floor.
tier="${CI_TEST_TIER:-test}"
floor_key="${CI_TEST_FLOOR_KEY:-test}"
features=()
if [ -n "${CI_TEST_FEATURES:-}" ]; then
  IFS=, read -r -a features <<< "$CI_TEST_FEATURES"
fi
ci_tier_begin "$tier"

if ! ci_backend_ready; then
  ci_skip test "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi

tooling="${AKANTU_CI_ANALYSER_DIR:-$here/tools}"
if [ ! -f "$tooling/analyze_tests.py" ] || [ ! -f "$tooling/known-failures.txt" ]; then
  ci_fail analyze "no $tooling/analyze_tests.py + known-failures.txt: without the analyser an early-exit test binary would pass on cargo's exit code alone; the test tier needs both files"
  ci_tier_end
  exit $?
fi
if ! ci_have python3; then
  ci_skip analyze "python3 is needed on the host for the analyser"
  ci_tier_end
  exit $?
fi

ci_step fetch -- cargo fetch --locked
ci_step metadata --stdout "$(ci_file metadata.json)" -- cargo metadata --no-deps --format-version 1 --locked
ci_step build --stdout "$(ci_file test-artifacts.jsonl)" -- cargo test --workspace --locked ${features[@]+"${features[@]}"} --no-run --message-format=json
# The analyser reads the libtest log under this exact name (test.log).
# 101 = test failures: accepted here only because the analyse step below
# judges them against the reviewed baseline (anything else fails there).
ci_step test --ok-exit "0 101" -- cargo test --workspace --locked --no-fail-fast ${features[@]+"${features[@]}"}

kf="$tooling/known-failures.txt"
sha="$( (sha256sum < "$kf" || shasum -a 256 < "$kf") 2> /dev/null)"
echo "baseline=$kf sha256=${sha%% *}" > "$(ci_file baseline.txt)"
if [ "$CI_DRY_RUN" = 1 ]; then
  ci_skip analyze "dry run"
  ci_skip floor "dry run"
else
  # analyze_tests.py inputs: summary.tsv (step, exit code, seconds) and
  # checks.txt in the job directory, plus the files written above.
  awk -F '\t' 'BEGIN { OFS = "\t" } { print $1, $3, $4 }' "$(ci_file steps.tsv)" > "$(ci_file summary.tsv)"
  : > "$(ci_file checks.txt)"
  ci_step analyze --host -- python3 "$tooling/analyze_tests.py" --job-dir "$CI_TIER_DIR" --repo "$CI_REPO" --known-failures "$kf"
  ci_step floor --host -- python3 -I "$here/tools/count_tests.py" "$(ci_file test.log)" --floors "$here/floors.txt" --key "$floor_key"
fi

# The thread-count invariant of the execution policy (D-123): the crates
# with threaded kernels run again with 3 threads and every batch split as
# finely as possible; every bitwise fixture and exec_threads.rs must give
# the one-thread bits.
if [ "$tier" = test ] && [ -f "$CI_REPO/crates/akantu-solid/Cargo.toml" ]; then
  ci_step threads -- env AKANTU_NUM_THREADS=3 AKANTU_EXEC_GRAIN=finest cargo test --locked -p akantu-solid -p akantu-solver -p akantu-fe -p akantu-cohesive
fi

ci_tier_end
