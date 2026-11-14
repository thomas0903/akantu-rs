# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe: upstream test_sparse_solver_mumps
# built out of tree against an oracle install, original and corrected gather.
cmake_minimum_required(VERSION 3.16)
project(mumps_gather_probe CXX)
set(ORACLE_CONFIG "strict" CACHE STRING "oracle install to use (strict|default)")
include(/build/install-${ORACLE_CONFIG}/share/akantu-oracle/use-akantu-oracle.cmake)
add_executable(probe_corrected probe.cc)
target_link_libraries(probe_corrected PRIVATE akantu)
add_executable(probe_original probe.cc)
target_compile_definitions(probe_original PRIVATE ORACLE_PROBE_ORIGINAL)
target_link_libraries(probe_original PRIVATE akantu)
