#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: build the patched oracle variant `embedded` (D-060,
# docs/ORACLE_BUILD.md), run upstream's embedded tests on it and generate
# the fixtures fixtures/model/embedded/* with oracle/probes/embedded/
# embedded_probe.cc (three identical runs per case, D-035).
#
# Usage: oracle/probes/embedded/generate.sh [--config NAME] [--out DIR] [--meta DIR]
# Runs in akantu-oracle:dev against the installed oracle /build/install-<config>
# (the D-050-pinned x86_64 install of the x86_64 oracle host);
# reference checkout and build volume read-only; refuses an unpinned or
# dirty reference checkout. EMBEDDED_PROBE_CASES="<case> ..." restricts the
# cases. Side outputs (upstream test results, logs, provenance) go to --meta
# (default target/embedded-variant).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
meta="${repo}/target/embedded-variant"
cmdline="oracle/probes/embedded/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --meta) meta="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR] [--meta DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
describe="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)"
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes oracle/patches/embedded 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
mkdir -p "${out}" "${meta}"
out="$(cd "${out}" && pwd)"
meta="$(cd "${meta}" && pwd)"
docker run --rm --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/oracle/patches/embedded:/patches:ro" \
  -v "${out}:/fixtures" \
  -v "${meta}:/fixtures-meta" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e EMBEDDED_PROBE_CASES="${EMBEDDED_PROBE_CASES:-}" \
  "${ORACLE_IMAGE}" bash /probes/embedded/run-in-container.sh "${config}"
