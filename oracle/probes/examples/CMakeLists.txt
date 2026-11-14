# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: oracle variants of upstream examples
# (oracle/probes/examples/*.cc: copies of examples/c++/** with the changes
# marked `akantu-rs:`), built against an installed Akantu like
# oracle/probes/smm/CMakeLists.txt (same floating-point flags). The upstream
# example binaries themselves come from the oracle build tree
# (/build/<config>/examples). Driven by oracle/probes/examples/run.sh.
cmake_minimum_required(VERSION 3.20)
project(akantu_examples_probe LANGUAGES C CXX)
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
set(AKANTU_EXAMPLES_SRC "" CACHE PATH "examples/c++ of the reference checkout")
if(NOT EXISTS "${AKANTU_EXAMPLES_SRC}/solid_mechanics_model/new_material/local_material_damage.cc")
  message(FATAL_ERROR "AKANTU_EXAMPLES_SRC must point to examples/c++ of the reference")
endif()
separate_arguments(_probe_fp_flags UNIX_COMMAND "${AKANTU_PROBE_CXX_FLAGS}")
string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
set(_effective "${AKANTU_PROBE_CXX_FLAGS} ${CMAKE_CXX_FLAGS} ${CMAKE_CXX_FLAGS_${_bt}}")
string(STRIP "${_effective}" _effective)
set(_material "${AKANTU_EXAMPLES_SRC}/solid_mechanics_model/new_material")
foreach(_probe static_eigen example_parser_eigen implicit_dynamic_eigen
               implicit_dynamic_eigen_state explicit_dynamic_full
               new_local_material_full parallel_2d_partition
               # cohesive examples (Eigen cache sizes pinned, D-050)
               cohesive_extrinsic_full cohesive_intrinsic_full
               cohesive_extrinsic_ig_tg_full scalability_test_full
               # the velocity functor's write observable
               cohesive_extrinsic_ig_tg_vel
               # diffusion, structural, viscoelastic, phase field,
               # contact and node-to-node examples
               heat_diffusion_static_2d_eigen bernoulli_beam_2_example_static
               material_viscoelastic_maxwell_energies_eigen
               phase_field_notch_eigen phase_field_parallel_partition
               phase_field_parallel_eigen
               contact_explicit_dynamic_full contact_explicit_static_eigen
               cohesive_contact_explicit_dynamic_full bloc_friction_full
               steady_state_full)
  if(_probe STREQUAL "new_local_material_full")
    # the example's material is compiled from the reference sources
    add_executable(${_probe} ${_probe}.cc ${_material}/local_material_damage.cc)
    target_include_directories(${_probe} PRIVATE ${_material})
  else()
    add_executable(${_probe} ${_probe}.cc)
  endif()
  target_link_libraries(${_probe} PRIVATE akantu)
  target_compile_options(${_probe} PRIVATE ${_probe_fp_flags})
  target_compile_definitions(${_probe} PRIVATE
    AKANTU_PROBE_CONFIG="${AKANTU_PROBE_CONFIG}"
    AKANTU_PROBE_CXX_FLAGS="${_effective}"
    AKANTU_PROBE_BUILD_TYPE="${CMAKE_BUILD_TYPE}")
endforeach()

# LD_PRELOAD library pinning Eigen's cache sizes in the
# unmodified upstream example binaries (eigen_cache_pin.cc).
add_library(eigen_cache_pin SHARED eigen_cache_pin.cc)
target_link_libraries(eigen_cache_pin PRIVATE Eigen3::Eigen)
target_compile_options(eigen_cache_pin PRIVATE ${_probe_fp_flags})
