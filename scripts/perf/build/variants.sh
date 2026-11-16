#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): build configurations of
# the build-profile study (docs/benchmarks/perf-campaign/build-profile/).
# Sourced by build_variant.sh and ci_variant.sh: variant_env NAME exports the
# cargo environment of one configuration (profile overrides through
# CARGO_PROFILE_RELEASE_*, code generation flags through RUSTFLAGS). Names
# are joined with '+', e.g. "fat+cgu1+v3":
#   base      the workspace release profile as committed
#   cgu1      codegen-units = 1
#   thin      lto = "thin"            fat   lto = "fat"
#   v2|v3|v4  -C target-cpu=x86-64-v2|v3|v4
#   native    -C target-cpu=native
#   abort     panic = "abort"
#   o2        opt-level = 2
#   pgogen    -C profile-generate=$PGO_DIR (instrumented build)
#   pgouse    -C profile-use=$PGO_DIR/merged.profdata
variant_env() {
  local name="$1" part flags=()
  unset CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
    CARGO_PROFILE_RELEASE_PANIC CARGO_PROFILE_RELEASE_OPT_LEVEL RUSTFLAGS
  IFS=+ read -r -a parts <<< "$name"
  for part in "${parts[@]}"; do
    case "$part" in
      base) ;;
      cgu1) export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 ;;
      thin) export CARGO_PROFILE_RELEASE_LTO=thin ;;
      fat) export CARGO_PROFILE_RELEASE_LTO=fat ;;
      v2 | v3 | v4) flags+=(-C "target-cpu=x86-64-$part") ;;
      native) flags+=(-C target-cpu=native) ;;
      abort) export CARGO_PROFILE_RELEASE_PANIC=abort ;;
      o2) export CARGO_PROFILE_RELEASE_OPT_LEVEL=2 ;;
      pgogen) flags+=(-C "profile-generate=${PGO_DIR:?}") ;;
      pgouse) flags+=(-C "profile-use=${PGO_DIR:?}/merged.profdata" -C llvm-args=-pgo-warn-missing-function) ;;
      *) echo "unknown variant part $part" >&2; return 2 ;;
    esac
  done
  if [ ${#flags[@]} -gt 0 ]; then export RUSTFLAGS="${flags[*]}"; fi
  return 0
}
