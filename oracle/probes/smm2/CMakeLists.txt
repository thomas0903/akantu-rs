# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: oracle probe (oracle/probes/smm2/smm2_probe.cc), built
# against an installed Akantu like oracle/probes/smm/CMakeLists.txt (same
# install-tree workarounds, same floating-point flags). Driven by
# oracle/probes/smm2/generate.sh.
cmake_minimum_required(VERSION 3.20)
project(akantu_smm2_probe LANGUAGES C CXX)
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
set(AKANTU_PROBE_CONFIG "unknown" CACHE STRING "Oracle configuration name")
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
set(_effective "${AKANTU_PROBE_CXX_FLAGS} ${CMAKE_CXX_FLAGS} ${CMAKE_CXX_FLAGS_${_bt}}")
string(STRIP "${_effective}" _effective)
set(AKANTU_PROBE_UPSTREAM_SRC "/akantu-src" CACHE PATH "Read-only reference checkout")
# TEST-135: the upstream test's user material, compiled unchanged from the
# reference tree (test_local_material registers it through MaterialFactory)
set(_local_material_dir
  ${AKANTU_PROBE_UPSTREAM_SRC}/test/test_model/test_solid_mechanics_model/test_materials)
add_executable(smm2_probe smm2_probe.cc ${_local_material_dir}/local_material_damage.cc)
target_include_directories(smm2_probe PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include
  ${_local_material_dir})
target_link_libraries(smm2_probe PRIVATE akantu)
target_compile_options(smm2_probe PRIVATE ${_probe_fp_flags})
# FEEngineTemplate instantiations (facet FE engine) exceed the 4 GiB container
# cap with debug information (as oracle/probes/CMakeLists.txt fe_engine_probe);
# -g0 does not change floating-point semantics.
target_compile_options(smm2_probe PRIVATE -g0)
target_compile_definitions(smm2_probe PRIVATE
  AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
  AKANTU_PROBE_CXX_FLAGS="${_effective}"
  AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
