#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/examples/run.sh. Builds the oracle variants of
# oracle/probes/examples/*.cc against /build/install-<config>, then runs
# every case of cases.tsv (EXAMPLES_CASES=<name>[,<name>...] selects some)
# (`-` for no frozen mesh, `probe:<file>` inputs copied from
# oracle/probes/examples, optional 9th column = command-line arguments)
# EXAMPLES_RUNS times (default 3, D-035) in fresh directories holding the
# upstream input files and the frozen meshes, and collects each case into
# /fixtures/examples/<name> (collect.py). Nothing is written to /fixtures
# unless every selected case succeeded, and an existing case directory is
# replaced only with EXAMPLES_FORCE=1 (run.sh --force).
# D-050 pins: every binary runs with Eigen's cache sizes pinned
# to 16384/524288/524288 (the variants call Eigen::setCpuCacheSizes at the
# start of main; every run, variants and unmodified upstream binaries, also
# preloads eigen_cache_pin.so, whose stderr line is checked) and with
# OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
build="/build/${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
runs="${EXAMPLES_RUNS:-3}"
work="$(mktemp -d /tmp/examples-probe.XXXXXX)"
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/examples" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_EXAMPLES_SRC="${SRC}/examples/c++" \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j2 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
# `emb:<target>` binaries are built against the patched embedded
# oracle variant (D-060, oracle/patches/embedded, docs/ORACLE_BUILD.md;
# the embedded sources do not compile at the reference commit): the
# same patched copies as oracle/probes/embedded/run-in-container.sh (the
# reference checkout and the install are never modified), CMake project
# oracle/probes/examples/embedded.
emb_variant=""
if awk -F'\t' -v sel=",${EXAMPLES_CASES:-}," '$1 !~ /^#/ && $2 ~ /^emb:/ &&
     (sel == ",," || index(sel, "," $1 ",")) {f=1} END {exit !f}' "${PROBES}/examples/cases.tsv"; then
  [ -d /patches-embedded ] || { echo "emb: cases need /patches-embedded (run.sh mounts it)" >&2; exit 2; }
  E=src/model/solid_mechanics/solid_mechanics_model_embedded_interface
  M=src/model/solid_mechanics/materials/material_embedded
  tree="${work}/emb-tree"
  for f in src/model/model_options.hh ${E}/embedded_interface_model.hh ${E}/embedded_interface_model.cc \
           ${E}/embedded_interface_intersector.hh ${E}/embedded_interface_intersector.cc \
           ${M}/material_reinforcement.hh ${M}/material_reinforcement_tmpl.hh; do
    mkdir -p "${tree}/$(dirname "$f")"
    cp "${SRC}/$f" "${tree}/$f"
  done
  cmp "${prefix}/include/akantu/model_options.hh" "${SRC}/src/model/model_options.hh" ||
    { echo "installed model_options.hh differs from the reference" >&2; exit 3; }
  for pf in /patches-embedded/0*.patch; do
    # the source patches only: 0007/0008 patch upstream's embedded tests
    if grep -q '^+++ b/test/' "${pf}"; then continue; fi
    patch -s -p1 --no-backup-if-mismatch -d "${tree}" < "${pf}"
    emb_variant="${emb_variant}oracle/patches/embedded/$(basename "${pf}") sha256=$(sha256sum "${pf}" | cut -d' ' -f1); "
  done
  mkdir -p "${work}/emb-inc" "${work}/emb-src"
  cp -r "${prefix}/include/akantu/." "${work}/emb-inc/"
  cp "${tree}/src/model/model_options.hh" "${tree}/${E}/"*.hh "${tree}/${M}/"*.hh "${work}/emb-inc/"
  cp "${tree}/${E}/"*.cc "${work}/emb-src/"
  cmake -S "${PROBES}/examples/embedded" -B "${work}/emb-build" -G Ninja \
    -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
    -DEMBEDDED_PATCHED_INC="${work}/emb-inc" -DEMBEDDED_PATCHED_SRC="${work}/emb-src" \
    -DAKANTU_EXAMPLES_SRC="${SRC}/examples/c++" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" \
    >"${work}/emb-cmake.log" 2>&1 || { tail -40 "${work}/emb-cmake.log"; exit 1; }
  # Akantu translation units need several GB each: -j1 (oracle/probes/embedded)
  ninja -C "${work}/emb-build" -j1 >"${work}/emb-ninja.log" 2>&1 ||
    { grep -E "error|Error" "${work}/emb-ninja.log" | head -60; exit 1; }
