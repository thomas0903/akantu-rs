#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# scaling screening on the development machine: sizes s, m (5 reps), l (3 reps)
export PATH=$HOME/.cargo/bin:$PATH
cd ~/w/suite-setup/akantu-rs
CARGO_TARGET_DIR=$HOME/w/suite-setup/target cargo build --release -p akantu-bench --bin suite_setup --locked || exit 1
B=$HOME/w/suite-setup/target/release/suite_setup
export AKANTU_SUITE_COMMIT=$(git rev-parse --short=9 HEAD)
cd ~/w/suite-setup
C=(msh22_read_tet4 msh41_read_tet4 msh22_read_hex8 msh_write_tet4 facets_tet4 facets_hex8 groups_tet4 erase_purify_tet4 periodic_tet4 cohesive_insert_tet4 partition_rcb_tet4 parser_dat dump_paraview_tet4)
n=${#C[@]}
for spec in s:5 m:5 l:3; do
  size=${spec%%:*}; reps=${spec##*:}; out=scale/$size/base; mkdir -p $out
  for ((r = 0; r < reps; r++)); do
    for ((k = 0; k < n; k++)); do
      c=${C[$(( (k + r) % n ))]}
      rm -f $out/$c.r$r.jsonl
      taskset -c 1 $B --case $c --size $size --reps 1 --out $out/$c.r$r.jsonl 2>> scale/errors.log || echo "FAIL $size $c r$r" >> scale/errors.log
    done
    echo "$size rep $r $(date -u +%T)"
  done
done
echo scale-done
