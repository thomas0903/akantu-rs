#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Builds and runs libm_sincos.cc in the oracle image
# (strict flags, as the oracle's Akantu build) and refreshes
# crates/akantu-core/tests/fixtures/random/libm_sincos.json.
# The `default` (-O2) variant must agree (glibc is a prebuilt binary; only the
# probe's own alpha/k arithmetic could change, and it must not).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
# Numerical-environment pin and x86-64-v3 host check of every oracle container
# start (D-050 (5), D-073 (2)). Empty on arm64.
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
image="${IMAGE:-akantu-oracle:dev}"
work="$(mktemp -d "${TMPDIR:-/tmp}/akantu-sincos-probe.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$here/libm_sincos.cc" "$work/"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory 4g -v "$work":/w -w /w "$image" bash -euo pipefail -c '
  g++ -std=c++20 -O2 -ffp-contract=off -fno-builtin -DPROBE_FLAGS="\"-std=c++20 -O2 -ffp-contract=off -fno-builtin\"" -o strict libm_sincos.cc
  g++ -std=c++20 -O2 -fno-builtin -DPROBE_FLAGS="\"-std=c++20 -O2 -fno-builtin\"" -o default libm_sincos.cc
  ./strict > strict.json
  ./default > default.json
'
if ! cmp -s <(tail -n +2 "$work/strict.json") <(tail -n +2 "$work/default.json"); then
  echo "DIFFERENCE between strict and default" >&2
  exit 1
fi
cp "$work/strict.json" "$repo/crates/akantu-core/tests/fixtures/random/libm_sincos.json"
echo "wrote crates/akantu-core/tests/fixtures/random/libm_sincos.json"