fi
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)/examples/c++"
# OPENBLAS_CORETYPE: D-050 BLAS pin (MUMPS solves of the upstream binaries)
export AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
export OPENBLAS_CORETYPE=Haswell
pin="${work}/build/libeigen_cache_pin.so"
[ -f "${pin}" ] || { echo "no ${pin}" >&2; exit 1; }
pin_line="akantu-rs eigen_cache_pin: l1=16384 l2=524288 l3=524288"
export OMPI_MCA_rmaps_base_oversubscribe=1
stage="${work}/stage"
mkdir -p "${stage}"
status=0
while IFS=$'\t' read -r name binary dir inputs meshes ranks keep arrays args; do
  case "${name}" in ''|'#'*) continue ;; esac
  if [ -n "${EXAMPLES_CASES:-}" ] && [[ ",${EXAMPLES_CASES}," != *",${name},"* ]]; then
    continue
  fi
  case "${binary}" in
    up:*) exe="${build}/examples/c++/${dir}/${binary#up:}" ;;
    probe:*) exe="${work}/build/${binary#probe:}" ;;
    emb:*) exe="${work}/emb-build/${binary#emb:}" ;;
    *) echo "bad binary ${binary}" >&2; exit 2 ;;
  esac
  [ -x "${exe}" ] || { echo "${name}: no executable ${exe}" >&2; status=1; continue; }
  mkdir -p "${work}/eigen-pin"
  case "${binary}" in
    probe:*)
      if grep -q 'Eigen::setCpuCacheSizes(16384, 524288, 524288)' "${PROBES}/examples/${binary#probe:}.cc"; then
        echo "eigen cache sizes: pinned to (16384, 524288, 524288) by the variant (D-050)" > "${work}/eigen-pin/${name}"
      fi ;;
    emb:*)
      {
        echo "oracle variant (D-060): embedded, the upstream source patches applied to copies: ${emb_variant}"
        if [ -f "${PROBES}/examples/${binary#emb:}.cc" ] &&
           grep -q 'Eigen::setCpuCacheSizes(16384, 524288, 524288)' "${PROBES}/examples/${binary#emb:}.cc"; then
          echo "eigen cache sizes: pinned to (16384, 524288, 524288) by the variant (D-050)"
        fi
      } > "${work}/eigen-pin/${name}" ;;
  esac
  run_dirs=()
  for r in $(seq 1 "${runs}"); do
    d="${work}/runs/${name}/run${r}"
    mkdir -p "${d}/paraview"
    if [ "${inputs}" != "-" ]; then
      for f in ${inputs//,/ }; do
        case "${f}" in
          probe:*) cp "${PROBES}/examples/${f#probe:}" "${d}/" ;;
          *) cp "${SRC}/examples/c++/${dir}/${f}" "${d}/" ;;
        esac
      done
    fi
    if [ "${meshes}" != "-" ]; then
      for m in ${meshes//,/ }; do cp "${frozen}/${dir}/${m}" "${d}/"; done
    fi
    launcher="env LD_PRELOAD=${pin}"
    if [ "${ranks}" != 1 ]; then
      launcher="mpirun -np ${ranks} --oversubscribe --bind-to none -x AKA_SEED -x SCOTCH_PTHREAD_NUMBER"
      launcher="${launcher} -x OPENBLAS_CORETYPE -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS -x LD_PRELOAD=${pin}"
    fi
    # (command.txt keeps the wording: the pins are in provenance.txt)
    pretty="${launcher#env LD_PRELOAD=${pin}}"
    pretty="${pretty% -x OPENBLAS_CORETYPE -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS -x LD_PRELOAD=${pin}}"
    echo "${pretty:+${pretty} }${binary}${args:+ ${args}} (in a copy of ${dir}: ${inputs} ${meshes}; AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1)" \
      > "${d}/command.txt"
    set +e
    # stdin from /dev/null: mpirun would otherwise read the rest of cases.tsv
    # shellcheck disable=SC2086 # launcher and args are word lists
    (cd "${d}" && timeout -k 10 3600 ${launcher} "${exe}" ${args} < /dev/null > stdout.txt 2> stderr.txt)
    rc=$?
    # the cache pin was effective in every process of the run
    if [ "$(grep -c "^${pin_line}\$" "${d}/stderr.txt")" -lt "${ranks}" ]; then
      echo "${name} run ${r}: Eigen cache pin line missing in stderr.txt" >&2
      [ "${rc}" != 0 ] || rc=97
    fi
    echo "${rc}" > "${d}/exit_code"
    set -e
    run_dirs+=("${d}")
  done
  copied=""
  for f in ${inputs//,/ } ${meshes//,/ }; do
    # (probe inputs land under their base name, e.g. probe:embedded_p1/concrete.msh)
    [ "${f}" = "-" ] || copied="${copied:+${copied},}$(basename "${f#probe:}")"
  done
  python3 -I "${PROBES}/examples/collect.py" "${name}" "${stage}/${name}" "${keep}" "${ranks}" \
    "${arrays}" "${copied}" "${run_dirs[@]}" || { status=1; tail -5 "${run_dirs[0]}/stderr.txt" >&2; }
  rm -rf "${work}/runs/${name}"
done < "${PROBES}/examples/cases.tsv"
if [ "${status}" != 0 ]; then
  echo "examples: a case failed; staged results kept in ${stage}, nothing promoted" >&2
  if [ "${EXAMPLES_STAGE_ON_FAILURE:-0}" = 1 ]; then
    mkdir -p /fixtures/examples-failed && cp -r "${stage}/." /fixtures/examples-failed/
  fi
  exit 1
fi
provenance() {
  echo "Generated by oracle/probes/examples/run.sh (cases oracle/probes/examples/cases.tsv)"
  echo "command: ${AKANTU_PROBE_COMMAND:-?}"
  echo "probe sources commit: ${AKANTU_PROBE_SOURCES_COMMIT:-?}"
  echo "oracle: ${prefix} (${config}), upstream binaries ${build}/examples, scripts $(cat "${prefix}/READY")"
  echo "reference: ${AKANTU_PROBE_REFERENCE_SHA:-?} (${AKANTU_PROBE_REFERENCE_DESCRIBE:-?})"
  echo "image: ${ORACLE_IMAGE_ID:-?}"
  echo "platform: $(uname -m), Debian $(cat /etc/debian_version), $(g++ --version | head -1)"
  echo "cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
  echo "runs: ${runs}; environment: AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1"
  echo "eigen cache sizes (D-050): l1=16384 l2=524288 l3=524288; variants: Eigen::setCpuCacheSizes at the start of main; every run: LD_PRELOAD eigen_cache_pin.so (oracle/probes/examples/eigen_cache_pin.cc), effective sizes checked in each process's stderr"
  echo "meshes: frozen ${frozen#/frozen-meshes/} (D-009)"
  echo "date: ${AKANTU_PROBE_CREATED:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}"
}
for d in "${stage}"/*/; do
  provenance > "${d}provenance.txt"
  # D-050: whether the case's binary also pins Eigen's cache sizes itself
  # (variants; same sizes as the preloaded eigen_cache_pin.so)
  variant_pin="${work}/eigen-pin/$(basename "${d}")"
  [ -f "${variant_pin}" ] && cat "${variant_pin}" >> "${d}provenance.txt"
done
mkdir -p /fixtures/examples
for d in "${stage}"/*/; do
  case_dir="/fixtures/examples/$(basename "${d}")"
  if [ -e "${case_dir}" ] && [ "${EXAMPLES_FORCE:-0}" != 1 ]; then
    echo "examples: ${case_dir} exists (never replace a golden: use a new --out, or --force)" >&2
    exit 1
  fi
done
for d in "${stage}"/*/; do
  # replace each regenerated case as a whole (no stale files of a previous run)
  case_dir="/fixtures/examples/$(basename "${d}")"
  [ -d "${case_dir}" ] && find "${case_dir}" -mindepth 1 -delete
  mkdir -p "${case_dir}"
  cp -r "${d}." "${case_dir}/"
done
(cd /fixtures/examples && find . -type f ! -name SHA256SUMS | sed 's|^\./||' | LC_ALL=C sort |
  xargs sha256sum > SHA256SUMS)
echo "examples: promoted $(ls "${stage}" | wc -l) case(s) into /fixtures/examples"
