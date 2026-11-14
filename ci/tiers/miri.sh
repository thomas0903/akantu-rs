#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-010, CI-020: valgrind/ASAN jobs → Miri on unsafe code).
# miri: cargo +nightly miri test of the crates with unsafe code (Stacked and Tree Borrows)
#
# ARCHITECTURE / D-024: `cargo +nightly miri test -p akantu-python` and
# `-p akantu-core array_storage` under Stacked Borrows and Tree Borrows for
# every change touching `unsafe` or the foreign-view contract (the
# upstream valgrind job ubuntu_lts_gcc_valgrind and the ASAN/LSAN
# suppressions have no other counterpart: safe Rust needs neither). Host
# backend; needs the pinned nightly toolchain with the miri component
# (installed with CI_INSTALL_TOOLCHAINS=1; CI_MIRI_NIGHTLY selects another).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin miri

# Pinned: a floating `nightly` would change the verdict without a
# change of the tree. The date needs the miri and rust-src components and a
# rustc >= the workspace rust-version.
tc="${CI_MIRI_NIGHTLY:-nightly-2026-10-01}"
if ! ci_have rustup; then
  ci_skip miri "rustup not installed on the host"
else
  if ! rustup run "$tc" cargo miri --version > /dev/null 2>&1 && [ "${CI_INSTALL_TOOLCHAINS:-0}" = 1 ] && [ "$CI_DRY_RUN" != 1 ]; then
    ci_step "install-$tc-miri" --host -- rustup toolchain install "$tc" --profile minimal --component miri,rust-src --no-self-update
  fi
  if [ "$CI_DRY_RUN" != 1 ] && ! rustup run "$tc" cargo miri --version > /dev/null 2>&1; then
    ci_skip miri "toolchain $tc with the miri component not installed (CI_INSTALL_TOOLCHAINS=1 installs it)"
  else
    tdir="${CARGO_TARGET_DIR:-$CI_REPO/target}/miri"
    for model in stacked tree; do
      flags=""
      [ "$model" = tree ] && flags="-Zmiri-tree-borrows"
      ci_step "python:$model" --host -- env CARGO_TARGET_DIR="$tdir" MIRIFLAGS="$flags" rustup run "$tc" cargo miri test -p akantu-python --locked
      ci_step "core-array-storage:$model" --host -- env CARGO_TARGET_DIR="$tdir" MIRIFLAGS="$flags" rustup run "$tc" cargo miri test -p akantu-core --lib --locked array_storage
      # A filter that matches nothing (a renamed module) exits 0 with "0
      # passed": the floors of ci/floors.txt make that a failure.
      if [ "$CI_DRY_RUN" != 1 ]; then
        ci_step "floor:python:$model" --host -- python3 -I "$here/tools/count_tests.py" "$(ci_file "$(ci_slug "python:$model").log")" --floors "$here/floors.txt" --key miri-python
        ci_step "floor:core-array-storage:$model" --host -- python3 -I "$here/tools/count_tests.py" "$(ci_file "$(ci_slug "core-array-storage:$model").log")" --floors "$here/floors.txt" --key miri-core-array-storage
      fi
    done
  fi
fi

ci_tier_end
