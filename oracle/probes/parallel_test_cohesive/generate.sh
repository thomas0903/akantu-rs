#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/parallel_test_cohesive/* with the
# Probe (oracle/probes/parallel_test_cohesive/
# parallel_test_cohesive_probe.cc): upstream's
# TestSMMCFixture 2-D and 3-D cases at 2 ranks with the port's RCB partition.
#
# Usage: oracle/probes/parallel_test_cohesive/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/probes/parallel_cohesive/generate.sh: runs in
# akantu-oracle:dev against the installed oracle /build/install-<config>,
# reference checkout and build volume read-only, refuses an unpinned or dirty
# reference checkout, runs every case under `mpirun -np 2` and the whole set
# PTC_RUNS times (default 3), promotes only the cases whose runs succeeded and
# are byte-identical, and reports every failing or run-dependent case. PTC_CASES
# (comma-separated) restricts the cases; PTC_TIMEOUT (s, default 900) bounds
# each mpirun; PTC_STAGE_ONLY=1 copies the promoted cases and the reports to
# <out>/stage/ and <out>/reports/ instead of finalising <out>. The numerical
# environment pin and x86-64-v3 host check of oracle/scripts/oracle-env.sh
# apply (OPENBLAS_CORETYPE=Haswell on amd64, recorded in every case).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/parallel_test_cohesive/generate.sh $*"
# version 2: PTC_RECORD=boundary writes the boundary
# record under fixtures/parallel_test_cohesive_boundary (part of the command)
[[ -z "${PTC_RECORD:-}" || "${PTC_RECORD}" == boundary ]] ||
  { echo "invalid PTC_RECORD ${PTC_RECORD}" >&2; exit 2; }
[ -n "${PTC_RECORD:-}" ] && cmdline="PTC_RECORD=${PTC_RECORD} ${cmdline}"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${PTC_CASES:-}" || "${PTC_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid PTC_CASES ${PTC_CASES}" >&2; exit 2; }
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
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=6g \
  --memory-swap=6g --shm-size=1g \
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
  -e PTC_STAGE_ONLY="${PTC_STAGE_ONLY:-0}" \
  ${PTC_CASES:+-e PTC_CASES="${PTC_CASES}"} \
  ${PTC_RECORD:+-e PTC_RECORD="${PTC_RECORD}"} \
  ${PTC_RUNS:+-e PTC_RUNS="${PTC_RUNS}"} \
  ${PTC_TIMEOUT:+-e PTC_TIMEOUT="${PTC_TIMEOUT}"} \
  "${ORACLE_IMAGE}" bash /probes/parallel_test_cohesive/run-in-container.sh "${config}"
