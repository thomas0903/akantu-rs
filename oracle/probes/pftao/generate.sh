#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate fixtures/solver/petsc/oracle/pftao/<case>.<arch>[-<core>].json
# with the probe (oracle/probes/pftao/pftao_probe.cc) in
# akantu-oracle:dev, reference checkout and build volume read-only. Refuses
# an unpinned or dirty reference checkout and an existing output file (golden
# files are never replaced); writes nothing unless the three runs of every
# case are identical. The D-050 numerical pin and the x86-64-v3 host check
# come from oracle/scripts/oracle-env.sh. Run on the x86_64 oracle
# host:
#   AKANTU_SRC_HOST=/path/to/akantu oracle/probes/pftao/generate.sh [--config strict] [--out DIR]
# --out DIR writes into DIR instead of the fixtures tree (a
# scratch regeneration, e.g. to compare the committed records with a new
# probe version; the same refusal of existing files applies there).
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
outdir="${repo}/fixtures/solver/petsc/oracle/pftao"
cmdline="oracle/probes/pftao/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) outdir="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
level="$(oracle_x86_64_level)"
if [ "${ORACLE_ARCH}" = amd64 ] && [ "${level}" != v3 ] && [ "${ORACLE_ALLOW_NON_V3:-0}" != 1 ]; then
  echo "host is ${level}: the authoritative x86_64 oracle needs x86-64-v3 (D-073)" >&2
  exit 1
fi
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
arch="$(uname -m)"
suffix=""
if [ "${ORACLE_ARCH}" = amd64 ]; then suffix="-haswell"; fi
cases="${PFTAO_CASES:-optimal_linear tao_linear_no_split tao_linear_volumetric_deviatoric tao_quadratic_no_split tao_quadratic_volumetric_deviatoric cpl_tao_linear_no_split cpl_tao_linear_volumetric_deviatoric cpl_tao_quadratic_no_split cpl_tao_quadratic_volumetric_deviatoric}"
for c in ${cases}; do
  [ ! -e "${outdir}/${c}.${arch}${suffix}.json" ] ||
    { echo "${outdir}/${c}.${arch}${suffix}.json exists: golden files are never replaced" >&2; exit 1; }
done
tmp="$(mktemp -d)"
chmod 777 "${tmp}"
docker run --rm --init --memory=4g --memory-swap=4g --shm-size=1g \
  ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${tmp}:/out" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e ORACLE_X86_64_LEVEL="${level}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e PFTAO_CASES="${cases}" \
  "${ORACLE_IMAGE}" bash /probes/pftao/run.sh "${config}" /out
mkdir -p "${outdir}"
for c in ${cases}; do
  mv "${tmp}/${c}.json" "${outdir}/${c}.${arch}${suffix}.json"
  echo "wrote ${outdir}/${c}.${arch}${suffix}.json ($(wc -c < "${outdir}/${c}.${arch}${suffix}.json") bytes)"
done
rmdir "${tmp}" 2>/dev/null || true
