# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: oracle probe (petsc_probe.cc), built against an installed
# Akantu (DOFManagerPETSc, SparseSolverPETSc, NonLinearSolverPETSc/TAO) like
# oracle/probes/solver_model/CMakeLists.txt (same floating-point flags). Driven
# by oracle/probes/petsc/run.sh (oracle/scripts/run-probe.sh petsc) and
# oracle/probes/petsc/generate.sh.
cmake_minimum_required(VERSION 3.20)
project(akantu_petsc_probe LANGUAGES C CXX)
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
set(AKANTU_PROBE_CXX_FLAGS "" CACHE STRING "Floating-point flags of the oracle")
set(AKANTU_PROBE_UPSTREAM_TEST_DIR "/akantu-src/test/test_model/test_common/test_model_solver" CACHE PATH
  "Directory of the upstream test_model_solver_my_model.hh (included unmodified)")
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
find_package(PkgConfig REQUIRED)
pkg_search_module(PETSC REQUIRED PETSc petsc)
add_executable(petsc_probe petsc_probe.cc)
target_include_directories(petsc_probe PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include
  ${AKANTU_PROBE_UPSTREAM_TEST_DIR} ${PETSC_INCLUDE_DIRS})
target_link_libraries(petsc_probe PRIVATE akantu ${PETSC_LINK_LIBRARIES})
target_compile_options(petsc_probe PRIVATE ${_probe_fp_flags})
