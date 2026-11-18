#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Run a command inside the akantu-rs MPI test image with the repository
# mounted read-only at /src, a named volume for the cargo target directory and
# one for the cargo registry. Memory-limited (6 GB), 4 CPUs, cargo -j4 (image
# env). AKANTU_RS_DOCKER_CPUS / AKANTU_RS_DOCKER_MEMORY override the caps
# (e.g. 8 / 12g on a larger host); authoritative gate runs use the defaults.
#
#   scripts/mpi/in-docker.sh cargo test -p akantu-comm --features mpi --locked
#
# Disk: the target volume is what fills a sandbox (a full-debug-info
# workspace build with incremental caches reached 40 GB of a 79 GB disk).
# Builds in the container therefore run without incremental compilation and
# with line-table debug info only (backtraces keep file:line; code
# generation, debug assertions and overflow checks are unchanged).
# AKANTU_RS_CARGO_INCREMENTAL=1 / AKANTU_RS_DEV_DEBUG=2 restore cargo's
# defaults for a debugger session. Stale artifacts of earlier trees still
# accumulate: see "Sandbox disk" in docs/TESTING.md.
#
# AKANTU_RS_CONTAINER_NAME overrides the container name (default
# akantu-rs-mpi-<pid>): ci/lib.sh names the container of each step so that a
# timeout kills only that container.
#
# DOCKER_CPUSET (default: AKANTU_JOB_CPUS, which only the job wrappers of a
# shared measurement host set) restricts the container to those cores
# (docker --cpuset-cpus); unset, no restriction.
#
# AKANTU_COLLECTIVE_CHECKS and AKANTU_COLLECTIVE_HEADER (D-126) are passed
# into the container (empty: the library's defaults).
#
# AKANTU_RS_MPI_TARGET (default /target) selects the cargo target directory
# inside the target volume: give each worktree its own (never share a target
# directory between worktrees).
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
cpus="${AKANTU_RS_DOCKER_CPUS:-4}"
memory="${AKANTU_RS_DOCKER_MEMORY:-6g}"
tty=()
cpuset=()
cs="${DOCKER_CPUSET:-${AKANTU_JOB_CPUS:-}}"
[ -z "$cs" ] || cpuset=(--cpuset-cpus "$cs")
if [ -t 0 ] && [ -t 1 ]; then tty=(-it); fi
exec docker run --rm ${tty[@]+"${tty[@]}"} ${cpuset[@]+"${cpuset[@]}"} \
  --memory "$memory" --memory-swap "$memory" --cpus "$cpus" \
  --shm-size 512m \
  --name "${AKANTU_RS_CONTAINER_NAME:-akantu-rs-mpi-$$}" \
  -v "$here:/src:ro" \
  -v akantu-rs-mpi-target:/target \
  -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -e AKANTU_MPI_TEST_TIMEOUT_S="${AKANTU_MPI_TEST_TIMEOUT_S:-}" \
  -e AKANTU_COLLECTIVE_CHECKS="${AKANTU_COLLECTIVE_CHECKS:-}" \
  -e AKANTU_COLLECTIVE_HEADER="${AKANTU_COLLECTIVE_HEADER:-}" \
  -e CARGO_TARGET_DIR="${AKANTU_RS_MPI_TARGET:-/target}" \
  -e CARGO_INCREMENTAL="${AKANTU_RS_CARGO_INCREMENTAL:-0}" \
  -e CARGO_PROFILE_DEV_DEBUG="${AKANTU_RS_DEV_DEBUG:-line-tables-only}" \
  -e CARGO_PROFILE_TEST_DEBUG="${AKANTU_RS_DEV_DEBUG:-line-tables-only}" \
  -e AKANTU_RS_GIT_HEAD="${AKANTU_RS_GIT_HEAD:-}" \
  -e AKANTU_RS_MPI_IMAGE_ID="${AKANTU_RS_MPI_IMAGE_ID:-}" \
  -w /src "$image" "$@"
