#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/model/diffusion/* with the
# probe (oracle/probes/diffusion/diffusion_probe.cc) and the distributed-run
# evidence oracle/results/diffusion-parallel-strict.txt
# (diffusion_par_probe.cc, 2 and 4 ranks; UD-275; amd64 oracle:
# oracle/results/x86_64/, override with ORACLE_RESULTS_DIR).
#
# Usage: oracle/probes/diffusion/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/probes/smm/generate.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with THREE runs (D-035), promotes nothing on failure and
# finalises the fixtures tree (SHA256SUMS, size policy).
# DIFF_PROBE_CASES=<id>[,<id>...] generates (and promotes) only the listed
# cases (ids without the model/diffusion/ prefix); DIFF_PROBE_STAGE_ONLY=1
# with --out <dir> copies the staged cases without finalisation.
# DIFF_PROBE_SET=v102 generates the review adversarial cases
# (diffusion_v102_probe, model/diffusion/v102_*) instead. Both probes pin
# Eigen's cache sizes (16384 / 524288 / 524288, D-050) and
# the container gets OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1; the
# recorded generator command states both pins.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/diffusion/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${DIFF_PROBE_CASES:-}" || "${DIFF_PROBE_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid DIFF_PROBE_CASES ${DIFF_PROBE_CASES}" >&2; exit 2; }
[[ -z "${DIFF_PROBE_SET:-}" || "${DIFF_PROBE_SET}" = v102 ]] ||
  { echo "invalid DIFF_PROBE_SET ${DIFF_PROBE_SET} (v102 or empty)" >&2; exit 2; }
cmdline="${DIFF_PROBE_SET:+DIFF_PROBE_SET=${DIFF_PROBE_SET} }OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 ${cmdline} (probe pins Eigen::setCpuCacheSizes(16384, 524288, 524288))"
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
# Results per architecture (as oracle/scripts/common.sh, D-043):
# an amd64 oracle writes its distributed evidence to oracle/results/x86_64/
# so that the recorded aarch64 evidence is never overwritten.
if [ -z "${ORACLE_RESULTS_DIR:-}" ]; then
  case "$(docker version --format '{{.Server.Arch}}' 2>/dev/null || uname -m)" in
    amd64|x86_64) ORACLE_RESULTS_DIR="${repo}/oracle/results/x86_64" ;;
    *) ORACLE_RESULTS_DIR="${repo}/oracle/results" ;;
  esac
fi
mkdir -p "${out}" "${ORACLE_RESULTS_DIR}"
out="$(cd "${out}" && pwd)"
exec docker run --rm --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${ORACLE_RESULTS_DIR}:/results" \
  -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e DIFF_PROBE_STAGE_ONLY="${DIFF_PROBE_STAGE_ONLY:-0}" \
  -e DIFF_PROBE_SET="${DIFF_PROBE_SET:-}" \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  ${DIFF_PROBE_CASES:+-e DIFF_PROBE_CASES="${DIFF_PROBE_CASES}"} \
  "${ORACLE_IMAGE}" bash /probes/diffusion/run-in-container.sh "${config}"
