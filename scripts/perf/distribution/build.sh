#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# build.sh <tree-dir> <label>: release suite_setup (mpi,scotch), suite_models, suite_solid and cohesive_extrinsic
# built in the Scotch image (scripts/mpi/in-docker.sh), copied to ~/w/dist/bin/<label>/
set -eu
tree=$1 label=$2
mkdir -p ~/w/dist/bin/"$label"
cd "$tree"
H=$(git rev-parse --short HEAD 2>/dev/null || echo wt)
export AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev AKANTU_RS_DOCKER_CPUS=8 AKANTU_RS_DOCKER_MEMORY=12g AKANTU_RS_MPI_TARGET=/target/dist-$label AKANTU_RS_CONTAINER_NAME=dist-build-$label
scripts/mpi/in-docker.sh bash -c "cargo build --release --locked -p akantu-bench --bin suite_setup --bin suite_models --bin suite_solid --bin cohesive_extrinsic --features mpi,scotch 2>&1 | tail -3 && mkdir -p /target/out-$label && cp /target/dist-$label/release/suite_setup /target/dist-$label/release/suite_models /target/dist-$label/release/suite_solid /target/dist-$label/release/cohesive_extrinsic /target/out-$label/"
docker run --rm -v akantu-rs-mpi-target:/target -v ~/w/dist/bin/"$label":/o akantu-rs-scotch:dev bash -c "cp /target/out-$label/* /o/ && chown $(id -u):$(id -g) /o/*"
echo "BUILT $label $H"
