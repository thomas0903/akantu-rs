#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/parallel/generate.sh. Builds parallel_probe, runs it under
# `mpirun -np P` for P = 1, 2, 4, PARALLEL_PROBE_RUNS times (default 2;
# determinism check, every run compared with the first), and promotes the
# fixtures parallel/*. PARALLEL_PROBE_SCENARIOS (comma-separated) restricts
# the scenarios (only the new cases are promoted).
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/parallel-probe.XXXXXX)"
base_args=(--expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree "${PROBES}"
           --install-prefix "${prefix}")
[ -n "${ORACLE_IMAGE_ID:-}" ] && base_args+=(--expect-image "${ORACLE_IMAGE_ID}")
[ -f "/build/${config}/compile_commands.json" ] &&
  base_args+=(--library-compile-commands "/build/${config}/compile_commands.json")
if [ -d "/build/src-${config}" ]; then
  base_args+=(--reference-tree "${SRC}" --patched-tree "/build/src-${config}" --patches-out "${work}/patches.txt")
fi
python3 -I "${PROBES}/python/build_env.py" base "${config}" installed "${flags_file}" \
  "${work}/build_env.json" "${base_args[@]}"
if [ -f "${work}/patches.txt" ]; then
  AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
  export AKANTU_PROBE_PATCHES
fi
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/parallel" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.parallel_probe.json" parallel_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "parallel/parallel_probe.cc"
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
# Scotch 7 partitions with as many threads as cores by default; its
# multi-threaded graph partitioning is not reproducible run to run even with
# a fixed seed (observed: two 4-part partitions of cube.msh differ). One
# thread makes the seeded partition reproducible (recorded in every case).
export SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}"
export OMPI_MCA_rmaps_base_oversubscribe=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
extra_env=(-x OPENBLAS_NUM_THREADS)
[ -n "${PARALLEL_PROBE_SCENARIOS:-}" ] && { export PARALLEL_PROBE_SCENARIOS; extra_env+=(-x PARALLEL_PROBE_SCENARIOS); }
# D-050 pin (x86_64 hosts), recorded by the probe
[ -n "${OPENBLAS_CORETYPE:-}" ] && { export OPENBLAS_CORETYPE; extra_env+=(-x OPENBLAS_CORETYPE); }
run() { # out_dir
  for np in ${PARALLEL_PROBE_RANKS:-1 2 4}; do
    mkdir -p "$1/scratch-np${np}"
    AKANTU_PROBE_BUILD_INFO="${work}/build_env.parallel_probe.json" \
      timeout -k 10 "${PARALLEL_PROBE_TIMEOUT:-600}" mpirun -np "${np}" --oversubscribe --bind-to none -x SCOTCH_PTHREAD_NUMBER \
      "${extra_env[@]}" \
      "${work}/build/parallel_probe" "$1/stage" "${frozen}" "$1/scratch-np${np}" \
      "${PROBES}/parallel/adv2d.msh"
  done
}
runs="${PARALLEL_PROBE_RUNS:-2}"
[[ "${runs}" =~ ^[0-9]+$ ]] && [ "${runs}" -ge 2 ] || { echo "PARALLEL_PROBE_RUNS must be >= 2" >&2; exit 2; }
mkdir -p "${work}/run1/stage"
run "${work}/run1"
for k in $(seq 2 "${runs}"); do
  mkdir -p "${work}/run${k}/stage"
  run "${work}/run${k}" >"${work}/rerun${k}.log" 2>&1 || { tail -20 "${work}/rerun${k}.log"; exit 1; }
  if ! diff -r "${work}/run1/stage" "${work}/run${k}/stage" >"${work}/determinism${k}.diff"; then
    head -40 "${work}/determinism${k}.diff" >&2
    echo "parallel_probe: runs 1 and ${k} differ; nothing is promoted" >&2
    exit 1
  fi
done
echo "[parallel_probe] ${runs} identical runs: $(find "${work}/run1/stage" -name manifest.json | wc -l) case(s)"
if [ "${PARALLEL_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/run1/stage/." /fixtures/
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/run1/stage" \
  --created "${AKANTU_PROBE_CREATED}"
