# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: mesh I/O oracle probe (oracle/probes/mesh_io/mesh_io_probe.cc),
# built against an installed Akantu like oracle/probes/CMakeLists.txt (same
# install-tree workarounds, same floating-point flags). Driven by
# oracle/probes/mesh_io/generate.sh.
cmake_minimum_required(VERSION 3.20)
project(akantu_mesh_io_probe LANGUAGES C CXX)
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
add_executable(mesh_io_probe mesh_io_probe.cc)
target_include_directories(mesh_io_probe PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include)
target_link_libraries(mesh_io_probe PRIVATE akantu)
target_compile_options(mesh_io_probe PRIVATE ${_probe_fp_flags})
target_compile_definitions(mesh_io_probe PRIVATE
  AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
  AKANTU_PROBE_CXX_FLAGS="${_effective}"
  AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
