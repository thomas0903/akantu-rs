# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle configuration `default`: same features as `strict` but
# upstream-default floating-point flags (GCC -ffp-contract=fast default,
# Eigen NEON vectorisation on). Used to measure baseline FP variation.
include(${CMAKE_CURRENT_LIST_DIR}/common.cmake)
# D-050 Eigen cache-size pin on x86_64 (empty on aarch64), see common.cmake.
if(AKANTU_ORACLE_EIGEN_FLAGS)
  set(CMAKE_CXX_FLAGS "${AKANTU_ORACLE_EIGEN_FLAGS}" CACHE STRING "")
endif()
