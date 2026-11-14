#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: build and run the independent black-box complex-division probes
# in the oracle image (x86_64 host). Run from the root of a clone:
#   oracle/probes/dense/complex_guard/run.sh OUTDIR [SCALE] [M]
# Writes OUTDIR/{toolchain.txt,gen.txt,cdiv.bin,cdiv_e25.bin,e25_v6.txt,e25_dump.txt,guard_scan.txt}.
# The author's clean-room headers come from oracle/probes/dense/complex_cleanroom.
set -euo pipefail
repo=$(pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
scale="${2:-1}"
m="${3:-20000}"
art=oracle/probes/dense/complex_guard
cr=oracle/probes/dense/complex_cleanroom
echo "oracle image $(docker image inspect --format '{{.Id}}' akantu-oracle:dev)" > "$out/image.txt"
docker run --rm --memory=2g --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "$repo":/w:ro -v "$out":/out -w /w akantu-oracle:dev bash -euc "
  cxx='g++ -O2 -std=c++20 -ffp-contract=off -I$cr -I$art'
  { g++ --version | head -1; echo \"debian \$(cat /etc/debian_version) \$(uname -m)\"; } > /out/toolchain.txt
  \$cxx -c $art/v110_div_tu.cc -o /tmp/div_tu.o
  \$cxx $art/v110_cdiv_gen.cc /tmp/div_tu.o -o /tmp/gen
  \$cxx $art/v110_guard_scan.cc /tmp/div_tu.o -o /tmp/scan
  \$cxx $art/v110_e25_dump.cc /tmp/div_tu.o -o /tmp/e25dump
  sed -e 's/model_v5.h/model_v6.h/' -e 's/w = model_v5(a, b, c, d)/w = model_v6(a, b, c, d)/g' \
      -e 's/if (shown++ < 20)/if (shown++ < 200)/' $cr/e25_special_random.cc > /tmp/e25_v6.cc
  grep -c 'model_v6(a, b, c, d)' /tmp/e25_v6.cc > /out/e25_v6_sed_check.txt
  \$cxx /tmp/e25_v6.cc -o /tmp/e25v6
  /tmp/scan $m > /out/guard_scan.txt
  /tmp/e25v6 > /out/e25_v6.txt
  /tmp/e25dump /out/cdiv_e25.bin > /out/e25_dump.txt
  /tmp/gen /out/cdiv.bin $scale > /out/gen.txt
"
cat "$out/toolchain.txt" "$out/e25_v6_sed_check.txt"
grep -h "^E25" "$out/e25_v6.txt"
cat "$out/e25_dump.txt"
grep -h "^V110" "$out/gen.txt"
