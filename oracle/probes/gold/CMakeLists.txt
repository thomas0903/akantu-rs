# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: builds upstream test drivers from the read-only
# reference checkout against an installed oracle configuration. The driver
# sources are copied by run-in-container.sh into GOLD_SRC_DIR; a driver that
# does not compile at the pinned commit gets the minimal API patch of
# patches/<driver>.patch there (documented in docs/UPSTREAM_DEFECTS.md, the
# reference checkout itself is never modified).
# Ported from Akantu (EPFL/LSMS) test/test_model/test_solid_mechanics_model/
# test_cohesive/test_cohesive_extrinsic_implicit/CMakeLists.txt (the
# `register_test` of test_assembling_K_cohe_elements; its add_akantu_test is
# commented out upstream, so the oracle build tree does not contain it).
#
# The upstream source is compiled with `-Dmain=akantu_upstream_main`;
# pin_main.cc provides `main`, pins Eigen's cache sizes (D-050:
# Eigen::setCpuCacheSizes(16384, 524288, 524288)) and calls the upstream main.
cmake_minimum_required(VERSION 3.20)
project(akantu_gold_probe LANGUAGES C CXX)
if(POLICY CMP0167)
  cmake_policy(SET CMP0167 NEW)
endif()
find_package(Boost CONFIG REQUIRED COMPONENTS graph regex)
find_package(Eigen3 CONFIG REQUIRED)
find_package(CGAL CONFIG QUIET)
find_package(Akantu REQUIRED)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "" FORCE)
endif()
set(GOLD_SRC_DIR "" CACHE PATH "Copies of the upstream driver sources (patched where needed)")
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")

add_executable(test_assembling_K_cohe_elements
  "${GOLD_SRC_DIR}/test_assembling_K_cohe_elements.cc"
  "${CMAKE_CURRENT_SOURCE_DIR}/pin_main.cc")
set_source_files_properties("${GOLD_SRC_DIR}/test_assembling_K_cohe_elements.cc"
  PROPERTIES COMPILE_DEFINITIONS "main=akantu_upstream_main")
target_link_libraries(test_assembling_K_cohe_elements PRIVATE akantu)
target_compile_options(test_assembling_K_cohe_elements PRIVATE ${_probe_fp_flags})
