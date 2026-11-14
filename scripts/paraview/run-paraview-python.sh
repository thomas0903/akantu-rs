#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): loads and renders the
# outputs of the Python examples (run-python-examples.sh) with the headless
# ParaView image, as run-paraview.sh does for the compiled examples.
#   scripts/paraview/run-paraview-python.sh <py-out> <result-out> [case ...]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
py="${1:?usage: run-paraview-python.sh <py-out> <result-out> [case ...]}"
res="${2:?}"
shift 2
mkdir -p "${res}"
py="$(cd "${py}" && pwd)"; res="$(cd "${res}" && pwd)"
image="${PVIEW_IMAGE:-akantu-rs-paraview:5.13.3}"
want=("$@")
while IFS=$'\t' read -r pcase _ocase desc; do
  case "${pcase}" in ''|'#'*) continue ;; esac
  if [ "${#want[@]}" != 0 ] && [[ " ${want[*]} " != *" ${pcase} "* ]]; then continue; fi
  [ -d "${py}/${pcase}/port/run" ] || { echo "${pcase}: no run directory, skipped" >&2; continue; }
  d="${res}/py-${pcase}"; rm -rf "${d}"; mkdir -p "${d}"
  extra=()
  [ -f "${here}/views.tsv" ] && v="$(awk -F'\t' -v n="py-${pcase}" '$1==n{print $2}' "${here}/views.tsv")" && [ -n "${v}" ] && extra=(--views "${v}")
  t0=$(date +%s)
  set +e
  docker run --rm --init --memory 8g --memory-swap 8g --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "${here}:/pv:ro" -v "${py}:/py:ro" -v "${d}:/res" \
    "${image}" pvpython /pv/pv_example.py --example "py-${pcase}" --port "/py/${pcase}/port/run" --oracle "/py/${pcase}/upstream/run" --out /res \
      --title "py-${pcase}: ${desc%%.*}" ${extra[@]+"${extra[@]}"} > "${d}/stdout.txt" 2> "${d}/stderr.txt"
  rc=$?
  set -e
  echo "${rc}" > "${d}/exit_code"
  echo "py-${pcase}: pvpython exit ${rc} in $(( $(date +%s) - t0 )) s; stderr $(wc -c < "${d}/stderr.txt") bytes; VTK messages $(grep -c . "${d}/messages.txt" || true); $(find "${d}" -maxdepth 1 -name '*.png' | wc -l) pictures"
done < "${here}/python-cases.tsv"
