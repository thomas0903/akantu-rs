# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: the patched oracle variant `embedded` (D-060, docs/ORACLE_BUILD.md). Builds Akantu's (EPFL/LSMS) embedded model from patched COPIES of the
# reference sources (oracle/patches/embedded) against an installed
# CGAL-enabled oracle (embedded package OFF in the install), then upstream's
# three embedded tests (patched copies) and the fixture probe
# embedded_probe.cc. Same compiler and floating-point flags as the install
# (AKANTU_PROBE_CXX_FLAGS = the install's CMAKE_CXX_FLAGS).
cmake_minimum_required(VERSION 3.20)
project(akantu_embedded_variant LANGUAGES C CXX)
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
set(EMBEDDED_PATCHED_INC "" CACHE PATH "copy of the installed headers + patched embedded headers")
set(EMBEDDED_PATCHED_SRC "" CACHE PATH "patched embedded .cc files")
set(EMBEDDED_UPSTREAM_TESTS "" CACHE PATH "patched copy of test_embedded_interface/")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
set(_effective "${AKANTU_PROBE_CXX_FLAGS} ${CMAKE_CXX_FLAGS} ${CMAKE_CXX_FLAGS_${_bt}}")
string(STRIP "${_effective}" _effective)

# the embedded package (patched): AKANTU_EMBEDDED enables the model's options
# in model_options.hh / model.hh for these translation units only (the
# library itself has no embedded-dependent code)
add_library(emb STATIC ${EMBEDDED_PATCHED_SRC}/embedded_interface_intersector.cc
                       ${EMBEDDED_PATCHED_SRC}/embedded_interface_model.cc)
target_include_directories(emb BEFORE PUBLIC ${EMBEDDED_PATCHED_INC})
target_compile_definitions(emb PUBLIC AKANTU_EMBEDDED)
target_compile_options(emb PUBLIC ${_probe_fp_flags})
target_link_libraries(emb PUBLIC akantu CGAL::CGAL)

foreach(_t test_embedded_element_matrix test_embedded_interface_model
           test_embedded_interface_model_prestress)
  add_executable(${_t} ${EMBEDDED_UPSTREAM_TESTS}/${_t}.cc)
  target_link_libraries(${_t} PRIVATE emb)
endforeach()

add_executable(embedded_probe embedded_probe.cc)
target_link_libraries(embedded_probe PRIVATE emb)
target_include_directories(embedded_probe PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include)
target_compile_definitions(embedded_probe PRIVATE
  AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
  AKANTU_PROBE_CXX_FLAGS="${_effective}"
  AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
