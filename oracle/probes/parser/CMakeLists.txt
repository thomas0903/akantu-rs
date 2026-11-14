# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe of the parser (see README.md). Configure inside the
# akantu-oracle:dev image against the installed oracle:
#   cmake -S oracle/probes/parser -B /tmp/probe-parser -G Ninja \
#         -DAKANTU_ORACLE_PREFIX=/build/install-strict
cmake_minimum_required(VERSION 3.20)
project(akantu_rs_parser_probe CXX)

set(AKANTU_ORACLE_PREFIX "/build/install-strict" CACHE PATH "oracle install prefix")
include("${AKANTU_ORACLE_PREFIX}/share/akantu-oracle/use-akantu-oracle.cmake")

add_executable(dump_parser dump_parser.cc)
target_link_libraries(dump_parser PRIVATE akantu)

# Clean-room black-box probe of the C++ standard library (D-030): standard
# library only, no Akantu.
add_executable(stream_blackbox stream_blackbox.cc)

# cppargparse probe.
add_executable(cli_probe cli_probe.cc)
target_link_libraries(cli_probe PRIVATE akantu)

# Registered string parameters keep their bytes.
add_executable(registry_probe registry_probe.cc)
target_link_libraries(registry_probe PRIVATE akantu)
