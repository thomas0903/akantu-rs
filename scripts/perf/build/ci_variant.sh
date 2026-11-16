#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): run CI tiers of the
# current tree with one build configuration of the build-profile study
# (variants.sh). A scratch worktree of REPO at its HEAD gets an untracked
# .cargo/config.toml holding the configuration's rustflags and release
# profile settings (the project image passes no RUSTFLAGS, so cargo inside
# the container reads them), then ci/run-all.sh --backend docker runs the
# tiers with the target directory /target/<TARGET_PREFIX>-<NAME> of the
# image's target volume.
#   ci_variant.sh REPO NAME TIERS OUT_DIR [SCRATCH_ROOT]
# TIERS is a comma list (e.g. test,release). Exit status: run-all.sh's.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/perf/build/variants.sh
. "$here/variants.sh"
repo="$1" name="$2" tiers="$3" out="$4" sroot="${5:-$HOME/w/build/ci}"
variant_env "$name"
slug="$(printf '%s' "$name" | tr '+' '-')"
wt="$sroot/$slug"
git -C "$repo" worktree remove --force "$wt" 2> /dev/null || true
git -C "$repo" worktree add --detach "$wt" HEAD > /dev/null
mkdir -p "$wt/.cargo"
{
  echo "# SPDX-License-Identifier: LGPL-3.0-or-later"
  echo "# scratch configuration '$name' of scripts/perf/build/ci_variant.sh (never committed)"
  if [ -n "${RUSTFLAGS:-}" ]; then
    printf '[build]\nrustflags = ['
    # shellcheck disable=SC2086 # split the flags on spaces
    set -- $RUSTFLAGS
    sep=""
    for f in "$@"; do printf '%s"%s"' "$sep" "$f"; sep=", "; done
    printf ']\n'
  fi
  if [ -n "${CARGO_PROFILE_RELEASE_LTO:-}${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-}${CARGO_PROFILE_RELEASE_PANIC:-}${CARGO_PROFILE_RELEASE_OPT_LEVEL:-}" ]; then
    echo "[profile.release]"
    [ -z "${CARGO_PROFILE_RELEASE_LTO:-}" ] || echo "lto = \"$CARGO_PROFILE_RELEASE_LTO\""
    [ -z "${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-}" ] || echo "codegen-units = $CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
    [ -z "${CARGO_PROFILE_RELEASE_PANIC:-}" ] || echo "panic = \"$CARGO_PROFILE_RELEASE_PANIC\""
    [ -z "${CARGO_PROFILE_RELEASE_OPT_LEVEL:-}" ] || echo "opt-level = $CARGO_PROFILE_RELEASE_OPT_LEVEL"
  fi
} > "$wt/.cargo/config.toml"
unset RUSTFLAGS CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_RELEASE_PANIC CARGO_PROFILE_RELEASE_OPT_LEVEL
cat "$wt/.cargo/config.toml"
cd "$wt"
st=0
ci/run-all.sh --backend docker --tiers "$tiers" --target-name "${TARGET_PREFIX:-build}-$slug" --out "$out" --label "build-profile $name" || st=$?
exit $st
