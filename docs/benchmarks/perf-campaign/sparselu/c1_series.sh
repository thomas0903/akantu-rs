#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the cohesive static
# solve with Eigen's SparseLU on both sides (c1prep cases cppeig, rusteig),
# the port before (perf/wave2-base) and after (this branch) alternated per
# repetition, one rank pinned to core 2; run from the root of the checkout.
#   docs/benchmarks/perf-campaign/sparselu/c1_series.sh OUT H REPS
# RUST_BEFORE / RUST_AFTER: driver paths in the target volume.
set -euo pipefail
OUT=${1:?OUT}; H=${2:?H}; REPS=${3:-5}
B=${RUST_BEFORE:-/target/sparselu-before/release/cohesive_extrinsic}
A=${RUST_AFTER:-/target/sparselu-after/release/cohesive_extrinsic}
export OUT PIN_CPUS_FROM=2 C1PREP_LOCK=${C1PREP_LOCK:-$OUT/c1.lock}
for k in $(seq 1 "$REPS"); do
  if [ $((k % 2)) = 1 ]; then order="after before"; else order="before after"; fi
  for v in $order; do
    if [ "$v" = after ]; then bin=$A; cases="cppeig rusteig"; else bin=$B; cases="rusteig"; fi
    RUST_BIN=$bin bash crates/akantu-bench/scripts/c1prep_campaign.sh \
      "h$H-$v-r$k" "$H" 10 1 1 "$cases"
  done
done
