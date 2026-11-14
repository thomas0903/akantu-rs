#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/parallel/variation-study.sh. Builds parallel_probe exactly as
# run-in-container.sh does, then runs it VARIATION_RUNS times under
# `mpirun -np VARIATION_NP` with PARALLEL_PROBE_UNMASKED_SUM=1 (the
# scenarios of PARALLEL_PROBE_SCENARIOS), every run into /out/run<k>/stage.
# Nothing is compared or promoted here (variation_analysis.py does the
# analysis).
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
runs="${VARIATION_RUNS:?}"
np="${VARIATION_NP:?}"
work="$(mktemp -d /tmp/parallel-variation.XXXXXX)"
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
export SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}"
export OMPI_MCA_rmaps_base_oversubscribe=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
export PARALLEL_PROBE_UNMASKED_SUM=1 PARALLEL_PROBE_SCENARIOS
extra_env=(-x OPENBLAS_NUM_THREADS -x PARALLEL_PROBE_SCENARIOS -x PARALLEL_PROBE_UNMASKED_SUM)
[ -n "${OPENBLAS_CORETYPE:-}" ] && { export OPENBLAS_CORETYPE; extra_env+=(-x OPENBLAS_CORETYPE); }
for k in $(seq 1 "${runs}"); do
  mkdir -p "/out/run${k}/stage" "/out/run${k}/scratch"
  AKANTU_PROBE_BUILD_INFO="${work}/build_env.parallel_probe.json" \
    timeout -k 10 600 mpirun -np "${np}" --oversubscribe --bind-to none -x SCOTCH_PTHREAD_NUMBER \
    "${extra_env[@]}" \
    "${work}/build/parallel_probe" "/out/run${k}/stage" "${frozen}" "/out/run${k}/scratch" \
    "${PROBES}/parallel/adv2d.msh" >"/out/run${k}/log" 2>&1 ||
    { tail -20 "/out/run${k}/log"; echo "variation run ${k} failed" >&2; exit 1; }
  echo "[variation] run ${k}/${runs} done"
done
echo "[variation] ${runs} runs at ${np} ranks: $(find /out/run1/stage -name manifest.json | wc -l) case(s) per run"
