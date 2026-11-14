# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI test image: Linux aarch64 or x86_64, stable Rust pinned by
# rust-toolchain.toml, OpenMPI from Debian trixie (the same distribution and
# OpenMPI version as the C++ oracle image akantu-oracle:dev, 5.0.7-1),
# libclang for rsmpi's bindgen step, mpi4py for the mpi4py-first lifecycle
# scenario, and git for the tests that call it (the fixture-script
# tests of akantu-validation run `git rev-parse`).
#
# Build with scripts/mpi/build-image.sh (tag: akantu-rs-mpi:dev); run tests
# with scripts/mpi/run-tests.sh. Documentation: docs/TESTING.md.
#
# Reproducibility scope (as for the oracle image, D-034): the base image is
# pinned by digest, the apt packages are not version-pinned; the resolved
# package set is recorded in /usr/share/akantu-rs-mpi-packages.tsv inside the
# image and copied to docker/mpi/packages.tsv by build-image.sh.

# rust:1.99.1-slim-trixie pinned by the digest of its multi-architecture
# OCI image index (one value for every architecture: Docker selects the
# manifest of the build platform). The Rust version must match
# rust-toolchain.toml. Platform manifests in this index (recorded
# 2026-11-11 with `docker buildx imagetools inspect`):
#   linux/arm64/v8  sha256:10d470c049d88eb5dbfc85612f51571edd1ebbbdade35c5d3d89d9ab8fd6bda3
#   linux/amd64     sha256:e97df055c54a05160e7c67ba3b2963de6ddb95903911baf8922dbcd1b394f698
# (the index also lists arm/v7, 386, ppc64le, riscv64, s390x). The aarch64
# image (docker/mpi/image-id.txt) and the amd64 image of the x86_64 hosts
# were built from this same index digest. To pin a single
# platform, pass --build-arg BASE_IMAGE=rust:1.99.1-slim-trixie@<manifest>.
ARG BASE_IMAGE=rust:1.99.1-slim-trixie@sha256:f6c4a35d21dbb3c89d7a8d056c193e94fc09037f2b58dd858b09c39a47dd02d7
FROM ${BASE_IMAGE}

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      ca-certificates \
      openmpi-bin libopenmpi-dev \
      libclang-dev clang \
      pkg-config \
      python3 python3-dev python3-mpi4py \
      procps jq \
      git \
    && rm -rf /var/lib/apt/lists/*

# The repository is bind-mounted read-only at /src (scripts/mpi/in-docker.sh);
# on hosts where its owner is not the container user, git would refuse it as
# a "dubious ownership" repository, so trust /src explicitly.
RUN git config --system --add safe.directory /src

# Components requested by rust-toolchain.toml, installed at build time so the
# container never needs the network for the toolchain.
RUN rustup component add rustfmt clippy && rustc --version && cargo --version

# Non-root user (OpenMPI refuses to run as root by default). Cargo home and the
# target directory are owned by it so named volumes mounted there inherit the
# ownership on first use.
ARG RS_UID=1000
ARG RS_GID=1000
RUN groupadd -g ${RS_GID} akantu \
    && useradd -m -u ${RS_UID} -g ${RS_GID} -s /bin/bash akantu \
    && mkdir -p /target /src /usr/local/cargo/registry \
    && chown -R akantu:akantu /usr/local/cargo /usr/local/rustup /target

# OpenMPI 5 settings for containers (same intent as the oracle image): TCP +
# self + shared memory, no CMA single-copy (needs ptrace), oversubscription
# allowed (4 ranks on a small container), no binding.
ENV OMPI_MCA_btl=tcp,self,sm \
    OMPI_MCA_btl_sm_single_copy_mechanism=none \
    OMPI_MCA_btl_base_warn_component_unused=0 \
    OMPI_MCA_rmaps_base_oversubscribe=1 \
    PRTE_MCA_rmaps_default_mapping_policy=:oversubscribe \
    OMPI_MCA_hwloc_base_binding_policy=none \
    PRTE_MCA_hwloc_default_binding_policy=none \
    CARGO_TARGET_DIR=/target \
    CARGO_BUILD_JOBS=4 \
    CARGO_TERM_COLOR=never \
    OPENBLAS_NUM_THREADS=1 \
    OMP_NUM_THREADS=1

USER root
RUN dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' \
      > /usr/share/akantu-rs-mpi-packages.tsv
USER akantu
WORKDIR /src
CMD ["/bin/bash"]
