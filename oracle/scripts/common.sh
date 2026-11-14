# SPDX-License-Identifier: LGPL-3.0-or-later
# Shared settings for the akantu-rs oracle scripts (sourced, not executed).
# See docs/ORACLE_BUILD.md.

ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
# The upstream Akantu checkout (read-only, at AKANTU_REF_SHA). Required by every
# script that mounts it; build-image.sh does not need it.
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:-}"
AKANTU_REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"

ORACLE_SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ORACLE_DIR="$(cd "${ORACLE_SCRIPTS_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${ORACLE_DIR}/.." && pwd)"

# Results directory per architecture (D-043): arm64 results stay in
# oracle/results/ (the recorded aarch64 oracle); results produced by an amd64
# oracle go to oracle/results/x86_64/ so they never overwrite aarch64 records.
# Override with ORACLE_RESULTS_DIR. Shared inputs (known-failures.json) stay in
# oracle/results/.
source "${ORACLE_SCRIPTS_DIR}/oracle-env.sh"   # ORACLE_ARCH, ORACLE_ENV_ARGS (D-050)
if [ -z "${ORACLE_RESULTS_DIR:-}" ]; then
  case "${ORACLE_ARCH}" in
    amd64) ORACLE_RESULTS_DIR="${ORACLE_DIR}/results/x86_64" ;;
    *) ORACLE_RESULTS_DIR="${ORACLE_DIR}/results" ;;
  esac
fi

require_akantu_src() {
  [ -n "${AKANTU_SRC_HOST}" ] && return 0
  echo "ERROR: set AKANTU_SRC_HOST to the upstream Akantu checkout (commit ${AKANTU_REF_SHA})" >&2
  exit 2
}
case "$(basename "$0")" in
  build-image.sh) ;;
  *) require_akantu_src ;;
esac

# Standard mounts: reference source read-only, build volume, and the oracle/
# directory of this repo (read-only: scripts, patches, config files).
# (Plain array, no mapfile: host scripts must run with macOS bash 3.2.)
ORACLE_MOUNTS=(
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro"
  -v "${ORACLE_VOLUME}:/build"
  -v "${ORACLE_DIR}:/oracle:ro"
)

# Resource settings for containers: large /dev/shm for OpenMPI shared memory.
ORACLE_RUN_OPTS=(--shm-size=4g --init ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"})

check_reference_sha() {
  local sha
  sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
  if [ "${sha}" != "${AKANTU_REF_SHA}" ]; then
    echo "ERROR: reference checkout is at ${sha}, expected ${AKANTU_REF_SHA}" >&2
    return 1
  fi
}
