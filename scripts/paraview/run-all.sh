#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the whole ParaView visual
# check in one command (scripts/paraview/README.md). Host: a Linux machine
# with Docker, the project images (akantu-rs-mpi:dev) and the C++ oracle image
# and build volume (akantu-oracle:dev, akantu-oracle-build; docs/ORACLE_BUILD.md),
# i.e. an x86_64 Linux host.
#
#   AKANTU_SRC_HOST=<reference checkout> scripts/paraview/run-all.sh <work-dir>
#
# Steps (STEPS="image examples oracle port ex012 wheel python paraview runinfo"
# by default; set STEPS to a subset to resume):
#   image     docker build of the headless ParaView image (docker/paraview)
#   examples  release build of the Rust examples in the project image
#   oracle    the upstream examples in the oracle image (run-oracle.sh)
#   port      the Rust examples (run-port.sh)
#   ex012     python_user_defined_bc, the example embedding CPython, both sides
#             (run-ex012.sh: feature `python`, NumPy in a throw-away container)
#   wheel     the port's Python wheel (build-wheel.sh)
#   python    upstream's Python examples, port and oracle (run-python-examples.sh)
#   paraview  load + render everything (run-paraview.sh, run-paraview-python.sh)
#   runinfo   <work-dir>/runinfo.json (what each run wrote and printed)
# Afterwards, on any machine with python3, from the copied <work-dir>/res and
# <work-dir>/runinfo.json, and the vtu-diff directory <vd> written by
# vtu_diff.py (scripts/paraview/README.md):
#   python3 -I scripts/paraview/make_report.py <res> <runinfo.json> <vd>
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
work="${1:?usage: run-all.sh <work-dir>}"
mkdir -p "${work}"; work="$(cd "${work}" && pwd)"
: "${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the reference checkout}"
STEPS="${STEPS:-image examples oracle port ex012 wheel python paraview runinfo}"
has() { [[ " ${STEPS} " == *" $1 "* ]]; }
export OPENBLAS_CORETYPE=Haswell
if has image; then docker build --memory 6g -t akantu-rs-paraview:5.13.3 "${repo}/docker/paraview"; fi
if has examples; then "${repo}/scripts/mpi/in-docker.sh" cargo build --release -p akantu --examples --features cgal --locked; fi
if has oracle; then
  cases="$(awk -F'\t' '$1!~/^#/ && NF>=5 && $3!="-"{print $3}' "${here}/cases.tsv" | sort -u | paste -sd, -)"
  ORACLE_CASES="${cases}" "${here}/run-oracle.sh" "${work}/oracle"
fi
if has port; then "${here}/run-port.sh" "${work}/port" "${work}/oracle"; fi
if has ex012; then "${here}/run-ex012.sh" "${work}"; fi
if has wheel; then "${here}/build-wheel.sh" "${work}/py"; fi
if has python; then "${here}/run-python-examples.sh" "${work}/py-runs" "${work}/py/port"; fi
if has paraview; then
  "${here}/run-paraview.sh" "${work}/port" "${work}/oracle" "${work}/res"
  "${here}/run-paraview-python.sh" "${work}/py-runs" "${work}/res"
fi
if has runinfo; then python3 -I "${here}/runinfo.py" "${work}" > "${work}/runinfo.json"; fi
echo "done: ${work}"
