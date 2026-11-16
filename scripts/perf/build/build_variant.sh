#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): build the timing
# binaries of the build-profile study in one configuration (variants.sh)
# from a clean target directory, record the wall time of the build and the
# binary sizes, and copy the binaries to BIN_ROOT/NAME/.
#   build_variant.sh REPO NAME [TARGET_ROOT] [BIN_ROOT]
# Binaries: explicit_step_timing (akantu-solid example), cohesive_extrinsic
# (akantu-bench), facet_scaling (akantu-mesh example). One line of JSON on
# stdout per build. With STRIP_COPY=1 the copies in BIN_ROOT have their
# DWARF debug information removed (`strip --strip-debug`: the symbol table
# and the code stay identical; smaller files to move between machines);
# the sizes recorded are those of cargo's unstripped binaries.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/perf/build/variants.sh
. "$here/variants.sh"
repo="$1" name="$2" troot="${3:-$HOME/w/build/t}" broot="${4:-$HOME/w/build/bin}"
variant_env "$name"
export CARGO_TARGET_DIR="$troot/$name"
case "$CARGO_TARGET_DIR" in "$troot/"?*) rm -rf "$CARGO_TARGET_DIR" ;; *) exit 2 ;; esac
mkdir -p "$broot/$name"
cd "$repo"
cargo fetch --locked -q
t0=$(date +%s.%N)
cargo build --release --locked -q -p akantu-solid --example explicit_step_timing
cargo build --release --locked -q -p akantu-bench --bin cohesive_extrinsic
cargo build --release --locked -q -p akantu-mesh --example facet_scaling
t1=$(date +%s.%N)
cp "$CARGO_TARGET_DIR/release/examples/explicit_step_timing" "$broot/$name/explicit"
cp "$CARGO_TARGET_DIR/release/cohesive_extrinsic" "$broot/$name/cohesive"
cp "$CARGO_TARGET_DIR/release/examples/facet_scaling" "$broot/$name/facet"
if [ "${STRIP_COPY:-0}" = 1 ]; then
  strip --strip-debug "$broot/$name/explicit" "$broot/$name/cohesive" "$broot/$name/facet"
fi
sz() { stat -c %s "$1"; }
txt() { size -A "$1" | awk '$1 == ".text" {print $2}'; }
printf '{"variant":"%s","build_s":%.1f,"rustflags":"%s","lto":"%s","cgu":"%s","panic":"%s","load":"%s","utc":"%s","commit":"%s",' \
  "$name" "$(echo "$t1 - $t0" | bc)" "${RUSTFLAGS:-}" "${CARGO_PROFILE_RELEASE_LTO:-}" \
  "${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-}" "${CARGO_PROFILE_RELEASE_PANIC:-}" \
  "$(cut -d' ' -f1-3 /proc/loadavg)" "$(date -u +%FT%TZ)" "$(git rev-parse --short=9 HEAD)"
printf '"explicit_bytes":%s,"explicit_text":%s,"cohesive_bytes":%s,"cohesive_text":%s,"facet_bytes":%s,"facet_text":%s}\n' \
  "$(sz "$CARGO_TARGET_DIR/release/examples/explicit_step_timing")" "$(txt "$broot/$name/explicit")" \
  "$(sz "$CARGO_TARGET_DIR/release/cohesive_extrinsic")" "$(txt "$broot/$name/cohesive")" \
  "$(sz "$CARGO_TARGET_DIR/release/examples/facet_scaling")" "$(txt "$broot/$name/facet")"
