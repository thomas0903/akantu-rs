#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate the fixtures fixtures/solver_model/* with the solver-model
# probe (oracle/probes/solver_model/solver_model_probe.cc).
#
# Usage: oracle/probes/solver_model/generate.sh [--config NAME] [--out DIR]
# Same rules as oracle/scripts/generate-fixtures.sh: runs in akantu-oracle:dev
# against the installed oracle /build/install-<config>, reference checkout and
# build volume read-only, refuses an unpinned or dirty reference checkout,
# checks determinism with a second run, promotes nothing on failure and
# finalises the fixtures tree (SHA256SUMS, size policy).
# SOLVER_MODEL_PROBE_CASES=<id>[,<id>...] generates (and promotes) only the
# listed cases; the other committed cases are left untouched.
# SOLVER_MODEL_PROBE_NP=2 runs the probe under `mpirun -np 2` (the case
# dbltrace_newton_raphson_np2, probe v5); SOLVER_MODEL_PROBE_RUNS=N (default
# 2) compares N runs before promoting (D-035: at least 3 for a new golden).
# OpenBLAS core type: ORACLE_ENV_ARGS (oracle-env.sh) pins Haswell on an amd64
# host; on arm64 it stays unset, so a run records the environment aarch64-auto
# (D-121); OPENBLAS_CORETYPE=<type> overrides it on either.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
out="${repo}/fixtures"
cmdline="oracle/probes/solver_model/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME] [--out DIR]" >&2; exit 2 ;;
  esac
done
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config ${config}" >&2; exit 2; }
[[ -z "${SOLVER_MODEL_PROBE_CASES:-}" || "${SOLVER_MODEL_PROBE_CASES}" =~ ^[a-z0-9_]+(,[a-z0-9_]+)*$ ]] ||
  { echo "invalid SOLVER_MODEL_PROBE_CASES ${SOLVER_MODEL_PROBE_CASES}" >&2; exit 2; }
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
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=4g --memory-swap=4g --shm-size=1g \
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
  ${OPENBLAS_CORETYPE:+-e OPENBLAS_CORETYPE="${OPENBLAS_CORETYPE}"} -e OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-1}" \
  -e SOLVER_MODEL_PROBE_STAGE_ONLY="${SOLVER_MODEL_PROBE_STAGE_ONLY:-0}" \
  -e SOLVER_MODEL_PROBE_NP="${SOLVER_MODEL_PROBE_NP:-1}" \
  -e SOLVER_MODEL_PROBE_RUNS="${SOLVER_MODEL_PROBE_RUNS:-2}" \
  ${SOLVER_MODEL_PROBE_TRACE:+-e SOLVER_MODEL_PROBE_TRACE=1} \
  ${SOLVER_MODEL_PROBE_CASES:+-e SOLVER_MODEL_PROBE_CASES="${SOLVER_MODEL_PROBE_CASES}"} \
  "${ORACLE_IMAGE}" bash /probes/solver_model/run-in-container.sh "${config}"
