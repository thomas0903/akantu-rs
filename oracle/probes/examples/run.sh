#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: oracle runs of the upstream examples ported by
# crates/akantu/examples (fixtures/examples/<case>, see
# oracle/probes/examples/README.md).
#
# Usage (host with the oracle image and volume, docs/ORACLE_BUILD.md):
#   AKANTU_SRC_HOST=/path/to/akantu oracle/probes/examples/run.sh [--config NAME] [--out DIR] [--force]
# EXAMPLES_CASES=<name>[,<name>...] runs a subset of cases.tsv;
# EXAMPLES_RUNS=<n> the number of runs per case (default 3).
# An existing <out>/examples/<case> directory is never replaced unless
# --force is given (goldens are never overwritten by default);
# generate into a scratch --out and compare instead.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
force=0
cmdline="oracle/probes/examples/run.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --force) force=1; shift ;;
    *) echo "usage: $0 [--config NAME] [--out DIR] [--force]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${EXAMPLES_CASES:-}" || "${EXAMPLES_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid EXAMPLES_CASES ${EXAMPLES_CASES}" >&2; exit 2; }
[[ "${EXAMPLES_RUNS:-3}" =~ ^[1-9][0-9]*$ ]] || { echo "invalid EXAMPLES_RUNS" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
describe="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)"
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes/examples 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
if [ "${force}" != 1 ]; then
  existing=()
  while IFS=$'\t' read -r name _; do
    case "${name}" in ''|'#'*) continue ;; esac
    if [ -n "${EXAMPLES_CASES:-}" ] && [[ ",${EXAMPLES_CASES}," != *",${name},"* ]]; then
      continue
    fi
    [ ! -e "${out}/examples/${name}" ] || existing+=("${name}")
  done < "${here}/cases.tsv"
  if [ "${#existing[@]}" != 0 ]; then
    echo "refusing to replace existing case directories in ${out}/examples: ${existing[*]}" >&2
    echo "(generate into a scratch --out and compare, or pass --force)" >&2
    exit 1
  fi
fi
exec docker run --rm --init --memory=6g --memory-swap=6g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/oracle/patches/embedded:/patches-embedded:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  ${EXAMPLES_CASES:+-e EXAMPLES_CASES="${EXAMPLES_CASES}"} \
  ${EXAMPLES_RUNS:+-e EXAMPLES_RUNS="${EXAMPLES_RUNS}"} \
  ${EXAMPLES_STAGE_ON_FAILURE:+-e EXAMPLES_STAGE_ON_FAILURE="${EXAMPLES_STAGE_ON_FAILURE}"} \
  -e EXAMPLES_FORCE="${force}" \
  "${ORACLE_IMAGE}" bash /probes/examples/run-in-container.sh "${config}"
