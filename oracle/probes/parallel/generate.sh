#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/parallel/* with the
# probe (oracle/probes/parallel/parallel_probe.cc).
#
# Usage: oracle/probes/parallel/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/scripts/generate-fixtures.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with a second run (PARALLEL_PROBE_RUNS, default 2),
# restricts the scenarios with PARALLEL_PROBE_SCENARIOS, promotes nothing on failure and
# finalises the fixtures tree (SHA256SUMS, size policy). OPENBLAS_CORETYPE is
# forwarded when set (D-050 pin on x86_64 hosts: OPENBLAS_CORETYPE=Haswell;
# recorded in the version-3 cases).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/parallel/generate.sh $*"
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
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=6g --memory-swap=6g --shm-size=1g \
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
  -e PARALLEL_PROBE_STAGE_ONLY="${PARALLEL_PROBE_STAGE_ONLY:-0}" \
  ${PARALLEL_PROBE_RANKS:+-e PARALLEL_PROBE_RANKS="${PARALLEL_PROBE_RANKS}"} \
  ${PARALLEL_PROBE_SCENARIOS:+-e PARALLEL_PROBE_SCENARIOS="${PARALLEL_PROBE_SCENARIOS}"} \
  ${PARALLEL_PROBE_RUNS:+-e PARALLEL_PROBE_RUNS="${PARALLEL_PROBE_RUNS}"} \
  ${PARALLEL_PROBE_TIMEOUT:+-e PARALLEL_PROBE_TIMEOUT="${PARALLEL_PROBE_TIMEOUT}"} \
  ${OPENBLAS_CORETYPE:+-e OPENBLAS_CORETYPE="${OPENBLAS_CORETYPE}"} \
  "${ORACLE_IMAGE}" bash /probes/parallel/run-in-container.sh "${config}"
