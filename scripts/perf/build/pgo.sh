#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): profile-guided
# optimisation of the timing binaries of the build-profile study, with the
# raw rustc flags (variants.sh parts pgogen / pgouse) and the toolchain's
# llvm-profdata (rustup component llvm-tools).
#   pgo.sh REPO BASE_VARIANT PGO_DIR
# 1. builds BASE_VARIANT+pgogen (instrumented); 2. runs the TRAINING set,
# which is disjoint from the measured cases of series.sh: the explicit-step
# harness on tet4-20k, tet10-20k and hex8-20k (small strain and finite
# deformation), and the cohesive driver on another mesh than the measured
# one (cube-h0.15.msh from benchmarks/cohesive-perf/inputs/cube.geo with
# h = 0.15) with the `cohesive` material (300 steps) and the `elastic`
# material (100 steps), both with the static preload; facet_scaling is not
# trained (it measures what PGO does to code outside the training set);
# 3. merges the profiles; 4. builds BASE_VARIANT+pgouse.
# MESH_DIR as series.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$1" basev="$2" pgo="$3"
meshdir="${MESH_DIR:-$HOME/w/build/meshes}" broot="${BIN_ROOT:-$HOME/w/build/bin}"
export PGO_DIR="$pgo"
case "$pgo" in /*?/*) rm -rf "$pgo" ;; *) echo "PGO_DIR must be an absolute path" >&2; exit 2 ;; esac
mkdir -p "$pgo/raw"
PGO_DIR="$pgo/raw" "$here/build_variant.sh" "$repo" "$basev+pgogen"
b="$broot/$basev+pgogen"
export LLVM_PROFILE_FILE="$pgo/raw/%p-%m.profraw"
for m in tet4-20k tet10-20k hex8-20k; do
  for fd in "" --fd; do
    # shellcheck disable=SC2086 # empty or one flag
    "$b/explicit" --mesh "$meshdir/$m.msh" --steps 5 --reps 1 --warmup 3 --phases step,fint,stress $fd > /dev/null
  done
done
for mat in cohesive:300 elastic:100; do
  d="$pgo/coh-${mat%%:*}"; mkdir -p "$d"
  sed -e "s|^  max_steps = .*|  max_steps = ${mat#*:}|" -e "s|^  mesh = .*|  mesh = $meshdir/cube-h0.15.msh|" \
    "$repo/benchmarks/cohesive-perf/inputs/material-${mat%%:*}.dat" > "$d/input.dat"
  (cd "$d" && "$b/cohesive" --aka_input_file input.dat > out.txt 2> err.txt)
done
unset LLVM_PROFILE_FILE
pd="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata"
"$pd" merge -o "$pgo/merged.profdata" "$pgo"/raw/*.profraw
ls -la "$pgo/merged.profdata" >&2
PGO_DIR="$pgo" "$here/build_variant.sh" "$repo" "$basev+pgouse"
