#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# build.sh <commit> <outname> : build suite_setup at <commit> in ~/w/io/akantu-rs, copy to ~/w/io/bin/<outname>
set -euo pipefail
. ~/.cargo/env
cd ~/w/io/akantu-rs
git fetch -q origin
git checkout -q --detach "$1"
cargo build --locked --release -p akantu-bench --bin suite_setup
cp target/release/suite_setup ~/w/io/bin/"$2"
echo "built $2 at $(git rev-parse --short HEAD)"
