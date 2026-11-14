#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the adversarial non-local fixtures
# fixtures/model/non_local/<case> (review cases, UD-557 damage bands,
# UD-558 two-law references) with oracle/probes/non_local/adversarial/
# adversarial_probe.cc.
#
# Usage: oracle/probes/non_local/adversarial/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/probes/non_local/generate.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# 6 GB container, OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 and the
# Eigen cache sizes pinned in the probe (D-050). Every case runs three times
# in its own process (D-035; a crashing case, update_rate = 0 / UD-560, is
# recorded in <status dir>/adversarial-case-status.tsv, default
# target/non-local-adversarial, and promotes nothing); only
# cases with three identical runs and a manifest are staged. Finalisation
# (SHA256SUMS, size policy) promotes the staged cases only; the cases are
# new, so no existing golden is replaced. NL_PROBE_STAGE_ONLY=1 copies the
# staged cases to --out without finalisation (inspection).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/non_local/adversarial/generate.sh $*"
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
status_dir="${NL_ADV_STATUS_DIR:-${repo}/target/non-local-adversarial}"
mkdir -p "${status_dir}"
status_dir="$(cd "${status_dir}" && pwd)"
exec docker run --rm --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${out}:/fixtures" \
  -v "${status_dir}:/status" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 \
  -e NL_PROBE_STAGE_ONLY="${NL_PROBE_STAGE_ONLY:-0}" \
  "${ORACLE_IMAGE}" bash /probes/non_local/adversarial/run-in-container.sh "${config}"
