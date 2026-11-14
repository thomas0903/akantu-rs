#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/solver_model/generate.sh. Builds solver_model_probe, runs it twice
# (determinism check) and promotes the fixtures solver_model/*.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/solver-model-probe.XXXXXX)"
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
cmake -S "${PROBES}/solver_model" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.solver_model_probe.json" solver_model_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "solver_model/solver_model_probe.cc"
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
# SOLVER_MODEL_PROBE_NP=2 (probe v5): the probe runs under `mpirun -np 2`
# (only the case dbltrace_newton_raphson_np2 runs there); default 1: one
# process, no mpirun (the v1-v4 cases).
np="${SOLVER_MODEL_PROBE_NP:-1}"
[[ "${np}" =~ ^[1-9][0-9]*$ ]] || { echo "invalid SOLVER_MODEL_PROBE_NP ${np}" >&2; exit 2; }
run() {
  if [ "${np}" = 1 ]; then
    AKANTU_PROBE_BUILD_INFO="${work}/build_env.solver_model_probe.json" \
      "${work}/build/solver_model_probe" "$1" "${SRC}" "${frozen}"
  else
    OMPI_MCA_rmaps_base_oversubscribe=1 \
      AKANTU_PROBE_BUILD_INFO="${work}/build_env.solver_model_probe.json" \
      timeout -k 10 600 mpirun -np "${np}" --oversubscribe --bind-to none \
      -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} \
      ${SOLVER_MODEL_PROBE_CASES:+-x SOLVER_MODEL_PROBE_CASES} \
      "${work}/build/solver_model_probe" "$1" "${SRC}" "${frozen}"
  fi
}
# SOLVER_MODEL_PROBE_RUNS=N (default 2): N runs, every rerun compared with
# the first (D-035 asks for at least 3 identical runs of a new golden).
runs="${SOLVER_MODEL_PROBE_RUNS:-2}"
[[ "${runs}" =~ ^[0-9]+$ ]] && [ "${runs}" -ge 2 ] || { echo "SOLVER_MODEL_PROBE_RUNS must be >= 2" >&2; exit 2; }
mkdir -p "${work}/stage"
run "${work}/stage"
for k in $(seq 2 "${runs}"); do
  mkdir -p "${work}/rerun${k}"
  run "${work}/rerun${k}" >"${work}/rerun${k}.log" 2>&1 || { tail -20 "${work}/rerun${k}.log"; exit 1; }
  if ! diff -r "${work}/stage" "${work}/rerun${k}" >"${work}/determinism${k}.diff"; then
    head -40 "${work}/determinism${k}.diff" >&2
    echo "solver_model_probe: runs 1 and ${k} differ; nothing is promoted" >&2
    exit 1
  fi
done
echo "[solver_model_probe] ${runs} identical runs: $(find "${work}/stage" -name manifest.json | wc -l) case(s)"
# Debug aid: SOLVER_MODEL_PROBE_STAGE_ONLY=1 copies the staged cases into /fixtures
# without finalisation (no SHA256SUMS, no size policy) for inspection.
if [ "${SOLVER_MODEL_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/stage/." /fixtures/
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
