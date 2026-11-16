#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): build one benchmark-
# suite binary (crates/akantu-bench `suite_<area>`, on the suite branches of
# the performance campaign) in one configuration of the build-profile study
# (variants.sh) and copy it, debug information stripped, to
# BIN_ROOT/<name>/suite_<area>. PGO configurations use PGO_DIR as in
# variants.sh (a profile trained on other binaries applies to the library
# code they share).
#   suite_variant.sh CHECKOUT AREA NAME TARGET_DIR BIN_ROOT
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/perf/build/variants.sh
. "$here/variants.sh"
co="$1" area="$2" name="$3" tdir="$4" broot="$5"
variant_env "$name"
export CARGO_TARGET_DIR="$tdir"
cd "$co"
cargo build --release --locked -q -p akantu-bench --bin "suite_$area"
mkdir -p "$broot/$name"
cp "$tdir/release/suite_$area" "$broot/$name/suite_$area"
strip --strip-debug "$broot/$name/suite_$area"
echo "built suite_$area $name ($(git -C "$co" rev-parse --short=9 HEAD))"
