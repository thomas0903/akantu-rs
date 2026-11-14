#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: freeze the meshes the reference generates with Gmsh at build
# time (`add_mesh()`, cmake/Modules/FindGMSH.cmake) — decision D-009.
#
# Usage: oracle/probes/mesh_io/freeze-meshes.sh [--out DIR]
#   --out DIR  destination (default: <repo>/target/frozen-meshes, a scratch
#              directory; re-freezing the committed D-009 inputs needs an
#              explicit --out fixtures/meshes)
#
# Gmsh meshes are platform-dependent (D-073 (4), UD-300):
# the frozen meshes in fixtures/meshes are the aarch64 files and stay the
# inputs of every probe; on an amd64 Docker host this script refuses to
# write into fixtures/meshes (105 of 122 meshes differ on x86_64).
#
# Runs `gmsh -<DIM> -order <ORDER> -o <OUTPUT> <geo>` (the exact add_mesh
# command) for every add_mesh call of test/, examples/ and extra_packages/
# with the Gmsh of the oracle image, writes the meshes under
# <out>/gmsh-<version>/<binary-dir path>/<OUTPUT>, compares each with the
# copy in the oracle build tree (volume akantu-oracle-build, when that
# target was built) and records provenance and SHA-256 sums in
# <out>/frozen_meshes.json. Reference checkout and build volume are mounted
# read-only.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
out="${repo}/target/frozen-meshes"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) [ $# -ge 2 ] || { echo "usage: $0 [--out DIR]" >&2; exit 2; }; out="$2"; shift 2 ;;
    *) echo "usage: $0 [--out DIR]" >&2; exit 2 ;;
  esac
done
mkdir -p "${out}"
out="$(cd "${out}" && pwd -P)"
inputs="$(cd "${repo}/fixtures/meshes" && pwd -P)"
case "${out}/" in
  "${inputs}/"*)
    if [ "${ORACLE_ARCH}" = amd64 ]; then
      echo "ERROR: refusing to write x86_64 Gmsh meshes into ${repo}/fixtures/meshes: the frozen D-009 inputs are the aarch64 meshes (D-073 (4)); use --out <scratch dir>" >&2
      exit 1
    fi
    ;;
esac
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=4g --memory-swap=4g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${here}:/tools:ro" \
  -v "${out}:/out" \
  -e REF_SHA="${REF_SHA}" -e ORACLE_IMAGE_ID="${image_id}" \
  "${ORACLE_IMAGE}" python3 -I /tools/freeze_in_container.py /akantu-src /build/strict /out
