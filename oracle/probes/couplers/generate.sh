#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/model/couplers/* with the couplers
# probe (oracle/probes/couplers/couplers_probe.cc).
#
# Usage: oracle/probes/couplers/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/scripts/generate-fixtures.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with two more runs (D-035: three identical runs), promotes nothing on failure and
# finalises the fixtures tree (SHA256SUMS, size policy).
# CPL_PROBE_CASES=<id>[,<id>...] generates (and promotes) only the
# listed cases; the other committed cases are left untouched.
# D-050: the probe pins Eigen's cache sizes (Eigen::setCpuCacheSizes(16384,
# 524288, 524288)) and the container gets OPENBLAS_CORETYPE=Haswell,
# OPENBLAS_NUM_THREADS=1 (recorded in each fixture's parameters).
# CPL_PROBE_VARIANT=ud001 labels the variant-only case
# contact_static_friction (skipped without it) with the patch
# oracle/patches/variants/ud001-contact-zero-init.patch, its sha256 and the
# defect UD-001 (parameters.oracle_variant): run it only after
# oracle/probes/contact/build-ud001-variant.sh in a disposable x86_64 host,
# and generate the regular cases before building the variant.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/couplers/generate.sh $*"
# the environment that selects cases / the variant is part of the command
[ -n "${CPL_PROBE_VARIANT:-}" ] && cmdline="CPL_PROBE_VARIANT=${CPL_PROBE_VARIANT} ${cmdline}"
[ -n "${CPL_PROBE_CASES:-}" ] && cmdline="CPL_PROBE_CASES=${CPL_PROBE_CASES} ${cmdline}"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${CPL_PROBE_CASES:-}" || "${CPL_PROBE_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid CPL_PROBE_CASES ${CPL_PROBE_CASES}" >&2; exit 2; }
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
variant_env=()
case "${CPL_PROBE_VARIANT:-}" in
  "") ;;
  ud001)
    vpatch="oracle/patches/variants/ud001-contact-zero-init.patch"
    vsha="$(sha256sum "${repo}/${vpatch}" | cut -d' ' -f1)"
    variant_env=(-e CPL_PROBE_VARIANT_PATCH="${vpatch}" -e CPL_PROBE_VARIANT_SHA256="${vsha}"
                 -e CPL_PROBE_VARIANT_DEFECT=UD-001) ;;
  *) echo "unknown CPL_PROBE_VARIANT ${CPL_PROBE_VARIANT}" >&2; exit 2 ;;
esac
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
  -e CPL_PROBE_STAGE_ONLY="${CPL_PROBE_STAGE_ONLY:-0}" \
  ${CPL_PROBE_TRACE:+-e CPL_PROBE_TRACE=1} \
  ${CPL_PROBE_CASES:+-e CPL_PROBE_CASES="${CPL_PROBE_CASES}"} \
  ${variant_env[@]+"${variant_env[@]}"} \
  "${ORACLE_IMAGE}" bash /probes/couplers/run-in-container.sh "${config}"
