#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/par4/generate.sh. Builds par4_probe, runs every scenario under
# `mpirun -np P` for P in PAR4_PROBE_RANKS (default 1 2 4), one process tree
# per (scenario, P) (each scenario owns the static parser and the material
# registry), PAR4_PROBE_RUNS times (default 3; D-035 determinism check, every
# run compared with the first byte for byte), and promotes the fixtures.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/par4-probe.XXXXXX)"
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
cmake -S "${PROBES}/par4" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -80 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.par4_probe.json" par4_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "par4/par4_probe.cc"
# Scotch: one thread makes the seeded partition reproducible (as
# oracle/probes/parallel/run-in-container.sh; recorded in every case)
export SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}"
export OMPI_MCA_rmaps_base_oversubscribe=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
extra_env=(-x OPENBLAS_NUM_THREADS -x SCOTCH_PTHREAD_NUMBER)
[ -n "${OPENBLAS_CORETYPE:-}" ] && { export OPENBLAS_CORETYPE; extra_env+=(-x OPENBLAS_CORETYPE); }
# <scenario>[@<np>[+<np>...]]: with two neighbourhoods upstream aborts on
# some partitions (UD-1100: nl_pair on 4 ranks, nl_mazars_two on 2 and 4),
# so those are recorded where upstream completes; the aborts are reproduced
# with PAR4_PROBE_ABORTS=1. Later additions: nl_damage_wf,
# nl_mazars_two_strips@3 (strips: upstream completes and its second erasure
# pass acts) and ntn_update_normals.
scenarios="${PAR4_PROBE_SCENARIOS:-nl_region nl_mazars nl_mixed nl_pair@1+2 nl_mazars_two@1 ntn_compression ntrf_coulomb nl_damage_wf nl_mazars_two_strips@3 ntn_update_normals}"
scenarios="${scenarios//,/ }"
run() { # out_dir
  for entry in ${scenarios}; do
    s="${entry%%@*}"
    ranks="${PAR4_PROBE_RANKS:-1 2 4}"
    if [ "${entry}" != "${s}" ]; then
      ranks="${entry#*@}"
      ranks="${ranks//+/ }"
    fi
    for np in ${ranks}; do
      echo "[par4_probe] ${s} np${np}"
      mkdir -p "$1/scratch-${s}-np${np}"
      AKANTU_PROBE_BUILD_INFO="${work}/build_env.par4_probe.json" \
        timeout -k 10 "${PAR4_PROBE_TIMEOUT:-900}" mpirun -np "${np}" --oversubscribe --bind-to none \
        "${extra_env[@]}" \
        "${work}/build/par4_probe" "$1/stage" "${SRC}" "$1/scratch-${s}-np${np}" "${s}"
    done
  done
}
if [ "${PAR4_PROBE_ABORTS:-0}" = 1 ]; then
  # UD-1100 evidence: the two-neighbourhood scenarios on more than one rank;
  # upstream must abort (an assertion of the strict build); the messages are
  # printed, nothing is written to the fixtures.
  status=0
  for entry in nl_pair@4 nl_mazars_two@2 nl_mazars_two@4; do
    s="${entry%%@*}"
    for np in ${entry#*@}; do
      mkdir -p "${work}/aborts/stage" "${work}/aborts/scratch-${s}-np${np}"
      set +e
      AKANTU_PROBE_BUILD_INFO="${work}/build_env.par4_probe.json" \
        timeout -k 10 "${PAR4_PROBE_TIMEOUT:-900}" mpirun -np "${np}" --oversubscribe --bind-to none \
        "${extra_env[@]}" \
        "${work}/build/par4_probe" "${work}/aborts/stage" "${SRC}" "${work}/aborts/scratch-${s}-np${np}" "${s}" \
        >"${work}/abort-${s}-np${np}.log" 2>&1
      rc=$?
      set -e
      echo "[par4_probe] ${s} np${np}: exit ${rc}"
      grep -h "par4_probe (rank" "${work}/abort-${s}-np${np}.log" | LC_ALL=C sort || true
      [ "${rc}" -ne 0 ] || status=1
    done
  done
  exit "${status}"
fi
if [ "${PAR4_PROBE_FULL_LISTS:-0}" = 1 ] && [ "${PAR4_PROBE_STAGE_ONLY:-0}" != 1 ]; then
  echo "PAR4_PROBE_FULL_LISTS=1 needs PAR4_PROBE_STAGE_ONLY=1 (never into the fixtures)" >&2
  exit 2
fi
runs="${PAR4_PROBE_RUNS:-3}"
[[ "${runs}" =~ ^[0-9]+$ ]] && [ "${runs}" -ge 2 ] || { echo "PAR4_PROBE_RUNS must be >= 2" >&2; exit 2; }
mkdir -p "${work}/run1/stage"
run "${work}/run1"
for k in $(seq 2 "${runs}"); do
  mkdir -p "${work}/run${k}/stage"
  run "${work}/run${k}" >"${work}/rerun${k}.log" 2>&1 || { tail -20 "${work}/rerun${k}.log"; exit 1; }
  if ! diff -r "${work}/run1/stage" "${work}/run${k}/stage" >"${work}/determinism${k}.diff"; then
    head -40 "${work}/determinism${k}.diff" >&2
    echo "par4_probe: runs 1 and ${k} differ; nothing is promoted" >&2
    exit 1
  fi
done
echo "[par4_probe] ${runs} identical runs: $(find "${work}/run1/stage" -name manifest.json | wc -l) case(s)"
if [ "${PAR4_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/run1/stage/." /fixtures/
  if [ "${PAR4_PROBE_FULL_LISTS:-0}" = 1 ]; then
    # The full pair / weight lists, next to the staged cases (the
    # output directory of a stage-only run is a scratch directory)
    for d in "${work}"/run1/scratch-*; do
      [ -d "${d}/full-lists" ] || continue
      mkdir -p "/fixtures/full-lists/$(basename "${d}" | sed 's/^scratch-//')"
      cp -r "${d}/full-lists/." "/fixtures/full-lists/$(basename "${d}" | sed 's/^scratch-//')/"
    done
  fi
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/run1/stage" \
  --created "${AKANTU_PROBE_CREATED}"
