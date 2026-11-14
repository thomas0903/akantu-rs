# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs memory-checker image (D-085): one of
# the project images (akantu-rs-mpi:dev, akantu-rs-mumps:dev or
# akantu-rs-petsc:dev, chosen with BASE_IMAGE) plus valgrind, so that the FFI
# `unsafe` code (Open MPI, MUMPS, Scotch, PETSc, libm) can be run under
# memcheck with the same libraries and the same cargo setup as the tests.
# Built in the job by ci/tiers/memcheck.sh (tag akantu-rs-memcheck-<kind>:dev);
# the base images are not modified. valgrind and libc6-dbg come from the
# Debian trixie repositories (not version-pinned, like the base images' apt
# packages, D-034); the versions are printed by ci/memcheck/run-in-image.sh.
ARG BASE_IMAGE=akantu-rs-mpi:dev
FROM ${BASE_IMAGE}
USER root
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      valgrind libc6-dbg \
    && rm -rf /var/lib/apt/lists/*
USER akantu
