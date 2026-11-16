#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the cohesive static
# solve with Eigen's SparseLU on both sides (c1prep cases cppeig and
# rusteig, one rank): C++ (no threads in its solver), the port's base at
# one thread and the branch at 1, 2, 4 and 8 threads (RUST_THREADS), the
# runs of a repetition in a rotated order; run from the root of the checkout.
#   docs/benchmarks/perf-campaign/sparselu-threads/c1_threads.sh OUT H REPS
# RUST_BASE / RUST_HEAD: driver paths in the target volume.
set -euo pipefail
OUT=${1:?OUT}
H=${2:?H}
REPS=${3:-3}
B=${RUST_BASE:?RUST_BASE}
A=${RUST_HEAD:?RUST_HEAD}
export OUT PIN_CPUS_FROM=0 C1PREP_LOCK=${C1PREP_LOCK:-${OUT}/c1.lock}
runs=(cpp base-t1 head-t1 head-t2 head-t4 head-t8)
for k in $(seq 1 "${REPS}"); do
  n=${#runs[@]}
  for ((i = 0; i < n; i++)); do
    r=${runs[$(((i + k) % n))]}
    case ${r} in
      cpp) bin=${A}; cases=cppeig; t=1 ;;
      base-t1) bin=${B}; cases=rusteig; t=1 ;;
      head-t*) bin=${A}; cases=rusteig; t=${r#head-t} ;;
    esac
    RUST_BIN=${bin} RUST_THREADS=${t} bash crates/akantu-bench/scripts/c1prep_campaign.sh \
      "h${H}-${r}-r${k}" "${H}" 10 1 1 "${cases}"
  done
done
