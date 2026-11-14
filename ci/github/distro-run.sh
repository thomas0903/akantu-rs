#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-012).
# The second distribution (Ubuntu 22.04: glibc 2.35, Open MPI 4.1, Scotch 6) as
# a digest-pinned container, so that the job depends neither on the
# `ubuntu-22.04` runner image's lifecycle (announced retirement) nor on the
# host's own libraries, and so that the SAME command runs on any machine with
# Docker.
#
#   ci/github/distro-run.sh <out-dir>
#
# Mounts the repository read-only at /src and <out-dir> at /out, runs
# ci/github/distro-inner.sh as root in the container (apt packages, rustup,
# then `ci/run-all.sh --backend host --preset distro`) and returns its exit
# code (0 PASS, 1 FAIL, 3 INCOMPLETE as run-all.sh).
# DISTRO_IMAGE overrides the image (a developer's experiment; the workflow
# never sets it). The digest below is the amd64 manifest of ubuntu:22.04
# read from Docker Hub on 2026-11-12.
set -euo pipefail
out="${1:?usage: distro-run.sh <out-dir>}"
image="${DISTRO_IMAGE:-ubuntu:22.04@sha256:74b12b11ffa550fba1929ad4d8d16533c34f6612728469671afba1b03b66881c}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
exec docker run --rm \
  -v "$repo:/src:ro" -v "$out:/out" \
  -e CI_STEP_TIMEOUT="${CI_STEP_TIMEOUT:-3600}" \
  "$image" bash /src/ci/github/distro-inner.sh
