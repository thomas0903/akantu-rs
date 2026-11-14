# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness. Derived from the upstream
# CMakeLists.txt of gitlab.com/akantu/performance-testing-cohesive @ 483bace
# (kept verbatim as CMakeLists.upstream.txt). Changes: Akantu is imported from
# an oracle install tree (docs/ORACLE_BUILD.md), whose helper also applies
# the oracle compile flags (FP semantics, Eigen ABI, build type); the
# executable also contains the fillNodesToElements interposer
# (joss_fnte_interposer.cc, JOSS-patch emulation, see ../README.md).
cmake_minimum_required(VERSION 3.16)
project(perf-test CXX)

set(AKANTU_ORACLE_INSTALL "/build/install-default" CACHE PATH
  "Oracle install tree (/build/install-default or /build/install-strict)")
include("${AKANTU_ORACLE_INSTALL}/share/akantu-oracle/use-akantu-oracle.cmake")

# Call sites of Mesh::fillNodesToElements in this libakantu.so that the JOSS
# patch removes (generated from the installed library).
get_filename_component(_libakantu "${AKANTU_ORACLE_INSTALL}/lib/libakantu.so" REALPATH)
execute_process(
  COMMAND python3 -I "${CMAKE_CURRENT_SOURCE_DIR}/../scripts/in-container/fnte_sites.py"
          "${_libakantu}" "${CMAKE_CURRENT_BINARY_DIR}/fnte_sites.h"
          "${CMAKE_CURRENT_BINARY_DIR}/fnte_sites.txt"
  RESULT_VARIABLE _fnte_result)
if(NOT _fnte_result EQUAL 0)
  message(FATAL_ERROR "fnte_sites.py failed on ${_libakantu}")
endif()

find_package(OpenMP)
add_executable(cohesive_extrinsic
  cohesive_extrinsic.cc
  joss_fnte_interposer.cc)
target_include_directories(cohesive_extrinsic PRIVATE "${CMAKE_CURRENT_BINARY_DIR}")
# Export the interposing definition so that libakantu's PLT binds to it.
target_link_options(cohesive_extrinsic PRIVATE
  "LINKER:--export-dynamic-symbol=_ZN6akantu4Mesh19fillNodesToElementsEi")
target_link_libraries(cohesive_extrinsic PRIVATE akantu ${CMAKE_DL_LIBS})
