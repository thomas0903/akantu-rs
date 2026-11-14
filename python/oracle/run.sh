#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: behavioural comparison of upstream Python API
# snippets between the oracle image's upstream module and the port
# (docs/TESTING.md).
# Usage: python/oracle/run.sh [snippet ...]   (default: all snippets)
# Inputs: python/oracle/inputs.txt ("<name> <repository path>" per line).
# Expected differences (reported, not failing; docs/TESTING.md):
# bi_material (the oracle solves with MUMPS, bi_material_eigen is the exact
# twin), selector_temporary (UD-833 probe).
# Env:   PYTHON (port interpreter, default .venv/bin/python of the repo),
#        WORK (scratch directory, default: mktemp)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
py="${PYTHON:-${repo}/.venv/bin/python}"
work="${WORK:-$(mktemp -d)}"
snippets=("$@")
[ ${#snippets[@]} -eq 0 ] && snippets=(mesh_read parser_values fe_engine smm_static smm_explicit
  solver_layer custom_material bi_material bi_material_eigen laws_api selector_temporary
  dumpable cohesive other_models phase_field contact_detection contact_coupler fe_engine_ops
  residual_distributed)
expected_differ=" bi_material selector_temporary "
mkdir -p "${work}"
cp "${here}"/snippets/*.py "${work}/"
while read -r name path; do
  case "${name}" in ''|'#'*) continue ;; esac
  cp "${repo}/${path}" "${work}/${name}"
done < "${here}/inputs.txt"
status=0
for s in "${snippets[@]}"; do
  echo "== ${s}"
  # every snippet runs in a fresh copy of the work dir (snippets write files)
  mkdir -p "${work}/${s}.d"
  cp "${work}"/*.py "${work}"/*.msh "${work}"/*.dat "${work}/${s}.d/" 2>/dev/null || true
  # oracle: upstream module, read-only build volume, 4 GB memory cap
  docker run --rm -i --memory 4g --init -v akantu-oracle-build:/build:ro \
    -v "${work}/${s}.d:/work" -w /work akantu-oracle:dev bash -c \
    ". /build/install-strict/share/akantu-oracle/env.sh && python3 ${s}.py" \
    > "${work}/${s}.oracle" 2> "${work}/${s}.oracle.err" || true
  mkdir -p "${work}/${s}.p"
  cp "${work}"/*.py "${work}"/*.msh "${work}"/*.dat "${work}/${s}.p/" 2>/dev/null || true
  (cd "${work}/${s}.p" && "${py}" "${s}.py" > "../${s}.port" 2> "../${s}.port.err") || true
  if ! python3 "${here}/compare.py" "${work}/${s}.oracle" "${work}/${s}.port" 0; then
    case "${expected_differ}" in
      *" ${s} "*) echo "   (expected difference, see docs/TESTING.md)" ;;
      *) status=1 ;;
    esac
  fi
done
echo "outputs in ${work}"
exit ${status}
