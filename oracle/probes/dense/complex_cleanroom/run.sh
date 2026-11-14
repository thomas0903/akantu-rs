#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Clean-room black-box probes of std::complex<double> division (D-030
# amendment). Builds and runs every probe in the oracle image,
# writes the outputs to results/, and regenerates the Rust test fixture
#   crates/akantu-linalg/tests/fixtures/dense/complex_div_cleanroom.txt
# Run from the repository root (about 3 minutes, < 1 GB RAM):
#   oracle/probes/dense/complex_cleanroom/run.sh
# Only the RESULTS of the compiled division are observed; no library source
# is read and no compiled library code is disassembled or inspected.
# Optional: CDIV_BULK_OUT=/abs/path/file.bin also writes 6e6 bulk oracle
# records (probe E27, 288 MB) for the opt-in Rust test
#   AKANTU_CDIV_BULK=/abs/path/file.bin cargo test -p akantu-linalg --release --test complex_div_cleanroom
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
dir=oracle/probes/dense/complex_cleanroom
mkdir -p "$repo/$dir/results"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g -v "$repo":/w -w "/w/$dir" akantu-oracle:dev bash -euc '
  cxx="g++ -O2 -std=c++20 -ffp-contract=off"
  { g++ --version | head -1; echo "debian $(cat /etc/debian_version) $(uname -m)"; } > results/toolchain.txt
  $cxx e0_basic.cc -o /tmp/e0 && /tmp/e0 1 1 1 -1  1 1 1 1  1 2 1 2  2 1 2 1  1 2 3 4  1 1 0 0  inf 1 1 1  1 1 inf 1  nan 1 1 1 > results/e0.txt
  $cxx e1_smith_variants.cc -o /tmp/e1 && /tmp/e1 | sort -t= -k8 -n > results/e1.txt
  $cxx e2_wide_range.cc -o /tmp/e2 && /tmp/e2 > results/e2.txt
  $cxx e3_r_zero.cc -o /tmp/e3 && /tmp/e3 > results/e3.txt
  $cxx e3b_r_small.cc -o /tmp/e3b && /tmp/e3b > results/e3b.txt
  $cxx e3c_r_threshold.cc -o /tmp/e3c && /tmp/e3c > results/e3c.txt
  $cxx -DMODEL=model_v1 -DMODEL_HEADER=model_v1.h e4_residual.cc -o /tmp/e4v1 && /tmp/e4v1 4000000 20 > results/e4_model_v1.txt
  $cxx e5_numerator_upscale.cc -o /tmp/e5 && /tmp/e5 > results/e5.txt
  $cxx e5b_scan.cc -o /tmp/e5b && {
    /tmp/e5b 1.5 999 0 0 1.25 -266 1.75 -967 d -1000 -300 | awk "{st=\$NF; if (st!=prev) {print; prev=st}}"
    /tmp/e5b 1.5 999 0 0 1.25 -266 1.75 -967 c -600 100 | awk "{st=\$NF; if (st!=prev) {print; prev=st}}"
    /tmp/e5b 1.5 999 0 0 1.25 -266 1.75 -967 a 900 1023 | awk "{st=\$NF; if (st!=prev) {print; prev=st}}"
  } > results/e5b.txt
  $cxx e6_bisect.cc -o /tmp/e6 && {
    /tmp/e6 0x1.8p999 0 0x1p-60 0x1.cp-967 c 0x1p-60 0x1p-40 inf_y
    /tmp/e6 0x1p960 0 0x1p-60 0x1.cp-967 a 0x1p960 0x1p1000 inf_y
    /tmp/e6 0x1p960 0 0x1p-300 0x1.cp-967 a 0x1p960 0x1p1000 inf_y
  } > results/e6.txt
  $cxx e7_upscale_map.cc -o /tmp/e7 && /tmp/e7 | head -4 > results/e7.txt
  $cxx -DMODEL=model_v2 -DMODEL_HEADER=model_v2.h e4_residual.cc -o /tmp/e4v2 && /tmp/e4v2 4000000 20 > results/e4_model_v2.txt
  $cxx e8_explain.cc -o /tmp/e8 && /tmp/e8 0x00030f4ebc85ed16 0x3f4e59737f508959 0xbed14a85202cb612 0x0000000000782bee \
      0x000000000026387b 0x32d541f0096c0888 0x3de37c874f0eeada 0x8780b2c9b0cc0949 > results/e8.txt
  $cxx e9_scan_exact.cc -o /tmp/e9 && /tmp/e9 0x000000000026387b 0x32d541f0096c0888 0x3de37c874f0eeada 0x8780b2c9b0cc0949 b -100 1000 > results/e9.txt
  $cxx e10_tiny_candidates.cc -o /tmp/e10 && /tmp/e10 > results/e10.txt
  $cxx e11_trigger_map.cc -o /tmp/e11 && /tmp/e11 > results/e11.txt
  $cxx e12_trigger_dump.cc -o /tmp/e12 && /tmp/e12 20000000 | cut -c1 | sort | uniq -c > results/e12_summary.txt
  $cxx e13_all4.cc -o /tmp/e13 && /tmp/e13 20000000 dump > results/e13.txt
  $cxx e14_scan_a52.cc -o /tmp/e14 && {
    B="0x000000000026387b 0x32d541f0096c0888 0x3de37c874f0eeada 0x8780b2c9b0cc0949"
    for w in a b c d; do echo "# case4: $w scaled"; /tmp/e14 $B $w -200 1100; done
    B="0x00030f4ebc85ed16 0x3f4e59737f508959 0xbed14a85202cb612 0x0000000000782bee"
    for w in a b c d; do echo "# case1: $w scaled"; /tmp/e14 $B $w -200 1100; done
  } > results/e14.txt
  $cxx e15_large_ab.cc -o /tmp/e15 && /tmp/e15 > results/e15.txt
  $cxx e16_ab_threshold.cc -o /tmp/e16 && /tmp/e16 > results/e16.txt
  $cxx e17_trigger_details.cc -o /tmp/e17 && /tmp/e17 > results/e17.txt
  $cxx e18_guards.cc -o /tmp/e18 && /tmp/e18 > results/e18.txt
  $cxx -DMODEL=model_v3 -DMODEL_HEADER=model_v3.h e4_residual.cc -o /tmp/e4v3 && /tmp/e4v3 20000000 10 > results/e4_model_v3.txt
  $cxx e19_downscale.cc -o /tmp/e19 && /tmp/e19 > results/e19.txt
  $cxx -DMODEL=model_v4 -DMODEL_HEADER=model_v4.h e4_residual.cc -o /tmp/e4v4 && /tmp/e4v4 40000000 10 > results/e4_model_v4.txt
  $cxx e20_guard_sensitivity.cc -o /tmp/e20 && /tmp/e20 > results/e20.txt
  $cxx e21_exclusive.cc -o /tmp/e21 && /tmp/e21 > results/e21.txt
  $cxx e22_special.cc -o /tmp/e22 && /tmp/e22 > results/e22.txt
  $cxx e23_special_max.cc -o /tmp/e23 && /tmp/e23 > results/e23.txt
  $cxx e24_nan_sign.cc -o /tmp/e24 && /tmp/e24 > results/e24.txt
  $cxx e25_special_random.cc -o /tmp/e25 && /tmp/e25 > results/e25.txt
  for f in "-O0" "-O2" "-O3" "-O2 -ffp-contract=off -DEIGEN_DONT_VECTORIZE -DNDEBUG" "-O2 -ffp-contract=fast"; do
    g++ -std=c++20 $f -c e26_oracle_tu.cc -o /tmp/tu.o && $cxx e26_flags.cc /tmp/tu.o -o /tmp/e26 && echo "oracle TU flags [$f]: $(/tmp/e26)"
  done > results/e26.txt
  $cxx gen_fixture.cc -o /tmp/gen && /tmp/gen > /tmp/fixture.txt 2> results/gen_fixture_stats.txt
  {
    echo "# Clean-room complex division fixture (D-030 amendment, review AR2-10). Generated by"
    echo "# oracle/probes/dense/complex_cleanroom/run.sh from black-box runs of std::complex<double>"
    echo "# division in akantu-oracle:dev (g++ $(g++ -dumpfullversion), Debian $(cat /etc/debian_version), $(uname -m))."
    echo "# Format: cdivx <tag> | a b c d | re im   (IEEE-754 bit patterns; tags: gen_fixture.cc)"
    cat /tmp/fixture.txt
  } > results/fixture.txt
'
if [ -n "${CDIV_BULK_OUT:-}" ]; then
  out_dir=$(cd "$(dirname "$CDIV_BULK_OUT")" && pwd)
  docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g -v "$repo":/w -v "$out_dir":/out -w "/w/$dir" akantu-oracle:dev bash -euc "
    g++ -O2 -std=c++20 -ffp-contract=off e27_bulk.cc -o /tmp/e27 && /tmp/e27 6000000 > /out/$(basename "$CDIV_BULK_OUT")
  " 2> "$repo/$dir/results/e27.txt"
fi
mv "$repo/$dir/results/fixture.txt" "$repo/crates/akantu-linalg/tests/fixtures/dense/complex_div_cleanroom.txt"
cat "$repo/$dir/results/gen_fixture_stats.txt"
