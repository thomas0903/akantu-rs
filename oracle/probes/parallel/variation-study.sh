#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: variation study of upstream's run-dependent periodic `_sum`
# (D-039 amendment). Runs the
# parallel probe (oracle/probes/parallel/parallel_probe.cc, probe version 4)
# RUNS times at NP ranks with PARALLEL_PROBE_UNMASKED_SUM=1 (the probe then
# also records the unmasked non-dyadic `_sum`) and keeps every run in
# <out>/run<k>/stage; nothing is promoted into fixtures/. Analyse with
#   python3 -I oracle/probes/parallel/variation_analysis.py <out>
#
# Usage: oracle/probes/parallel/variation-study.sh --out DIR [--runs N]
#          [--np P] [--config NAME] [--scenarios LIST]
# Same container, mounts and checks as generate.sh (akantu-oracle:dev, the
# installed oracle /build/install-<config>, reference checkout and build
# volume read-only, pinned and clean reference checkout). OPENBLAS_CORETYPE
# is forwarded when set (D-050 pin on x86_64 hosts).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out=""
runs=12
np=4
scenarios="periodic_1d_x,periodic_2d_x,periodic_2d_xy,periodic_3d_x,periodic_3d_xy,periodic_3d_xyz,v101_periodic_2d_x,v101_periodic_2d_xy,v101_periodic_3d_xyz"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --runs) runs="$2"; shift 2 ;;
    --np) np="$2"; shift 2 ;;
    --scenarios) scenarios="$2"; shift 2 ;;
    *) echo "usage: $0 --out DIR [--runs N] [--np P] [--config NAME] [--scenarios LIST]" >&2; exit 2 ;;
  esac
done
[ -n "${out}" ] || { echo "--out is required" >&2; exit 2; }
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ "${runs}" =~ ^[0-9]+$ ]] && [ "${runs}" -ge 2 ] || { echo "--runs must be >= 2" >&2; exit 2; }
[[ "${np}" =~ ^[0-9]+$ ]] && [ "${np}" -ge 1 ] || { echo "--np must be >= 1" >&2; exit 2; }
[[ "${scenarios}" =~ ^[A-Za-z0-9_,]+$ ]] || { echo "invalid scenario list" >&2; exit 2; }
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
[ -z "$(ls -A "${out}")" ] || { echo "${out} is not empty" >&2; exit 2; }
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/out" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="oracle/probes/parallel/variation-study.sh (variation study, not promoted)" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e PARALLEL_PROBE_SCENARIOS="${scenarios}" \
  -e VARIATION_RUNS="${runs}" -e VARIATION_NP="${np}" \
  ${OPENBLAS_CORETYPE:+-e OPENBLAS_CORETYPE="${OPENBLAS_CORETYPE}"} \
  "${ORACLE_IMAGE}" bash /probes/parallel/variation-runner.sh "${config}"
