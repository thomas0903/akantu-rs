#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-025 developer presets; D-025 MSRV claim).
# msrv: cargo +<rust-version> check --workspace (the advertised MSRV of Cargo.toml)
#
# D-025: "the advertised MSRV is a claim only if CI runs it". Reads
# workspace.package.rust-version from Cargo.toml and checks the libraries,
# binaries and every target with that toolchain (host backend only: the
# project images carry the pinned 1.99.1 toolchain alone). The toolchain is
# installed with rustup only when CI_INSTALL_TOOLCHAINS=1 (minimal profile);
# otherwise a missing toolchain skips the tier (incomplete, never a pass).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin msrv

msrv="$(sed -n '/^\[workspace.package\]/,/^\[/s/^rust-version *= *"\([^"]*\)".*/\1/p' "$CI_REPO/Cargo.toml" | head -n 1)"
if [ -z "$msrv" ]; then
  ci_na msrv "no workspace.package.rust-version in Cargo.toml"
elif ! ci_have rustup; then
  ci_skip "msrv-$msrv" "rustup not installed on the host"
else
  tc="$msrv"
  case "$tc" in *.*.*) ;; *) tc="$tc.0" ;; esac
  if ! rustup run "$tc" cargo -V > /dev/null 2>&1; then
    if [ "${CI_INSTALL_TOOLCHAINS:-0}" = 1 ] && [ "$CI_DRY_RUN" != 1 ]; then
      ci_step "install-$tc" --host -- rustup toolchain install "$tc" --profile minimal --no-self-update
    fi
  fi
  if [ "$CI_DRY_RUN" = 1 ] || rustup run "$tc" cargo -V > /dev/null 2>&1; then
    # A separate target directory: another rustc must not share artefacts.
    ci_step "check-lib-$tc" --host -- env CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CI_REPO/target}/msrv-$tc" rustup run "$tc" cargo check --workspace --lib --bins --locked
    ci_step "check-all-targets-$tc" --host -- env CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CI_REPO/target}/msrv-$tc" rustup run "$tc" cargo check --workspace --all-targets --locked
  else
    ci_skip "check-$tc" "toolchain $tc not installed (set CI_INSTALL_TOOLCHAINS=1 to install it)"
  fi
fi

ci_tier_end
