#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/parallel_cohesive/* with the
# probe (oracle/probes/parallel_cohesive/parallel_cohesive_probe.cc).
#
# Usage: oracle/probes/parallel_cohesive/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/probes/parallel/generate.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# runs every scenario once per rank count (1, 2, 4; PAR_COH_RANKS) and the
# whole set PAR_COH_RUNS times (default 3), promotes only the scenarios whose
# runs succeeded and are byte-identical, and reports every failing or
# run-dependent scenario (upstream-defect candidates; with
# PAR_COH_STAGE_ONLY=1 the reports are written to <out>/reports/). PAR_COH_SCENARIOS (comma-separated) restricts the
# scenarios; PAR_COH_TIMEOUT (s, default 900) bounds each mpirun.
# OPENBLAS_CORETYPE is forwarded when set (D-050 pin on x86_64 hosts:
# OPENBLAS_CORETYPE=Haswell; recorded in every case). PAR_COH_STAGE_ONLY=1
# copies the promoted cases to <out>/stage/ instead of finalising <out>.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/parallel_cohesive/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
describe="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)"
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
exec docker run --rm --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e PAR_COH_STAGE_ONLY="${PAR_COH_STAGE_ONLY:-0}" \
  ${PAR_COH_RANKS:+-e PAR_COH_RANKS="${PAR_COH_RANKS}"} \
  ${PAR_COH_SCENARIOS:+-e PAR_COH_SCENARIOS="${PAR_COH_SCENARIOS}"} \
  ${PAR_COH_RUNS:+-e PAR_COH_RUNS="${PAR_COH_RUNS}"} \
  ${PAR_COH_TIMEOUT:+-e PAR_COH_TIMEOUT="${PAR_COH_TIMEOUT}"} \
  ${OPENBLAS_CORETYPE:+-e OPENBLAS_CORETYPE="${OPENBLAS_CORETYPE}"} \
  "${ORACLE_IMAGE}" bash /probes/parallel_cohesive/run-in-container.sh "${config}"
