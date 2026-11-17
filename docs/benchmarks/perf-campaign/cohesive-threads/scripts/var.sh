#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# var.sh <name>...: build the driver from ~/w/cohthreads/asm with linear_<name>.rs as the linear law, copy to bin/coh-v<name>
source ~/.cargo/env
I=~/w/cohthreads; cd $I/asm
for v in "$@"; do
  cp $I/linear_$v.rs crates/akantu-cohesive/src/laws/linear.rs
  CARGO_TARGET_DIR=$I/target-asm cargo build -q --release --locked -p akantu-bench --bin cohesive_extrinsic 2>&1 | grep -E "^error" | head -3
  cp $I/target-asm/release/cohesive_extrinsic $I/bin/coh-v$v
done
git checkout -q -- crates/akantu-cohesive/src/laws/linear.rs
