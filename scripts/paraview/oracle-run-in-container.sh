#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): runs INSIDE the oracle
# image (akantu-oracle:dev), started by scripts/paraview/run-oracle.sh. A cut
# down copy of oracle/probes/examples/run-in-container.sh: the same build of
# the oracle variants, the same pins (D-050: Eigen cache sizes through the
# eigen_cache_pin.so preload and the variants, OPENBLAS_CORETYPE=Haswell, one
# BLAS thread), the same fresh run directory per case (upstream's input files
# and the frozen Gmsh meshes), but ONE run of each selected case, and the whole
# run directory (every file the example wrote) is copied to /out/<case>
# instead of being collected into fixtures. Nothing under /fixtures is touched.
#   ORACLE_CASES=<name>[,<name>...]  cases of oracle/probes/examples/cases.tsv
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
build="/build/${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
: "${ORACLE_CASES:?set ORACLE_CASES}"
work="$(mktemp -d /tmp/pview-oracle.XXXXXX)"
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/examples" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_EXAMPLES_SRC="${SRC}/examples/c++" \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
# build only the variants that the selected cases use (Akantu TUs are heavy)
targets="eigen_cache_pin"
while IFS=$'\t' read -r name binary _rest; do
  case "${name}" in ''|'#'*) continue ;; esac
  [[ ",${ORACLE_CASES}," == *",${name},"* ]] || continue
  case "${binary}" in probe:*) targets="${targets} ${binary#probe:}" ;; esac
done < "${PROBES}/examples/cases.tsv"
# shellcheck disable=SC2086 # word list of targets
ninja -C "${work}/build" -j2 ${targets} >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
frozen="$(printf '%s\n' /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)/examples/c++"
export AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
export OPENBLAS_CORETYPE=Haswell
pin="${work}/build/libeigen_cache_pin.so"
[ -f "${pin}" ] || { echo "no ${pin}" >&2; exit 1; }
pin_line="akantu-rs eigen_cache_pin: l1=16384 l2=524288 l3=524288"
export OMPI_MCA_rmaps_base_oversubscribe=1
status=0
# shellcheck disable=SC2034 # keep, arrays: columns of cases.tsv that this script does not use
while IFS=$'\t' read -r name binary dir inputs meshes ranks keep arrays args; do
  case "${name}" in ''|'#'*) continue ;; esac
  [[ ",${ORACLE_CASES}," == *",${name},"* ]] || continue
  case "${binary}" in
    up:*) exe="${build}/examples/c++/${dir}/${binary#up:}" ;;
    probe:*) exe="${work}/build/${binary#probe:}" ;;
    *) echo "bad binary ${binary}" >&2; exit 2 ;;
  esac
  [ -x "${exe}" ] || { echo "${name}: no executable ${exe}" >&2; status=1; continue; }
  d="/out/${name}"
  rm -rf "${d}"; mkdir -p "${d}/paraview"
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
  echo "${launcher} ${binary}${args:+ ${args}} (in a copy of ${dir}: ${inputs} ${meshes})" > "${d}/command.txt"
  t0=$(date +%s)
  set +e
  # shellcheck disable=SC2086 # launcher and args are word lists
  (cd "${d}" && timeout -k 10 3600 ${launcher} "${exe}" ${args} < /dev/null > stdout.txt 2> stderr.txt)
  rc=$?
  set -e
  if [ "$(grep -c "^${pin_line}\$" "${d}/stderr.txt")" -lt "${ranks}" ]; then
    echo "${name}: Eigen cache pin line missing in stderr.txt" >&2
    [ "${rc}" != 0 ] || rc=97
  fi
  echo "${rc}" > "${d}/exit_code"
  echo "${name}: exit ${rc} in $(( $(date +%s) - t0 )) s, $(find "${d}" -type f | wc -l) files, $(du -sh "${d}" | cut -f1)"
  [ "${rc}" = 0 ] || status=1
done < "${PROBES}/examples/cases.tsv"
{
  echo "oracle: ${prefix} (${config}), upstream binaries ${build}/examples, scripts $(cat "${prefix}/READY")"
  echo "reference: ${AKANTU_PROBE_REFERENCE_SHA:-?} (${AKANTU_PROBE_REFERENCE_DESCRIBE:-?})"
  echo "image: ${ORACLE_IMAGE_ID:-?}"
  echo "platform: $(uname -m), Debian $(cat /etc/debian_version), $(g++ --version | head -1)"
  echo "environment: AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1"
  echo "cases: ${ORACLE_CASES}"
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > /out/oracle-provenance.txt
exit "${status}"
