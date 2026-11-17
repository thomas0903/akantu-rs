#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# build-mpi.sh <tree>: MPI + MUMPS + Scotch driver in the project image, target /target/ct-<tree>
cd ~/w/cohthreads/$1 && AKANTU_RS_MPI_IMAGE=akantu-rs-mumps:dev AKANTU_RS_MPI_TARGET=/target/ct-$1 \
  scripts/mpi/in-docker.sh cargo build --locked --release -p akantu-bench --bin cohesive_extrinsic --features mpi,scotch,mumps 2>&1 | tail -n 2
