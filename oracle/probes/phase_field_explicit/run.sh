#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: build and run the PhaseFieldModel explicit-branch error-path
# probe in akantu-oracle:dev against the installed
# oracle /build/install-<config> (reference checkout and build volume
# read-only). Prints the probe's report on stdout; no fixture is written.
# On the x86_64 oracle host set
# AKANTU_SRC_HOST=/path/to/akantu.
#
# Usage: oracle/probes/phase_field_explicit/run.sh [--config NAME]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
[ "${1:-}" = "--config" ] && config="$2"
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
echo "oracle image $(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}"), config ${config}, reference ${sha}"
exec docker run --rm --init --memory=4g --memory-swap=4g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
prefix=/build/install-'"${config}"'
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
fp_flags="$(sed -nE "s/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p" "${flags_file}" | head -1)"
cgal_dir="$(sed -n "s/^CGAL_DIR:PATH=//p" "${flags_file}" | head -1)"
work="$(mktemp -d /tmp/pf-explicit.XXXXXX)"
cmake -S /probes/phase_field_explicit -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
d=/akantu-src/test/test_model/test_phase_field_model
cd "${work}"
for run in 1 2 3; do
  "${work}/build/explicit_branches_probe" "${d}/material_coupling.dat" "${d}/test_one_element.msh" >"run${run}.txt" 2>&1 || true
done
cmp run1.txt run2.txt && cmp run1.txt run3.txt && echo "three identical runs"
cat run1.txt
'
