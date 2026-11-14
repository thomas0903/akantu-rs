# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle probes: small programs linked against an installed Akantu
# (oracle configuration `strict` by default) that write golden fixtures.
# Normally driven by oracle/scripts/generate-fixtures.sh, which configures:
#   cmake -S oracle/probes -B <build> -DCMAKE_PREFIX_PATH=/build/install-<config>
#         -DCMAKE_BUILD_TYPE=RelWithDebInfo
#         -DAKANTU_PROBE_CONFIG=<config> -DAKANTU_PROBE_CXX_FLAGS="<oracle flags>"
#
# The element-class kernels are header templates instantiated *in the probe*,
# so the probes must be compiled with the oracle library's floating-point
# flags (e.g. -ffp-contract=off -DEIGEN_DONT_VECTORIZE for `strict`).
cmake_minimum_required(VERSION 3.20)
project(akantu_probes LANGUAGES C CXX)
if(POLICY CMP0167)
  cmake_policy(SET CMP0167 NEW) # Boost's own CMake config (as AkantuConfig does)
endif()

# Workaround (reference install defect): the installed AkantuConfig.cmake
# calls find_package(Boost) without components, but the exported `akantu`
# target links Boost::graph, which is then undefined for out-of-tree
# consumers. Import the components first.
find_package(Boost CONFIG REQUIRED COMPONENTS graph regex)
# Same defect for CGAL (header-only, installed under /opt in the oracle
# image): the link interface names `CGAL`, which must be a target or the
# linker looks for a non-existent libCGAL. CGAL_DIR comes from O1's
# share/akantu-oracle/build-info.txt (passed by run-in-container.sh).
find_package(Eigen3 CONFIG REQUIRED)
find_package(CGAL CONFIG QUIET)
find_package(Akantu REQUIRED)

# compile_commands.json records the actual probe compile commands; they are
# copied into the fixture manifests (build.probe_compile_command).
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "" FORCE)
endif()

set(AKANTU_PROBE_CONFIG "unknown" CACHE STRING
  "Oracle configuration name recorded in fixtures (strict, default, ...)")
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING
  "Floating-point relevant flags of the oracle library build")

separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
set(_effective "${AKANTU_PROBE_CXX_FLAGS} ${CMAKE_CXX_FLAGS} ${CMAKE_CXX_FLAGS_${_bt}}")
string(STRIP "${_effective}" _effective)

# FE engine on meshes (fixtures/fe/engine*, probe source under fe/).
# One translation unit for every element type exceeds the 4 GiB container
# cap: the bodies live in fe/fe_engine_probe_impl.hh and each
# fe/fe_engine_probe_*.cc instantiates a few types.
file(GLOB _fe_engine_probe_units CONFIGURE_DEPENDS
  ${CMAKE_CURRENT_SOURCE_DIR}/fe/fe_engine_probe_*.cc)
set(_probe_source_fe_engine_probe fe/fe_engine_probe.cc ${_fe_engine_probe_units})
# extra material_probe translation units (cpp/material_probe_*.cc),
# e.g. probe-only material keywords such as `neohookean_kernel`.
file(GLOB _material_probe_units CONFIGURE_DEPENDS
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp/material_probe_*.cc)
set(_probe_source_material_probe cpp/material_probe.cc ${_material_probe_units})
# FE outputs not replayed
# by fe_engine_probe (fixtures/fe/engine_extra), law-level random internal
# fields (fixtures/cl), akantu::initialize seed policy (fixtures/core) and
# parameter sub-registries / sub-section dispatch (fixtures/parser).
file(GLOB _fe_extra_probe_units CONFIGURE_DEPENDS
  ${CMAKE_CURRENT_SOURCE_DIR}/fe/fe_extra_probe_*.cc)
set(_probe_source_fe_extra_probe fe/fe_extra_probe.cc ${_fe_extra_probe_units})
foreach(probe element_probe mesh_probe material_probe fe_engine_probe fe_extra_probe
        law_random_probe init_probe parsable_probe)
  if(DEFINED _probe_source_${probe})
    add_executable(${probe} ${_probe_source_${probe}})
  else()
    add_executable(${probe} cpp/${probe}.cc)
  endif()
  target_include_directories(${probe} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
  target_link_libraries(${probe} PRIVATE akantu)
  target_compile_options(${probe} PRIVATE ${_probe_fp_flags})
  if(probe STREQUAL "fe_engine_probe" OR probe STREQUAL "fe_extra_probe")
    # FEEngineTemplate instantiations exceed the 4 GiB container cap with
    # debug information; -g0 does not change floating-point semantics.
    target_compile_options(${probe} PRIVATE -g0)
  endif()
  target_compile_definitions(${probe} PRIVATE
    AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
    AKANTU_PROBE_CXX_FLAGS="${_effective}"
    AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
endforeach()

# Dependency-free self-test of the header-only writer.
add_executable(writer_selftest selftest/writer_selftest.cc)
target_include_directories(writer_selftest PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_compile_options(writer_selftest PRIVATE ${_probe_fp_flags})
