# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs benchmark image: the MUMPS gate image
# (docker/mumps/Dockerfile, akantu-rs-mumps:dev: Open MPI, MUMPS 5.7.3,
# OpenBLAS pinned to Haswell, Scotch; OMP / OpenBLAS / Scotch threads = 1)
# plus the release build of the cohesive scalability driver
# `cohesive_extrinsic` (cargo build --release -p akantu-bench --features
# mpi,scotch,mumps) and the upstream benchmark inputs, so that a plain
# x86_64 Linux host with Docker runs the Rust side of the benchmark from one
# tarball. Built and exported by crates/akantu-bench/scripts/c1_images.sh
# (docs/TESTING.md); the build context holds
# the binary and the inputs only.
ARG BASE_IMAGE=akantu-rs-mumps:dev
FROM ${BASE_IMAGE}
USER root
COPY cohesive_extrinsic /usr/local/bin/cohesive_extrinsic
COPY binary.sha256 provenance.txt /usr/share/akantu-rs-bench/
COPY material-cohesive.dat material-elastic.dat /usr/share/akantu-rs-bench/inputs/
RUN chmod 0755 /usr/local/bin/cohesive_extrinsic && cohesive_extrinsic --help > /dev/null
USER akantu
