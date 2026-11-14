# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): headless ParaView
# (pvpython, offscreen OSMesa rendering) for the visual check of the example
# outputs (scripts/paraview/, docs/reports/2026-11-12-paraview-visual-check.md).
#
# ParaView 5.13.3, the official Kitware binary "osmesa-MPI-Linux-Python3.10-
# x86_64" tarball from https://www.paraview.org/files/v5.13/ (BSD-3-Clause
# licence of ParaView / VTK; the tarball bundles its own Python 3.10, NumPy and
# Mesa OSMesa software renderer, so no GPU and no X server are needed).
# The download is anonymous; the tarball is checked against the SHA-256 below
# (recorded 2026-11-12; the MD5 49b6a573fc9d09c4b70be7bf34887802 equals the one
# published in https://www.paraview.org/files/v5.13/md5sum.txt).
#
# Base: Debian 12 (bookworm), pinned by digest (recorded 2026-11-12). Not
# trixie: the tarball's bundled libOSMesa needs libglapi.so.0, which Debian 13
# no longer ships as a separate library (merged into mesa-libgallium).
# apt packages are not version-pinned (D-034).
ARG BASE_IMAGE=debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
FROM ${BASE_IMAGE}

ARG PV_VERSION=5.13.3
ARG PV_TARBALL=ParaView-5.13.3-osmesa-MPI-Linux-Python3.10-x86_64.tar.gz
ARG PV_URL=https://www.paraview.org/files/v5.13/ParaView-5.13.3-osmesa-MPI-Linux-Python3.10-x86_64.tar.gz
ARG PV_SHA256=7c63b9aa349950358b8fd323b1dc0b8c7e6ad9439362dbbdd9a38180c92b4d77

ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 LC_ALL=C.UTF-8

# runtime libraries of the binary tarball (found with `ldd` on pvpython/pvserver)
RUN apt-get -qq update && apt-get -qq -y --no-install-recommends install \
      ca-certificates curl \
      libgomp1 libx11-6 libxext6 libxt6 libxcb1 libsm6 libice6 libxrender1 \
      libexpat1 libfontconfig1 libfreetype6 libglib2.0-0 \
      libglapi-mesa libgl1 libjpeg62-turbo libtbb12 \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sSfL "${PV_URL}" -o "/tmp/${PV_TARBALL}" \
 && echo "${PV_SHA256}  /tmp/${PV_TARBALL}" | sha256sum -c - \
 && mkdir -p /opt/paraview \
 && tar -xzf "/tmp/${PV_TARBALL}" -C /opt/paraview --strip-components=1 \
 && rm -f "/tmp/${PV_TARBALL}"

ENV PATH=/opt/paraview/bin:${PATH} \
    PV_VERSION=${PV_VERSION}
WORKDIR /work
CMD ["pvpython", "--version"]
