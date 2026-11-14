#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/parallel_test_cohesive/generate.sh.
# Builds parallel_test_cohesive_probe, runs every case under
# `mpirun -np 2`, the whole set PTC_RUNS times (default 3), and promotes the
# cases whose runs all succeeded and are byte-identical. A case that fails,
# times out or differs between runs is not promoted; the summary (exit codes
# per run), the verdicts and the stderr tails of the failures are printed and,
# in staging mode (PTC_STAGE_ONLY=1, a scratch --out), copied to
# <out>/reports/.
set -uo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/parallel-test-cohesive-probe.XXXXXX)"
base_args=(--expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree "${PROBES}"
           --install-prefix "${prefix}")
[ -n "${ORACLE_IMAGE_ID:-}" ] && base_args+=(--expect-image "${ORACLE_IMAGE_ID}")
[ -f "/build/${config}/compile_commands.json" ] &&
  base_args+=(--library-compile-commands "/build/${config}/compile_commands.json")
if [ -d "/build/src-${config}" ]; then
  base_args+=(--reference-tree "${SRC}" --patched-tree "/build/src-${config}" --patches-out "${work}/patches.txt")
fi
python3 -I "${PROBES}/python/build_env.py" base "${config}" installed "${flags_file}" \
  "${work}/build_env.json" "${base_args[@]}" || exit 1
if [ -f "${work}/patches.txt" ]; then
  AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
  export AKANTU_PROBE_PATCHES
fi
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/parallel_test_cohesive" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log" ||
  { tail -40 "${work}/cmake.log"; exit 1; }
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -80 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.parallel_test_cohesive_probe.json" parallel_test_cohesive_probe \
  --compile-commands "${work}/build/compile_commands.json" \
  --source-suffix "parallel_test_cohesive/parallel_test_cohesive_probe.cc" || exit 1
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
export OMPI_MCA_rmaps_base_oversubscribe=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
extra_env=(-x OPENBLAS_NUM_THREADS)
# D-050 pin (x86_64 hosts), recorded by the probe
[ -n "${OPENBLAS_CORETYPE:-}" ] && { export OPENBLAS_CORETYPE; extra_env+=(-x OPENBLAS_CORETYPE); }
all_cases="extrinsic_mode_i_triangle_3,extrinsic_mode_i_quadrangle_4,extrinsic_mode_i_triangle_3_quadrangle_4,extrinsic_mode_i_triangle_6,extrinsic_mode_i_quadrangle_8,extrinsic_mode_i_triangle_6_quadrangle_8,extrinsic_mode_i_tetrahedron_4,extrinsic_mode_i_tetrahedron_10,extrinsic_mode_i_finite_def_triangle_3,extrinsic_mode_i_finite_def_quadrangle_4,extrinsic_mode_i_finite_def_triangle_3_quadrangle_4,extrinsic_mode_i_finite_def_triangle_6,extrinsic_mode_i_finite_def_quadrangle_8,extrinsic_mode_i_finite_def_triangle_6_quadrangle_8,extrinsic_mode_i_finite_def_tetrahedron_4,extrinsic_mode_i_finite_def_tetrahedron_10,extrinsic_mode_ii_triangle_3,extrinsic_mode_ii_quadrangle_4,extrinsic_mode_ii_triangle_3_quadrangle_4,extrinsic_mode_ii_triangle_6,extrinsic_mode_ii_quadrangle_8,extrinsic_mode_ii_triangle_6_quadrangle_8,extrinsic_mode_ii_tetrahedron_4,extrinsic_mode_ii_tetrahedron_10,intrinsic_mode_i_triangle_3,intrinsic_mode_i_quadrangle_4,intrinsic_mode_i_triangle_3_quadrangle_4,intrinsic_mode_i_triangle_6,intrinsic_mode_i_quadrangle_8,intrinsic_mode_i_triangle_6_quadrangle_8,intrinsic_mode_i_tetrahedron_4,intrinsic_mode_i_tetrahedron_10,intrinsic_mode_ii_triangle_3,intrinsic_mode_ii_quadrangle_4,intrinsic_mode_ii_triangle_3_quadrangle_4,intrinsic_mode_ii_triangle_6,intrinsic_mode_ii_quadrangle_8,intrinsic_mode_ii_triangle_6_quadrangle_8,intrinsic_mode_ii_tetrahedron_4,intrinsic_mode_ii_tetrahedron_10"
scenarios="${PTC_CASES:-${all_cases}}"
# version 2: PTC_RECORD=boundary writes the boundary
# record under its own root
case "${PTC_RECORD:-}" in
  "") root_name=parallel_test_cohesive ;;
  boundary) root_name=parallel_test_cohesive_boundary; export PTC_RECORD; extra_env+=(-x PTC_RECORD) ;;
  *) echo "unknown PTC_RECORD ${PTC_RECORD}" >&2; exit 2 ;;
