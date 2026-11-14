# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: shared settings (sourced, not
# executed). Host scripts run with macOS bash 3.2 or Linux bash 5. See
# ../README.md.

PERF_IMAGE="${PERF_IMAGE:-akantu-oracle:dev}"
# Oracle build volume (install trees), mounted read-only at /build.
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
# Scratch volume of this harness (meshes, driver builds, run directories),
# mounted at /perf. Remove with `docker volume rm akantu-perf-cohesive`.
PERF_VOLUME="${PERF_VOLUME:-akantu-perf-cohesive}"
# Container memory limit (RAM = RAM+swap: no swap). Override per run.
PERF_MEMORY="${PERF_MEMORY:-8g}"

PERF_SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PERF_DIR="$(cd "${PERF_SCRIPTS_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${PERF_DIR}/../.." && pwd)"

# Input validation, shared with the in-container scripts.
# shellcheck source=in-container/validate.sh
. "${PERF_SCRIPTS_DIR}/in-container/validate.sh"

PERF_MOUNTS=(
  -v "${ORACLE_VOLUME}:/build:ro"
  -v "${PERF_VOLUME}:/perf"
  -v "${PERF_DIR}:/harness:ro"
)

# OpenMPI shared memory needs a large /dev/shm; --init reaps MPI daemons.
perf_set_run_opts() {
  perf_check_memory "${PERF_MEMORY}" || exit 2
  PERF_RUN_OPTS=(--shm-size=4g --init --memory="${PERF_MEMORY}" --memory-swap="${PERF_MEMORY}")
}
perf_set_run_opts

perf_image_digest() {
  docker image inspect --format '{{.Id}}' "${PERF_IMAGE}"
}

perf_docker() {
  # Usage: perf_docker <cmd> [args...]   (runs as the image user `akantu`)
  docker volume inspect "${PERF_VOLUME}" >/dev/null 2>&1 \
    || docker volume create "${PERF_VOLUME}" >/dev/null
  # A new volume is owned by root, and Docker re-applies the mount point's
  # ownership while the volume is empty: create the sub-directories and hand
  # everything to the image user (uid 1000) once.
  if ! docker run --rm -v "${PERF_VOLUME}:/perf" "${PERF_IMAGE}" test -w /perf/runs; then
    docker run --rm -u 0 -v "${PERF_VOLUME}:/perf" "${PERF_IMAGE}" \
      sh -c 'mkdir -p /perf/meshes /perf/builds /perf/runs && chown -R 1000:1000 /perf'
  fi
  # Run settings forwarded to the container (and by run-one.sh to every
  # rank): Scotch/RandomGenerator seed, Scotch thread count (the image's
  # Scotch 7 partitions multithreaded by default, and its partitions then vary
  # from run to run even with a fixed seed), fillNodesToElements mode. The
  # image id is recorded in every build and run record.
  [ -n "${PERF_IMAGE_ID:-}" ] || PERF_IMAGE_ID="$(perf_image_digest)"
  docker run --rm "${PERF_RUN_OPTS[@]}" "${PERF_MOUNTS[@]}" -w /perf \
    -e AKA_SEED="${AKA_SEED:-1}" -e SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}" \
    -e AKANTU_RS_FILL_NODES_TO_ELEMENTS="${AKANTU_RS_FILL_NODES_TO_ELEMENTS:-upstream}" \
    -e PERF_IMAGE="${PERF_IMAGE}" -e PERF_IMAGE_ID="${PERF_IMAGE_ID}" \
    -e PERF_STATIC_SPARSE_SOLVER="${PERF_STATIC_SPARSE_SOLVER:-}" \
    "${PERF_IMAGE}" "$@"
}

perf_volume_write() {
  # Usage: perf_volume_write /perf/<path> < data   (creates parent directories;
  # the path must be under /perf/runs/ or /perf/meshes/, without symlinks)
  docker run --rm -i -v "${PERF_VOLUME}:/perf" -v "${PERF_DIR}:/harness:ro" "${PERF_IMAGE}" \
    bash -c '
      set -euo pipefail
      . /harness/scripts/in-container/validate.sh
      case "$1" in
        /perf/runs/*) root=/perf/runs ;;
        /perf/meshes/*) root=/perf/meshes ;;
        *) echo "ERROR: refusing to write $1" >&2; exit 2 ;;
      esac
      perf_confine_dir "${root}" "$1" || exit 2
      mkdir -p "$(dirname "$1")"
      cat > "$1"
    ' _ "$1"
}
