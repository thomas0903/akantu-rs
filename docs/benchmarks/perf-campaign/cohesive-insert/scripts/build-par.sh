#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# build-par.sh <tree> [prefix]: release driver with MPI, Scotch and MUMPS in the project image
# (target in the docker volume akantu-rs-mpi-target under /target/<prefix>-<tree>)
set -e
p=${2:-insert}
docker run --rm -v ~/w/insert/$1:/src:ro -v akantu-rs-mpi-target:/target -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/target/$p-$1 -e CARGO_INCREMENTAL=0 -e RUSTFLAGS="${RUSTFLAGS:-}" -w /src akantu-rs-mumps:dev \
  cargo build --release --locked -p akantu-bench --features mumps,scotch --bin cohesive_extrinsic 2>&1 | tail -n 3
