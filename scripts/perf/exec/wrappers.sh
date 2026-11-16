#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): thread-count labels for
# the explicit-step series of D-123 (timing_rotated.sh and digests.sh run
# $BIN_DIR/<label>). For a harness binary BIN_DIR/<name>, writes
# BIN_DIR/<name>-t<N> for every N: AKANTU_NUM_THREADS=N, pinned to cores
# 0..N-1 (one thread per physical core).
#   wrappers.sh BIN_DIR NAME "N..."
set -euo pipefail
dir="$1" name="$2"; read -r -a counts <<< "$3"
for n in "${counts[@]}"; do
  w="$dir/$name-t$n"
  printf '#!/bin/sh\nAKANTU_NUM_THREADS=%s exec taskset -c 0-%s "%s/%s" "$@"\n' "$n" "$((n - 1))" "$dir" "$name" > "$w"
  chmod +x "$w"
  echo "$w"
done
