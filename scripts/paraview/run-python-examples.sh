#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): runs upstream's Python
# examples (python/examples, unchanged copies) once with the port's `akantu`
# module and once with the oracle's upstream module, both in the oracle image
# (akantu-oracle:dev: CPython 3.13.5, NumPy 2.2.4; the same calls as
# python/examples/oracle/run.sh), and leaves each run directory in
# <out>/<case>/port/run and <out>/<case>/upstream/run.
#   scripts/paraview/run-python-examples.sh <out-dir> <port-dir> [case ...]
# <port-dir>: the unpacked wheel (scripts/paraview/build-wheel.sh: <out>/port).
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
out="${1:?usage: run-python-examples.sh <out-dir> <port-dir> [case ...]}"
port="${2:?port wheel dir}"
shift 2
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"; port="$(cd "${port}" && pwd)"
want=("$@")
img=akantu-oracle:dev
envs=(-e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1)
while IFS=$'\t' read -r pcase ocase _desc; do
  case "${pcase}" in ''|'#'*) continue ;; esac
  if [ "${#want[@]}" != 0 ] && [[ " ${want[*]} " != *" ${pcase} "* ]]; then continue; fi
  for side in upstream port; do
    w="${out}/${pcase}/${side}"; rm -rf "${w}"; mkdir -p "${w}"
    t0=$(date +%s)
    if [ "${side}" = upstream ]; then
      docker run --rm --memory 4g --init "${envs[@]}" -v akantu-oracle-build:/build:ro \
        -v "${repo}:/src:ro" -v "${w}:/w" -w /w "${img}" bash -c \
        ". /build/install-strict/share/akantu-oracle/env.sh && python3 -u /src/python/examples/harness.py run ${ocase} /w --side oracle --repo /src > /w/stdout.txt 2> /w/stderr.txt; echo \$? > /w/exit_code"
    else
      docker run --rm --memory 4g --init "${envs[@]}" -v "${port}:/port:ro" \
        -v "${repo}:/src:ro" -v "${w}:/w" -w /w -e PYTHONPATH=/port "${img}" bash -c \
        "python3 -u /src/python/examples/harness.py run ${pcase} /w --side port --repo /src > /w/stdout.txt 2> /w/stderr.txt; echo \$? > /w/exit_code"
    fi
    echo "${pcase} ${side}: exit $(cat "${w}/exit_code" 2>/dev/null || echo ?) in $(( $(date +%s) - t0 )) s, $(find "${w}/run" -type f 2>/dev/null | wc -l) files, $(du -sh "${w}" | cut -f1)"
  done
done < "${here}/python-cases.tsv"
