#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-020: Coverage build, gcovr, D-085 U5).
# coverage: line coverage of the tests with cargo-llvm-cov (LLVM source-based coverage); a report, no threshold
#
# Upstream's Coverage build (`CMAKE_BUILD_TYPE=Coverage`, gcovr) has this
# counterpart: cargo-llvm-cov (MIT OR Apache-2.0), version CI_LLVM_COV_VERSION
# (default 0.9.1, released 2026-09-06), installed with `cargo install --locked`
# from crates.io into a tools directory (never into the repository), with the
# rustup component `llvm-tools-preview` of the pinned toolchain. The install
# step runs when CI_INSTALL_TOOLCHAINS=1 (the workflow sets it; a developer
# machine without the tool gets a skip, i.e. an incomplete tier, never a pass).
#
# What it runs: `cargo fetch --locked` first (every package of Cargo.lock in
# the registry: the licence tests run `cargo tree --offline` for feature sets
# the instrumented build never downloads; ci/tiers/test.sh), then `cargo llvm-cov --locked --no-report <selection>` (the test
# suites of the selection built with instrumentation and executed; a failing
# test fails the step, it is never ignored), then `cargo llvm-cov report`
# (per-file table in the step log; with the host backend also lcov.info and
# summary.json in the tier directory). The default selection is `--workspace`;
# extra arguments after `--` of run-all.sh replace it (`-- -p akantu-core`).
# Doc tests are not instrumented (stable rustc has no doctest coverage).
# The instrumented debug build is slow and large: a nightly / on-demand job,
# not part of any pull-request gate. No threshold is applied
# (`--fail-under-lines` would be a project decision, listed in
# ci/github/README.md, "Repository settings"): the job reports.
# Backend docker runs in the project image (target and tools in the cargo
# target volume); host runs with the host's rustup.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin coverage
ver="${CI_LLVM_COV_VERSION:-0.9.1}"
if ! ci_backend_ready; then
  ci_skip coverage "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi
if [ "$CI_BACKEND" = docker ]; then
  tools="/target/llvm-cov-tools-$ver"
  report_dir=""
else
  tools="${CI_COVERAGE_TOOLS:-${XDG_CACHE_HOME:-$HOME/.cache}/akantu-ci-tools/llvm-cov-$ver}"
  report_dir="$CI_TIER_DIR"
fi
sel=("$@")
[ "${#sel[@]}" -gt 0 ] || sel=(--workspace)

if [ "${CI_INSTALL_TOOLCHAINS:-0}" = 1 ]; then
  # shellcheck disable=SC2016
  ci_step install-tool -- sh -c '
    set -e
    rustup component add llvm-tools-preview
    if PATH="$1/bin:$PATH" cargo llvm-cov --version 2>/dev/null | grep -q "cargo-llvm-cov $2"; then
      echo "cargo-llvm-cov $2 already installed in $1"
    else
      cargo install --locked --version "$2" --root "$1" cargo-llvm-cov
    fi
    PATH="$1/bin:$PATH" cargo llvm-cov --version
  ' sh "$tools" "$ver"
elif [ "$CI_BACKEND" = docker ] ||
  ! PATH="$tools/bin:$PATH" cargo llvm-cov --version 2> /dev/null | grep -q "cargo-llvm-cov $ver"; then
  if [ "$CI_DRY_RUN" != 1 ]; then
    ci_skip install-tool "cargo-llvm-cov $ver is not installed in $tools (CI_INSTALL_TOOLCHAINS=1 installs it from crates.io)"
    ci_tier_end
    exit $?
  fi
fi
# Each step is one `sh -c` script with its words as arguments, so that PATH is
# the PATH of the process that runs it (the container's for the docker
# backend: a double-quoted `env "PATH=$tools/bin:$PATH"` would expand the
# HOST's PATH, which has cargo elsewhere than the image on a GitHub runner,
# selftest check "no host PATH handed to a docker step").
# The rustup component is added inside the container of the run itself (the
# install step's container is gone; cargo-llvm-cov would otherwise download it
# again by answering its own prompt). Stale profile data of an earlier run in
# the shared target volume is removed first, and the report is produced only
# after a passing instrumented run (a report of stale data is not a report).
ci_step fetch -- cargo fetch --locked
# shellcheck disable=SC2016
ci_step clean-profile -- sh -c '
  set -e
  PATH="$1/bin:$PATH" cargo llvm-cov clean --workspace
' sh "$tools"
# shellcheck disable=SC2016
ci_step instrumented-tests -- sh -c '
  set -e
  tools="$1" install="$2"
  shift 2
  if [ "$install" = 1 ]; then rustup component add llvm-tools-preview; fi
  PATH="$tools/bin:$PATH" exec cargo llvm-cov --locked --no-report "$@"
' sh "$tools" "${CI_INSTALL_TOOLCHAINS:-0}" "${sel[@]}"
if [ "$(ci_step_status instrumented-tests)" = pass ] || [ "$CI_DRY_RUN" = 1 ]; then
  # shellcheck disable=SC2016
  ci_step report -- sh -c '
    set -e
    export PATH="$1/bin:$PATH"
    cargo llvm-cov report --locked
    if [ -n "$2" ]; then
      cargo llvm-cov report --locked --lcov --output-path "$2/lcov.info"
      cargo llvm-cov report --locked --json --summary-only --output-path "$2/summary.json"
    fi
  ' sh "$tools" "$report_dir"
else
  ci_skip report "the instrumented run did not pass: its profile data is not reported"
fi
ci_tier_end
