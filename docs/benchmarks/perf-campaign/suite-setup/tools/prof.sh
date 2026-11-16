#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# prof.sh [size] [cases...]: release build, then perf profiles of serial cases
set -u
cd /home/user/akantu-rs
export CARGO_TARGET_DIR=/home/user/target
cargo build --release -p akantu-bench --bin suite_setup --locked || exit 1
B=$CARGO_TARGET_DIR/release/suite_setup
size="${1:-m}"; shift || true
cases="${*:-msh22_read_tet4 msh41_read_tet4 msh22_read_hex8 msh_write_tet4 facets_tet4 facets_hex8 groups_tet4 erase_purify_tet4 periodic_tet4 cohesive_insert_tet4 partition_rcb_tet4 parser_dat dump_paraview_tet4 dump_paraview_ascii_tet4 dump_text_tet4 py_twin_explicit_tri3}"
P=/home/user/prof; mkdir -p $P
for c in $cases; do
  echo "=== $c $(date -u +%T)"
  sudo perf record -q -F 499 --call-graph dwarf,32768 -o $P/$c-$size.data -- \
    "$B" --case "$c" --size "$size" --loop-seconds 10 --out $P/$c-$size.jsonl || { echo "FAIL $c"; continue; }
  /home/user/report.sh $P/$c-$size
  sudo chown user $P/$c-$size.data
done
echo done
