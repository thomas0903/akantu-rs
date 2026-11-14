#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: stage the v87_probe cases of fixtures/model/v87 (adapted from
# oracle/probes/smm2/generate.sh; same container settings, D-050 pins).
# Usage (AKANTU_SRC_HOST: a clean checkout of the reference commit):
#   AKANTU_SRC_HOST=<path> oracle/probes/v87/generate.sh --out <stage dir>
# The committed manifests record the command under the path this probe had
# when the fixtures were generated (docs/ORACLE_BUILD.md).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] --out DIR" >&2; exit 2 ;;
  esac
done
[ -n "${out}" ] || { echo "--out DIR is required" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
describe="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)"
probes_commit="$(git -C "${repo}" rev-parse HEAD)+v87-probe"
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
exec docker run --rm --init --memory=4g --memory-swap=4g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${out}:/out" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  -e AKANTU_PROBE_COMMAND="oracle/probes/v87/generate.sh $*" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 \
  "${ORACLE_IMAGE}" bash /probes/v87/run-in-container.sh "${config}"
