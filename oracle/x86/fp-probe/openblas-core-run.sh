#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Probe: run
# oracle/scripts/generate-fixtures.sh targets (in-container part, staged into
# <out-dir>, nothing promoted into fixtures/) with OpenBLAS forced to one
# DYNAMIC_ARCH kernel set (OPENBLAS_CORETYPE), to measure how MUMPS/BLAS-based
# results depend on the x86_64 host CPU. `default` = OpenBLAS's own choice
# (the D-050 Haswell pin is removed for that run).
# Usage: oracle/x86/fp-probe/openblas-core-run.sh <out-dir> <coretype|default> <target>...
# Env: AKANTU_SRC_HOST (reference checkout).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
out="${1:?out dir}"; core="${2:?coretype|default}"; shift 2
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"
# `default` = OpenBLAS's own choice: unset the D-050 pin (image ENV and
# ORACLE_ENV_ARGS) inside the container; otherwise force the given core type.
env_core=(-e OPENBLAS_VERBOSE=2)
unset_core=()
if [ "${core}" = default ]; then unset_core=(env -u OPENBLAS_CORETYPE); else env_core+=(-e OPENBLAS_CORETYPE="${core}"); fi
image_id="$(docker image inspect --format '{{.Id}}' akantu-oracle:dev)"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=4g --memory-swap=4g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST:?}:/akantu-src:ro" -v akantu-oracle-build:/build:ro \
  -v "${repo}/oracle/probes:/probes:ro" -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA=35a851780d0b9471d258ed53a003ae98aaa95ea0 \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE=diagnostic -e AKANTU_PROBE_CREATED=diagnostic \
  -e AKANTU_PROBE_COMMAND="oracle/x86/fp-probe/openblas-core-run.sh ${core} $*" \
  -e AKANTU_PROBE_SOURCES_COMMIT="$(git -C "${repo}" rev-parse HEAD)" \
  -e AKANTU_ALLOW_IMAGE_MISMATCH="${AKANTU_ALLOW_IMAGE_MISMATCH:-0}" -e ORACLE_IMAGE_ID="${image_id}" \
  -e OMPI_MCA_rmaps_base_oversubscribe=1 "${env_core[@]}" \
  akantu-oracle:dev ${unset_core[@]+"${unset_core[@]}"} bash /probes/run-in-container.sh strict installed "$@"
