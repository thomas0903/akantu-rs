# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: geometry oracle probes (oracle/probes/geometry/geometry_probe.cc,
# geometry_adversarial_probe.cc) and the UD-414 check, built against an installed CGAL-enabled Akantu like
# oracle/probes/mesh_utils (same install-tree work-arounds and FP flags).
# GEOMETRY_PATCHED_INCLUDE / GEOMETRY_PATCHED_INCLUDE_0001 hold copies of the
# header-only MeshSphereIntersector with the probe-local patches
# (oracle/patches/geometry); they are searched before the installed headers.
cmake_minimum_required(VERSION 3.20)
project(akantu_geometry_probe LANGUAGES C CXX)
if(POLICY CMP0167)
  cmake_policy(SET CMP0167 NEW)
endif()
find_package(Boost CONFIG REQUIRED COMPONENTS graph regex)
find_package(Eigen3 CONFIG REQUIRED)
find_package(CGAL CONFIG REQUIRED)
find_package(Akantu REQUIRED)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "" FORCE)
endif()
set(AKANTU_PROBE_CONFIG "unknown" CACHE STRING "Oracle configuration name")
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
set(GEOMETRY_PATCHED_INCLUDE "" CACHE PATH "MeshSphereIntersector headers, patches 0001+0002")
set(GEOMETRY_PATCHED_INCLUDE_0001 "" CACHE PATH "MeshSphereIntersector headers, patch 0001 only")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
set(_effective "${AKANTU_PROBE_CXX_FLAGS} ${CMAKE_CXX_FLAGS} ${CMAKE_CXX_FLAGS_${_bt}}")
string(STRIP "${_effective}" _effective)
foreach(_t geometry_probe geometry_adversarial_probe ud414_check)
  add_executable(${_t} ${_t}.cc)
  target_link_libraries(${_t} PRIVATE akantu CGAL::CGAL)
  target_compile_options(${_t} PRIVATE ${_probe_fp_flags})
  target_compile_definitions(${_t} PRIVATE
    AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
    AKANTU_PROBE_CXX_FLAGS="${_effective}"
    AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
endforeach()
target_include_directories(geometry_probe BEFORE PRIVATE ${GEOMETRY_PATCHED_INCLUDE}
  ${CMAKE_CURRENT_SOURCE_DIR}/../include)
# The adversarial cases, same patched headers.
target_include_directories(geometry_adversarial_probe BEFORE PRIVATE ${GEOMETRY_PATCHED_INCLUDE}
  ${CMAKE_CURRENT_SOURCE_DIR}/../include)
target_include_directories(ud414_check BEFORE PRIVATE ${GEOMETRY_PATCHED_INCLUDE_0001})