esac
runs="${PTC_RUNS:-3}"
[[ "${runs}" =~ ^[0-9]+$ ]] && [ "${runs}" -ge 2 ] || { echo "PTC_RUNS must be >= 2" >&2; exit 2; }
summary="${work}/summary.tsv"
printf 'run\tcase\tranks\trc\tseconds\n' > "${summary}"
failures="${work}/failures.txt"
: > "${failures}"
for k in $(seq 1 "${runs}"); do
  mkdir -p "${work}/run${k}/stage"
  for sc in ${scenarios//,/ }; do
    for np in 2; do
      scratch="${work}/run${k}/scratch-${sc}-np${np}"
      mkdir -p "${scratch}"
      t0=$(date +%s)
      AKANTU_PROBE_BUILD_INFO="${work}/build_env.parallel_test_cohesive_probe.json" \
        timeout -k 10 "${PTC_TIMEOUT:-900}" mpirun -np "${np}" --oversubscribe --bind-to none \
        "${extra_env[@]}" \
        "${work}/build/parallel_test_cohesive_probe" "${work}/run${k}/stage" "${SRC}" "${frozen}" "${sc}" \
        >"${scratch}/log" 2>&1 < /dev/null
      rc=$?
      t1=$(date +%s)
      printf '%s\t%s\t%s\t%s\t%s\n' "${k}" "${sc}" "${np}" "${rc}" "$((t1 - t0))" >> "${summary}"
      echo "[parallel_test_cohesive_probe] run ${k} ${sc} np${np}: rc=${rc} ($((t1 - t0)) s)"
      if [ "${rc}" != 0 ]; then
        { echo "=== run ${k} ${sc} np${np} rc=${rc}"; tail -40 "${scratch}/log"; } >> "${failures}"
      fi
    done
  done
done
# per scenario: every run and rank count succeeded and the runs are identical
promote="${work}/promote/stage"
mkdir -p "${promote}"
verdicts="${work}/verdicts.tsv"
printf 'scenario\tverdict\tdetail\n' > "${verdicts}"
for sc in ${scenarios//,/ }; do
  bad=$(awk -F'\t' -v s="${sc}" 'NR > 1 && $2 == s && $4 != 0' "${summary}" | wc -l)
  if [ "${bad}" != 0 ]; then
    printf '%s\tFAILED\t%s failing (run, ranks) pairs\n' "${sc}" "${bad}" >> "${verdicts}"
    continue
  fi
  same=1
  for k in $(seq 2 "${runs}"); do
    if ! diff -r "${work}/run1/stage/${root_name}/${sc}" "${work}/run${k}/stage/${root_name}/${sc}" \
      > "${work}/determinism-${sc}-${k}.diff" 2>&1; then
      same=0
      { echo "=== ${sc}: runs 1 and ${k} differ"; head -40 "${work}/determinism-${sc}-${k}.diff"; } >> "${failures}"
    fi
  done
  if [ "${same}" = 0 ]; then
    printf '%s\tRUN-DEPENDENT\truns differ (see failures)\n' "${sc}" >> "${verdicts}"
    continue
  fi
  mkdir -p "${promote}/${root_name}"
  cp -r "${work}/run1/stage/${root_name}/${sc}" "${promote}/${root_name}/"
  printf '%s\tPROMOTED\t%s identical runs\n' "${sc}" "${runs}" >> "${verdicts}"
done
cat "${summary}" "${verdicts}"
cat "${failures}"
n=$(find "${promote}" -name manifest.json | wc -l)
echo "[parallel_test_cohesive_probe] ${runs} runs; promoted case(s): ${n}"
if [ "${PTC_STAGE_ONLY:-0}" = 1 ]; then
  # staging mode (a scratch --out): the promoted cases and the reports
  mkdir -p /fixtures/stage /fixtures/reports
  cp -r "${promote}/." /fixtures/stage/
  cp "${summary}" "${verdicts}" "${failures}" /fixtures/reports/
  exit 0
fi
[ "${n}" -gt 0 ] || exit 1
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${promote}" \
  --created "${AKANTU_PROBE_CREATED}"
