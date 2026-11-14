# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probe (not part of the main probe build): upstream
# enum operator>> on chosen spellings and Array / CircularArray printself
# under debug levels and module filters. Built against an installed Akantu
# like oracle/probes/phase_field_explicit/CMakeLists.txt. Driven by run.sh.
cmake_minimum_required(VERSION 3.20)
project(akantu_v104_probe LANGUAGES C CXX)
if(POLICY CMP0167)
  cmake_policy(SET CMP0167 NEW)
endif()
find_package(Boost CONFIG REQUIRED COMPONENTS graph regex)
find_package(Eigen3 CONFIG REQUIRED)
find_package(CGAL CONFIG QUIET)
find_package(Akantu REQUIRED)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "" FORCE)
endif()
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
add_executable(v104_probe v104_probe.cc)
target_link_libraries(v104_probe PRIVATE akantu)
target_compile_options(v104_probe PRIVATE ${_probe_fp_flags})
