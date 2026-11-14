#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev. Builds the listed periodic-order
# probes against /build/install-strict and leaves the binaries in /work/build.
# usage: bash v90_run_in_container.sh <probe>...
set -euo pipefail
prefix=/build/install-strict
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
probes="$(IFS=';'; echo "$*")"
cmake -S /v90 -B /work/build -G Ninja -DCMAKE_PREFIX_PATH="${prefix}" \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" -DV90_PROBES="${probes}" >/work/cmake.log
ninja -C /work/build -j2 >/work/ninja.log || { tail -60 /work/ninja.log; exit 1; }
echo "fp_flags=${fp_flags}"
g++ --version | head -1
