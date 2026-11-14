#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: the numerator-guard scan (mode ab2 of v110_guard_scan.cc) on the
# oracle image; from the root of a clone:
#   oracle/probes/dense/complex_guard/run_ab2.sh OUTDIR [M]
set -euo pipefail
repo=$(pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
m="${2:-20000}"
art=oracle/probes/dense/complex_guard
cr=oracle/probes/dense/complex_cleanroom
docker run --rm --memory=2g --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "$repo":/w:ro -v "$out":/out -w /w akantu-oracle:dev bash -euc "
  cxx='g++ -O2 -std=c++20 -ffp-contract=off -I$cr -I$art'
  \$cxx -c $art/v110_div_tu.cc -o /tmp/div_tu.o
  \$cxx $art/v110_guard_scan.cc /tmp/div_tu.o -o /tmp/scan
  /tmp/scan $m ab2 > /out/guard_scan_ab2.txt
"
cat "$out/guard_scan_ab2.txt"
