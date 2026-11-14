# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe: upstream SparseSolverMumps on systems
# exported by the Rust MUMPS acceptance binary, built out of tree against an
# oracle install.
cmake_minimum_required(VERSION 3.16)
project(mumps_probe C CXX)
set(ORACLE_CONFIG "strict" CACHE STRING "oracle install to use (strict|default)")
include(/build/install-${ORACLE_CONFIG}/share/akantu-oracle/use-akantu-oracle.cmake)
add_executable(mumps_probe probe.cc)
target_link_libraries(mumps_probe PRIVATE akantu)
# UD-216 interposer: LD_PRELOADed dmumps_c wrapper that lowers ICNTL(14)
# once (oracle runs of the workspace-retry path).
add_library(icntl14_interposer SHARED icntl14_interposer.c)
set_target_properties(icntl14_interposer PROPERTIES LINKER_LANGUAGE C)
target_link_libraries(icntl14_interposer PRIVATE dl)
