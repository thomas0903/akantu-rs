#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-028: build profiles that change behaviour).
# release: the optimised-build test runs that docs/TESTING.md lists as release tier
#
# Upstream CI runs Coverage (debug, AKANTU_NDEBUG off) and Release builds
# (CI-028); the port's debug fast tier is complemented by the release-mode
# runs documented per area in docs/TESTING.md (overflow checks and
# debug assertions differ, and some cases run only optimised):
#   cohesive (all fixture cases), linalg, solid 3-D implicit dynamics,
#   core + parser, dumpers, and the facade crate (the examples' whole runs);
#   and akantu-solid + akantu-solver at 3 threads, finest grain (the
#   thread-count invariant of D-123; the `test` tier repeats only
#   exec_threads.rs and the solid fixture tests with threads).
#
# Each run is judged like the `test` tier: its own
# `--no-run` build, then `cargo test --release`, then
# ci/tools/analyze_tests.py against the ONE waiver list
# ci/tools/known-failures.txt (empty on main; ci/tools/release_scope.py
# scopes the metadata and the baseline to the packages/targets of the run),
# then the test-count floor of ci/floors.txt. A release run therefore no
# longer fails permanently on a failure the reviewed baseline already
# waives, and a binary that exits early is a failure. The analyser is
# required: the tier fails without it (AKANTU_CI_ANALYSER_DIR may name
# another directory holding analyze_tests.py and known-failures.txt).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin release
tooling="${AKANTU_CI_ANALYSER_DIR:-$here/tools}"
if ! ci_backend_ready; then
  ci_skip release "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi
if [ ! -f "$tooling/analyze_tests.py" ] || [ ! -f "$tooling/known-failures.txt" ]; then
  ci_fail analyze "no $tooling/analyze_tests.py + known-failures.txt: the release runs are judged by the analyser and the one waiver list"
  ci_tier_end
  exit $?
fi
if ! ci_have python3; then
  ci_skip analyze "python3 is needed on the host for the analyser"
  ci_tier_end
  exit $?
fi
kf="$tooling/known-failures.txt"
release_env=()
sha="$( (sha256sum < "$kf" || shasum -a 256 < "$kf") 2> /dev/null)"
echo "baseline=$kf sha256=${sha%% *}" > "$(ci_file baseline.txt)"

# release_run <name> <spec,spec,...> <cargo test selection words...>
#   (the array release_env holds extra VAR=value words for both cargo runs)
#   spec: PKG or PKG:test=A,B (ci/tools/release_scope.py); the selection is
#   what follows `cargo test --release --locked` (e.g. -p akantu-linalg).
release_run() {
  local name="$1" specs="$2" d s
  # CI_RELEASE_ONLY="facade linalg": run only these runs (a developer's or an
  # agent's partial run; the workflows never set it).
  # shellcheck disable=SC2086
  if [ -n "${CI_RELEASE_ONLY:-}" ] && ! printf ' %s ' $CI_RELEASE_ONLY | grep -q " $name "; then return 0; fi
  local spec_args
  spec_args=()
  shift 2
  d="$(ci_file "$name")"
  mkdir -p "$d"
  ci_step "$name:metadata" --stdout "$d/metadata.full.json" -- cargo metadata --no-deps --format-version 1 --locked
  ci_step "$name:build" --stdout "$d/test-artifacts.jsonl" -- env COH_ALL=1 ${release_env[@]+"${release_env[@]}"} cargo test --release --locked --no-run --message-format=json "$@"
  ci_step "$name:test" --ok-exit "0 101" -- env COH_ALL=1 ${release_env[@]+"${release_env[@]}"} cargo test --release --locked --no-fail-fast "$@"
  if [ "$CI_DRY_RUN" = 1 ]; then
    ci_skip "$name:analyze" "dry run"
    return 0
  fi
  for s in $(printf '%s' "$specs" | tr ',' ' '); do spec_args+=("$s"); done
  # (a spec holding a comma list of tests uses ';' here: PKG:test=A;B)
  ci_step "$name:scope" --host -- python3 -I "$here/tools/release_scope.py" --metadata "$d/metadata.full.json" --known-failures "$kf" --out "$d" "${spec_args[@]//;/,}"
  cp "$(ci_file "$(ci_slug "$name:test").log")" "$d/test.log" 2> /dev/null || : > "$d/test.log"
  # analyze_tests.py reads step rows named metadata / build / test.
  awk -F '\t' -v n="$name" 'BEGIN { OFS = "\t" }
    $1 == n ":metadata" { print "metadata", $3, $4 }
    $1 == n ":build" { print "build", $3, $4 }
    $1 == n ":test" { print "test", $3, $4 }' "$(ci_file steps.tsv)" > "$d/summary.tsv"
  : > "$d/checks.txt"
  ci_step "$name:analyze" --host -- python3 "$tooling/analyze_tests.py" --job-dir "$d" --repo "$CI_REPO" --known-failures "$d/known-failures.txt"
  ci_step "$name:floor" --host -- python3 -I "$here/tools/count_tests.py" "$d/test.log" --floors "$here/floors.txt" --key "release-$name"
}

release_run cohesive akantu-cohesive -p akantu-cohesive
release_run linalg akantu-linalg -p akantu-linalg
release_run solid-ported "akantu-solid:test=model_ported_tests" -p akantu-solid --test model_ported_tests
release_run core-parser akantu-core,akantu-parser -p akantu-core -p akantu-parser
# The facade crate in release: the examples' whole-run oracle comparisons and the
# default-mode tests past the first contact of bloc_friction exist only in
# optimised builds (gate line `cargo test --release -p akantu`).
# The `gates` tier skips that line.
release_run facade akantu -p akantu
release_env=(AKANTU_NUM_THREADS=3 AKANTU_EXEC_GRAIN=finest)
release_run threads akantu-solid,akantu-solver -p akantu-solid -p akantu-solver
release_env=()
release_run dumpers "akantu-io:test=dump_oracle;test_dumper;dump_smm;dump_properties;dump_review_regressions" -p akantu-io --test dump_oracle --test test_dumper --test dump_smm --test dump_properties --test dump_review_regressions
ci_tier_end
