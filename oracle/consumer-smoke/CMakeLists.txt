# SPDX-License-Identifier: LGPL-3.0-or-later
# Out-of-tree consumer smoke test for an oracle install tree
# (/build/install-<config>). Run by oracle/scripts/in-container/finalize-install.sh.
cmake_minimum_required(VERSION 3.16)
project(akantu_oracle_consumer_smoke CXX)

# Upstream AkantuConfig.cmake bugs: the exported akantu target links
# Boost::graph (debug_tools) and the CGAL target, but the config file only
# imports Boost headers and Eigen3. Import them first (CGAL_DIR is set in the
# image). share/akantu-oracle/use-akantu-oracle.cmake does all of this.
find_package(Boost REQUIRED COMPONENTS graph)
# Eigen3 in CONFIG mode before CGAL: CGAL puts its legacy FindEigen3.cmake
# (no Eigen3::Eigen target) on CMAKE_MODULE_PATH.
find_package(Eigen3 REQUIRED NO_MODULE)
find_package(CGAL REQUIRED)
find_package(Akantu REQUIRED)
# Same FP flags / Eigen ABI as the oracle build (see docs/DECISIONS.md D-031).
include(${AKANTU_ORACLE_FLAGS_FILE})

add_executable(smoke smoke.cc)
target_link_libraries(smoke PRIVATE akantu)
