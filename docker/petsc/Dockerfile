# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs PETSc test image: the MPI test image
# (docker/mpi/Dockerfile, akantu-rs-mpi:dev, Debian trixie + OpenMPI 5.0.7)
# plus Debian trixie's PETSc — the same distribution, package and configure
# options as the C++ oracle image akantu-oracle:dev (`petsc-dev`, i.e.
# libpetsc-real3.22-dev 3.22.5+dfsg1-2: real scalars, 32-bit PetscInt,
# Debian's configure options recorded in
# /usr/lib/petscdir/petsc3.22/<triplet>-real/lib/petsc/conf/petscvariables
# and copied to docker/petsc/petsc-configure.txt) — for the `petsc` features
# of akantu-linalg / akantu-solver. KSP iteration counts and residual
# histories depend on the shared objects (PETSc, OpenBLAS, MUMPS when a
# `-pc_factor_mat_solver_type mumps` option is given): the resolved package
# set is recorded in /usr/share/akantu-rs-petsc-packages.tsv (copied to
# docker/petsc/packages.tsv by scripts/petsc/build-image.sh).
#
# BLAS pin (D-050; single definition
# scripts/mumps/blas-pin.sh, shared with the MUMPS image): single-threaded
# OpenBLAS everywhere; on x86_64 (linux/amd64) the OpenBLAS core type is
# pinned to Haswell, because OpenBLAS (DYNAMIC_ARCH) otherwise selects its
# kernels from the run-time CPU and the default-KSP results (ILU/GMRES
# through BLAS) change with them. The scripts also pass the pin explicitly
# into every container they start, and the PETSc tier refuses the bitwise
# oracle comparison unless the running OpenBLAS core is the recorded one.
# Needs BuildKit (default since Docker 23) for TARGETARCH.
#
# amd64 only (D-043): build on an amd64 host.
#   scripts/petsc/build-image.sh            # tag akantu-rs-petsc:dev
#   AKANTU_RS_MPI_IMAGE=akantu-rs-petsc:dev scripts/petsc/run-petsc-tests.sh
ARG BASE_IMAGE=akantu-rs-mpi:dev
ARG TARGETARCH
FROM ${BASE_IMAGE} AS petsc-common
USER root
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      petsc-dev \
      libopenblas-dev \
      pkg-config \
      time \
    && rm -rf /var/lib/apt/lists/* \
    && dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' \
         > /usr/share/akantu-rs-petsc-packages.tsv \
    && { pkg-config --modversion PETSc; pkg-config --cflags --libs PETSc; \
         grep -E '^(CONFIGURE_OPTIONS|PETSC_ARCH|PETSC_WITH_EXTERNAL_LIB) ' \
           "$(pkg-config --variable=prefix PETSc)/lib/petsc/conf/petscvariables" || true; } \
         > /usr/share/akantu-rs-petsc-configure.txt \
    && { for a in $(update-alternatives --get-selections | awk '/^lib(blas|lapack)\.so\.3/ {print $1}'); do \
           update-alternatives --query "$a" | grep -E '^(Name|Value):'; done; } \
         > /usr/share/akantu-rs-petsc-alternatives.txt
# Deterministic, single-threaded native kernels (same intent as the MUMPS
# image): no threaded BLAS, single-threaded Scotch orderings inside MUMPS.
ENV OPENBLAS_NUM_THREADS=1 \
    OMP_NUM_THREADS=1 \
    SCOTCH_PTHREAD_NUMBER=1

# x86_64: OpenBLAS core type pinned (D-050).
FROM petsc-common AS petsc-amd64
ENV OPENBLAS_CORETYPE=Haswell

# aarch64: OpenBLAS's run-time core selection (no aarch64 PETSc fixtures).
FROM petsc-common AS petsc-arm64

FROM petsc-${TARGETARCH}
USER akantu
