#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): loads and renders the
# outputs of both runs with the headless ParaView image (scripts/paraview/README.md).
#   scripts/paraview/run-paraview.sh <port-out> <oracle-out> <result-out> [example ...]
# <port-out>/<example> and <oracle-out>/<oracle case> are the directories made by
# run-port.sh / run-oracle.sh; results go to <result-out>/<example>/ (PNGs,
# result.json, messages.txt = the VTK output window, stderr.txt = pvpython's
# standard error: both must be empty for a clean load).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
port="${1:?usage: run-paraview.sh <port-out> <oracle-out> <result-out> [example ...]}"
oracle="${2:?}"; res="${3:?}"
shift 3
mkdir -p "${res}"
port="$(cd "${port}" && pwd)"; oracle="$(cd "${oracle}" && pwd)"; res="$(cd "${res}" && pwd)"
image="${PVIEW_IMAGE:-akantu-rs-paraview:5.13.3}"
want=("$@")
while IFS=$'\t' read -r name _binary ocase _args desc; do
  case "${name}" in ''|'#'*) continue ;; esac
  if [ "${#want[@]}" != 0 ] && [[ " ${want[*]} " != *" ${name} "* ]]; then continue; fi
  [ -d "${port}/${name}" ] || { echo "${name}: no port output, skipped" >&2; continue; }
  o="-"; mount=()
  if [ "${ocase}" != "-" ] && [ -d "${oracle}/${ocase}" ]; then o="/oracle/${ocase}"; mount=(-v "${oracle}:/oracle:ro"); fi
  d="${res}/${name}"; rm -rf "${d}"; mkdir -p "${d}"
  extra=()
  [ -f "${here}/views.tsv" ] && v="$(awk -F'\t' -v n="${name}" '$1==n{print $2}' "${here}/views.tsv")" && [ -n "${v}" ] && extra=(--views "${v}")
  t0=$(date +%s)
  set +e
  docker run --rm --init --memory 8g --memory-swap 8g --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "${here}:/pv:ro" -v "${port}:/port:ro" ${mount[@]+"${mount[@]}"} -v "${d}:/res" \
    "${image}" pvpython /pv/pv_example.py --example "${name}" --port "/port/${name}" --oracle "${o}" --out /res \
      --title "${name}: ${desc%%.*}" ${extra[@]+"${extra[@]}"} > "${d}/stdout.txt" 2> "${d}/stderr.txt"
  rc=$?
  set -e
  echo "${rc}" > "${d}/exit_code"
  echo "${name}: pvpython exit ${rc} in $(( $(date +%s) - t0 )) s; stderr $(wc -c < "${d}/stderr.txt") bytes; VTK messages $(grep -c . "${d}/messages.txt" || true); $(find "${d}" -maxdepth 1 -name '*.png' | wc -l) pictures"
done < "${here}/cases.tsv"
