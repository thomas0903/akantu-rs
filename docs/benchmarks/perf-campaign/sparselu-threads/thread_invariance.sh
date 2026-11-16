#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the digest of every
# solver case of the suites, the base at one thread against the branch at
# 1, 2, 3, 4 and 8 threads (AKANTU_NUM_THREADS) with the automatic and the
# finest grain (AKANTU_EXEC_GRAIN), size m. BIN holds base_{solid,models}
# and head_{solid,models}. One line per case: "ok" per configuration, or
# the configuration and its digest.
#   BIN=DIR docs/benchmarks/perf-campaign/sparselu-threads/thread_invariance.sh
set -euo pipefail
BIN=${BIN:?BIN: the directory of the suite binaries}
digest() { python3 -I -c 'import sys, json; print(json.loads(sys.stdin.read().splitlines()[-1])["digest"])'; }
for c in solid:static_linear_eigen solid:implicit_dynamic solid:cohesive_intrinsic_implicit \
  solid:static_newton_plastic models:heat_implicit_hex8_kvar models:heat_static_tri6 \
  models:structural_beam3_frame_static models:structural_dkt18_plate_static \
  models:phasefield_staggered_tri3 models:phasefield_staggered_tet4; do
  suite=${c%%:*}
  case=${c#*:}
  ref=$("${BIN}/base_${suite}" --case "${case}" --size m --reps 1 2> /dev/null | digest)
  line="${case} ref=${ref}"
  for t in 1 2 3 4 8; do
    for g in auto finest; do
      d=$(AKANTU_NUM_THREADS=${t} AKANTU_EXEC_GRAIN=${g} "${BIN}/head_${suite}" \
        --case "${case}" --size m --reps 1 2> /dev/null | digest)
      if [ "${d}" = "${ref}" ]; then line="${line} ok"; else line="${line} DIFF:${t}/${g}:${d}"; fi
    done
  done
  echo "${line}"
done
