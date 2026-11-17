#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the cohesive static
# solve with Eigen's SparseLU, C++ (the oracle's strict build, COLAMD, one
# thread) against the port with COLAMD and with nested dissection at 1, 2,
# 4 and 8 threads (c1prep cases cppeig and rusteig, one rank, 10 steps after
# the static solve); the runs of a repetition in a rotated order. Run from
# the root of the checkout.
#   docs/benchmarks/perf-campaign/sparselu-nd/c1_nd.sh OUT H REPS
# RUST_HEAD: the driver's path in the target volume.
set -euo pipefail
OUT=${1:?OUT}
H=${2:?H}
REPS=${3:-3}
A=${RUST_HEAD:?RUST_HEAD}
export OUT PIN_CPUS_FROM=0 C1PREP_LOCK=${C1PREP_LOCK:-${OUT}/c1.lock}
runs=(cpp colamd-t1 colamd-t2 colamd-t4 colamd-t8 nd-t1 nd-t2 nd-t4 nd-t8)
for k in $(seq 1 "${REPS}"); do
  n=${#runs[@]}
  for ((i = 0; i < n; i++)); do
    r=${runs[$(((i + k) % n))]}
    extra=""
    case ${r} in
      cpp) cases=cppeig; t=1 ;;
      colamd-t*) cases=rusteig; t=${r#colamd-t} ;;
      nd-t*) cases=rusteig; t=${r#nd-t}; extra="rust_sparse_ordering=nested_dissection" ;;
    esac
    RUST_THREADS=${t} bash crates/akantu-bench/scripts/c1prep_campaign.sh \
      --rust-bin "${A}" --rust-extra "${extra}" "h${H}-${r}-r${k}" "${H}" 10 1 1 "${cases}"
  done
done
