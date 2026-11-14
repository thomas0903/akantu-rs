# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs Scotch test image: the MPI test image
# (docker/mpi/Dockerfile, akantu-rs-mpi:dev) plus Debian trixie's Scotch
# (libscotch-dev, the same distribution and version as the C++ oracle image
# akantu-oracle:dev, 7.0.7-1; 32-bit SCOTCH_Num like the oracle's Int) for
# the `scotch` feature of akantu-mesh. Build with
#   docker build -t akantu-rs-scotch:dev docker/scotch
# and run AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev scripts/mpi/run-par-tests.sh --scotch
ARG BASE_IMAGE=akantu-rs-mpi:dev
FROM ${BASE_IMAGE}
USER root
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install libscotch-dev \
    && rm -rf /var/lib/apt/lists/* \
    && dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' 'libscotch*' > /usr/share/akantu-rs-scotch-packages.tsv
USER akantu
