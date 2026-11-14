# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle configuration `strict`: primary golden-data build.
# Floating point as close as possible to IEEE-strict Rust semantics:
#  * -ffp-contract=off : forbid GCC from fusing a*b+c into FMA (GCC's default
#    in GNU mode is -ffp-contract=fast and aarch64 always has FMA).
#  * -DEIGEN_DONT_VECTORIZE : Eigen's NEON packet path calls vfmaq_f64
#    (explicit FMA intrinsic, unaffected by -ffp-contract) in pmadd; disabling
#    explicit vectorisation removes it. See docs/DECISIONS.md (D-031).
include(${CMAKE_CURRENT_LIST_DIR}/common.cmake)
set(AKANTU_ORACLE_FP_FLAGS "-ffp-contract=off -DEIGEN_DONT_VECTORIZE")
set(CMAKE_C_FLAGS       "${AKANTU_ORACLE_FP_FLAGS}" CACHE STRING "")
# + the D-050 Eigen cache-size pin on x86_64 (empty on aarch64), common.cmake.
if(AKANTU_ORACLE_EIGEN_FLAGS)
  set(CMAKE_CXX_FLAGS   "${AKANTU_ORACLE_FP_FLAGS} ${AKANTU_ORACLE_EIGEN_FLAGS}" CACHE STRING "")
else()
  set(CMAKE_CXX_FLAGS   "${AKANTU_ORACLE_FP_FLAGS}" CACHE STRING "")
endif()
set(CMAKE_Fortran_FLAGS "-ffp-contract=off"         CACHE STRING "")
