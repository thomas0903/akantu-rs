# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): C++ reference timing
# driver of the explicit-step benchmark, built against an oracle install tree (cpp_build.sh);
# the oracle helper imports Akantu and applies the install's compile flags
# (FP semantics, Eigen ABI, build type; docs/ORACLE_BUILD.md).
cmake_minimum_required(VERSION 3.16)
project(explicit_step_timing CXX)
set(AKANTU_ORACLE_INSTALL "/build/install-strict" CACHE PATH "Oracle install tree")
include("${AKANTU_ORACLE_INSTALL}/share/akantu-oracle/use-akantu-oracle.cmake")
add_executable(explicit_step_timing explicit_step_timing.cc)
target_link_libraries(explicit_step_timing PRIVATE akantu)
