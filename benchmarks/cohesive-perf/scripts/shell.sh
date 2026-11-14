#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: interactive shell (or one command)
# in the oracle image with /build (ro), /perf (scratch volume) and /harness
# (this directory, ro) mounted. Memory limit: PERF_MEMORY (default 8g).
# Usage: scripts/shell.sh [cmd args...]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
if [ $# -eq 0 ]; then
  PERF_RUN_OPTS+=(-it)
  set -- bash
fi
perf_docker "$@"
