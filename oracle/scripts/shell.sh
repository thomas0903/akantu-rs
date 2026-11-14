#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Interactive shell (or one command) in the oracle image with the standard
# mounts: /akantu-src (ro), /build (volume akantu-oracle-build), /oracle (ro).
# Usage: oracle/scripts/shell.sh                 # interactive bash
#        oracle/scripts/shell.sh <cmd> [args...] # run a command
# Env:   ORACLE_EXTRA_MOUNTS="-v /host/dir:/work"  extra docker args
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
tty=(-i)
[ -t 0 ] && [ -t 1 ] && tty=(-it)
# shellcheck disable=SC2086
exec docker run --rm "${tty[@]}" "${ORACLE_RUN_OPTS[@]}" "${ORACLE_MOUNTS[@]}" \
  ${ORACLE_EXTRA_MOUNTS:-} -w /build "${ORACLE_IMAGE}" "${@:-bash}"
