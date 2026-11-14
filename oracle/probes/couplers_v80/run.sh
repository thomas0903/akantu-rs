#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs adversarial coupler probe: host side (a Docker host with the
# akantu-oracle:dev image and the akantu-oracle-build volume, docs/ORACLE_BUILD.md).
# Usage: AKANTU_SRC_HOST=<akantu-src> run.sh <repo> <out_dir>
# V80_MODES="m1 m2" selects the modes (cohesive_static_solo, coupler_energies
# and cohesive_static_iters run only when named there). Copy
# <out_dir>/<mode>.run1.txt to crates/akantu-couplers/tests/data/v80_oracle/.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="${1:?repo}"
out="${2:?out}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
mkdir -p "${out}"
docker run --rm --init --memory=4g --memory-swap=4g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v akantu-oracle-build:/build:ro \
  -v "${here}:/v80:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/out" \
  ${V80_MODES:+-e V80_MODES="$V80_MODES"} -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  akantu-oracle:dev bash /v80/run-in-container.sh
