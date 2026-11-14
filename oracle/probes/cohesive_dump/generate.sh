#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/model/cohesive_dump/* with the
# Probe (oracle/probes/cohesive_dump/cohesive_dump_probe.cc).
#
# Usage: oracle/probes/cohesive_dump/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/probes/cohesive/generate.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with three runs (D-035), promotes nothing on failure and
# finalises the fixtures tree (SHA256SUMS, size policy). D-050: the probe pins
# Eigen's cache sizes itself; OPENBLAS_CORETYPE=Haswell and
# OPENBLAS_NUM_THREADS=1 are passed to the container (no BLAS-backed solver
# is used: explicit lumped-mass dynamics).
# COH_DUMP_PROBE_CASES=<id>[,<id>...] generates (and promotes) only the listed
# cases (ids without the model/cohesive_dump/ prefix);
# COH_DUMP_PROBE_STAGE_ONLY=1 with --out <dir> copies the staged cases without
# finalisation (inspection); COH_DUMP_PROBE_EVIDENCE=1 with --out <dir> runs
# only the never-promoted diagnostic case three times and copies the three
# runs to <dir>/diag_run{1,2,3}.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/cohesive_dump/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${COH_DUMP_PROBE_CASES:-}" || "${COH_DUMP_PROBE_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid COH_DUMP_PROBE_CASES ${COH_DUMP_PROBE_CASES}" >&2; exit 2; }
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
exec docker run --rm --init --memory=4g --memory-swap=4g --shm-size=1g \
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
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  -e COH_DUMP_PROBE_STAGE_ONLY="${COH_DUMP_PROBE_STAGE_ONLY:-0}" \
  -e COH_DUMP_PROBE_EVIDENCE="${COH_DUMP_PROBE_EVIDENCE:-0}" \
  ${COH_DUMP_PROBE_CASES:+-e COH_DUMP_PROBE_CASES="${COH_DUMP_PROBE_CASES}"} \
  "${ORACLE_IMAGE}" bash /probes/cohesive_dump/run-in-container.sh "${config}"
