# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe (MUTIL-004 / UD-131): root cause of
# upstream's buildFacets on structural meshes, built out of tree against an
# oracle install.
cmake_minimum_required(VERSION 3.16)
project(ud131_probe CXX)
set(ORACLE_CONFIG "strict" CACHE STRING "oracle install to use (strict|default)")
include(/build/install-${ORACLE_CONFIG}/share/akantu-oracle/use-akantu-oracle.cmake)
add_executable(ud131_probe probe.cc)
target_link_libraries(ud131_probe PRIVATE akantu)
