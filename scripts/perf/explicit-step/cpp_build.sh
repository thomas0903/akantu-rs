#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): build the C++ reference
# timing driver (explicit_step_timing.cc) against an oracle install inside the
# oracle image (built as in docs/ORACLE_BUILD.md: volume akantu-oracle-build,
# image akantu-oracle:dev). Output: $OUT/explicit_step_timing_<config>.
#   cpp_build.sh OUT [strict|default]...
# DOCKER_CPUSET (optional, e.g. 4-7): build on these cores only.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$1"; shift
configs=("$@"); [ ${#configs[@]} -gt 0 ] || configs=(strict)
mkdir -p "$out"
cpuset=(); [ -z "${DOCKER_CPUSET:-}" ] || cpuset=(--cpuset-cpus "${DOCKER_CPUSET}")
for c in "${configs[@]}"; do
  docker run --rm ${cpuset[@]+"${cpuset[@]}"} -v akantu-oracle-build:/build -v "$here":/src:ro -v "$out":/out akantu-oracle:dev bash -c "
    set -euo pipefail
    . /build/install-$c/share/akantu-oracle/env.sh
    rm -rf /tmp/b && cmake -S /src -B /tmp/b -DAKANTU_ORACLE_INSTALL=/build/install-$c > /tmp/configure.log 2>&1 \\
      || { tail -40 /tmp/configure.log; exit 1; }
    cmake --build /tmp/b -v > /tmp/build.log 2>&1 || { tail -40 /tmp/build.log; exit 1; }
    grep -m1 -o ' -O[0-9s].*explicit_step_timing.cc' /tmp/build.log || true
    cp /tmp/b/explicit_step_timing /out/explicit_step_timing_$c
    echo built /out/explicit_step_timing_$c"
done
