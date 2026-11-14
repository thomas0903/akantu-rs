# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: the upstream example examples/c++/embedded_model/
# embedded.cc and its oracle variant embedded_eigen.cc, built against the
# patched embedded oracle variant (D-060, oracle/patches/embedded,
# docs/ORACLE_BUILD.md): the embedded sources do not compile at the
# reference commit, so the example is not in the oracle build tree.
# The patched headers and sources are prepared by
# oracle/probes/examples/run-in-container.sh exactly as
# oracle/probes/embedded/run-in-container.sh does (same patch series, same
# static library `emb`, AKANTU_EMBEDDED for these translation units only).
cmake_minimum_required(VERSION 3.20)
project(akantu_examples_embedded_variant LANGUAGES C CXX)
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
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
set(EMBEDDED_PATCHED_INC "" CACHE PATH "copy of the installed headers + patched embedded headers")
set(EMBEDDED_PATCHED_SRC "" CACHE PATH "patched embedded .cc files")
set(AKANTU_EXAMPLES_SRC "" CACHE PATH "examples/c++ of the reference checkout")
if(NOT EXISTS "${AKANTU_EXAMPLES_SRC}/embedded_model/embedded.cc")
  message(FATAL_ERROR "AKANTU_EXAMPLES_SRC must point to examples/c++ of the reference")
endif()
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")

add_library(emb STATIC ${EMBEDDED_PATCHED_SRC}/embedded_interface_intersector.cc
                       ${EMBEDDED_PATCHED_SRC}/embedded_interface_model.cc)
target_include_directories(emb BEFORE PUBLIC ${EMBEDDED_PATCHED_INC})
target_compile_definitions(emb PUBLIC AKANTU_EMBEDDED)
target_compile_options(emb PUBLIC ${_probe_fp_flags})
target_link_libraries(emb PUBLIC akantu CGAL::CGAL)

# the upstream example, unmodified
add_executable(embedded ${AKANTU_EXAMPLES_SRC}/embedded_model/embedded.cc)
target_link_libraries(embedded PRIVATE emb)
# its Eigen-pinned variant
add_executable(embedded_eigen ${CMAKE_CURRENT_SOURCE_DIR}/../embedded_eigen.cc)
target_link_libraries(embedded_eigen PRIVATE emb)
