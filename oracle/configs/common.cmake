# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle: CMake initial cache shared by all oracle configurations.
# Loaded with `cmake -C /oracle/configs/<config>.cmake` (see
# oracle/scripts/configure-build.sh and docs/ORACLE_BUILD.md).
# Feature set = upstream CI `.configure` job + every optional package that
# builds at the reference commit (CGAL geometry, debug_tools, unstable tests,
# examples tested). Packages that do not compile upstream are OFF (see below).

set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "")
set(CMAKE_C_COMPILER   /usr/bin/gcc      CACHE FILEPATH "")
set(CMAKE_CXX_COMPILER /usr/bin/g++      CACHE FILEPATH "")
set(CMAKE_Fortran_COMPILER /usr/bin/gfortran CACHE FILEPATH "")

# Tests / examples / python
set(AKANTU_TESTS                ON  CACHE BOOL "")
set(AKANTU_BUILD_ALL_TESTS      ON  CACHE BOOL "")
set(AKANTU_BUILD_UNSTABLE_TESTS ON  CACHE BOOL "")   # upstream CI: OFF
set(AKANTU_EXAMPLES             ON  CACHE BOOL "")
set(AKANTU_BUILD_ALL_EXAMPLES   ON  CACHE BOOL "")
set(AKANTU_TEST_EXAMPLES        ON  CACHE BOOL "")
set(AKANTU_PYTHON_INTERFACE     ON  CACHE BOOL "")

# Parallelism and implicit solvers
set(AKANTU_PARALLEL        ON            CACHE BOOL "")
set(AKANTU_IMPLICIT        ON            CACHE BOOL "")
set(AKANTU_IMPLICIT_SOLVER "Mumps+PETSc" CACHE STRING "")

# Packages
set(AKANTU_SOLID_MECHANICS      ON CACHE BOOL "")
set(AKANTU_COHESIVE_ELEMENT     ON CACHE BOOL "")
set(AKANTU_CONTACT_MECHANICS    ON CACHE BOOL "")
set(AKANTU_DIFFUSION            ON CACHE BOOL "")
set(AKANTU_PHASE_FIELD          ON CACHE BOOL "")
set(AKANTU_MODEL_COUPLERS       ON CACHE BOOL "")
set(AKANTU_STRUCTURAL_MECHANICS ON CACHE BOOL "")
set(AKANTU_DAMAGE_NON_LOCAL     ON CACHE BOOL "")
# embedded does not compile at the reference commit: model_options.hh calls
# DECLARE_NAMED_ARGUMENT with 2 arguments (candidate fix kept, not applied, in
# oracle/patches/unused/0002) and material_reinforcement_tmpl.hh is unfinished
# upstream WIP against the pre-shared_ptr constitutive-law API. See D-032.
set(AKANTU_EMBEDDED             OFF CACHE BOOL "")
# CGAL alone (src/geometry + 4 geometry tests) builds with CGAL 5.6.2 from the
# image (CGAL_DIR env); Debian's CGAL 6 is API-incompatible. See D-032.
set(AKANTU_USE_CGAL             ON CACHE BOOL "")   # external package option
set(AKANTU_DUMPERS              ON CACHE BOOL "")
set(AKANTU_DEBUG_TOOLS          ON CACHE BOOL "")
# Extra packages (extra_packages/)
# extra-materials does not compile at the reference commit (written against
# the Akantu 4 API: Array<T>::iterator<>, Matrix::copy, Math::matrix33_eigenvalues,
# removed material instantiation macros). See D-032.
set(AKANTU_EXTRA_MATERIALS                OFF CACHE BOOL "")
set(AKANTU_EXTRA_MATERIALS_NON_LOCAL      OFF CACHE BOOL "")
# IGFEM does not compile at the reference commit (bit-rot: includes
# static_communicator.hh / aka_debug_tools.hh, removed from Akantu years ago)
# and, when enabled, breaks core TUs that include its headers. See
# docs/DECISIONS.md (D-032).
set(AKANTU_IGFEM                          OFF CACHE BOOL "")
set(AKANTU_TRACTION_AT_SPLIT_NODE_CONTACT ON CACHE BOOL "")

# Use the Debian packages installed in the image instead of network fetches
# (no configure-time downloads). Versions are fixed only within the recorded
# image (oracle/docker/image-id.txt, packages.tsv; D-034): Eigen 3.4.0 (upstream
# CI image also uses the system Eigen), pybind11 2.13.6.
set(AKANTU_USE_SYSTEM_EIGEN3   ON CACHE STRING "")
set(AKANTU_USE_SYSTEM_PYBIND11 ON CACHE STRING "")
# googletest: upstream default is AUTO -> find_package(gtest) (Debian only ships
# GTestConfig.cmake, so this fails, as in upstream CI whose image has no gtest)
# -> third-party/cmake/gtest.cmake FetchContent of googletest "main" (unpinned).
# Keep upstream's code path (googletest built in-tree via FetchContent, target
# GTest::gtest_main) but feed it the Debian googletest sources of the image
# (googletest 1.16.0-1 in the recorded image; an image rebuild may differ).
set(AKANTU_USE_SYSTEM_GTEST OFF CACHE STRING "")
set(FETCHCONTENT_SOURCE_DIR_GOOGLETEST /usr/src/googletest CACHE PATH "")
# Forbid any other network download at configure time.
set(FETCHCONTENT_FULLY_DISCONNECTED ON CACHE BOOL "")

# MPI rank counts for PARALLEL tests without an explicit PARALLEL_LEVEL:
# with oracle/patches/0001 they are registered for 8, 4, 2 and 1 ranks
# (upstream registers them with 1 rank only, see the patch header and D-033).
set(MPIEXEC_MAX_NUMPROCS 8 CACHE STRING "")

# OpenMPI inside Docker (non-root user, but keep upstream's docker switch off).
set(AKANTU_RUN_IN_DOCKER OFF CACHE BOOL "")
set(AKANTU_USE_CCACHE    ON  CACHE BOOL "")

# D-050: pin Eigen's cache sizes on x86_64. Eigen 3.4.0 sizes its GEMM
# / triangular-solve blocking (also inside SparseLU) from the L1/L2/L3 sizes,
# which it queries with CPUID on x86_64 (host-dependent results) and takes from
# its built-in defaults 16 KiB / 512 KiB / 512 KiB on aarch64. On x86_64 every
# oracle TU (library, tests, examples, Python module) and every probe (they read
# CMAKE_CXX_FLAGS from share/akantu-oracle/build-info.txt or oracle-flags.cmake)
# is compiled without the CPUID query and with the aarch64 defaults, so no
# binary can look at the host CPU (checked: no `cpuid` instruction in
# libakantu.so). Empty on aarch64: the recorded aarch64 build is unchanged.
execute_process(COMMAND uname -m OUTPUT_VARIABLE _akantu_oracle_arch
                OUTPUT_STRIP_TRAILING_WHITESPACE)
if(_akantu_oracle_arch STREQUAL "x86_64")
  set(AKANTU_ORACLE_EIGEN_FLAGS
      "-DEIGEN_NO_CPUID -DEIGEN_DEFAULT_L1_CACHE_SIZE=16384 -DEIGEN_DEFAULT_L2_CACHE_SIZE=524288 -DEIGEN_DEFAULT_L3_CACHE_SIZE=524288")
else()
  set(AKANTU_ORACLE_EIGEN_FLAGS "")
endif()
