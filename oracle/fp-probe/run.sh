#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle tooling (not ported from Akantu): FP contraction probe, see README.md.
set -e
cd /work
count() { objdump -d "$1" | grep -E -c '\s(fmadd|fmsub|fnmadd|fnmsub|fmla|fmls)\s' || true; }
echo "g++ $(g++ -dumpfullversion) on $(uname -m)"
for std in "-std=gnu++20" "-std=c++20"; do
 for fc in "" "-ffp-contract=off"; do
  g++ -O2 $std $fc -c scalar.cc -o /tmp/s.o; echo "scalar a*b+c  $std ${fc:-<default>}: fused insns = $(count /tmp/s.o)"
 done
done
for flags in "-O2 -std=gnu++20" "-O2 -std=gnu++20 -ffp-contract=off" "-O2 -std=gnu++20 -ffp-contract=off -DEIGEN_DONT_VECTORIZE" "-O2 -std=gnu++20 -DEIGEN_DONT_VECTORIZE"; do
  g++ $flags -I/usr/include/eigen3 -c eigen.cc -o /tmp/e.o
  echo "eigen [$flags]: fused insns = $(count /tmp/e.o)"
done
echo "predefined: $(echo | g++ -dM -E -x c++ - | grep -E '__ARM_FEATURE_FMA|__ARM_NEON ' | tr '\n' ' ')"
