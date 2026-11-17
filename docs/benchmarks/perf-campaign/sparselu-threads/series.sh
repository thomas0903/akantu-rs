#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the base at one thread
# against the branch at 1, 2, 4 and 8 threads (AKANTU_NUM_THREADS) on the
# suite cases of the sparse direct solver; T threads pinned to the cores
# 0..T-1, the variants of a case back to back, their order rotated per
# repetition. BIN holds base_{solid,models} and head_{solid,models}.
#   BIN=DIR docs/benchmarks/perf-campaign/sparselu-threads/series.sh OUT REPS
# OUT/<variant>/<case>.r<k>.jsonl, read by scripts/perf/suite/summarize.py.
set -euo pipefail
OUT=${1:?OUT}
REPS=${2:-5}
BIN=${BIN:?BIN: the directory of the suite binaries}
CASES="solid:static_linear_eigen models:heat_implicit_hex8_kvar models:structural_beam3_frame_static models:structural_dkt18_plate_static models:phasefield_staggered_tri3 models:phasefield_staggered_tet4"
VARIANTS="base-t1 head-t1 head-t2 head-t4 head-t8"
for ((k = 0; k < REPS; k++)); do
  for c in ${CASES}; do
    suite=${c%%:*}
    case=${c#*:}
    read -r -a vs <<< "${VARIANTS}"
    n=${#vs[@]}
    for ((i = 0; i < n; i++)); do
      v=${vs[$(((i + k) % n))]}
      bin=${v%-t*}
      t=${v##*-t}
      mkdir -p "${OUT}/${v}"
      AKANTU_NUM_THREADS=${t} taskset -c "0-$((t - 1))" "${BIN}/${bin}_${suite}" \
        --case "${case}" --size m --reps 1 --threads "${t}" \
        --out "${OUT}/${v}/${case}.r${k}.jsonl" > /dev/null 2>&1
    done
  done
done
