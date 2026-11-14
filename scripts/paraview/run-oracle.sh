#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): runs the upstream
# examples (cases of oracle/probes/examples/cases.tsv) once in the oracle image
# and leaves every file they write in <out>/<case>/ (scripts/paraview/README.md).
#   AKANTU_SRC_HOST=<reference checkout> ORACLE_CASES=static_eigen,... \
#     scripts/paraview/run-oracle.sh <out-dir> [--config strict]
# Host: an x86_64 Linux host with the oracle image and its build volume (oracle image + build volume).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
out="${1:?usage: run-oracle.sh <out-dir> [--config NAME]}"
config=strict
[ "${2:-}" = "--config" ] && config="${3:?}"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST (the reference checkout)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
[ "$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)" = "${REF_SHA}" ] || { echo "reference not at ${REF_SHA}" >&2; exit 1; }
: "${ORACLE_CASES:?set ORACLE_CASES=<case>[,<case>...] (oracle/probes/examples/cases.tsv)}"
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"
exec docker run --rm --init --memory="${PVIEW_ORACLE_MEMORY:-6g}" --memory-swap="${PVIEW_ORACLE_MEMORY:-6g}" --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${repo}/scripts/paraview:/pview:ro" \
  -v "${out}:/out" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)" \
  -e ORACLE_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")" \
  -e ORACLE_CASES="${ORACLE_CASES}" \
  "${ORACLE_IMAGE}" bash /pview/oracle-run-in-container.sh "${config}"
