#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (D-101 (1)): build the MPI build of the Python
# module (crate akantu-python, feature `mpi` = MPI + Scotch) in the
# akantu-rs-scotch:dev image (Debian trixie: Open MPI 5.0.7, Scotch 7.0.7,
# CPython 3.13 headers: the same libraries as the oracle image, which runs
# it) and lay it out as a package directory.
#
#   python/oracle/mpi/build-module.sh <repo> <target-dir> <out-python-dir>
#
# <target-dir>: a host directory owned by the image's user (uid 1000) used as
# CARGO_TARGET_DIR. The extension is the abi3 cdylib built with
# PYO3_BUILD_EXTENSION_MODULE=1 (no libpython link, as maturin does), copied
# to <out-python-dir>/akantu/py11_akantu.abi3.so next to a copy of
# python/akantu/__init__.py.
#
# Container caps: AKANTU_RS_DOCKER_CPUS (default 4)
# and AKANTU_RS_DOCKER_MEMORY (default 7g), so that the build runs on a
# `default` sandbox (4 vCPU, 8 GB; Docker refuses more CPUs than the host
# has); on a `large` sandbox `AKANTU_RS_DOCKER_CPUS=6
# AKANTU_RS_DOCKER_MEMORY=12g` gives the earlier caps.
set -eu
REPO=$1 TARGET=$2 OUT=$3
IMG=${AKANTU_RS_SCOTCH_IMAGE:-akantu-rs-scotch:dev}
CPUS=${AKANTU_RS_DOCKER_CPUS:-4}
MEM=${AKANTU_RS_DOCKER_MEMORY:-7g}
mkdir -p "$TARGET" "$OUT"
docker run --rm --memory "$MEM" --cpus "$CPUS" \
  -v "$REPO:/src:ro" -v "$TARGET:/target" -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/target -e CARGO_INCREMENTAL=0 -e PYO3_BUILD_EXTENSION_MODULE=1 \
  -w /src "$IMG" cargo build --release -p akantu-python --features mpi --locked
rm -rf "$OUT/akantu"
cp -a "$REPO/python/akantu" "$OUT/akantu"
cp "$TARGET/release/libpy11_akantu.so" "$OUT/akantu/py11_akantu.abi3.so"
ldd "$OUT/akantu/py11_akantu.abi3.so" | grep -E "mpi|scotch|python" || true
sha256sum "$OUT/akantu/py11_akantu.abi3.so"
