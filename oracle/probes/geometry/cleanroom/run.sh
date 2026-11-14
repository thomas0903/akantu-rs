#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Builds and runs the geometry clean-room black-box
# experiments (D-030, crates/akantu-geometry/CLEANROOM_LOG.md) with the
# oracle's toolchain (Debian trixie, GCC 14 libstdc++) and writes the
# fixtures crates/akantu-geometry/tests/fixtures/cleanroom/{nth_element,rb_set,
# nth_element_b3}.json (B3: inputs from b3_inputs.txt).
#
# Only the public standard-library API is used; no library header is read.
# Runs on an x86_64 Linux host with Docker (D-043).
#
# Usage: oracle/probes/geometry/cleanroom/run.sh [image]
#   image defaults to akantu-oracle:dev if present, else debian:trixie
#   (g++ installed on the fly).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
# Numerical-environment pin and x86-64-v3 host check of every oracle container
# start (D-050 (5), D-073 (2)). Empty on arm64.
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
out="$repo/target/cleanroom-geometry"
mkdir -p "$out"
image="${1:-}"
if [[ -z "$image" ]]; then
  if docker image inspect akantu-oracle:dev >/dev/null 2>&1; then image=akantu-oracle:dev; else image=debian:trixie; fi
fi
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g -v "$here":/src:ro -v "$out":/out -w /out "$image" \
  bash -euo pipefail -c '
    command -v g++ >/dev/null || { apt-get -qq update && apt-get -qq -y --no-install-recommends install g++ >/dev/null; }
    flags="-std=c++20 -O2 -ffp-contract=off -Wall -Wextra"
    g++ $flags -o nth_element_bb /src/nth_element_bb.cc && ./nth_element_bb > nth_element.json
    g++ $flags -o rb_set_bb /src/rb_set_bb.cc && ./rb_set_bb > rb_set.json
    g++ $flags -o nth_element_input_bb /src/nth_element_input_bb.cc &&
      ./nth_element_input_bb < /src/b3_inputs.txt > nth_element_b3.json
    { g++ --version | head -1; dpkg-query -W -f="\${Package} \${Version}\n" libstdc++6 g++ 2>/dev/null; uname -m; } > toolchain.txt
  '
cat "$out/toolchain.txt"
dest="$repo/crates/akantu-geometry/tests/fixtures/cleanroom"
mkdir -p "$dest"
cp "$out/nth_element.json" "$out/rb_set.json" "$out/nth_element_b3.json" "$out/toolchain.txt" "$dest/"
ls -l "$dest"
