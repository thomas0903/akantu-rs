#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: build the benchmark driver against an
# oracle install tree, out of tree, in the scratch volume.
#
# Usage: scripts/build.sh [default|strict]...   (default: both)
# Result: /perf/builds/<config>/cohesive_extrinsic (volume akantu-perf-cohesive),
#         /perf/builds/<config>/build-record.json (immutable build record:
#         executable, loaded libraries with digests, harness
#         digests, flags, selected sparse solver, image id; copied into every
#         run directory by run-one.sh) and build-info.txt (the same, readable).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

configs=("$@")
[ ${#configs[@]} -gt 0 ] || configs=(default strict)
for config in "${configs[@]}"; do
  perf_check_config "${config}" || exit 2
done
for config in "${configs[@]}"; do
  perf_docker bash -c '
    set -euo pipefail
    . /harness/scripts/in-container/validate.sh
    config="$1"
    perf_check_config "${config}" || exit 2
    install="/build/install-${config}"
    test -f "${install}/READY" || { echo "ERROR: ${install} is not READY" >&2; exit 1; }
    b="/perf/builds/${config}"
    rm -rf "${b}"; mkdir -p "${b}"
    . "${install}/share/akantu-oracle/env.sh"
    cmake --fresh -S /harness/src -B "${b}" -G Ninja \
      -DAKANTU_ORACLE_INSTALL="${install}" -DCMAKE_EXPORT_COMPILE_COMMANDS=ON > "${b}/configure.log"
    ninja -C "${b}" > "${b}/build.log"
    # The interposer must be exported for libakantu to bind to it.
    nm -D --defined-only "${b}/cohesive_extrinsic" | grep -c " T _ZN6akantu4Mesh19fillNodesToElementsEi$" > /dev/null \
      || { echo "ERROR: interposer not exported" >&2; exit 1; }
    python3 -I /harness/scripts/in-container/build_record.py record "${config}" "${install}" "${b}" \
      > "${b}/build-record.json.tmp"
    mv "${b}/build-record.json.tmp" "${b}/build-record.json"
    python3 -I /harness/scripts/in-container/build_record.py info "${b}/build-record.json" \
      > "${b}/build-info.txt"
    cat "${b}/build-info.txt"
  ' _ "${config}"
done
