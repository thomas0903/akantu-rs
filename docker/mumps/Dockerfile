# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MUMPS test image: the MPI test image
# (docker/mpi/Dockerfile, akantu-rs-mpi:dev) plus Debian trixie's parallel
# MUMPS and its closure — the same distribution and package versions as the
# C++ oracle image akantu-oracle:dev (libmumps-dev 5.7.3-3+b1, ScaLAPACK
# 2.2.2-1, Scotch/PT-Scotch 7.0.7-1, METIS 5.1.0, OpenBLAS 0.3.29+ds-3 as the
# libblas.so.3 / liblapack.so.3 alternative) — for the `mumps` feature of
# akantu-linalg. Bitwise agreement with the oracle's MUMPS solves needs the
# same shared objects: the resolved package set is recorded in
# /usr/share/akantu-rs-mumps-packages.tsv (copied to docker/mumps/packages.tsv)
# and the BLAS/LAPACK alternatives in /usr/share/akantu-rs-mumps-alternatives.txt
# (multiarch triplet from `uname -m`: aarch64 and x86_64 hosts, D-043).
#
# BLAS pin (D-050; scripts/mumps/blas-pin.sh): single-threaded
# OpenBLAS everywhere; on x86_64 (linux/amd64) the OpenBLAS core type is
# pinned to Haswell, because OpenBLAS otherwise picks its kernels from the
# run-time CPU and MUMPS's results change with them. The aarch64 image keeps
# OpenBLAS's run-time selection (the configuration of the committed aarch64
# fixture variant). Needs BuildKit (default since Docker 23) for TARGETARCH.
#
#   docker build -t akantu-rs-mumps:dev docker/mumps
#   AKANTU_RS_MPI_IMAGE=akantu-rs-mumps:dev scripts/mumps/run-mumps-tests.sh
ARG BASE_IMAGE=akantu-rs-mpi:dev
ARG TARGETARCH
FROM ${BASE_IMAGE} AS mumps-common
USER root
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      libmumps-dev libmumps-headers-dev \
      libscalapack-openmpi-dev libptscotch-dev libscotch-dev libmetis-dev \
      libopenblas-dev \
      time \
    && rm -rf /var/lib/apt/lists/* \
    && dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' \
         > /usr/share/akantu-rs-mumps-packages.tsv \
    && { arch="$(uname -m)-linux-gnu"; \
         for a in "libblas.so.3-$arch" "liblapack.so.3-$arch"; do \
           update-alternatives --query "$a" | grep -E '^(Name|Value):'; done; } \
         > /usr/share/akantu-rs-mumps-alternatives.txt
# Single-threaded Scotch inside MUMPS orderings (ORACLE_BUILD: threaded
# Scotch breaks run-to-run repeatability) and single-threaded BLAS.
ENV SCOTCH_PTHREAD_NUMBER=1 \
    OPENBLAS_NUM_THREADS=1 \
    OMP_NUM_THREADS=1

# x86_64: OpenBLAS core type pinned (D-050).
FROM mumps-common AS mumps-amd64
ENV OPENBLAS_CORETYPE=Haswell

# aarch64 platform variant: OpenBLAS's run-time core selection.
FROM mumps-common AS mumps-arm64

FROM mumps-${TARGETARCH}
USER akantu
