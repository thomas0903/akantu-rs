#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the digest of every
# solver case of the suites, the base at one thread against the branch at
# the thread counts THREADS (default "1 3 8") with the automatic and the
# finest grain, size m. The thread count and the grain are set with the
# suites' --threads / --grain; every JSON line's recorded "threads" is
# checked. BIN holds base_{solid,models} and head_{solid,models}. One line
# per case: "ok" per configuration, or the configuration and its digest.
#   BIN=DIR docs/benchmarks/perf-campaign/sparselu-threads/thread_invariance.sh
set -euo pipefail
BIN=${BIN:?BIN: the directory of the suite binaries}
THREADS=${THREADS:-1 3 8}
# digest of the last line; exit 1 when a line records another thread count
digest() {
  python3 -I -c '
import sys, json
want = int(sys.argv[1])
lines = [json.loads(l) for l in sys.stdin if l.startswith("{")]
assert lines and all(j["threads"] == want for j in lines), "recorded thread count"
print(lines[-1]["digest"])' "$1"
}
for c in solid:static_linear_eigen solid:implicit_dynamic solid:cohesive_intrinsic_implicit \
  solid:static_newton_plastic models:heat_implicit_hex8_kvar models:heat_static_tri6 \
  models:structural_beam3_frame_static models:structural_dkt18_plate_static \
  models:phasefield_staggered_tri3 models:phasefield_staggered_tet4; do
  suite=${c%%:*}
  case=${c#*:}
  ref=$("${BIN}/base_${suite}" --case "${case}" --size m --reps 1 --threads 1 2> /dev/null | digest 1)
  line="${case} ref=${ref}"
  for t in ${THREADS}; do
    for g in auto finest; do
      d=$("${BIN}/head_${suite}" --case "${case}" --size m --reps 1 --threads "${t}" --grain "${g}" \
        2> /dev/null | digest "${t}")
      if [ "${d}" = "${ref}" ]; then line="${line} ok"; else line="${line} DIFF:${t}/${g}:${d}"; fi
    done
  done
  echo "${line}"
done
