#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the explicit-step
# harness digests (FNV-1a of u, v, a, f_int after n steps) of every binary
# LABEL at every thread count, against the recorded digests (port = C++
# strict) of docs/benchmarks/perf-campaign/baseline/explicit-digests.txt.
#   digest_check.sh "LABEL..." "THREADS..." [RECORDED]
# BIN_DIR (binaries named LABEL), MESH_DIR (the explicit-step meshes);
# AKANTU_PRECISION / AKANTU_SIMD are passed through. Exit 1 on a mismatch.
set -euo pipefail
read -r -a labels <<<"$1"; read -r -a threads <<<"$2"
rec="${3:-$(dirname "$0")/../baseline/explicit-digests.txt}"
bindir="${BIN_DIR:?}"; meshdir="${MESH_DIR:?}"
status=0
while read -r mesh mode n port _; do
  case "$mesh" in tet*|hex*) ;; *) continue ;; esac
  n="${n#n=}"
  extra=(); [ "$mode" = fd ] && extra=(--fd)
  for l in "${labels[@]}"; do
    for t in "${threads[@]}"; do
      d=$(AKANTU_NUM_THREADS="$t" "$bindir/$l" --mesh "$meshdir/$mesh.msh" --digest-steps "$n" "${extra[@]}" \
        | grep '"digest"' | sed 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/')
      ok=ok; [ "$d" = "$port" ] || { ok=MISMATCH; status=1; }
      echo "$mesh $mode n=$n $l t=$t $d $ok"
    done
  done
done < "$rec"
exit "$status"
