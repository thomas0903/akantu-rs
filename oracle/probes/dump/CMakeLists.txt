# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe of the dumpers (see README.md). Configure inside the
# akantu-oracle:dev image against the installed oracle:
#   cmake -S oracle/probes/dump -B /tmp/probe-dump -G Ninja \
#         -DAKANTU_ORACLE_PREFIX=/build/install-strict
cmake_minimum_required(VERSION 3.20)
project(akantu_rs_dump_probe CXX)

set(AKANTU_ORACLE_PREFIX "/build/install-strict" CACHE PATH "oracle install prefix")
include("${AKANTU_ORACLE_PREFIX}/share/akantu-oracle/use-akantu-oracle.cmake")

add_executable(dump_probe dump_probe.cc)
# The LAMMPS writers and the raw IOHelper API live in libiohelper (installed
# next to libakantu, headers in include/iohelper; Akantu's and IOHelper's
# dumper_text.hh share a name, so both are included with their directory).
target_include_directories(dump_probe PRIVATE "${AKANTU_ORACLE_PREFIX}/include")
target_link_libraries(dump_probe PRIVATE akantu "${AKANTU_ORACLE_PREFIX}/lib/libiohelper.so")
