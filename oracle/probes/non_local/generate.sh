#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/model/non_local/* with the non-local
# probe (oracle/probes/non_local/non_local_probe.cc).
#
# Usage: oracle/probes/non_local/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/scripts/generate-fixtures.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with two more runs (D-035: three identical runs),
# promotes nothing on failure and finalises the fixtures tree (SHA256SUMS,
# size policy). NL_PROBE_STAGE_ONLY=1 with --out <dir> copies the staged
# cases without finalisation (inspection). NL_PROBE_CASES=<id,...> restricts
# the run to the listed filterable cases (weight_rate_sweep). The x86_64 oracle runs with the
# Eigen cache sizes pinned in the probe and OPENBLAS_CORETYPE=Haswell,
# OPENBLAS_NUM_THREADS=1 (D-050). The container gets 6 GB: the probe TU
# (solid model + non-local laws) exceeds 4 GB with -O2 -g.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/non_local/generate.sh $*"
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
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 \
  -e NL_PROBE_STAGE_ONLY="${NL_PROBE_STAGE_ONLY:-0}" \
  ${NL_PROBE_CASES:+-e NL_PROBE_CASES="${NL_PROBE_CASES}"} \
  "${ORACLE_IMAGE}" bash /probes/non_local/run-in-container.sh "${config}"
