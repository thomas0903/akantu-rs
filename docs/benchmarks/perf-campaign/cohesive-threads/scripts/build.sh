#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion step); not ported from Akantu.
# build.sh <tree> [extra RUSTFLAGS]: serial release binaries of the driver and suites
source ~/.cargo/env
t=$1; shift
cd ~/w/cohthreads/$t
RUSTFLAGS="$*" CARGO_TARGET_DIR=${TGT:-~/w/cohthreads/target-$t} cargo build --release --locked -p akantu-bench --bin cohesive_extrinsic --bin suite_setup --bin suite_solid 2>&1 | tail -n 3
