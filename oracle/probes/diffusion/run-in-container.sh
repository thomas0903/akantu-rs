#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/diffusion/generate.sh. Builds diffusion_probe,
# diffusion_par_probe and diffusion_v102_probe, runs the serial probe three
# times (D-035 determinism check), promotes the fixtures model/diffusion/*
# and writes the distributed-run evidence
# /results/diffusion-parallel-<config>.txt.
# DIFF_PROBE_SET=v102: instead, the review adversarial cases
# (diffusion_v102_probe, fixtures model/diffusion/v102_*; three runs, each
# with the separate ud283_cohesive_init pass), no distributed evidence.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/diffusion-probe.XXXXXX)"
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
cmake -S "${PROBES}/diffusion" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -80 "${work}/ninja.log"; exit 1; }
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
if [ "${DIFF_PROBE_SET:-}" = v102 ]; then
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
    "${work}/build_env.diffusion_v102_probe.json" diffusion_v102_probe \
    --compile-commands "${work}/build/compile_commands.json" \
    --source-suffix "diffusion/diffusion_v102_probe.cc"
  run_v102() {
    AKANTU_PROBE_BUILD_INFO="${work}/build_env.diffusion_v102_probe.json" \
      "${work}/build/diffusion_v102_probe" "$1" "${SRC}" || return 1
    # the cohesive-init case runs in a process of its own (upstream throws)
    V102_CASES=ud283_cohesive_init AKANTU_PROBE_BUILD_INFO="${work}/build_env.diffusion_v102_probe.json" \
      "${work}/build/diffusion_v102_probe" "$1" "${SRC}"
  }
  mkdir -p "${work}/stage" "${work}/rerun1" "${work}/rerun2"
  run_v102 "${work}/stage"
  for r in rerun1 rerun2; do
    run_v102 "${work}/${r}" >"${work}/${r}.log" 2>&1 || { tail -20 "${work}/${r}.log"; exit 1; }
    if ! diff -r "${work}/stage" "${work}/${r}" >"${work}/determinism-${r}.diff"; then
      head -40 "${work}/determinism-${r}.diff" >&2
      echo "diffusion_v102_probe: runs differ (${r}); nothing is promoted" >&2
      exit 1
    fi
  done
  echo "[diffusion_v102_probe] three identical runs: $(find "${work}/stage" -name manifest.json | wc -l) case(s)"
  if [ "${DIFF_PROBE_STAGE_ONLY:-0}" = 1 ]; then
    cp -r "${work}/stage/." /fixtures/
    exit 0
  fi
  python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
    --created "${AKANTU_PROBE_CREATED}"
  exit 0
fi
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.diffusion_probe.json" diffusion_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "diffusion/diffusion_probe.cc"
run() {
  AKANTU_PROBE_BUILD_INFO="${work}/build_env.diffusion_probe.json" \
    "${work}/build/diffusion_probe" "$1" "${SRC}" "${frozen}"
}
mkdir -p "${work}/stage" "${work}/rerun1" "${work}/rerun2"
run "${work}/stage"
for r in rerun1 rerun2; do
  run "${work}/${r}" >"${work}/${r}.log" 2>&1 || { tail -20 "${work}/${r}.log"; exit 1; }
  if ! diff -r "${work}/stage" "${work}/${r}" >"${work}/determinism-${r}.diff"; then
    head -40 "${work}/determinism-${r}.diff" >&2
    echo "diffusion_probe: runs differ (${r}); nothing is promoted" >&2
    exit 1
  fi
done
echo "[diffusion_probe] three identical runs: $(find "${work}/stage" -name manifest.json | wc -l) case(s)"
# Distributed evidence (UD-275): 2 and 4 ranks on the _triangle_3 patch mesh
# and on the frozen irregular square.msh.
export SCOTCH_PTHREAD_NUMBER=1 OMPI_MCA_rmaps_base_oversubscribe=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
evidence="/results/diffusion-parallel-${config}.txt"
{
  echo "# upstream HeatTransferModel on a distributed mesh (UD-275)"
  echo "# reference ${AKANTU_PROBE_REFERENCE_SHA} (${AKANTU_PROBE_REFERENCE_DESCRIBE}), image ${ORACLE_IMAGE_ID:-unknown}, config ${config}"
  echo "# probe oracle/probes/diffusion/diffusion_par_probe.cc, sources ${AKANTU_PROBE_SOURCES_COMMIT:-unknown}, created ${AKANTU_PROBE_CREATED:-unknown}"
} >"${evidence}"
for mesh in "${SRC}/test/test_model/patch_tests/data/_triangle_3.msh" \
            "${frozen}/examples/c++/diffusion_model/square.msh"; do
  for np in 2 4; do
    out="${work}/par-np${np}"
    rm -f "${out}".*
    set +e
    timeout -k 10 300 mpirun -np "${np}" --oversubscribe --bind-to none -x SCOTCH_PTHREAD_NUMBER \
      "${work}/build/diffusion_par_probe" "${out}" "${mesh}" \
      "${SRC}/test/test_model/patch_tests/data/heat_transfer_input.dat" >"${work}/par-np${np}.log" 2>&1
    rc=$?
    set -e
    {
      echo "## mesh ${mesh#${SRC}/} ${mesh#/frozen-meshes/}, np ${np}, exit code ${rc}"
      for f in "${out}".*; do [ -f "$f" ] && cat "$f"; done
    } >>"${evidence}"
  done
done
cat "${evidence}"
if [ "${DIFF_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/stage/." /fixtures/
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
