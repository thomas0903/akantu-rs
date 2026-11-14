# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle image: build environment for the Akantu C++ reference.
# Derived from upstream Akantu `test/ci/debian:stable/Dockerfile`
# (commit 35a851780d0b9471d258ed53a003ae98aaa95ea0), extended with CGAL,
# pybind11, googletest, ScaLAPACK, matplotlib/pandas and measurement tools.
#
# Reproducibility scope (D-034): the base image is pinned by
# digest and CGAL by SHA-256, but the apt packages below come from the live
# Debian trixie repositories without version pins. The oracle results are tied
# to the recorded image (oracle/docker/image-id.txt, package manifest
# oracle/docker/packages.tsv); a clean rebuild may resolve other versions and is
# a potentially different numerical environment. build-image.sh builds a
# candidate and compares its manifest with packages.tsv before anything is
# promoted or recorded.
#
# Build with oracle/scripts/build-image.sh (tag: akantu-oracle:dev).
# Documentation: docs/ORACLE_BUILD.md.

# Pinned base: debian:trixie (13.7) as pulled on 2026-11-09, linux/arm64.
ARG BASE_IMAGE=debian:trixie@sha256:9c6e381421d0f48d1b8133e573f33c185ee16cbf505eda9c4a8a4791b25ea8a6
# Set by BuildKit from --platform (arm64 | amd64); selects the final stage.
ARG TARGETARCH
FROM ${BASE_IMAGE} AS oracle-common

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# --- Toolchain and library dependencies (upstream CI list + extras) ---------
# Upstream list: g++ gfortran clang cmake openmpi-bin libmumps-dev
# libscotch-dev petsc-dev libboost-dev libopenblas-dev libeigen3-dev
# libhdf5-dev libhdf5-openmpi-dev python3 python3-dev python3-numpy
# python3-scipy python3-mpi4py; CI extras: gmsh python3-pytest ninja-build ...
# Additions for akantu-rs: CGAL (see below), libboost-graph-dev
# (debug_tools), pybind11-dev (python_interface, avoids network fetch),
# libgtest-dev/googletest (Debian sources instead of upstream's FetchContent
# of googletest "main"; version as resolved at image build, 1.16.0-1 in the
# recorded image),
# libscalapack-openmpi-dev, libptscotch-dev, matplotlib/pandas/termcolor
# (imported by python examples), GNU time (peak-RSS measurement).
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      ca-certificates \
      g++ gcc gfortran clang cmake ninja-build make pkg-config \
      git patch rsync xsltproc jq file binutils xz-utils curl \
      openmpi-bin libopenmpi-dev \
      libmumps-dev libscotch-dev libptscotch-dev petsc-dev \
      libscalapack-openmpi-dev libmetis-dev \
      libboost-dev libboost-graph-dev \
      libopenblas-dev \
      libeigen3-dev \
      libgmp-dev libmpfr-dev \
      libhdf5-dev libhdf5-openmpi-dev \
      python3 python3-dev python3-numpy python3-scipy python3-mpi4py \
      python3-pytest python3-matplotlib python3-pandas python3-termcolor \
      python3-click python3-tqdm \
      pybind11-dev python3-pybind11 \
      libgtest-dev googletest \
      gmsh \
      ccache \
      time procps less gdb \
    && rm -rf /var/lib/apt/lists/*

# --- CGAL 5.6.2 (header-only) ----------------------------------------------
# Akantu's geometry code (src/geometry, package CGAL; used by `embedded`) is
# written against the CGAL <= 5.x AABB-tree API (boost::optional results);
# Debian trixie ships CGAL 6.0.1, which switched to std::optional and breaks
# it. Use the pinned upstream 5.6.2 release instead (CGAL is header-only
# since 5.0; it needs GMP/MPFR/Boost from Debian). See docs/DECISIONS.md D-032.
# The tarball is fetched (with retries) and checksum-verified by
# oracle/scripts/build-image.sh into the git-ignored build-context cache;
# it is verified again here.
# Source: https://github.com/CGAL/cgal/releases/download/v5.6.2/CGAL-5.6.2-library.tar.xz
COPY cache/CGAL-5.6.2-library.tar.xz /tmp/
RUN echo "2d93d96c8c73c73c9999556d512775648d26e1e04035ceb958208d25e6e2d10f  /tmp/CGAL-5.6.2-library.tar.xz" \
      | sha256sum -c - \
    && tar -xJf /tmp/CGAL-5.6.2-library.tar.xz -C /opt \
    && rm /tmp/CGAL-5.6.2-library.tar.xz
ENV CGAL_DIR=/opt/CGAL-5.6.2/lib/cmake/CGAL

# --- Non-root user (OpenMPI refuses to run as root by default) ------------
ARG ORACLE_UID=1000
ARG ORACLE_GID=1000
RUN groupadd -g ${ORACLE_GID} akantu \
    && useradd -m -u ${ORACLE_UID} -g ${ORACLE_GID} -s /bin/bash akantu \
    && mkdir -p /build /akantu-src /oracle \
    && chown akantu:akantu /build /oracle

# OpenMPI 5 settings for containers: TCP + self + shared memory, no CMA
# single-copy (needs ptrace), allow oversubscription. Equivalent intent to
# upstream test/ci/debian:stable/.openmpi/mca-params.conf.
ENV OMPI_MCA_btl=tcp,self,sm \
    OMPI_MCA_btl_sm_single_copy_mechanism=none \
    OMPI_MCA_btl_base_warn_component_unused=0 \
    OMPI_MCA_rmaps_base_oversubscribe=1 \
    PRTE_MCA_rmaps_default_mapping_policy=:oversubscribe \
    OMPI_MCA_hwloc_base_binding_policy=none \
    PRTE_MCA_hwloc_default_binding_policy=none \
    CCACHE_DIR=/build/ccache \
    CCACHE_MAXSIZE=20G

USER akantu
# The reference checkout is mounted read-only and owned by another uid.
RUN git config --global --add safe.directory /akantu-src \
    && git config --global --add safe.directory '*'

# Record the exact package set inside the image for provenance.
USER root
RUN dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' \
      > /usr/share/akantu-oracle-packages.tsv
USER akantu

# Single-threaded BLAS/OpenMP: bitwise-reproducible dense kernels and no
# thread oversubscription when ctest runs several MPI jobs concurrently.
ENV OPENBLAS_NUM_THREADS=1 \
    OMP_NUM_THREADS=1

WORKDIR /build
CMD ["/bin/bash"]

# --- Per-architecture final stage (D-050) ---------------------------
# arm64: unchanged (identical layers to the recorded image). amd64: pin
# OpenBLAS's DYNAMIC_ARCH kernel set: OpenBLAS otherwise picks its kernels from
# the host CPU at run time (Cooperlake on the x86_64 measurement hosts) and MUMPS/PETSc
# results change with that choice (D-050 item 5). Haswell = AVX2 +
# FMA kernels, available on every x86-64-v3 host. Every oracle script also
# passes the pin explicitly (oracle/scripts/oracle-env.sh).
FROM oracle-common AS oracle-arm64
FROM oracle-common AS oracle-amd64
ENV OPENBLAS_CORETYPE=Haswell \
    OPENBLAS_NUM_THREADS=1
FROM oracle-${TARGETARCH}
