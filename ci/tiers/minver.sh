#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-029: declared dependency floors).
# minver: direct-minimal-versions lockfile (nightly) + cargo check, on a scratch copy
#
# The workspace manifests declare version floors ("faer = 0.24.4" means
# >= 0.24.4); this tier resolves every direct dependency to its lowest
# allowed version (`cargo +nightly update -Z direct-minimal-versions`) in a
# scratch copy of the committed tree (git archive HEAD: the repository and
# its Cargo.lock are never touched) and checks the workspace with the pinned
# toolchain. Needs network access to the crates.io index and a nightly
# toolchain (installed with CI_INSTALL_TOOLCHAINS=1), host backend only.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin minver

# Pinned nightly; a floating
# `nightly` would change the verdict without a change of the tree.
nightly="${CI_NIGHTLY:-nightly-2025-03-01}"
if ! ci_have rustup || ! ci_have git; then
  ci_skip minver "rustup and git are needed on the host"
else
  if ! rustup run "$nightly" cargo -V > /dev/null 2>&1 && [ "${CI_INSTALL_TOOLCHAINS:-0}" = 1 ] && [ "$CI_DRY_RUN" != 1 ]; then
    ci_step "install-$nightly" --host -- rustup toolchain install "$nightly" --profile minimal --no-self-update
  fi
  if [ "$CI_DRY_RUN" != 1 ] && ! rustup run "$nightly" cargo -V > /dev/null 2>&1; then
    ci_skip minver "toolchain $nightly not installed (set CI_INSTALL_TOOLCHAINS=1 to install it)"
  else
    scratch="$(ci_file scratch)"
    rm -rf "$scratch" && mkdir -p "$scratch"
    # shellcheck disable=SC2016
    ci_step export --host -- sh -c 'git -C "$1" archive HEAD | tar -x -C "$2"' sh "$CI_REPO" "$scratch"
    # shellcheck disable=SC2016
    ci_step resolve --host -- sh -c 'cd "$1" || exit 1; rustup run "$2" cargo update -Z direct-minimal-versions || exit 1; git diff --no-index --stat "$3/Cargo.lock" Cargo.lock || true' sh "$scratch" "$nightly" "$CI_REPO"
    if [ "$(ci_step_status resolve)" != pass ] && [ "$CI_DRY_RUN" != 1 ]; then
      # Checking the unchanged lockfile would prove nothing.
      ci_skip check "the minimal-versions resolution failed (see resolve.log)"
    else
      # shellcheck disable=SC2016
      ci_step check --host -- env CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CI_REPO/target}/minver" sh -c 'cd "$1" && cargo check --workspace --all-targets --locked' sh "$scratch"
    fi
    # Keep the minimal lockfile, drop the copy of the tree.
    cp "$scratch/Cargo.lock" "$(ci_file Cargo.minimal.lock)" 2> /dev/null || true
    rm -rf "$scratch"
  fi
fi

ci_tier_end
