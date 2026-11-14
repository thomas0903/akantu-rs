#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe runner: builds v104_probe in
# akantu-oracle:dev against /build/install-<config> (reference checkout and
# oracle volume read-only) and runs it three times; prints run 1.
# AKANTU_SRC_HOST: a clean checkout of the reference commit (required).
# Usage: run.sh [--config NAME]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the reference checkout}"
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
  -v "${here}:/probe:ro" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
prefix=/build/install-'"${config}"'
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
fp_flags="$(sed -nE "s/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p" "${flags_file}" | head -1)"
cgal_dir="$(sed -n "s/^CGAL_DIR:PATH=//p" "${flags_file}" | head -1)"
grep -E "^AKANTU_(IGFEM|STRUCTURAL_MECHANICS|COHESIVE_ELEMENT)[A-Z_]*:BOOL=" "${flags_file}" || true
work="$(mktemp -d /tmp/v104.XXXXXX)"
cmake -S /probe -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log" || { tail -40 "${work}/cmake.log"; exit 1; }
ninja -C "${work}/build" -j2 >"${work}/ninja.log" || { tail -80 "${work}/ninja.log"; exit 1; }
cd "${work}"
for run in 1 2 3; do
  "${work}/build/v104_probe" >"run${run}.txt" 2>"err${run}.txt"
done
cmp run1.txt run2.txt && cmp run1.txt run3.txt && echo "three identical runs"
echo "==== stdout run 1"
cat run1.txt
echo "==== stderr run 1 (last 20 lines)"
tail -20 err1.txt
'
